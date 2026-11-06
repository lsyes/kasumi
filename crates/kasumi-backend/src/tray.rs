//! Cross-platform system tray.
//!
//! A tray icon lives in the desktop's status area and exposes a small menu:
//! "show the window", a theme switch, "quit". The three desktop platforms have
//! nothing in common at the API level, so this module defines one small model
//! and implements it per platform behind `cfg`:
//!
//! | Platform | Implementation |
//! |---|---|
//! | Linux | `ksni`, which speaks the freedesktop StatusNotifierItem protocol over D-Bus. No GTK and no libappindicator are needed. |
//! | Windows | `Shell_NotifyIconW` plus a message-only window and `TrackPopupMenu`, driven by the winit message pump. |
//! | macOS | `NSStatusBar`/`NSStatusItem` and `NSMenu` through `objc2`. |
//!
//! The application only ever sees [`TrayMenu`], [`TrayItem`] and [`TrayEvent`].
//! Activating an item posts a [`TrayEvent`] to the winit event loop, so the
//! application handles tray input in `App::view` on the main thread exactly
//! like any other input.
//!
//! ```ignore
//! let tray = Tray::new(TrayMenu::new("kasumi")
//!     .item(TrayItem::new("show", "Show Kasumi"))
//!     .separator()
//!     .item(TrayItem::new("theme", "Dark theme").checkbox(false))
//!     .separator()
//!     .item(TrayItem::new("quit", "Quit")));
//! ```
//!
//! ## Platform notes
//!
//! Linux needs a `StatusNotifierWatcher`, which KDE Plasma, XFCE, Cinnamon,
//! Budgie and the common tiling-bar trays provide. Stock GNOME needs the
//! AppIndicator shell extension. When no watcher is present the tray reports
//! [`TrayStatus::Unavailable`] instead of failing the application.
//!
//! Windows uses a hidden top-level window rather than a message-only window, so
//! the icon survives an Explorer restart (`TaskbarCreated`).
//!
//! macOS creates the status item on the main thread during `resumed` and
//! keeps it for the lifetime of the event loop.

use std::sync::Arc;

pub use kasumi::tray::{TrayEntry, TrayEvent, TrayMenu, TrayMenuItem, TrayStatus};

/// A tray menu together with its current status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySnapshot {
    pub status: TrayStatus,
    pub menu: TrayMenu,
}

impl TraySnapshot {
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

/// Wakes the event loop after a tray activation was queued.
///
/// The Linux tray runs its D-Bus service on a worker thread, so an activation
/// arrives while the winit loop is asleep. The backend installs a wake callback
/// (wrapping an `EventLoopProxy`) so the loop renders exactly one frame for it
/// instead of polling on a timer.
pub type WakeCallback = Arc<dyn Fn() + Send + Sync>;

#[cfg(target_os = "linux")]
pub mod platform {
    //! StatusNotifierItem over D-Bus through `ksni`.

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::sync::{Arc, Mutex};

    use ksni::blocking::TrayMethods as _;
    use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
    use ksni::{MenuItem, Tray as KsniTray};

    use super::{TrayEntry, TrayEvent, TrayMenu, TrayMenuItem, TraySnapshot, TrayStatus};

    /// State shared between the D-Bus service thread and the event loop.
    struct Shared {
        menu: Mutex<TrayMenu>,
        events: Mutex<Sender<TrayEvent>>,
        wake: Option<super::WakeCallback>,
        unavailable: AtomicBool,
    }

    impl Shared {
        fn post(&self, event: TrayEvent) {
            if let Ok(events) = self.events.lock() {
                let _ = events.send(event);
            }
            if let Some(wake) = &self.wake {
                wake();
            }
        }
    }

    struct Service {
        shared: Arc<Shared>,
    }

    impl Service {
        fn to_menu(&self, menu: &TrayMenu) -> Vec<MenuItem<Self>> {
            menu.entries()
                .iter()
                .map(|entry| match entry {
                    TrayEntry::Separator => MenuItem::Separator,
                    TrayEntry::Item(item) => self.convert(item),
                })
                .collect()
        }

