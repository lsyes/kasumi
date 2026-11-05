//! winit backend with selectable `glow` or `wgpu` rendering.
//!
//! Enable exactly one of the `glow` and `wgpu` crate features. Both targets
//! share the same event handling, runtime, draw-list conversion, clipboard and
//! IME code; only window-surface creation and presentation differ.
//!
//! Each platform window owns an independent `Runtime`, GPU surface and input
//! state. The application object is shared, and `App::view` runs once per
//! window per frame. `Ui::window()` identifies the window being rendered and
//! `Ui::windows()` lists every live window. Window commands emitted from a
//! frame are executed by this backend after presentation.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context as _, Result};
use kasumi::{
    App, ClipboardAction, FrameOutput, Runtime, Theme, WindowAction, WindowInfo, WindowKey,
    WindowRequest,
};
use kasumi_core::{
    CursorIcon, InputEvent, InputState, Key, Modifiers, MouseButton, Point, Size, Vec2,
};
use kasumi_paint::TextEngine;
use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, Ime, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::window::{CursorIcon as WinitCursorIcon, Window, WindowId};

#[cfg(all(feature = "glow", feature = "wgpu"))]
compile_error!("kasumi-backend features `glow` and `wgpu` are mutually exclusive");
#[cfg(not(any(feature = "glow", feature = "wgpu")))]
compile_error!("kasumi-backend requires either the `glow` or `wgpu` feature");

#[cfg(feature = "glow")]
mod atlas;
mod atlas_shelf;
#[cfg(feature = "glow")]
mod glow_backend;
#[cfg(feature = "glow")]
mod renderer;

/// The graphics API selected for this build.
pub const RENDERER: &str = if cfg!(feature = "wgpu") {
    "wgpu"
} else {
    "glow"
};

/// Chromium normalizes a classic wheel notch to 120 logical pixels (three
/// 40 px lines). Winit reports line deltas directly, so one notch becomes one
/// Chromium wheel step in the layout's logical coordinate space.
const WHEEL_PX_PER_LINE: f32 = 120.0;

#[derive(Debug, Clone)]
pub struct WindowOptions {
    pub title: String,
    pub logical_size: Size,
    pub min_size: Option<Size>,
    pub theme: Theme,
    pub vsync: bool,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "Kasumi".to_owned(),
            logical_size: Size::new(1100.0, 760.0),
            min_size: Some(Size::new(640.0, 480.0)),
            theme: Theme::default(),
            vsync: true,
        }
    }
}

impl WindowOptions {
    pub fn from_request(request: WindowRequest, fallback_theme: Theme) -> Self {
        Self {
            title: request.title,
            logical_size: request.size,
            min_size: request.min_size,
            theme: request.theme.unwrap_or(fallback_theme),
            vsync: request.vsync,
        }
    }
}

struct GpuState {
    backend: GpuBackend,
    physical_size: (u32, u32),
}

impl GpuState {
    fn new(event_loop: &ActiveEventLoop, options: &WindowOptions) -> Result<Self> {
        let backend = GpuBackend::new(event_loop, options)?;
        let initial = backend.window().inner_size();
        let physical_size = (initial.width, initial.height);
        Ok(Self {
            backend,
            physical_size,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.backend.resize(width, height);
            self.physical_size = (width, height);
        }
    }

    fn scale_factor(&self) -> f32 {
        self.backend.window().scale_factor() as f32
    }

    fn logical_size(&self) -> Size {
        let scale = self.scale_factor().max(0.01);
        Size::new(
            self.physical_size.0 as f32 / scale,
            self.physical_size.1 as f32 / scale,
        )
    }

    fn window(&self) -> &Window {
        self.backend.window()
    }

    fn render(&mut self, runtime: &Runtime, clear: kasumi_core::Color) -> Result<()> {
        let result = self
            .backend
            .render(runtime, self.logical_size(), self.physical_size, clear);
        self.backend.poll();
        result
    }
}

#[cfg(feature = "glow")]
type GpuBackend = glow_backend::GlowBackend;
#[cfg(feature = "wgpu")]
type GpuBackend = wgpu_renderer::WgpuBackend;

#[cfg(feature = "wgpu")]
mod wgpu_renderer;

struct WindowState {
    key: WindowKey,
    title: String,
    is_primary: bool,
    focused: bool,
    gpu: GpuState,
    runtime: Runtime,
    pending_events: Vec<InputEvent>,
    pointer: Point,
    pointer_inside: bool,
    modifiers: Modifiers,
    cursor: CursorIcon,
    next_frame_at: Option<Instant>,
    occluded: bool,
    accepts_ime: bool,
}

impl WindowState {
    fn info(&self) -> WindowInfo {
        WindowInfo {
            key: self.key,
            title: self.title.clone(),
            size: self.gpu.logical_size(),
            scale_factor: self.gpu.scale_factor(),
            is_primary: self.is_primary,
            focused: self.focused,
        }
    }

