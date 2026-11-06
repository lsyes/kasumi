//! `tray` — a cross-platform system tray behind one small menu model.
//!
//! ```sh
//! cargo run -p kasumi-demo --example tray
//! ```
//!
//! The tray is registered before the first frame from `WindowOptions::tray`.
//! Linux uses StatusNotifierItem over D-Bus, Windows uses `Shell_NotifyIconW`
//! and macOS uses `NSStatusBar`, but the application only sees a `TrayMenu`.
//!
//! Two details make the integration unusual for an immediate-mode UI:
//!
//! - **The menu is a snapshot.** Activations arrive as `TrayEvent`s once per
//!   frame. When state changes, build a fresh menu and hand it to
//!   `ui.set_tray_menu(..)`; the backend replaces the platform menu after the
//!   frame.
//! - **Events are drained per frame.** `ui.tray_events()` is cleared when the
//!   next frame starts, so a frame sees each activation exactly once. Clone the
//!   slice before mutating state if you also need `&mut Ui`.
//!
//! If the platform has no status notifier, `TrayStatus::Unavailable` explains
//! why and the in-window menu below still works.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// One source of truth: the same ids are used by the platform menu and the
/// in-window fallback, so the two cannot disagree about what a command means.
fn menu(dark: bool, paused: bool, page: &str) -> TrayMenu {
    let pages = [
        ("page-overview", "Overview", "overview"),
        ("page-controls", "Controls", "controls"),
        ("page-lists", "Lists", "lists"),
        ("page-media", "Media", "media"),
    ];
    TrayMenu::new("Kasumi — tray example")
        .item(TrayMenuItem::new("show", "Show Kasumi"))
        .separator()
        .item(
            TrayMenuItem::new("pages", "Open page").children(
                pages
                    .iter()
                    .map(|(id, label, _)| TrayMenuItem::new(*id, *label)),
            ),
        )
        .item(
            TrayMenuItem::new("dark", "Dark theme")
                .checkbox(dark)
                .child(TrayMenuItem::new("dark-extra", "Also use high contrast")),
        )
        .item(TrayMenuItem::new("paused", "Pause counters").checkbox(paused))
        .separator()
        .item(TrayMenuItem::new("current", format!("Current: {page}")).enabled(false))
        .item(TrayMenuItem::new("quit", "Quit"))
        .on_activate("show")
}

struct Tray;

impl App for Tray {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let dark = ui.state(|| false);
        let paused = ui.state(|| false);
        let page = ui.state(|| "overview".to_owned());
        let actions = ui.state(Vec::<String>::new);

        // Drain events once per frame. The slice borrows `ui`, so clone it
        // before writing any state.
        for event in ui.tray_events().to_vec() {
            match event {
                TrayEvent::Activated { id } => {
                    actions.update(|lines| {
                        lines.insert(0, format!("tray → {id}"));
                        lines.truncate(8);
                    });
                    match id.as_str() {
                        "show" => {
                            let key = ui.window().key;
                            ui.focus_window(key);
                        }
                        "dark" => dark.update(|value| *value = !*value),
                        "paused" => paused.update(|value| *value = !*value),
                        "quit" => {
                            let key = ui.window().key;
                            ui.close_window(key);
                        }
                        other => {
                            if let Some(next) = other.strip_prefix("page-") {
                                page.set(next.to_owned());
                            }
                        }
                    }
                }
                TrayEvent::Unavailable { reason } => {
                    actions.update(|lines| lines.insert(0, format!("tray unavailable: {reason}")));
                }
            }
        }

        // Rebuild the platform menu only when the values it reflects changed.
        let menu_signature = ui.state(|| String::new());
        let signature = format!("{}:{}:{}", dark.get(), paused.get(), page.get());
        if menu_signature.get() != signature {
            menu_signature.set(signature);
            ui.set_tray_menu(menu(dark.get(), paused.get(), &page.get()));
        }

