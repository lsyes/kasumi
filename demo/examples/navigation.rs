//! `navigation` — drawer, rail, collapsible groups and a modal drawer.
//!
//! ```sh
//! cargo run -p kasumi-demo --example navigation
//! ```
//!
//! `NavigationDrawer` is one declarative tree rendered in four presentations:
//!
//! | Variant | Width | Behaviour |
//! |---|---|---|
//! | `Standard` | 280 | permanent column in the layout |
//! | `Rail` | 96 | icons only, for compact windows |
//! | `ExpandedRail` | 220 | icon and label in a narrow column |
//! | `Modal` | 280 | overlay over a scrim you own |
//!
//! The same value drives all four, so a responsive application builds the tree
//! once and picks a variant from the viewport width. `NavResponse::selected`
//! reports the destination that was activated; nested groups expand in place
//! rather than indenting rows into unreadable stubs.

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

fn tree() -> NavigationDrawer {
    NavigationDrawer::new()
        .header(NavHeader::new("kasumi").subtitle("0.2.0 · gallery"))
        .item(NavItem::new("overview", "Overview").icon("◉"))
        .section("Surface", |section| {
            section
                .item(NavItem::new("typography", "Typography").icon("Aa"))
                .item(NavItem::new("layout", "Layout").icon("▤"))
        })
        .section("Components", |section| {
            section
                .item(NavItem::new("form", "Form").icon("✎"))
                .item(NavItem::new("controls", "Controls").icon("⚙"))
                .item(NavItem::new("data", "Data").icon("▦").children([
                    NavItem::new("lists", "Lists"),
                    NavItem::new("tables", "Tables").badge("new"),
                    NavItem::new("virtual", "Virtual scroll"),
                ]))
                .item(NavItem::new("network", "Network").icon("⇅").children([
                    NavItem::new("async", "Async resources"),
                    NavItem::new("tray", "System tray"),
                ]))
                .item(NavItem::new("menus", "Menus").icon("≡"))
                .item(NavItem::new("settings", "Settings").icon("◐"))
        })
        .footer(NavItem::new("about", "About").icon("ⓘ"))
}

/// The page body for a destination key.
fn destination(ui: &mut Ui, key: &str) {
    let theme = ui.theme().clone();
    let (title, detail) = match key {
        "overview" => ("Overview", "The landing page of this example."),
        "typography" => ("Typography", "Type scale, weights and fallback."),
        "layout" => ("Layout", "Flex, grid and stacking."),
        "form" => ("Form", "Inputs bound to State<T>."),
        "controls" => ("Controls", "Buttons, switches, sliders."),
        "lists" => ("Lists", "Material rows and virtualization."),
        "tables" => ("Tables", "Not implemented yet — a placeholder destination."),
        "virtual" => ("Virtual scroll", "10,000 rows, flat node count."),
        "async" => ("Async resources", "Loaders on a worker thread."),
        "tray" => ("System tray", "A StatusNotifierItem menu."),
        "menus" => ("Menus", "Nested submenus and context menus."),
        "settings" => ("Settings", "Theme and preferences."),
        "about" => ("About", "Kasumi is a desktop UI skeleton."),
        _ => ("Unknown", "This destination has no page."),
    };
    ui.card(
        Card::elevated()
            .padding(theme.spacing.lg)
            .gap(theme.spacing.md),
        |ui| {
            heading(ui, title, "NavResponse::selected");
            note(ui, detail);
            note(
                ui,
                "Activate a destination in any of the four drawers. They all render the same \
                 tree and all report through the same `selected` field.",
            );
            ui.row(|ui| {
                ui.width_full().gap(10.0).align_items(Align::Center);
                micro(ui, "current key");
                code(ui, key);
            });
        },
    );
}

struct Navigation;

impl App for Navigation {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let navigation = ui.state(|| tree());
        let selected = ui.state(|| "overview".to_owned());
        let modal_open = ui.state(|| false);
        let variant = ui.state(|| 0usize);

        let current = selected.get();
        let variant_index = variant.get().min(3);
        let variant_value = match variant_index {
            0 => NavVariant::Standard,
            1 => NavVariant::Rail,
            2 => NavVariant::ExpandedRail,
            _ => NavVariant::Modal,
        };

        // A permanent drawer takes part in layout; the modal one is an overlay.
        let modal = variant_value == NavVariant::Modal;