        fn convert(&self, item: &TrayMenuItem) -> MenuItem<Self> {
            if item.has_children() {
                let children = item
                    .children_ref()
                    .iter()
                    .map(|child| self.convert(child))
                    .collect::<Vec<_>>();
                return SubMenu {
                    label: item.label().to_owned(),
                    enabled: item.is_enabled(),
                    submenu: children,
                    ..Default::default()
                }
                .into();
            }
            let shared = Arc::clone(&self.shared);
            let id = item.id().to_owned();
            // The callback runs while ksni holds the tray lock, so it must only
            // mutate the tray or hand the activation to the event loop. It must
            // never call `Handle::update`, which would re-enter the same lock.
            let activate = Box::new(move |_tray: &mut Self| {
                if let Ok(mut menu) = shared.menu.lock()
                    && let Some(checked) = menu.find(&id).and_then(|item| item.checked())
                {
                    menu.set_checked(&id, !checked);
                }
                shared.post(TrayEvent::Activated { id: id.clone() });
            });
            match item.checked() {
                Some(checked) => CheckmarkItem {
                    label: item.label().to_owned(),
                    enabled: item.is_enabled(),
                    checked,
                    activate,
                    ..Default::default()
                }
                .into(),
                None => StandardItem {
                    label: item.label().to_owned(),
                    enabled: item.is_enabled(),
                    activate,
                    ..Default::default()
                }
                .into(),
            }
        }
    }

    impl KsniTray for Service {
        fn id(&self) -> String {
            "kasumi".to_owned()
        }

        fn title(&self) -> String {
            self.shared
                .menu
                .lock()
                .map(|menu| menu.title().to_owned())
                .unwrap_or_else(|_| "Kasumi".to_owned())
        }

        // A themed name keeps the icon crisp on every desktop without shipping
        // a bitmap, and every icon theme has this one.
        fn icon_name(&self) -> String {
            "application-x-executable".to_owned()
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            let id = self
                .shared
                .menu
                .lock()
                .ok()
                .and_then(|menu| menu.on_activate_id().map(str::to_owned))
                .unwrap_or_else(|| "activate".to_owned());
            self.shared.post(TrayEvent::Activated { id });
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            match self.shared.menu.lock() {
                Ok(menu) => self.to_menu(&menu),
                Err(_) => Vec::new(),
            }
        }

        fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
            self.shared.unavailable.store(true, Ordering::Relaxed);
            self.shared.post(TrayEvent::Unavailable {
                reason: format!("{reason:?}"),
            });
            // Stay alive: the watcher usually returns when a shell restarts,
            // and ksni re-registers automatically.
            true
        }
    }

    /// A running Linux tray. Dropping it unregisters the D-Bus object.
    pub struct Tray {
        handle: Option<ksni::blocking::Handle<Service>>,
        shared: Arc<Shared>,
        events: Receiver<TrayEvent>,
        status: TrayStatus,
    }

    impl Tray {
        /// Registers the tray. A missing status notifier is reported through
        /// [`Tray::status`] and an [`TrayEvent::Unavailable`] rather than an
        /// error, so an application keeps running on a desktop without one.
        pub fn new(menu: TrayMenu, wake: Option<super::WakeCallback>) -> Self {
            // The tray owns its own channel: the D-Bus thread is the sender and
            // the event loop is the receiver, so activations never need to be
            // funnelled through a caller-owned sender.
            let (sender, receiver) = channel();
            let shared = Arc::new(Shared {
                menu: Mutex::new(menu),
                events: Mutex::new(sender.clone()),
                wake,
                unavailable: AtomicBool::new(false),
            });
            // `assume_sni_available` turns a missing watcher into a soft error
            // routed to `watcher_offline`, so a desktop that starts the shell
            // after the app still gets a tray once the watcher appears.
            let spawned = Service {
                shared: Arc::clone(&shared),
            }
            .assume_sni_available(true)
            .spawn();
            let status = match &spawned {
                Ok(_) => TrayStatus::Active,
                Err(error) => TrayStatus::Unavailable(error.to_string()),
            };
            if let TrayStatus::Unavailable(reason) = &status {
                let _ = sender.send(TrayEvent::Unavailable {
                    reason: reason.clone(),
                });
            }
            Self {
                handle: spawned.ok(),
                shared,
                events: receiver,
                status,
            }
        }