    fn request_redraw(&self) {
        self.gpu.window().request_redraw();
    }

    fn hover_would_change(&self, point: Point, inside: bool) -> bool {
        let next = if inside {
            self.runtime.hover_at(point)
        } else {
            None
        };
        next != self.runtime.hovered()
    }

    fn push(&mut self, event: InputEvent) {
        self.pending_events.push(event);
    }
}

struct BackendApp<A: App> {
    app: A,
    options: WindowOptions,
    texts: Arc<TextEngine>,
    windows: HashMap<WindowId, WindowState>,
    next_window_key: u64,
    clipboard: Option<arboard::Clipboard>,
    error: Option<anyhow::Error>,
    stats: bool,
    exit_after_frames: Option<u64>,
    rendered_frames: u64,
    /// Wakes the winit loop from resource worker threads.
    proxy: EventLoopProxy<()>,
    start: Instant,
}

impl<A: App> BackendApp<A> {
    fn new(
        app: A,
        options: WindowOptions,
        texts: Arc<TextEngine>,
        proxy: EventLoopProxy<()>,
    ) -> Self {
        let exit_after_frames = std::env::var("KASUMI_EXIT_AFTER_FRAMES")
            .ok()
            .and_then(|value| value.parse().ok());
        let stats = std::env::var_os("KASUMI_STATS").is_some();
        Self {
            app,
            options,
            texts,
            windows: HashMap::new(),
            next_window_key: 2,
            clipboard: None,
            error: None,
            stats,
            exit_after_frames,
            rendered_frames: 0,
            proxy,
            start: Instant::now(),
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }

    fn allocate_key(&mut self, primary: bool) -> WindowKey {
        if primary {
            return WindowKey::MAIN;
        }
        let key = WindowKey(self.next_window_key);
        self.next_window_key = self.next_window_key.saturating_add(1);
        key
    }

    /// Creates a platform window, GPU surface and runtime. The window is not
    /// given its first frame here; it is requested from the event loop.
    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        options: WindowOptions,
        primary: bool,
    ) -> Result<WindowId> {
        let gpu = GpuState::new(event_loop, &options)?;
        let window_id = gpu.window().id();
        let key = self.allocate_key(primary);
        let viewport = gpu.logical_size();
        let info = WindowInfo {
            key,
            title: options.title.clone(),
            size: viewport,
            scale_factor: gpu.scale_factor(),
            is_primary: primary,
            focused: true,
        };
        let mut runtime = Runtime::new(Arc::clone(&self.texts), viewport, options.theme.clone());
        runtime.set_window_info(info.clone());
        let proxy = self.proxy.clone();
        runtime.set_resource_wake_callback(Arc::new(move || {
            let _ = proxy.send_event(());
        }));
        let mut infos: Vec<WindowInfo> = self
            .windows
            .values()
            .map(WindowState::info)
            .chain(std::iter::once(info))
            .collect();
        infos.sort_by_key(|info| info.key);
        runtime.set_windows(infos);
        let state = WindowState {
            key,
            title: options.title,
            is_primary: primary,
            focused: true,
            gpu,
            runtime,
            pending_events: Vec::new(),
            pointer: Point::ZERO,
            pointer_inside: true,
            modifiers: Modifiers::default(),
            cursor: CursorIcon::Default,
            next_frame_at: None,
            occluded: false,
            accepts_ime: false,
        };
        state.request_redraw();
        self.windows.insert(window_id, state);
        Ok(window_id)
    }

