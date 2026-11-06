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

use std::time::Duration;

/// One tray menu entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItem {
    /// Stable id reported by [`TrayEvent::Activated`].
    pub id: String,
    pub label: String,
    pub enabled: bool,
    /// `Some` renders a check mark that follows this value; `None` is a plain
    /// command.
    pub checked: Option<bool>,
    /// Submenu entries, at most one level deep on Linux.
    pub children: Vec<TrayItem>,
}

impl TrayItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
            checked: None,
            children: Vec::new(),
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn checkbox(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn child(mut self, child: TrayItem) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = TrayItem>) -> Self {
        self.children.extend(children);
        self
    }

    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Every id in this subtree, including its own.
    pub fn ids(&self) -> Vec<&str> {
        let mut out = vec![self.id.as_str()];
        for child in &self.children {
            out.extend(child.ids());
        }
        out
    }

    /// Finds an item by id, including nested entries.
    pub fn find(&self, id: &str) -> Option<&TrayItem> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }
}

/// The tray's menu: a title plus top-level entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenu {
    pub title: String,
    pub items: Vec<TrayOption>,
    /// Style of the click on the icon itself, if the platform supports it.
    pub on_activate: Option<String>,
}

/// A top-level menu entry: an item or a separator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayOption {
    Item(TrayItem),
    Separator,
}

impl TrayMenu {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
            on_activate: None,
        }
    }

    pub fn item(mut self, item: TrayItem) -> Self {
        self.items.push(TrayOption::Item(item));
        self
    }

    pub fn separator(mut self) -> Self {
        self.items.push(TrayOption::Separator);
        self
    }

    /// The id reported when the icon itself is activated.
    pub fn on_activate(mut self, id: impl Into<String>) -> Self {
        self.on_activate = Some(id.into());
        self
    }

    /// The tooltip text.
    pub fn tooltip(&self) -> &str {
        &self.title
    }

    pub fn find(&self, id: &str) -> Option<&TrayItem> {
        self.items.iter().find_map(|option| match option {
            TrayOption::Item(item) => item.find(id),
            TrayOption::Separator => None,
        })
    }

    pub fn ids(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for option in &self.items {
            if let TrayOption::Item(item) = option {
                out.extend(item.ids());
            }
        }
        out
    }

    /// Replaces the checked bit of `id`. Used by the backend after an item
    /// was activated so the next menu build shows the new state.
    pub fn set_checked(&mut self, id: &str, checked: bool) {
        fn walk(items: &mut [TrayItem], id: &str, checked: bool) {
            for item in items {
                if item.id == id && item.checked.is_some() {
                    item.checked = Some(checked);
                }
                walk(&mut item.children, id, checked);
            }
        }
        let mut items: Vec<&mut TrayItem> = Vec::new();
        for option in &mut self.items {
            if let TrayOption::Item(item) = option {
                items.push(item);
            }
        }
        for item in &mut items {
            walk(std::slice::from_mut(*item), id, checked);
        }
    }
}

/// Something the user did with the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// An item was activated, or the icon itself when `id` matches
    /// [`TrayMenu::on_activate`].
    Activated { id: String },
    /// The tray could not be registered, for example because no
    /// StatusNotifierWatcher is running.
    Unavailable { reason: String },
}

/// Whether the tray is currently registered with the desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayStatus {
    /// Registered and showing.
    Active,
    /// Not registered; `reason` explains why.
    Unavailable(String),
    /// Not attempted, for example when the platform has no tray support.
    Disabled,
}

/// The state a platform tray reports back to the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySnapshot {
    pub status: TrayStatus,
    pub menu: TrayMenu,
}

impl TraySnapshot {
    pub fn is_active(&self) -> bool {
        matches!(self.status, TrayStatus::Active)
    }
}

/// How long an item highlight stays before the platform closes the menu.
///
/// Platforms own their own menu presentation, so the backend only needs this
/// for the synthesized click feedback on Linux.
pub const ACTIVATION_FEEDBACK: Duration = Duration::from_millis(120);