        pub fn status(&self) -> TrayStatus {
            if self.shared.unavailable.load(Ordering::Relaxed) {
                return TrayStatus::Unavailable("no StatusNotifierWatcher".to_owned());
            }
            self.status.clone()
        }

        /// Replaces the menu and asks the desktop to re-read it.
        pub fn set_menu(&mut self, menu: TrayMenu) {
            if let Ok(mut current) = self.shared.menu.lock() {
                *current = menu;
            }
            if let Some(handle) = &self.handle {
                let _ = handle.update(|_service| {});
            }
        }

        /// Activations the desktop posted since the last poll.
        pub fn poll(&self) -> Vec<TrayEvent> {
            let mut events = Vec::new();
            while let Ok(event) = self.events.try_recv() {
                events.push(event);
            }
            events
        }

        pub fn snapshot(&self) -> TraySnapshot {
            TraySnapshot {
                status: self.status(),
                menu: self
                    .shared
                    .menu
                    .lock()
                    .map(|menu| menu.clone())
                    .unwrap_or_else(|_| TrayMenu::new("Kasumi")),
            }
        }

        pub fn is_active(&self) -> bool {
            self.handle.is_some() && !self.shared.unavailable.load(Ordering::Relaxed)
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                // Unregister so the icon disappears immediately instead of on
                // the next session.
                handle.shutdown().wait();
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub mod platform {
    //! `Shell_NotifyIconW` with a hidden window and `TrackPopupMenu`.
    //!
    //! The hidden window is a normal top-level window that is never shown,
    //! rather than a message-only window: message-only windows do not receive
    //! broadcast messages, so they miss `TaskbarCreated` and the icon would
    //! never come back after Explorer restarts. The window proc hands
    //! activations to the event loop through a wake callback, so all effectful
    //! work stays on the winit thread.

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION, NOTIFYICONDATAW,
        Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        GetCursorPos, HMENU, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
        MF_UNCHECKED, PostMessageW, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow,
        TPM_BOTTOMALIGN, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_COMMAND, WM_NULL, WNDCLASSW,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
    };

    use super::{
        TrayEntry, TrayEvent, TrayMenu, TrayMenuItem, TraySnapshot, TrayStatus, WakeCallback,
    };

    /// The private message the icon uses to report mouse activity.
    const WM_TRAY: u32 = WM_APP + 1;
    const WM_CONTEXTMENU: u32 = 0x007b;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_LBUTTONUP: u32 = 0x0202;

    /// Everything the window proc needs. One tray per process, so a static
    /// gives the proc access without threading a pointer through Win32.
    struct State {
        hwnd: HWND,
        menu: Mutex<TrayMenu>,
        wake: Option<WakeCallback>,
        pending: Mutex<Vec<TrayEvent>>,
        taskbar_created: u32,
        active: AtomicBool,
    }

    // SAFETY: the raw window handle is only used on the thread that created
    // it; every other field is ordinary synchronised data.
    unsafe impl Send for State {}
    unsafe impl Sync for State {}

    static STATE: Mutex<Option<Arc<State>>> = Mutex::new(None);

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let state = STATE
            .lock()
            .ok()
            .and_then(|state| state.as_ref().map(Arc::clone));
        let Some(state) = state else {
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        };
        if message == WM_TRAY {
            let mouse = (lparam as u32) & 0xffff;
            match mouse {
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(&state),
                WM_LBUTTONUP => {
                    let id = state
                        .menu
                        .lock()
                        .ok()
                        .and_then(|menu| menu.on_activate_id().map(str::to_owned));
                    match id {
                        Some(id) => post(&state, TrayEvent::Activated { id }),
                        None => show_menu(&state),
                    }
                }
                _ => {}
            }
            return 0;
        }
        if message == WM_COMMAND {
            let command = (wparam & 0xffff) as u32;
            let menu = state.menu.lock().ok().map(|menu| menu.clone());
            if let Some(menu) = menu
                && let Some(app_id) = id_of_command(&menu, command)
            {
                if let Some(item) = menu.find(&app_id)
                    && let Some(checked) = item.checked()
                {
                    if let Ok(mut menu) = state.menu.lock() {
                        menu.set_checked(&app_id, !checked);
                    }
                    if let Some(wake) = &state.wake {
                        wake();
                    }
                }
                post(&state, TrayEvent::Activated { id: app_id });
            }
            return 0;
        }
        if state.taskbar_created != 0 && message == state.taskbar_created {
            // Explorer restarted: put the icon back.
            unsafe { add_icon(&state) };
            return 0;
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    fn post(state: &Arc<State>, event: TrayEvent) {
        if let Ok(mut pending) = state.pending.lock() {
            pending.push(event);
        }
        if let Some(wake) = &state.wake {
            wake();
        }
    }

    /// The stable id of every leaf command, in menu order. The position in this
    /// list plus one is the Win32 command id, which is how `WM_COMMAND` maps
    /// back to an application id without Win32 owning any of our data.
    fn command_ids(menu: &TrayMenu) -> Vec<(u32, String, Option<bool>)> {
        fn walk(items: &[&TrayMenuItem], out: &mut Vec<(u32, String, Option<bool>)>) {
            for item in items {
                if item.has_children() {
                    let nested: Vec<&TrayMenuItem> = item.children_ref().iter().collect();
                    walk(&nested, out);
                } else {
                    out.push((out.len() as u32 + 1, item.id().to_owned(), item.checked()));
                }
            }
        }
        let mut out = Vec::new();
        let top: Vec<&TrayMenuItem> = menu
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                TrayEntry::Item(item) => Some(item),
                TrayEntry::Separator => None,
            })
            .collect();
        walk(&top, &mut out);
        out
    }

