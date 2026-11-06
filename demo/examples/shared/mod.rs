//! Small helpers shared by the gallery examples.
//!
//! Every file under `demo/examples/` compiles as an independent binary, so this
//! module is pulled in with `#[path = "shared/mod.rs"] mod shared;` rather than
//! being its own crate.
//!
//! Scope matters here: the module deliberately does **not** wrap any framework
//! widget. Cards, buttons, lists and the rest are used directly in the examples
//! so that reading an example shows the real API. What lives here is only the
//! boilerplate that would otherwise be copy-pasted into twenty files: a text
//! label trio, a page scaffold and a window that quits on Escape.

#![allow(dead_code)]

use kasumi::core::Key;
use kasumi::prelude::*;
use kasumi_backend::WindowOptions;

/// An explanatory paragraph in the muted body role.
pub fn note(ui: &mut Ui, text: impl Into<String>) {
    let theme = ui.theme().clone();
    ui.add(
        Label::new(text)
            .font_size(12.0)
            .color(theme.colors.on_surface_variant),
    );
}

/// Monospace caption used for API names and identifiers.
pub fn code(ui: &mut Ui, text: impl Into<String>) {
    let theme = ui.theme().clone();
    ui.add(
        Label::new(text)
            .font_size(10.5)
            .color(theme.colors.primary)
            .family(FontFamily::Monospace)
            .wrap(TextWrap::None),
    );
}

/// Tiny monospace kicker.
pub fn micro(ui: &mut Ui, text: impl Into<String>) {
    let theme = ui.theme().clone();
    ui.add(
        Label::new(text)
            .font_size(9.0)
            .color(theme.colors.outline)
            .family(FontFamily::Monospace)
            .wrap(TextWrap::None),
    );
}

/// The title row used at the top of a card: name on the left, API on the right.
///
/// The card itself is always declared by the example with
/// `ui.card(Card::elevated().., ..)`; this only fills in the header line.
pub fn heading(ui: &mut Ui, title: &str, kicker: &str) {
    let theme = ui.theme().clone();
    ui.row(|ui| {
        ui.width_full().align_items(Align::Center).gap(8.0);
        ui.add(
            Label::new(title)
                .style(theme.typography.title_medium.clone())
                .color(theme.colors.on_surface)
                .wrap(TextWrap::None),
        );
        ui.spacer();
        micro(ui, kicker);
    });
}

/// A scrollable page scaffold: header row, divider, scrolling body.
///
/// The body is keyed so switching pages keeps node identity stable.
pub fn page(ui: &mut Ui, title: &str, subtitle: &str, key: &str, body: impl FnOnce(&mut Ui)) {
    let theme = ui.theme().clone();
    ui.size_full().background(theme.colors.background);
    ui.column(|ui| {
        ui.size_full();
        ui.row(|ui| {
            ui.width_full()
                .padding_edges(Edges {
                    left: 32.0,
                    right: 32.0,
                    top: 22.0,
                    bottom: 14.0,
                })
                .gap(12.0)
                .align_items(Align::End);
            ui.column(|ui| {
                ui.gap(4.0);
                let mut kicker = theme.typography.label_medium.clone();
                kicker.color = theme.colors.primary;
                kicker.family = FontFamily::Monospace;
                ui.add(Label::new("KASUMI / EXAMPLE").style(kicker));
                ui.add(
                    Label::new(title)
                        .style(theme.typography.headline_medium.clone())
                        .color(theme.colors.on_surface),
                );
            });
            ui.spacer();
            ui.column(|ui| {
                ui.gap(2.0).align_items(Align::End);
                ui.add(
                    Label::new(subtitle)
                        .font_size(11.0)
                        .color(theme.colors.on_surface_variant)
                        .align(TextAlign::End),
                );
                micro(ui, "Esc · quit · Tab · focus");
            });
        });
        ui.divider();
        ui.scroll(ScrollView::vertical().grow(1.0), |ui| {
            ui.width_full()
                .padding_edges(Edges {
                    left: 32.0,
                    right: 32.0,
                    top: 20.0,
                    bottom: 28.0,
                })
                .gap(16.0);
            ui.keyed(key, body);
        });
    });
}

/// A window that quits on Escape for examples without their own key handling.
pub struct QuitOnEscape<A> {
    pub app: A,
}

impl<A: App> App for QuitOnEscape<A> {
    fn view(&mut self, ui: &mut Ui) {
        self.app.view(ui);
        if ui.key_pressed(&Key::Escape) {
            let key = ui.window().key;
            ui.close_window(key);
        }
    }
}

/// The default window size used by the examples.
pub fn options(title: &str, width: f32, height: f32) -> WindowOptions {
    WindowOptions {
        title: title.to_owned(),
        logical_size: Size::new(width, height),
        min_size: Some(Size::new(480.0, 360.0)),
        ..WindowOptions::default()
    }
}