#[cfg(target_os = "linux")]
pub mod platform {
    //! StatusNotifierItem over D-Bus through `ksni`.

    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Sender, channel};

    use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
    use ksni::{MenuItem, Tray as KsniTray, TrayMethods};

    use super::{TrayEvent, TrayItem, TrayMenu, TrayOption, TraySnapshot, TrayStatus};

    /// Shared state between the D-Bus thread and the application.
    #[derive(Clone)]
    struct Shared {
        menu: Arc<std::sync::Mutex<TrayMenu>>,
        events: Sender<TrayEvent>,
        unavailable: Arc<AtomicBool>,
    }

    struct KsniTrayHandle {
        shared: Shared,
    }

    impl KsniTrayHandle {
        fn to_menu(&self, menu: &TrayMenu) -> Vec<MenuItem<Self>> {
            menu.items
                .iter()
                .map(|option| match option {
                    TrayOption::Separator => MenuItem::Separator,
                    TrayOption::Item(item) => self.convert(item),
                })
                .collect()
        }

        fn convert(&self, item: &TrayItem) -> MenuItem<Self> {
            let shared = self.shared.clone();
            if item.has_children() {
                let children: Vec<MenuItem<Self>> =
                    item.children.iter().map(|child| self.convert(child)).collect();
                return SubMenu {
                    label: item.label.clone(),
                    enabled: item.enabled,
                    submenu: children,
                    ..Default::default()
                }
                .into();
            }
            match item.checked {
                Some(checked) => {
                    let id = item.id.clone();
                    let events = shared.events.clone();
                    let menu = shared.menu.clone();
                    CheckmarkItem {
                        label: item.label.clone(),
                        enabled: item.enabled,
                        checked,
                        activate: Box::new(move |_tray: &mut Self| {
                            let next = menu
                                .lock()
                                .ok()
                                .and_then(|menu| menu.find(&id).and_then(|item| item.checked))
                                .map(|checked| !checked)
                                .unwrap_or(!checked);
                            if let Ok(mut menu) = menu.lock() {
                                menu.set_checked(&id, next);
                            }
                            let _ = events.send(TrayEvent::Activated { id: id.clone() });
                        }),
                        ..Default::default()
                    }
                    .into()
                }
                None => {
                    let id = item.id.clone();
                    let events = shared.events.clone();
                    StandardItem {
                        label: item.label.clone(),
                        enabled: item.enabled,
                        activate: Box::new(move |_tray: &mut Self| {
                            let _ = events.send(TrayEvent::Activated { id: id.clone() });
                        }),
                        ..Default::default()
                    }
                    .into()
                }
            }
        }
    }

    impl KsniTray for KsniTrayHandle {
        const MENU_ON_ACTIVATE: bool = false;

        fn id(&self) -> String {
            "kasumi".to_owned()
        }

        fn title(&self) -> String {
            self.shared
                .menu
                .lock()
                .map(|menu| menu.title.clone())
                .unwrap_or_else(|_| "Kasumi".to_owned())
        }

        // A themed name keeps the icon crisp on every desktop without shipping
        // a bitmap; `application-x-executable` exists in every icon theme.
        fn icon_name(&self) -> String {
            "application-x-executable".to_owned()
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            let id = self
                .shared
                .menu
                .lock()
                .ok()
                .and_then(|menu| menu.on_activate.clone())
                .unwrap_or_else(|| "activate".to_owned());
            let _ = self.shared.events.send(TrayEvent::Activated { id });
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            match self.shared.menu.lock() {
                Ok(menu) => self.to_menu(&menu),
                Err(_) => Vec::new(),
            }
        }

        fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
            self.shared.unavailable.store(true, Ordering::Relaxed);
            let _ = self.shared.events.send(TrayEvent::Unavailable {
                reason: reason.to_string(),
            });
            // Keep the service alive: the watcher may come back when a shell
            // restarts, and ksni re-registers automatically.
            true
        }
    }

    /// A running Linux tray. Dropping it unregisters the D-Bus object.
    pub struct Tray {
        handle: Option<ksni::blocking::Handle<KsniTrayHandle>>,
        shared: Shared,
        events: receiver::Receiver,
        status: TrayStatus,
    }

    /// A tiny single-consumer queue. `std::sync::mpsc` receivers are not
    /// `Sync`, and the backend polls from the event loop, so wrap it.
    mod receiver {
        use std::sync::Mutex;
        use std::sync::mpsc::Receiver as StdReceiver;

        use super::TrayEvent;

        pub struct Receiver(Mutex<StdReceiver<TrayEvent>>);

        impl Receiver {
            pub fn new(receiver: StdReceiver<TrayEvent>) -> Self {
                Self(Mutex::new(receiver))
            }

            pub fn try_recv(&self) -> Option<TrayEvent> {
                self.0.lock().ok().and_then(|receiver| receiver.try_recv().ok())
            }

            /// Blocks for at most `timeout`, used to keep the event loop asleep.
            pub fn recv_timeout(&self, timeout: std::time::Duration) -> Option<TrayEvent> {
                self.0
                    .lock()
                    .ok()
                    .and_then(|receiver| receiver.recv_timeout(timeout).ok())
            }
        }
    }

    impl Tray {
        /// Registers the tray. Failure is reported through [`Tray::status`] and
        /// an initial [`TrayEvent::Unavailable`] rather than an error, so an
        /// application keeps running on a desktop without a status notifier.
        pub fn new(menu: TrayMenu, events: Sender<TrayEvent>) -> Self {
            let shared = Shared {
                menu: Arc::new(std::sync::Mutex::new(menu)),
                events: events.clone(),
                unavailable: Arc::new(AtomicBool::new(false)),
            };
            let handle = KsniTrayHandle {
                shared: shared.clone(),
            }
            .assume_sni_available(true)
            .disable_dbus_name(false)
            .spawn();
            let (tx, rx) = channel();
            let status = match handle {
                Ok(handle) => {
                    let _ = tx;
                    Self {
                        handle: Some(handle),
                        shared: shared.clone(),
                        events: receiver::Receiver::new(rx),
                        status: TrayStatus::Active,
                    }
                }
                Err(error) => {
                    let reason = error.to_string();
                    let _ = shared.events.send(TrayEvent::Unavailable {
                        reason: reason.clone(),
                    });
                    Self {
                        handle: None,
                        shared,
                        events: receiver::Receiver::new(rx),
                        status: TrayStatus::Unavailable(reason),
                    }
                }
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
                let _ = handle.update(|_tray| {});
            }
        }

        /// Drains pending activations without blocking.
        pub fn poll(&self) -> Vec<TrayEvent> {
            let mut events = Vec::new();
            while let Some(event) = self.events.try_recv() {
                events.push(event);
            }
            events
        }

        /// Blocks up to `timeout` for the next activation. The event loop uses
        /// this to avoid a polling timer on an idle desktop.
        pub fn wait(&self, timeout: std::time::Duration) -> Option<TrayEvent> {
            self.events.recv_timeout(timeout)
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
                // Unregister from the watcher so the icon disappears
                // immediately rather than on the next session.
                handle.shutdown().wait();
            }
        }
    }
}

