//! Platform-independent tray types.
//!
//! The application describes a tray menu and receives activations as events;
//! the backend owns the platform implementation. Keeping the types here means
//! an application never depends on `ksni`, Win32 or AppKit.

/// One tray menu entry.
///
/// Use [`TrayMenuItem::new`] for a command, [`TrayMenuItem::checkbox`] for a
/// checkable command, and [`TrayMenuItem::child`] to nest entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) enabled: bool,
    pub(crate) checked: Option<bool>,
    pub(crate) children: Vec<TrayMenuItem>,
}

impl TrayMenuItem {
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

    /// Makes the entry a checkable command. `true` starts it checked.
    pub fn checkbox(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn child(mut self, child: TrayMenuItem) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = TrayMenuItem>) -> Self {
        self.children.extend(children);
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn checked(&self) -> Option<bool> {
        self.checked
    }

    pub fn children_ref(&self) -> &[TrayMenuItem] {
        &self.children
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

    /// Finds an entry by id, including nested entries.
    pub fn find(&self, id: &str) -> Option<&TrayMenuItem> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }

    /// Flips a checkbox entry. Plain commands and separators are untouched.
    pub fn set_checked(&mut self, checked: bool) {
        if self.checked.is_some() {
            self.checked = Some(checked);
        }
        for child in &mut self.children {
            child.set_checked(checked);
        }
    }
}

/// A tray menu entry at the top level: an item or a separator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEntry {
    Item(TrayMenuItem),
    Separator,
}

/// The tray's icon tooltip and menu.
///
/// ```ignore
/// let menu = TrayMenu::new("Kasumi")
///     .item(TrayMenuItem::new("show", "Open Kasumi"))
///     .separator()
///     .item(TrayMenuItem::new("dark", "Dark theme").checkbox(true))
///     .separator()
///     .item(TrayMenuItem::new("quit", "Quit"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenu {
    pub(crate) title: String,
    pub(crate) entries: Vec<TrayEntry>,
    pub(crate) on_activate: Option<String>,
}

impl TrayMenu {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            entries: Vec::new(),
            on_activate: None,
        }
    }

    pub fn item(mut self, item: TrayMenuItem) -> Self {
        self.entries.push(TrayEntry::Item(item));
        self
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(TrayEntry::Separator);
        self
    }

    /// The id reported when the tray icon itself is activated. Without it a
    /// click on the icon does nothing on platforms that support activation.
    pub fn on_activate(mut self, id: impl Into<String>) -> Self {
        self.on_activate = Some(id.into());
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn entries(&self) -> &[TrayEntry] {
        &self.entries
    }

    pub fn on_activate_id(&self) -> Option<&str> {
        self.on_activate.as_deref()
    }

    pub fn find(&self, id: &str) -> Option<&TrayMenuItem> {
        self.entries.iter().find_map(|entry| match entry {
            TrayEntry::Item(item) => item.find(id),
            TrayEntry::Separator => None,
        })
    }

    pub fn ids(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for entry in &self.entries {
            if let TrayEntry::Item(item) = entry {
                out.extend(item.ids());
            }
        }
        out
    }

    /// Sets the checked bit of every checkable entry with `id`.
    pub fn set_checked(&mut self, id: &str, checked: bool) {
        for entry in &mut self.entries {
            if let TrayEntry::Item(item) = entry
                && let Some(found) = find_mut(item, id)
            {
                found.set_checked(checked);
            }
        }
    }

    /// Rebuilds the menu so every checkbox entry with `id` shows `checked`.
    pub fn with_checked(&self, id: &str, checked: bool) -> Self {
        let mut next = self.clone();
        next.set_checked(id, checked);
        next
    }
}

fn find_mut<'a>(item: &'a mut TrayMenuItem, id: &str) -> Option<&'a mut TrayMenuItem> {
    if item.id == id {
        return Some(item);
    }
    item.children
        .iter_mut()
        .find_map(|child| find_mut(child, id))
}

/// Something the user did with the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// An entry was activated, or the icon itself when its id matches
    /// [`TrayMenu::on_activate`].
    Activated { id: String },
    /// The tray could not be registered.
    Unavailable { reason: String },
}

/// Whether the tray is currently registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayStatus {
    /// Registered and visible.
    Active,
    /// Not registered; `reason` says why.
    Unavailable(String),
    /// No tray was requested, or the platform has no implementation.
    Disabled,
}

impl TrayStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

impl Default for TrayStatus {
    fn default() -> Self {
        Self::Disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> TrayMenu {
        TrayMenu::new("Kasumi")
            .item(TrayMenuItem::new("show", "Open"))
            .item(
                TrayMenuItem::new("theme", "Theme")
                    .child(TrayMenuItem::new("light", "Light"))
                    .child(TrayMenuItem::new("dark", "Dark").checkbox(true)),
            )
            .separator()
            .item(TrayMenuItem::new("quit", "Quit"))
            .on_activate("show")
    }

    #[test]
    fn ids_cover_nested_entries() {
        let menu = menu();
        assert_eq!(menu.ids(), vec!["show", "theme", "light", "dark", "quit"]);
        assert!(menu.find("dark").is_some());
        assert!(menu.find("nope").is_none());
    }

    #[test]
    fn checked_only_affects_checkboxes() {
        let mut menu = menu();
        assert_eq!(menu.find("show").expect("show").checked(), None);
        menu.set_checked("dark", true);
        assert_eq!(menu.find("dark").expect("dark").checked(), Some(true));
        menu.set_checked("show", true);
        assert_eq!(menu.find("show").expect("show").checked(), None);
    }

    #[test]
    fn separators_are_not_entries() {
        let menu = menu();
        let separators = menu
            .entries()
            .iter()
            .filter(|entry| matches!(entry, TrayEntry::Separator))
            .count();
        assert_eq!(separators, 1);
    }

    #[test]
    fn with_checked_returns_a_copy() {
        let menu = menu();
        let checked = menu.with_checked("dark", false);
        assert_eq!(menu.find("dark").expect("dark").checked(), Some(true));
        assert_eq!(checked.find("dark").expect("dark").checked(), Some(false));
    }
}
