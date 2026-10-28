//! winit backend with selectable `glow` or `wgpu` rendering.
//!
//! Enable exactly one of the `glow` and `wgpu` crate features. Both targets
//! share the same event handling, runtime, draw-list conversion, clipboard and
//! IME code; only window-surface creation and presentation differ.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context as _, Result};
use kasumi::{App, ClipboardAction, Runtime, Theme};
use kasumi_core::{
    CursorIcon, InputEvent, InputState, Key, Modifiers, MouseButton, Point, Size, Vec2,
};
use kasumi_paint::TextEngine;
use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, Ime, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
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

struct BackendApp<A: App> {
    app: A,
    options: WindowOptions,
    texts: Arc<TextEngine>,
    gpu: Option<GpuState>,
    runtime: Option<Runtime>,
    pending_events: Vec<InputEvent>,
    pointer: Point,
    pointer_inside: bool,
    modifiers: Modifiers,
    cursor: CursorIcon,
    next_frame_at: Option<Instant>,
    occluded: bool,
    rendered_frames: u64,
    exit_after_frames: Option<u64>,
    stats: bool,
    clipboard: Option<arboard::Clipboard>,
    accepts_ime: bool,
    error: Option<anyhow::Error>,
    start: Instant,
}

impl<A: App> BackendApp<A> {
    fn new(app: A, options: WindowOptions, texts: Arc<TextEngine>) -> Self {
        let exit_after_frames = std::env::var("KASUMI_EXIT_AFTER_FRAMES")
            .ok()
            .and_then(|value| value.parse().ok());
        let stats = std::env::var_os("KASUMI_STATS").is_some();
        Self {
            app,
            options,
            texts,
            gpu: None,
            runtime: None,
            pending_events: Vec::new(),
            pointer: Point::ZERO,
            pointer_inside: true,
            modifiers: Modifiers::default(),
            cursor: CursorIcon::Default,
            next_frame_at: None,
            occluded: false,
            rendered_frames: 0,
            exit_after_frames,
            stats,
            clipboard: None,
            accepts_ime: false,
            error: None,
            start: Instant::now(),
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }

    fn request_redraw(&self) {
        if let Some(gpu) = &self.gpu {
            gpu.window().request_redraw();
        }
    }

    fn push(&mut self, event: InputEvent) {
        self.pending_events.push(event);
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

    fn hover_would_change(&self, point: Point, inside: bool) -> bool {
        let Some(runtime) = self.runtime.as_ref() else {
            return true;
        };
        let next = if inside {
            runtime.hover_at(point)
        } else {
            None
        };
        next != runtime.hovered()
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        if self.occluded {
            return;
        }
        let Some(mut runtime) = self.runtime.take() else {
            return;
        };
        let Some(gpu) = self.gpu.as_mut() else {
            self.runtime = Some(runtime);
            return;
        };
        if gpu.physical_size.0 == 0 || gpu.physical_size.1 == 0 {
            self.runtime = Some(runtime);
            return;
        }
        let scale = gpu.scale_factor().max(0.01);
        let viewport = gpu.logical_size();

        let input = InputState {
            time: self.start.elapsed().as_secs_f64(),
            viewport,
            scale_factor: scale,
            pointer: self.pointer,
            pointer_inside: self.pointer_inside,
            modifiers: self.modifiers,
            events: std::mem::take(&mut self.pending_events),
        };

        let output = runtime.frame(&mut self.app, input);
        let ime = output.ime;
        if ime.focused != self.accepts_ime {
            self.accepts_ime = ime.focused;
            gpu.window().set_ime_allowed(ime.focused);
            if !ime.focused {
                runtime.take_ime_area_update(kasumi_core::Rect::ZERO);
            }
        }
        if ime.focused
            && let Some(rect) = ime.cursor
            && let Some(rect) = runtime.take_ime_area_update(rect)
        {
            gpu.window().set_ime_cursor_area(
                winit::dpi::LogicalPosition::new(rect.left(), rect.top()),
                winit::dpi::LogicalSize::new(rect.width().max(2.0), rect.height().max(1.0)),
            );
        }
        let target = runtime.frame_clipboard_target();
        let read_requested = output.clipboard.as_ref() == Some(&ClipboardAction::ReadText);
        self.runtime = Some(runtime);

        let paste_text = if read_requested {
            self.read_clipboard()
        } else {
            None
        };
        let Some(runtime) = self.runtime.take() else {
            return;
        };
        let Some(gpu) = self.gpu.as_mut() else {
            self.runtime = Some(runtime);
            return;
        };
        if let Err(error) = gpu.render(&runtime, self.options.theme.background) {
            self.runtime = Some(runtime);
            self.fail(event_loop, error);
            return;
        }

        if output.cursor != self.cursor {
            self.cursor = output.cursor;
            gpu.window().set_cursor(winit_cursor(output.cursor));
        }
        if let Some(text) = paste_text {
            self.pending_events.push(match target {
                Some(target) => InputEvent::PasteTo { target, text },
                None => InputEvent::Paste(text),
            });
        }
        self.rendered_frames += 1;
        if self.stats {
            let stats = output.stats;
            if stats.frame <= 3 || stats.frame % 120 == 0 {
                eprintln!(
                    "kasumi: frame {} · {} nodes · layout updates {}/{} measures {} · \
                     {} rects {} glyphs {} images · {:.2} ms",
                    stats.frame,
                    stats.nodes,
                    stats.layout.style_updates,
                    stats.layout.child_updates,
                    stats.layout.measures,
                    stats.paint.rects,
                    stats.paint.glyphs,
                    stats.paint.images,
                    stats.cpu_time.as_secs_f64() * 1000.0
                );
            }
        }

        if let Some(limit) = self.exit_after_frames
            && self.rendered_frames >= limit
        {
            self.runtime = Some(runtime);
            event_loop.exit();
            return;
        }

        match output.next_frame_in {
            Some(delay) if delay.is_zero() => {
                gpu.window().request_redraw();
                event_loop.set_control_flow(ControlFlow::Wait);
                self.next_frame_at = None;
            }
            Some(delay) => {
                let deadline = Instant::now() + delay;
                self.next_frame_at = Some(deadline);
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
            None => {
                self.next_frame_at = None;
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
        self.runtime = Some(runtime);

        if let Some(ClipboardAction::SetText(text)) = output.clipboard.as_ref()
            && let Some(clipboard) = self.clipboard()
            && let Err(error) = clipboard.set_text(text.clone())
        {
            eprintln!("kasumi: failed to update the clipboard: {error}");
        }
    }
}

impl<A: App> ApplicationHandler for BackendApp<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        match GpuState::new(event_loop, &self.options) {
            Ok(gpu) => {
                let viewport = gpu.logical_size();
                let runtime = Runtime::new(
                    Arc::clone(&self.texts),
                    viewport,
                    self.options.theme.clone(),
                );
                self.runtime = Some(runtime);
                self.gpu = Some(gpu);
                self.request_redraw();
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.gpu = None;
        self.runtime = None;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        if window_id != gpu.window().id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                }
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.request_redraw();
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                if !occluded {
                    self.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self
                    .gpu
                    .as_ref()
                    .map(|gpu| gpu.scale_factor())
                    .unwrap_or(1.0);
                let point = Point::new(position.x as f32 / scale, position.y as f32 / scale);
                self.pointer = point;
                self.pointer_inside = true;
                // A pointer move that stays inside the same hover target
                // changes nothing on screen. Skipping the frame here keeps
                // idle UI genuinely idle even when the compositor delivers a
                // steady trickle of motion events.
                if self.hover_would_change(point, true)
                    || self.runtime.as_ref().is_some_and(Runtime::pointer_captured)
                {
                    self.push(InputEvent::PointerMoved(point));
                    self.request_redraw();
                }
            }
            WindowEvent::CursorEntered { .. } => {
                self.pointer_inside = true;
                if self.hover_would_change(self.pointer, true) {
                    self.push(InputEvent::PointerMoved(self.pointer));
                    self.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                let had_hover = self.runtime.as_ref().and_then(|r| r.hovered()).is_some();
                self.pointer_inside = false;
                if had_hover {
                    self.push(InputEvent::PointerLeft);
                    self.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = mouse_button(button);
                let event = match state {
                    ElementState::Pressed => InputEvent::PointerDown {
                        position: self.pointer,
                        button,
                    },
                    ElementState::Released => InputEvent::PointerUp {
                        position: self.pointer,
                        button,
                    },
                };
                self.push(event);
                self.request_redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self
                    .gpu
                    .as_ref()
                    .map(|gpu| gpu.scale_factor())
                    .unwrap_or(1.0);
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        Vec2::new(x * 52.0 * scale, -y * 52.0 * scale)
                    }
                    MouseScrollDelta::PixelDelta(position) => {
                        Vec2::new(position.x as f32 / scale, -position.y as f32 / scale)
                    }
                };
                let delta = if self.modifiers.shift && delta.x.abs() < 0.01 {
                    Vec2::new(delta.y, 0.0)
                } else {
                    delta
                };
                self.push(InputEvent::Scroll(delta));
                self.request_redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers_from_winit(modifiers.state());
                self.push(InputEvent::ModifiersChanged(self.modifiers));
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = map_key(&event.logical_key);
                match event.state {
                    ElementState::Pressed => {
                        if let Some(key) = key.clone() {
                            self.push(InputEvent::KeyDown {
                                key,
                                repeat: event.repeat,
                            });
                        }
                        if (self.modifiers.ctrl || self.modifiers.logo)
                            && matches!(
                                &event.logical_key,
                                WinitKey::Character(text) if text.eq_ignore_ascii_case("v")
                            )
                        {
                            let has_focused_text = self
                                .runtime
                                .as_ref()
                                .is_some_and(Runtime::text_input_focused);
                            if !has_focused_text {
                                self.request_redraw();
                                return;
                            }
                            if let Some(text) = self.read_clipboard() {
                                self.push(InputEvent::Paste(text));
                            }
                            self.request_redraw();
                            return;
                        }
                        if let Some(text) = event.text.as_ref()
                            && !text.chars().all(char::is_control)
                        {
                            self.push(InputEvent::Text(text.to_string()));
                        }
                    }
                    ElementState::Released => {
                        if let Some(key) = key {
                            self.push(InputEvent::KeyUp { key });
                        }
                    }
                }
                self.request_redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.push(InputEvent::Text(text));
                self.request_redraw();
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                self.push(InputEvent::ImePreedit { text, cursor });
                self.request_redraw();
            }
            WindowEvent::Focused(focused) => {
                self.push(InputEvent::FocusChanged(focused));
                if !focused {
                    self.pointer_inside = false;
                }
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.render(event_loop);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(deadline) = self.next_frame_at {
            let now = Instant::now();
            if now >= deadline {
                self.next_frame_at = None;
                self.request_redraw();
                event_loop.set_control_flow(ControlFlow::Wait);
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

/// Runs an application until its window closes.
pub fn run<A: App + 'static>(app: A, options: WindowOptions) -> Result<()> {
    let event_loop = EventLoop::new().context("creating the winit event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let texts = Arc::new(TextEngine::new());
    let mut backend = BackendApp::new(app, options, texts);
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
