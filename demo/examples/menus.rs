//! `menus` — popup menus, arbitrarily nested submenus, context menus, tooltips.
//!
//! ```sh
//! cargo run -p kasumi-demo --example menus
//! ```
//!
//! A `Menu` is plain data: labels, shortcuts, check marks and child menus. It
//! carries no callbacks, so the same value can be rebuilt every frame and
//! compared cheaply. Activating an item returns its index path through the menu
//! tree; you map that path to your own command.
//!
//! Menus paint through a root overlay with a full-window scrim, so a click
//! outside dismisses the menu instead of reaching the page. That is also why
//! the page below never has to know a menu is open.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// What the last menu selection did, shown at the bottom of the page.
#[derive(Clone, Default)]
struct Log {
    command: String,
    path: String,
}

struct Menus;

impl App for Menus {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let log = ui.state(Log::default);
        let auto_save = ui.state(|| true);
        let grid = ui.state(|| false);
        let zoom = ui.state(|| 1usize);

        shared::page(
            ui,
            "Menus",
            "MenuButton · nested submenus · context menu · tooltip",
            "menus",
            |ui| {
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    // -------------------------------------------------------
                    // Menu button
                    // -------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Menu button", "MenuButton");
                                note(
                                    ui,
                                    "Click the button, then hover a parent row for 400 ms — or \
                                     press → — to cascade. ← closes one level; Esc closes the \
                                     whole menu.",
                                );

                                let menu = Menu::new()
                                    .item(
                                        MenuItem::new("New file")
                                            .shortcut("Ctrl+N")
                                            .checked(grid.get()),
                                    )
                                    .item(MenuItem::new("Open…").shortcut("Ctrl+O"))
                                    .separator()
                                    .item(
                                        MenuItem::new("Auto save")
                                            .shortcut("Ctrl+S")
                                            .checked(auto_save.get()),
                                    )
                                    .item(MenuItem::new("Disabled command").enabled(false))
                                    .separator()
                                    .submenu(
                                        "Export",
                                        Menu::new()
                                            .item(MenuItem::new("PNG").shortcut("Ctrl+E"))
                                            .item(MenuItem::new("SVG"))
                                            .submenu(
                                                "Slides",
                                                Menu::new()
                                                    .item(MenuItem::new("16:9"))
                                                    .item(MenuItem::new("4:3"))
                                                    .submenu(
                                                        "Custom",
                                                        Menu::new()
                                                            .item(MenuItem::new("1920×1080"))
                                                            .item(MenuItem::new("3840×2160")),
                                                    ),
                                            ),
                                    )
                                    .submenu(
                                        "View",
                                        Menu::new()
                                            .item(MenuItem::new("Show grid").checked(grid.get()))
                                            .item(MenuItem::new("Zoom in").shortcut("Ctrl++"))
                                            .item(MenuItem::new("Zoom out").shortcut("Ctrl+−")),
                                    )
                                    .separator()
                                    .item(MenuItem::new("Quit").shortcut("Ctrl+Q"));

                                let output =
                                    ui.add(MenuButton::new("Open menu", menu).full_width());
                                if let Some(selection) = output.selection {
                                    let mut next = Log {
                                        command: selection.label.clone(),
                                        path: selection
                                            .path
                                            .iter()
                                            .map(usize::to_string)
                                            .collect::<Vec<_>>()
                                            .join(" › "),
                                    };
                                    // Map the index path to a command. Deep paths are only
                                    // reported; shallow ones toggle real state.
                                    match selection.path.as_slice() {
                                        [0] => grid.update(|value| *value = !*value),
                                        [3] => auto_save.update(|value| *value = !*value),
                                        [6, 2, 0] => zoom.set(1),
                                        [6, 2, 1] => zoom.set(0),
                                        _ => {}
                                    }
                                    next.command = format!(
                                        "{} (auto save = {}, grid = {}, zoom = {})",
                                        next.command,
                                        auto_save.get(),
                                        grid.get(),
                                        zoom.get()
                                    );
                                    log.set(next);
                                }

                                ui.row(|ui| {
                                    ui.width_full().gap(10.0).align_items(Align::Center);
                                    micro(ui, "toggle state");
                                    ui.add(
                                        Label::new(format!(
                                            "auto save {} · grid {}",
                                            if auto_save.get() { "on" } else { "off" },
                                            if grid.get() { "on" } else { "off" }
                                        ))
                                        .font_size(11.0)
                                        .color(theme.colors.on_surface_variant)
                                        .family(FontFamily::Monospace),
                                    );
                                });
                            },
                        );
                    });

                    // -------------------------------------------------------
                    // Context menu
                    // -------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Context menu", "ui.context_menu");
                                note(
                                    ui,
                                    "Right-click the panel below. The menu opens at the pointer \
                                     rather than at the widget's edge, and flips to stay \
                                     inside the window.",
                                );

                                let target = ui.column(|ui| {
                                    ui.width_full()
                                        .height(150.0)
                                        .align_items(Align::Center)
                                        .justify(Justify::Center)
                                        .gap(6.0)
                                        .radius(theme.shape.medium)
                                        .border(1.0, theme.colors.outline_variant)
                                        .background(theme.colors.surface_container_low);
                                    ui.clickable();
                                    ui.add(
                                        Label::new("Right-click anywhere here")
                                            .font_size(14.0)
                                            .color(theme.colors.on_surface),
                                    );
                                    micro(ui, "hovered / pressed / focused are real Responses");
                                });

                                if target.hovered() {
                                    ui.cursor(kasumi::core::CursorIcon::Crosshair);
                                }

                                let context = Menu::new()
                                    .item(MenuItem::new("Cut").shortcut("Ctrl+X"))
                                    .item(MenuItem::new("Copy").shortcut("Ctrl+C"))
                                    .item(MenuItem::new("Paste").shortcut("Ctrl+V"))
                                    .separator()
                                    .item(MenuItem::new("Show grid").checked(grid.get()))
                                    .submenu(
                                        "Arrange",
                                        Menu::new()
                                            .item(MenuItem::new("Align left"))
                                            .item(MenuItem::new("Align center"))
                                            .item(MenuItem::new("Align right"))
                                            .separator()
                                            .submenu(
                                                "Distribute",
                                                Menu::new()
                                                    .item(MenuItem::new("Horizontally"))
                                                    .item(MenuItem::new("Vertically")),
                                            ),
                                    )
                                    .separator()
                                    .item(MenuItem::new("Delete").shortcut("Del"));

                                if let Some(selection) = ui.context_menu(&target, context) {
                                    if selection.path.as_slice() == [4] {
                                        grid.update(|value| *value = !*value);
                                    }
                                    log.set(Log {
                                        command: selection.label,
                                        path: selection
                                            .path
                                            .iter()
                                            .map(usize::to_string)
                                            .collect::<Vec<_>>()
                                            .join(" › "),
                                    });
                                }

                                code(
                                    ui,
                                    match &log.get().command {
                                        command if command.is_empty() => {
                                            "no command yet".to_owned()
                                        }
                                        command => {
                                            format!("{}  ·  path [{}]", command, log.get().path)
                                        }
                                    },
                                );
                            },
                        );
                    });
                });

                // -----------------------------------------------------------
                // Tooltips
                // -----------------------------------------------------------
                ui.card(
                    Card::outlined()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Tooltips", "ui.tooltip");
                        note(
                            ui,
                            "A tooltip is a root overlay positioned near the pointer. It has no \
                             delay of its own: it shows for as long as the owner reports \
                             `hovered()`, which is one frame behind reality like every other \
                             interaction.",
                        );
                        ui.wrap_row(|ui| {
                            ui.width_full().gap(8.0);
                            for (label, tip) in [
                                ("Cut", "Ctrl+X · move to clipboard"),
                                ("Copy", "Ctrl+C · duplicate to clipboard"),
                                ("Paste", "Ctrl+V · insert clipboard text"),
                                ("Undo", "Ctrl+Z · step back one edit"),
                                ("Redo", "Ctrl+Shift+Z · step forward"),
                            ] {
                                let response = ui.add(Button::new(label).outlined());
                                ui.tooltip(&response, tip);
                            }
                        });
                        ui.divider();
                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            let info =
                                ui.add(Button::new("i").text().font_size(15.0).min_width(40.0));
                            ui.tooltip(
                                &info,
                                "Only one overlay is declared per hovered owner, so several \
                                 tooltips cannot stack.",
                            );
                            ui.add(
                                Label::new("Hover the icon and the buttons above.")
                                    .font_size(11.0)
                                    .color(theme.colors.on_surface_variant),
                            );
                        });
                    },
                );

                // -----------------------------------------------------------
                // Path reference
                // -----------------------------------------------------------
                ui.card(
                    Card::filled()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Selection paths", "MenuSelection::path");
                        note(
                            ui,
                            "The path is the index of the activated row at each level, skipping \
                             nothing — separators and disabled rows are not selectable but still \
                             occupy an index. That makes a `match` on the path stable even when \
                             labels change.",
                        );
                        ui.grid(2, |ui| {
                            ui.width_full().gap(12.0);
                            for (source, paths) in [
                                (
                                    "MenuButton",
                                    vec![
                                        ("[0]", "New file"),
                                        ("[2]", "Auto save"),
                                        ("[6, 0]", "PNG"),
                                        ("[6, 2, 2, 1]", "3840×2160"),
                                    ],
                                ),
                                (
                                    "Context menu",
                                    vec![
                                        ("[0]", "Cut"),
                                        ("[4]", "Show grid"),
                                        ("[5, 3, 0]", "Distribute › Horizontally"),
                                        ("[7]", "Delete"),
                                    ],
                                ),
                            ] {
                                ui.column(|ui| {
                                    ui.gap(6.0);
                                    micro(ui, source);
                                    for (path, label) in paths {
                                        ui.row(|ui| {
                                            ui.width_full().gap(10.0);
                                            ui.column(|ui| {
                                                ui.width(96.0).flex_shrink(0.0);
                                                code(ui, path);
                                            });
                                            ui.add(
                                                Label::new(label)
                                                    .font_size(11.0)
                                                    .color(theme.colors.on_surface_variant),
                                            );
                                        });
                                    }
                                });
                            }
                        });
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Menus },
        options("Kasumi — menus", 1120.0, 880.0),
    )
}