    fn request_all_redraw(&self) {
        for state in self.windows.values() {
            state.request_redraw();
        }
    }

    fn window_id_for_key(&self, key: WindowKey) -> Option<WindowId> {
        self.windows
            .iter()
            .find_map(|(id, state)| (state.key == key).then_some(*id))
    }

    /// Create clipboard access on first use, after the window and event loop
    /// exist. Failed initialization is retried on the next request.
    fn clipboard(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.clipboard.is_none() {
            match arboard::Clipboard::new() {
                Ok(clipboard) => self.clipboard = Some(clipboard),
                Err(error) => {
                    eprintln!("kasumi: failed to initialize the clipboard: {error}");
                    return None;
                }
            }
        }
        self.clipboard.as_mut()
    }

    fn read_clipboard(&mut self) -> Option<String> {
        match self.clipboard().map(arboard::Clipboard::get_text) {
            Some(Ok(text)) if !text.is_empty() => Some(text),
            Some(Ok(_)) => {
                eprintln!("kasumi: clipboard contains no text");
                None
            }
            Some(Err(error)) => {
                eprintln!("kasumi: failed to read the clipboard: {error}");
                None
            }
            None => None,
        }
    }

    /// Run one frame for `state` and present it. Returns false when the window
    /// was closed by a window command during this frame.
    fn render_window(&mut self, event_loop: &ActiveEventLoop, state: &mut WindowState) -> bool {
        if state.occluded {
            return true;
        }
        if state.gpu.physical_size.0 == 0 || state.gpu.physical_size.1 == 0 {
            return true;
        }
        let scale = state.gpu.scale_factor().max(0.01);
        let viewport = state.gpu.logical_size();

        let info = WindowInfo {
            size: viewport,
            scale_factor: scale,
            focused: state.focused,
            ..state.info()
        };
        state.runtime.set_window_info(info.clone());
        let mut infos: Vec<WindowInfo> = self
            .windows
            .values()
            .filter(|other| other.key != state.key)
            .map(WindowState::info)
            .chain(std::iter::once(info))
            .collect();
        infos.sort_by_key(|info| info.key);
        state.runtime.set_windows(infos);

        let input = InputState {
            time: self.start.elapsed().as_secs_f64(),
            viewport,
            scale_factor: scale,
            pointer: state.pointer,
            pointer_inside: state.pointer_inside,
            modifiers: state.modifiers,
            events: std::mem::take(&mut state.pending_events),
        };

        let output = state.runtime.frame(&mut self.app, input);
        let FrameOutput {
            cursor,
            next_frame_in,
            clipboard,
            ime,
            window_actions,
            stats,
        } = output;

        if ime.focused != state.accepts_ime {
            state.accepts_ime = ime.focused;
            state.gpu.window().set_ime_allowed(ime.focused);
            if !ime.focused {
                state.runtime.take_ime_area_update(kasumi_core::Rect::ZERO);
            }
        }
        if ime.focused
            && let Some(rect) = ime.cursor
            && let Some(rect) = state.runtime.take_ime_area_update(rect)
        {
            state.gpu.window().set_ime_cursor_area(
                winit::dpi::LogicalPosition::new(rect.left(), rect.top()),
                winit::dpi::LogicalSize::new(rect.width().max(2.0), rect.height().max(1.0)),
            );
        }
        let target = state.runtime.frame_clipboard_target();
        let read_requested = clipboard.as_ref() == Some(&ClipboardAction::ReadText);
        let paste_text = if read_requested {
            self.read_clipboard()
        } else {
            None
        };

        let clear = state.runtime.theme().background;
        if let Err(error) = state.gpu.render(&state.runtime, clear) {
            self.fail(event_loop, error);
            return true;
        }

        if cursor != state.cursor {
            state.cursor = cursor;
            state.gpu.window().set_cursor(winit_cursor(cursor));
        }
        if let Some(text) = paste_text {
            state.pending_events.push(match target {
                Some(target) => InputEvent::PasteTo { target, text },
                None => InputEvent::Paste(text),
            });
        }

        // The loop below must not keep the old frame alive; borrow only the
        // state needed for scheduling after the render.
        match next_frame_in {
            Some(delay) if delay.is_zero() => {
                state.gpu.window().request_redraw();
                state.next_frame_at = None;
            }
            Some(delay) => {
                state.next_frame_at = Some(Instant::now() + delay);
            }
            None => {
                state.next_frame_at = None;
            }
        }

        self.rendered_frames += 1;
        if self.stats && (stats.frame <= 3 || stats.frame % 120 == 0) {
            eprintln!(
                "kasumi: {} · frame {} · {} nodes · {} resources · layout updates {}/{} \
                 measures {} · {} rects {} glyphs {} images · {:.2} ms",
                state.key,
                stats.frame,
                stats.nodes,
                stats.resources,
                stats.layout.style_updates,
                stats.layout.child_updates,
                stats.layout.measures,
                stats.paint.rects,
                stats.paint.glyphs,
                stats.paint.images,
                stats.cpu_time.as_secs_f64() * 1000.0
            );
        }

        let keep = self.apply_window_actions(event_loop, state, window_actions);

        if let Some(limit) = self.exit_after_frames
            && self.rendered_frames >= limit
        {
            event_loop.exit();
        }

        if let Some(ClipboardAction::SetText(text)) = clipboard.as_ref()
            && let Some(clipboard) = self.clipboard()
            && let Err(error) = clipboard.set_text(text.clone())
        {
            eprintln!("kasumi: failed to update the clipboard: {error}");
        }

        keep
    }