    fn id_of_command(menu: &TrayMenu, command: u32) -> Option<String> {
        command_ids(menu)
            .into_iter()
            .find(|(id, _, _)| *id == command)
            .map(|(_, stable, _)| stable)
    }

    fn leaf_count(item: &TrayMenuItem) -> usize {
        if item.has_children() {
            item.children_ref().iter().map(leaf_count).sum()
        } else {
            1
        }
    }

    /// The command id of the leaf at `index` when the entry is a plain row.
    fn command_id_for(menu: &TrayMenu, index: usize) -> u32 {
        let mut command = 1u32;
        for (position, entry) in menu.entries().iter().enumerate() {
            if position == index {
                return command;
            }
            if let TrayEntry::Item(item) = entry {
                command += leaf_count(item) as u32;
            }
        }
        command
    }

    /// Builds and shows the popup on the thread that owns the window and pumps
    /// its messages.
    fn show_menu(state: &Arc<State>) {
        let Ok(menu) = state.menu.lock().map(|menu| menu.clone()) else {
            return;
        };
        unsafe {
            let popup = CreatePopupMenu();
            if popup.is_null() {
                return;
            }
            build_popup(popup, &menu);
            let mut point = POINT { x: 0, y: 0 };
            GetCursorPos(&mut point);
            // Without these two calls the menu never dismisses when the user
            // clicks somewhere else.
            SetForegroundWindow(state.hwnd);
            TrackPopupMenu(
                popup,
                TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                point.x,
                point.y,
                0,
                state.hwnd,
                std::ptr::null(),
            );
            PostMessageW(state.hwnd, WM_NULL, 0, 0);
            DestroyMenu(popup);
        }
    }