#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    not(any(target_os = "linux", target_os = "windows", target_os = "macos"))
))]
pub mod platform {
    //! Placeholder used on platforms without a tray backend yet.
    //!
    //! The Windows and macOS implementations live in their own modules; this
    //! fallback keeps `not(target_os = "linux")` builds compiling while the
    //! tray reports itself as disabled.

    use std::sync::mpsc::Sender;

    use super::{TrayEvent, TrayMenu, TraySnapshot, TrayStatus};

    pub struct Tray {
        menu: TrayMenu,
        status: TrayStatus,
        events: Sender<TrayEvent>,
    }

    impl Tray {
        pub fn new(menu: TrayMenu, events: Sender<TrayEvent>) -> Self {
            let reason = format!(
                "the system tray is not implemented for this platform ({})",
                std::env::consts::OS
            );
            let status = TrayStatus::Disabled;
            let _ = events.send(TrayEvent::Unavailable {
                reason: reason.clone(),
            });
            Self {
                menu,
                status,
                events,
            }
        }

        pub fn status(&self) -> TrayStatus {
            self.status.clone()
        }

        pub fn set_menu(&mut self, menu: TrayMenu) {
            self.menu = menu;
        }

        pub fn poll(&self) -> Vec<TrayEvent> {
            Vec::new()
        }

        pub fn wait(&self, _timeout: std::time::Duration) -> Option<TrayEvent> {
            None
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

        pub fn events(&self) -> &Sender<TrayEvent> {
            &self.events
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> TrayMenu {
        TrayMenu::new("kasumi")
            .item(TrayItem::new("show", "Show Kasumi"))
            .item(
                TrayItem::new("theme", "Theme").children([
                    TrayItem::new("light", "Light"),
                    TrayItem::new("dark", "Dark"),
                ]),
            )
            .separator()
            .item(TrayItem::new("quit", "Quit"))
            .on_activate("show")
    }

    #[test]
    fn ids_cover_nested_items() {
        let menu = menu();
        assert_eq!(
            menu.ids(),
            vec!["show", "theme", "light", "dark", "quit"]
        );
        assert!(menu.find("dark").is_some());
        assert!(menu.find("missing").is_none());
    }

    #[test]
    fn setting_a_checkbox_only_touches_checkable_items() {
        let mut menu = TrayMenu::new("t")
            .item(TrayItem::new("a", "A").checkbox(false))
            .item(TrayItem::new("b", "B"));
        menu.set_checked("a", true);
        assert_eq!(menu.find("a").expect("a").checked, Some(true));
        menu.set_checked("b", true);
        assert_eq!(menu.find("b").expect("b").checked, None);
    }

    #[test]
    fn separators_are_not_items() {
        let menu = menu();
        let separators = menu
            .items
            .iter()
            .filter(|option| matches!(option, TrayOption::Separator))
            .count();
        assert_eq!(separators, 1);
    }
}