    /// Executes window commands requested during a frame.
    ///
    /// Returns false when the currently rendered window closed itself and must
    /// not be reinserted into the map.
    fn apply_window_actions(
        &mut self,
        event_loop: &ActiveEventLoop,
        state: &mut WindowState,
        actions: Vec<WindowAction>,
    ) -> bool {
        let mut keep = true;
        for action in actions {
            match action {
                WindowAction::Open(request) => {
                    let options = WindowOptions::from_request(request, self.options.theme.clone());
                    match self.create_window(event_loop, options, false) {
                        Ok(_) => self.request_all_redraw(),
                        Err(error) => {
                            eprintln!("kasumi: failed to open a window: {error}");
                            self.error = Some(error);
                        }
                    }
                    state.request_redraw();
                }
                WindowAction::Close(key) => {
                    if key == state.key {
                        keep = false;
                    } else if let Some(window_id) = self.window_id_for_key(key) {
                        self.windows.remove(&window_id);
                    }
                    if keep {
                        state.request_redraw();
                    }
                    self.request_all_redraw();
                    if !keep && self.windows.is_empty() {
                        event_loop.exit();
                    }
                }
                WindowAction::Focus(key) => {
                    if key == state.key {
                        state.gpu.window().focus_window();
                    } else if let Some(window_id) = self.window_id_for_key(key)
                        && let Some(target) = self.windows.get(&window_id)
                    {
                        target.gpu.window().focus_window();
                    }
                }
                WindowAction::SetTitle { window, title } => {
                    if window == state.key {
                        state.title = title.clone();
                        state.gpu.window().set_title(&title);
                    } else if let Some(window_id) = self.window_id_for_key(window)
                        && let Some(target) = self.windows.get_mut(&window_id)
                    {
                        target.title = title.clone();
                        target.gpu.window().set_title(&title);
                    }
                    state.request_redraw();
                    self.request_all_redraw();
                }
            }
        }
        keep
    }