    unsafe fn build_popup(popup: HMENU, menu: &TrayMenu) {
        for (index, entry) in menu.entries().iter().enumerate() {
            match entry {
                TrayEntry::Separator => unsafe {
                    AppendMenuW(popup, MF_SEPARATOR, 0, std::ptr::null());
                },
                TrayEntry::Item(item) if item.has_children() => {
                    let submenu = unsafe { CreatePopupMenu() };
                    if submenu.is_null() {
                        continue;
                    }
                    let mut nested = TrayMenu::new(item.label().to_owned());
                    for child in item.children_ref() {
                        nested = nested.item(child.clone());
                    }
                    unsafe { build_popup(submenu, &nested) };
                    let label = wide(item.label());
                    let flags = if item.is_enabled() {
                        MF_POPUP
                    } else {
                        MF_POPUP | MF_GRAYED
                    };
                    unsafe {
                        AppendMenuW(popup, flags, submenu as usize, label.as_ptr());
                    }
                }
                TrayEntry::Item(item) => {
                    let command = command_id_for(menu, index);
                    let mut flags = MF_STRING;
                    if !item.is_enabled() {
                        flags |= MF_GRAYED;
                    }
                    match item.checked() {
                        Some(true) => flags |= MF_CHECKED,
                        Some(false) => flags |= MF_UNCHECKED,
                        None => {}
                    }
                    let label = wide(item.label());
                    unsafe {
                        AppendMenuW(popup, flags, command as usize, label.as_ptr());
                    }
                }
            }
        }
    }

    unsafe fn add_icon(state: &Arc<State>) {
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = state.hwnd;
        data.uID = 1;
        data.uFlags = NIF_MESSAGE | NIF_SHOWTIP | NIF_TIP;
        data.uCallbackMessage = WM_TRAY;
        let title = state
            .menu
            .lock()
            .map(|menu| menu.title().to_owned())
            .unwrap_or_else(|_| "Kasumi".to_owned());
        let tooltip = wide(&title);
        for (slot, unit) in data.szTip.iter_mut().zip(tooltip.iter()) {
            *slot = *unit;
        }
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &data);
            data.Anonymous.uVersion = 4; // NOTIFYICON_VERSION_4
            Shell_NotifyIconW(NIM_SETVERSION, &data);
        }
    }

    /// A running Windows tray.
    pub struct Tray {
        state: Arc<State>,
        status: TrayStatus,
    }

    impl Tray {
        /// Creates the hidden window and adds the icon. Must run on the thread
        /// that owns the winit event loop.
        pub fn new(menu: TrayMenu, wake: Option<WakeCallback>) -> Self {
            let title = wide(menu.title());
            let class = wide(&format!("KasumiTray{}", std::process::id()));
            let hinstance: HINSTANCE = unsafe { GetModuleHandleW(std::ptr::null()) };
            let window_class = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
            };
            unsafe { RegisterClassW(&window_class) };
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null(),
                )
            };
            if hwnd.is_null() {
                return Self {
                    state: Arc::new(State {
                        hwnd,
                        menu: Mutex::new(menu),
                        wake,
                        pending: Mutex::new(Vec::new()),
                        taskbar_created: 0,
                        active: AtomicBool::new(false),
                    }),
                    status: TrayStatus::Unavailable("CreateWindowExW failed".to_owned()),
                };
            }
            let taskbar_created =
                unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
            let state = Arc::new(State {
                hwnd,
                menu: Mutex::new(menu),
                wake,
                pending: Mutex::new(Vec::new()),
                taskbar_created,
                active: AtomicBool::new(true),
            });
            if let Ok(mut slot) = STATE.lock() {
                *slot = Some(Arc::clone(&state));
            }
            unsafe { add_icon(&state) };
            Self {
                state,
                status: TrayStatus::Active,
            }
        }

        pub fn status(&self) -> TrayStatus {
            self.status.clone()
        }

        pub fn set_menu(&mut self, menu: TrayMenu) {
            if let Ok(mut current) = self.state.menu.lock() {
                *current = menu;
            }
        }

        pub fn poll(&self) -> Vec<TrayEvent> {
            self.state
                .pending
                .lock()
                .map(|mut pending| std::mem::take(&mut *pending))
                .unwrap_or_default()
        }

        pub fn snapshot(&self) -> TraySnapshot {
            TraySnapshot {
                status: self.status.clone(),
                menu: self
                    .state
                    .menu
                    .lock()
                    .map(|menu| menu.clone())
                    .unwrap_or_else(|_| TrayMenu::new("Kasumi")),
            }
        }

        pub fn is_active(&self) -> bool {
            self.state.active.load(Ordering::Relaxed)
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            let hwnd = self.state.hwnd;
            if !hwnd.is_null() {
                unsafe {
                    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
                    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                    data.hWnd = hwnd;
                    data.uID = 1;
                    Shell_NotifyIconW(NIM_DELETE, &data);
                    DestroyWindow(hwnd);
                }
            }
            self.state.active.store(false, Ordering::Relaxed);
            if let Ok(mut slot) = STATE.lock() {
                *slot = None;
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn command_ids_are_stable_and_skip_separators() {
            let menu = TrayMenu::new("t")
                .item(TrayMenuItem::new("a", "A"))
                .separator()
                .item(TrayMenuItem::new("b", "B").checkbox(true));
            let ids = command_ids(&menu);
            assert_eq!(ids.len(), 2);
            assert_eq!(ids[0].1, "a");
            assert_eq!(ids[1].1, "b");
            assert_eq!(id_of_command(&menu, ids[1].0).as_deref(), Some("b"));
            assert_eq!(id_of_command(&menu, 99), None);
        }

        #[test]
        fn nested_items_are_flattened_for_offsets() {
            let menu = TrayMenu::new("t")
                .item(
                    TrayMenuItem::new("group", "Group")
                        .child(TrayMenuItem::new("x", "X"))
                        .child(TrayMenuItem::new("y", "Y")),
                )
                .item(TrayMenuItem::new("after", "After"));
            let leaves: Vec<String> = command_ids(&menu)
                .into_iter()
                .map(|(_, id, _)| id)
                .collect();
            assert_eq!(leaves, vec!["x", "y", "after"]);
        }
    }
}