        shared::page(
            ui,
            "System tray",
            "TrayMenu · TrayEvent · per-frame menu snapshots",
            "tray",
            |ui| {
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Platform status", "ui.tray_status()");
                                let (label, detail, chip, text) = match ui.tray_status() {
                                    TrayStatus::Active => (
                                        "Active",
                                        "The status notifier is registered and its menu is live.",
                                        theme.colors.success_container,
                                        theme.colors.on_success_container,
                                    ),
                                    TrayStatus::Unavailable(_) => (
                                        "Unavailable",
                                        "The platform could not register a tray; the in-window menu below still works.",
                                        theme.colors.error_container,
                                        theme.colors.on_error_container,
                                    ),
                                    TrayStatus::Disabled => (
                                        "Disabled",
                                        "No tray was requested or this platform has no implementation.",
                                        theme.colors.surface_container_high,
                                        theme.colors.on_surface_variant,
                                    ),
                                };
                                ui.row(|ui| {
                                    ui.padding_xy(10.0, 5.0)
                                        .radius(theme.shape.full)
                                        .background(chip);
                                    ui.add(
                                        Label::new(label)
                                            .font_size(11.0)
                                            .color(text)
                                            .semibold(),
                                    );
                                });
                                note(ui, detail);
                                if let TrayStatus::Unavailable(reason) = ui.tray_status() {
                                    code(ui, reason.clone());
                                }
                                ui.divider();
                                note(
                                    ui,
                                    "On Linux this needs a StatusNotifierItem host (a desktop \
                                     bar with the SNI indicator). Without one the application \
                                     keeps running; only the icon is missing.",
                                );
                            },
                        );
                    });

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::filled()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "In-window equivalent", "same command ids");
                                note(
                                    ui,
                                    "These buttons fire the same commands the tray menu sends. \
                                     That is the easiest way to test the flow on a machine \
                                     without a status notifier host.",
                                );
                                ui.wrap_row(|ui| {
                                    ui.width_full().gap(6.0);
                                    for (label, id) in [
                                        ("Show", "show"),
                                        ("Dark", "dark"),
                                        ("Pause", "paused"),
                                        ("Overview", "page-overview"),
                                        ("Controls", "page-controls"),
                                        ("Lists", "page-lists"),
                                        ("Media", "page-media"),
                                    ] {
                                        if ui
                                            .add(
                                                Button::new(label)
                                                    .font_size(10.5)
                                                    .padding_xy(9.0, 5.0),
                                            )
                                            .clicked()
                                        {
                                            actions.update(|lines| {
                                                lines.insert(
                                                    0,
                                                    format!("button → {id}"),
                                                );
                                                lines.truncate(8);
                                            });
                                            match id {
                                                "dark" => {
                                                    dark.update(|value| *value = !*value)
                                                }
                                                "paused" => {
                                                    paused.update(|value| *value = !*value)
                                                }
                                                other => {
                                                    if let Some(next) =
                                                        other.strip_prefix("page-")
                                                    {
                                                        page.set(next.to_owned());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                });
                                ui.divider();
                                micro(ui, "current page");
                                code(ui, page.get());
                                micro(ui, format!("dark = {} · paused = {}", dark.get(), paused.get()));
                            },
                        );
                    });
                });

                ui.card(
                    Card::outlined()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "The menu model", "TrayMenu / TrayMenuItem");
                        note(
                            ui,
                            "A `TrayMenu` is a plain value: a title, a list of items and \
                             separators, and an optional activation id for the icon itself. \
                             Items can be checkable, disabled, and can have children — there is \
                             no platform-specific code in the application.",
                        );
                        let model = menu(dark.get(), paused.get(), &page.get());
                        ui.grid(2, |ui| {
                            ui.width_full().gap(12.0);
                            ui.column(|ui| {
                                ui.gap(4.0);
                                micro(ui, format!("title: {}", model.title()));
                                micro(ui, format!("on_activate: {:?}", model.on_activate_id()));
                                micro(ui, format!("{} entries", model.entries().len()));
                                micro(ui, format!("{} ids", model.ids().len()));
                            });
                            ui.column(|ui| {
                                ui.gap(3.0);
                                for id in model.ids() {
                                    let checked = model
                                        .find(id)
                                        .and_then(|item| item.checked())
                                        .map(|value| format!(" ✓{value}"))
                                        .unwrap_or_default();
                                    code(ui, format!("{id}{checked}"));
                                }
                            });
                        });
                    },
                );

                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Activation log", "ui.tray_events()");
                        note(
                            ui,
                            "Each activation is seen by exactly one frame. The log is a \
                             `State<Vec<String>>`, which is why it survives the frame boundary.",
                        );
                        let lines = actions.get();
                        if lines.is_empty() {
                            ui.add(
                                Label::new(
                                    "No activations yet — try the tray icon or the buttons above.",
                                )
                                .font_size(12.0)
                                .color(theme.colors.outline),
                            );
                        }
                        for (index, line) in lines.iter().enumerate() {
                            ui.row(|ui| {
                                ui.width_full().gap(8.0).align_items(Align::Center);
                                ui.column(|ui| {
                                    ui.width(24.0).flex_shrink(0.0);
                                    micro(ui, format!("{:02}", index));
                                });
                                ui.add(
                                    Label::new(line)
                                        .font_size(11.5)
                                        .color(theme.colors.on_surface_variant)
                                        .family(FontFamily::Monospace),
                                );
                            });
                        }
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    let mut options = options("Kasumi — system tray", 1080.0, 840.0);
    // The initial snapshot is handed to the backend before the first frame.
    // `view` replaces it whenever the values it reflects change.
    options.tray = Some(menu(false, false, "overview"));
    run(shared::QuitOnEscape { app: Tray }, options)
}