    fn dispatch_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        state: &mut WindowState,
        event: WindowEvent,
    ) -> bool {
        match event {
            WindowEvent::CloseRequested => return false,
            WindowEvent::Resized(size) => {
                state.gpu.resize(size.width, size.height);
                state.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                state.request_redraw();
            }
            WindowEvent::Occluded(occluded) => {
                state.occluded = occluded;
                if !occluded {
                    state.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = state.gpu.scale_factor();
                let point = Point::new(position.x as f32 / scale, position.y as f32 / scale);
                state.pointer = point;
                state.pointer_inside = true;
                // A pointer move that stays inside the same hover target
                // changes nothing on screen. Skipping the frame here keeps
                // idle UI genuinely idle even when the compositor delivers a
                // steady trickle of motion events.
                if state.hover_would_change(point, true) || state.runtime.pointer_captured() {
                    state.push(InputEvent::PointerMoved(point));
                    state.request_redraw();
                }
            }
            WindowEvent::CursorEntered { .. } => {
                state.pointer_inside = true;
                if state.hover_would_change(state.pointer, true) {
                    state.push(InputEvent::PointerMoved(state.pointer));
                    state.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                let had_hover = state.runtime.hovered().is_some();
                state.pointer_inside = false;
                if had_hover {
                    state.push(InputEvent::PointerLeft);
                    state.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: input_state,
                button,
                ..
            } => {
                let button = mouse_button(button);
                let event = match input_state {
                    ElementState::Pressed => InputEvent::PointerDown {
                        position: state.pointer,
                        button,
                    },
                    ElementState::Released => InputEvent::PointerUp {
                        position: state.pointer,
                        button,
                    },
                };
                state.push(event);
                state.request_redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = state.gpu.scale_factor();
                let (delta, line_delta) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (
                        Vec2::new(x * WHEEL_PX_PER_LINE, -y * WHEEL_PX_PER_LINE),
                        Some(Vec2::new(x, -y)),
                    ),
                    MouseScrollDelta::PixelDelta(position) => (
                        Vec2::new(position.x as f32 / scale, -position.y as f32 / scale),
                        None,
                    ),
                };
                let delta = if state.modifiers.shift && delta.x.abs() < 0.01 {
                    Vec2::new(delta.y, 0.0)
                } else {
                    delta
                };
                state.push(InputEvent::ScrollAt {
                    delta,
                    line_delta,
                    time: self.start.elapsed().as_secs_f64(),
                });
                state.request_redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                state.modifiers = modifiers_from_winit(modifiers.state());
                state.push(InputEvent::ModifiersChanged(state.modifiers));
                state.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = map_key(&event.logical_key);
                match event.state {
                    ElementState::Pressed => {
                        if let Some(key) = key.clone() {
                            state.push(InputEvent::KeyDown {
                                key,
                                repeat: event.repeat,
                            });
                        }
                        if (state.modifiers.ctrl || state.modifiers.logo)
                            && matches!(
                                &event.logical_key,
                                WinitKey::Character(text) if text.eq_ignore_ascii_case("v")
                            )
                        {
                            let has_focused_text = state.runtime.text_input_focused();
                            if !has_focused_text {
                                state.request_redraw();
                                return true;
                            }
                            if let Some(text) = self.read_clipboard() {
                                state.push(InputEvent::Paste(text));
                            }
                            state.request_redraw();
                            return true;
                        }
                        if let Some(text) = event.text.as_ref()
                            && !text.chars().all(char::is_control)
                        {
                            state.push(InputEvent::Text(text.to_string()));
                        }
                    }
                    ElementState::Released => {
                        if let Some(key) = key {
                            state.push(InputEvent::KeyUp { key });
                        }
                    }
                }
                state.request_redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                state.push(InputEvent::Text(text));
                state.request_redraw();
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                state.push(InputEvent::ImePreedit { text, cursor });
                state.request_redraw();
            }
            WindowEvent::Focused(focused) => {
                state.focused = focused;
                state.push(InputEvent::FocusChanged(focused));
                if !focused {
                    state.pointer_inside = false;
                }
                state.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                return self.render_window(event_loop, state);
            }
            _ => {}
        }
        true
    }
}

impl<A: App> ApplicationHandler for BackendApp<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.windows.is_empty() {
            return;
        }
        match self.create_window(event_loop, self.options.clone(), true) {
            Ok(_) => {
                self.request_all_redraw();
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.windows.clear();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(mut state) = self.windows.remove(&window_id) else {
            return;
        };
        if state.gpu.window().id() != window_id {
            self.windows.insert(window_id, state);
            return;
        }
        let keep = self.dispatch_window_event(event_loop, &mut state, event);
        if keep {
            self.windows.insert(window_id, state);
        } else {
            if self.windows.is_empty() {
                event_loop.exit();
            } else {
                self.request_all_redraw();
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        // A background resource finished. Redraw every window; each runtime
        // reads its own resource table and ignores the event if nothing it
        // declared changed.
        self.request_all_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let mut wake_at: Option<Instant> = None;
        for state in self.windows.values_mut() {
            if let Some(deadline) = state.next_frame_at {
                if now >= deadline {
                    state.next_frame_at = None;
                    state.request_redraw();
                } else {
                    wake_at = Some(wake_at.map_or(deadline, |earliest| earliest.min(deadline)));
                }
            }
        }
        match wake_at {
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

/// Runs an application until its last window closes.
pub fn run<A: App + 'static>(app: A, options: WindowOptions) -> Result<()> {
    let event_loop: EventLoop<()> = EventLoop::with_user_event()
        .build()
        .context("creating the winit event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();

    let texts = Arc::new(TextEngine::new());
    let mut backend = BackendApp::new(app, options, texts, proxy);
    event_loop
        .run_app(&mut backend)
        .context("running the winit event loop")?;

    if let Some(error) = backend.error {
        Err(error)
    } else {
        Ok(())
    }
}

fn mouse_button(button: WinitMouseButton) -> MouseButton {
    match button {
        WinitMouseButton::Left => MouseButton::Left,
        WinitMouseButton::Right => MouseButton::Right,
        WinitMouseButton::Middle => MouseButton::Middle,
        WinitMouseButton::Back => MouseButton::Other(3),
        WinitMouseButton::Forward => MouseButton::Other(4),
        WinitMouseButton::Other(other) => MouseButton::Other(other),
    }
}

fn map_key(key: &WinitKey) -> Option<Key> {
    let named = match key {
        WinitKey::Named(named) => named,
        WinitKey::Character(text) => {
            return Some(Key::Character(text.to_string()));
        }
        _ => return None,
    };
    let key = match named {
        NamedKey::Tab => Key::Tab,
        NamedKey::Enter => Key::Enter,
        NamedKey::Escape => Key::Escape,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::ArrowLeft => Key::ArrowLeft,
        NamedKey::ArrowRight => Key::ArrowRight,
        NamedKey::ArrowUp => Key::ArrowUp,
        NamedKey::ArrowDown => Key::ArrowDown,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::Space => Key::Space,
        _ => return None,
    };
    Some(key)
}

fn modifiers_from_winit(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        logo: state.super_key(),
    }
}

fn winit_cursor(cursor: CursorIcon) -> WinitCursorIcon {
    match cursor {
        CursorIcon::Default => WinitCursorIcon::Default,
        CursorIcon::Pointer => WinitCursorIcon::Pointer,
        CursorIcon::Text => WinitCursorIcon::Text,
        CursorIcon::Grab => WinitCursorIcon::Grab,
        CursorIcon::Grabbing => WinitCursorIcon::Grabbing,
        CursorIcon::Crosshair => WinitCursorIcon::Crosshair,
        CursorIcon::NotAllowed => WinitCursorIcon::NotAllowed,
        CursorIcon::ResizeHorizontal => WinitCursorIcon::EwResize,
        CursorIcon::ResizeVertical => WinitCursorIcon::NsResize,
    }
}