        ui.size_full().background(theme.colors.background);
        ui.row(|ui| {
            ui.size_full().align_items(Align::Stretch);

            if !modal {
                let nav = navigation
                    .get()
                    .variant(variant_value)
                    .selected(current.clone());
                let response = ui.navigation(&nav);
                if let Some(key) = response.selected {
                    selected.set(key);
                }
            } else {
                // The modal drawer needs a column to reserve its space; the
                // drawer itself floats above it.
                ui.column(|ui| {
                    ui.width(if modal_open.get() { 12.0 } else { 12.0 }).height_full();
                });
            }

            ui.column(|ui| {
                ui.grow(1.0).height_full().min_width(0.0);

                // Toolbar
                ui.row(|ui| {
                    ui.width_full()
                        .padding_edges(Edges {
                            left: 24.0,
                            right: 24.0,
                            top: 18.0,
                            bottom: 14.0,
                        })
                        .gap(10.0)
                        .align_items(Align::Center);
                    if ui.add(Button::new("☰ Drawer").subtle()).clicked() {
                        if !modal {
                            variant.set(3);
                        }
                        modal_open.update(|open| *open = !*open);
                    }
                    ui.add(
                        Label::new("Navigation drawer and rail")
                            .style(theme.typography.title_large.clone())
                            .color(theme.colors.on_surface),
                    );
                    ui.spacer();
                    for (index, label) in
                        ["Standard", "Rail", "Expanded", "Modal"].iter().enumerate()
                    {
                        let active = variant_index == index;
                        let mut button = Button::new(*label).font_size(10.5).padding_xy(10.0, 5.0);
                        button = if active { button.tonal() } else { button.subtle() };
                        if ui.add(button).clicked() {
                            variant.set(index);
                            if index == 3 {
                                modal_open.set(true);
                            }
                        }
                    }
                });
                ui.divider();

                ui.scroll(ScrollView::vertical().grow(1.0), |ui| {
                    ui.width_full()
                        .padding_edges(Edges {
                            left: 24.0,
                            right: 24.0,
                            top: 18.0,
                            bottom: 24.0,
                        })
                        .gap(16.0);
                    ui.keyed(&current, |ui| destination(ui, &current));

                    ui.card(
                        Card::outlined()
                            .padding(theme.spacing.lg)
                            .gap(theme.spacing.md),
                        |ui| {
                            heading(ui, "How the variants differ", "NavVariant");
                            note(
                                ui,
                                "Width and background are the only visual differences. The rows \
                                 keep the same 56 dp metrics and the same active indicator, which \
                                 is what makes switching at a breakpoint feel continuous.",
                            );
                            ui.grid(4, |ui| {
                                ui.width_full().gap(10.0);
                                for (label, width, detail) in [
                                    ("Standard", 280.0f32, "permanent column"),
                                    ("Rail", 96.0, "icons only"),
                                    ("Expanded", 220.0, "icon + label"),
                                    ("Modal", 280.0, "over a scrim"),
                                ] {
                                    ui.column(|ui| {
                                        ui.padding(12.0)
                                            .gap(4.0)
                                            .background(theme.colors.surface_container_low)
                                            .radius(theme.shape.small);
                                        code(ui, label);
                                        ui.add(
                                            Label::new(format!("{width:.0} dp"))
                                                .font_size(15.0)
                                                .color(theme.colors.on_surface)
                                                .semibold(),
                                        );
                                        micro(ui, detail);
                                    });
                                }
                            });
                        },
                    );

                    ui.card(
                        Card::filled()
                            .padding(theme.spacing.lg)
                            .gap(theme.spacing.md),
                        |ui| {
                            heading(ui, "Collapsible groups", "NavItem::children");
                            note(
                                ui,
                                "Hovering a parent row expands nothing by itself: click the chevron \
                                 or the row. Selecting a child of a collapsed group expands its \
                                 ancestors on the first frame so the active destination is never \
                                 hidden.",
                            );
                            note(
                                ui,
                                "Try selecting `Tables` or `Tray` while the rail is showing: the \
                                 group expands before the frame that paints the indicator.",
                            );
                        },
                    );
                });
            });
        });

        // Modal drawer over a scrim, declared last so it paints above the page.
        if modal && modal_open.get() {
            let viewport = ui.viewport();
            ui.stack(|ui| {
                ui.absolute(0.0, 0.0)
                    .width(viewport.width)
                    .height(viewport.height);
                let scrim = ui.column(|ui| {
                    ui.size_full()
                        .background(theme.colors.scrim.with_alpha(0.32));
                    ui.clickable();
                });
                if scrim.clicked() {
                    modal_open.set(false);
                }
                let nav = navigation
                    .get()
                    .variant(NavVariant::Modal)
                    .selected(current.clone());
                let response = ui.navigation(&nav);
                if let Some(key) = response.selected {
                    selected.set(key);
                    modal_open.set(false);
                }
                if ui.key_pressed(&kasumi::core::Key::Escape) {
                    modal_open.set(false);
                }
            });
        }
    }
}

fn main() -> anyhow::Result<()> {
    run(Navigation, options("Kasumi — navigation", 1180.0, 820.0))
}