#[cfg(target_os = "macos")]
pub mod platform {
    //! `NSStatusBar`/`NSStatusItem` and `NSMenu` through `objc2`.
    //!
    //! AppKit is main-thread only, so the status item is created during
    //! `resumed` on the winit thread. Every menu item targets a small `NSObject`
    //! subclass that holds a Rust closure and posts its activation into the
    //! tray's queue.

    use std::cell::RefCell;
    use std::sync::{Arc, Mutex};

    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{DefinedClass, define_class, msg_send, sel};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSControlStateValueOff,
        NSControlStateValueOn, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
        NSVariableStatusItemLength,
    };
    use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSString};

    use super::{
        TrayEntry, TrayEvent, TrayMenu, TrayMenuItem, TraySnapshot, TrayStatus, WakeCallback,
    };

    struct Activation {
        wake: Option<WakeCallback>,
        pending: Arc<Mutex<Vec<TrayEvent>>>,
        id: String,
    }

    struct HandlerIvars {
        activation: RefCell<Option<Arc<Activation>>>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "KasumiTrayHandler"]
        #[ivars = HandlerIvars]
        struct Handler;

        impl Handler {
            #[unsafe(method(activate:))]
            fn activate(&self, _sender: Option<&AnyObject>) {
                if let Some(activation) = self.ivars().activation.borrow().as_ref() {
                    if let Ok(mut pending) = activation.pending.lock() {
                        pending.push(TrayEvent::Activated {
                            id: activation.id.clone(),
                        });
                    }
                    if let Some(wake) = &activation.wake {
                        wake();
                    }
                }
            }
        }

        unsafe impl NSObjectProtocol for Handler {}
    );

    impl Handler {
        fn new(mtm: MainThreadMarker, activation: Arc<Activation>) -> Retained<Self> {
            let this = mtm.alloc::<Self>();
            let this = this.set_ivars(HandlerIvars {
                activation: RefCell::new(Some(activation)),
            });
            unsafe { msg_send![super(this), init] }
        }
    }

    /// A running macOS tray.
    ///
    /// Keep it alive for the lifetime of the app: dropping it removes the
    /// status item and every handler the menu targets.
    pub struct Tray {
        status_item: Option<Retained<NSStatusItem>>,
        handlers: Vec<Retained<Handler>>,
        menu: Arc<Mutex<TrayMenu>>,
        pending: Arc<Mutex<Vec<TrayEvent>>>,
        status: TrayStatus,
    }

    impl Tray {
        pub fn new(menu: TrayMenu, wake: Option<WakeCallback>) -> Self {
            let menu_state = Arc::new(Mutex::new(menu));
            let pending = Arc::new(Mutex::new(Vec::new()));
            let Some(mtm) = MainThreadMarker::new() else {
                return Self {
                    status_item: None,
                    handlers: Vec::new(),
                    menu: menu_state,
                    pending,
                    status: TrayStatus::Unavailable(
                        "the tray must be created on the main thread".to_owned(),
                    ),
                };
            };
            let status_item =
                NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
            if let Some(button) = status_item.button(mtm) {
                // A one-letter title avoids shipping an icon and still leaves a
                // visible target; `setImage` can replace it later.
                button.setTitle(&NSString::from_str("K"));
            }
            let mut handlers = Vec::new();
            let root = build_menu(mtm, &menu_state, &pending, wake, &mut handlers);
            status_item.setMenu(Some(&root));
            Self {
                status_item: Some(status_item),
                handlers,
                menu: menu_state,
                pending,
                status: TrayStatus::Active,
            }
        }

        pub fn status(&self) -> TrayStatus {
            self.status.clone()
        }

        pub fn set_menu(&mut self, menu: TrayMenu) {
            if let Ok(mut current) = self.menu.lock() {
                *current = menu;
            }
            let (Some(mtm), Some(status_item)) = (MainThreadMarker::new(), &self.status_item)
            else {
                return;
            };
            // Rebuilding is the only reliable way to refresh item states, and
            // it is cheap at tray-menu sizes.
            let mut handlers = Vec::new();
            let root = build_menu(mtm, &self.menu, &self.pending, None, &mut handlers);
            status_item.setMenu(Some(&root));
            self.handlers = handlers;
        }

        pub fn poll(&self) -> Vec<TrayEvent> {
            self.pending
                .lock()
                .map(|mut pending| std::mem::take(&mut *pending))
                .unwrap_or_default()
        }

        pub fn snapshot(&self) -> TraySnapshot {
            TraySnapshot {
                status: self.status.clone(),
                menu: self
                    .menu
                    .lock()
                    .map(|menu| menu.clone())
                    .unwrap_or_else(|_| TrayMenu::new("Kasumi")),
            }
        }

        pub fn is_active(&self) -> bool {
            self.status_item.is_some()
        }
    }

    fn build_menu(
        mtm: MainThreadMarker,
        menu_state: &Arc<Mutex<TrayMenu>>,
        pending: &Arc<Mutex<Vec<TrayEvent>>>,
        wake: Option<WakeCallback>,
        handlers: &mut Vec<Retained<Handler>>,
    ) -> Retained<NSMenu> {
        let menu = NSMenu::new(mtm);
        // Without this, items whose target does not implement validation are
        // silently greyed out.
        menu.setAutoenablesItems(false);
        let Ok(state) = menu_state.lock().map(|menu| menu.clone()) else {
            return menu;
        };
        for entry in state.entries() {
            match entry {
                TrayEntry::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
                TrayEntry::Item(item) if item.has_children() => {
                    let submenu = NSMenu::new(mtm);
                    submenu.setAutoenablesItems(false);
                    for child in item.children_ref() {
                        add_item(mtm, &submenu, child, pending, wake.clone(), handlers);
                    }
                    let parent = unsafe {
                        NSMenuItem::initWithTitle_action_keyEquivalent(
                            mtm.alloc::<NSMenuItem>(),
                            &NSString::from_str(item.label()),
                            None,
                            &NSString::from_str(""),
                        )
                    };
                    parent.setSubmenu(Some(&submenu));
                    menu.addItem(&parent);
                }
                TrayEntry::Item(item) => {
                    add_item(mtm, &menu, item, pending, wake.clone(), handlers);
                }
            }
        }
        menu
    }

    fn add_item(
        mtm: MainThreadMarker,
        menu: &NSMenu,
        item: &TrayMenuItem,
        pending: &Arc<Mutex<Vec<TrayEvent>>>,
        wake: Option<WakeCallback>,
        handlers: &mut Vec<Retained<Handler>>,
    ) {
        let entry = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc::<NSMenuItem>(),
                &NSString::from_str(item.label()),
                None,
                &NSString::from_str(""),
            )
        };
        entry.setEnabled(item.is_enabled());
        if let Some(checked) = item.checked() {
            entry.setState(if checked {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        let activation = Arc::new(Activation {
            wake,
            pending: Arc::clone(pending),
            id: item.id().to_owned(),
        });
        let handler = Handler::new(mtm, activation);
        unsafe {
            entry.setTarget(Some(&handler));
            entry.setAction(Some(sel!(activate:)));
        }
        menu.addItem(&entry);
        // `setTarget` is an unretained reference: holding the handlers here is
        // what keeps them alive for the menu's lifetime.
        handlers.push(handler);
    }

    /// Switches the process to the accessory activation policy, which is what a
    /// menu-bar-only app needs. `setActivationPolicy` is a no-op once the app is
    /// running, so call it as early as possible.
    pub fn hide_dock_icon() {
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub mod platform {
    //! Fallback for platforms without a tray backend.

    use super::{TrayEvent, TrayMenu, TraySnapshot, TrayStatus, WakeCallback};

    pub struct Tray {
        menu: TrayMenu,
        status: TrayStatus,
        pending: Vec<TrayEvent>,
    }

    impl Tray {
        pub fn new(menu: TrayMenu, _wake: Option<WakeCallback>) -> Self {
            Self {
                menu,
                status: TrayStatus::Disabled,
                pending: vec![TrayEvent::Unavailable {
                    reason: format!(
                        "the system tray is not implemented for {}",
                        std::env::consts::OS
                    ),
                }],
            }
        }

        pub fn status(&self) -> TrayStatus {
            self.status.clone()
        }

        pub fn set_menu(&mut self, menu: TrayMenu) {
            self.menu = menu;
        }

        pub fn poll(&mut self) -> Vec<TrayEvent> {
            std::mem::take(&mut self.pending)
        }

        pub fn snapshot(&self) -> TraySnapshot {
            TraySnapshot {
                status: self.status.clone(),
                menu: self.menu.clone(),
            }
        }

        pub fn is_active(&self) -> bool {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> TrayMenu {
        TrayMenu::new("Kasumi")
            .item(
                TrayMenuItem::new("show", "Show Kasumi").child(TrayMenuItem::new("about", "About")),
            )
            .item(TrayMenuItem::new("dark", "Dark theme").checkbox(true))
            .separator()
            .item(TrayMenuItem::new("quit", "Quit"))
            .on_activate("show")
    }

    #[test]
    fn ids_cover_nested_items() {
        let menu = menu();
        assert_eq!(menu.ids(), vec!["show", "about", "dark", "quit"]);
        assert!(menu.find("about").is_some());
        assert!(menu.find("missing").is_none());
    }

    #[test]
    fn setting_a_checkbox_only_touches_checkable_items() {
        let mut menu = menu();
        menu.set_checked("dark", false);
        assert_eq!(menu.find("dark").expect("dark").checked(), Some(false));
        menu.set_checked("quit", true);
        assert_eq!(menu.find("quit").expect("quit").checked(), None);
    }

    #[test]
    fn separators_are_not_items() {
        let menu = menu();
        let separators = menu
            .entries()
            .iter()
            .filter(|entry| matches!(entry, TrayEntry::Separator))
            .count();
        assert_eq!(separators, 1);
    }

    #[test]
    fn a_snapshot_reports_its_status() {
        let snapshot = TraySnapshot {
            status: TrayStatus::Active,
            menu: menu(),
        };
        assert!(snapshot.is_active());
        let disabled = TraySnapshot {
            status: TrayStatus::Disabled,
            menu: menu(),
        };
        assert!(!disabled.is_active());
    }
}
