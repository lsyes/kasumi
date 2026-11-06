//! `media` — RGBA images, resvg SVG, QR codes and CommonMark Markdown.
//!
//! ```sh
//! cargo run -p kasumi-demo --example media
//! ```
//!
//! Media widgets all end up in the same draw list as text: the renderer packs
//! glyph masks and color images into one RGBA atlas and issues a single
//! instanced draw call. That is why a QR code next to a paragraph costs one
//! extra instance rather than a second pipeline.
//!
//! Two performance notes that matter for real applications:
//!
//! - Decode once. `Image::from_bytes` and `Svg::new` are cheap in this example
//!   because the results are cached, but a file-backed `Image::from_path`
//!   decodes on every call. Put the decoded `ImageData` in a `OnceLock` or a
//!   `State<Arc<ImageData>>`.
//! - SVG is rasterized by resvg at a requested size. Ask for roughly the
//!   physical pixels you will paint; the atlas caps entries at 1024 px.

use std::sync::{Arc, OnceLock};

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// A 256×256 plasma gradient, generated once.
fn gradient() -> Arc<ImageData> {
    static IMAGE: OnceLock<Arc<ImageData>> = OnceLock::new();
    Arc::clone(IMAGE.get_or_init(|| {
        const SIZE: u32 = 256;
        let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let fx = x as f32 / SIZE as f32;
                let fy = y as f32 / SIZE as f32;
                let wave = (fx * 9.0).sin() * (fy * 7.0).cos();
                let blend = ((fx + fy) * 0.5 + wave * 0.18).clamp(0.0, 1.0);
                pixels.extend_from_slice(&[
                    (110.0 + blend * 120.0) as u8,
                    (150.0 + (1.0 - blend) * 80.0) as u8,
                    (235.0 - blend * 90.0) as u8,
                    255,
                ]);
            }
        }
        Arc::new(ImageData::from_rgba8(SIZE, SIZE, pixels))
    }))
}

/// A checkerboard used to show `ImageFit` behaviour with a non-square box.
fn checker() -> Arc<ImageData> {
    static IMAGE: OnceLock<Arc<ImageData>> = OnceLock::new();
    Arc::clone(IMAGE.get_or_init(|| {
        const CELL: u32 = 16;
        const SIDE: u32 = 8 * CELL;
        let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
        for y in 0..SIDE {
            for x in 0..SIDE {
                let dark = ((x / CELL) + (y / CELL)) % 2 == 0;
                let value = if dark { 40 } else { 210 };
                pixels.extend_from_slice(&[value, value, value, 255]);
            }
        }
        Arc::new(ImageData::from_rgba8(SIDE, SIDE, pixels))
    }))
}

const LOGO_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop stop-color="#7c9cff"/>
      <stop offset="1" stop-color="#74d6a0"/>
    </linearGradient>
  </defs>
  <rect width="200" height="200" rx="34" fill="#151923"/>
  <circle cx="100" cy="100" r="62" fill="url(#g)"/>
  <path d="M64 104l24 24 50-58" fill="none" stroke="#0f1117" stroke-width="16"
        stroke-linecap="round" stroke-linejoin="round"/>
</svg>"##;

const MARKETING_MD: &str = "\
### Full-width markdown

Markdown is rendered with **semantic blocks**, *inline emphasis*, `inline code`, \
ordered and unordered lists, and block quotes. Headings, paragraphs and list \
items become separate text nodes inside one column, so they participate in \
layout like any other widget.

- keyboard-first editing
- crisp rasterized SVG and QR
- images in the same draw list as text

1. One
2. Two
3. Three

> Media stays in the same draw list as text, which keeps the frame at one
> pipeline state change.

```rust
let clicks = ui.button(\"OK\").clicked();
```

---

A horizontal rule closes the sample.";

struct Media;

impl App for Media {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let fit = ui.state(|| 0usize);
        let payload = ui.state(|| "https://github.com/lsyes/kasumi".to_owned());
        let scale = ui.state(|| 220.0f32);

        shared::page(
            ui,
            "Media",
            "Image · Svg · QrCode · Markdown",
            "media",
            |ui| {
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    // ------------------------------------------------------
                    // Images and object-fit
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "RGBA images", "Image::from_shared");
                                note(
                                    ui,
                                    "A square source in a 2:1 box, under each `ImageFit`. \
                                     `Contain` letterboxes, `Cover` crops, `Fill` distorts and \
                                     `Original` paints at natural size.",
                                );
                                let fits = [
                                    ("Contain", ImageFit::Contain),
                                    ("Cover", ImageFit::Cover),
                                    ("Fill", ImageFit::Fill),
                                    ("Original", ImageFit::Original),
                                ];
                                ui.row(|ui| {
                                    ui.width_full().gap(10.0).align_items(Align::Center);
                                    for (index, (label, _)) in fits.iter().enumerate() {
                                        let active = fit.get() == index;
                                        let mut button = Button::new(*label)
                                            .font_size(10.5)
                                            .padding_xy(10.0, 5.0);
                                        button = if active {
                                            button.tonal()
                                        } else {
                                            button.subtle()
                                        };
                                        if ui.add(button).clicked() {
                                            fit.set(index);
                                        }
                                    }
                                    ui.spacer();
                                    micro(ui, fits[fit.get().min(3)].0);
                                });
                                ui.add(
                                    Image::from_shared(checker())
                                        .fit(fits[fit.get().min(3)].1)
                                        .size(Length::Percent(100.0), 160.0)
                                        .radius(theme.shape.small)
                                        .alt("A checkerboard test image"),
                                );
                                ui.row(|ui| {
                                    ui.width_full().gap(10.0);
                                    ui.add(
                                        Image::from_shared(gradient())
                                            .fit(ImageFit::Cover)
                                            .size(96.0, 96.0)
                                            .radius(theme.shape.medium),
                                    );
                                    ui.add(
                                        Image::from_shared(gradient())
                                            .fit(ImageFit::Cover)
                                            .size(96.0, 96.0)
                                            .radius(48.0),
                                    );
                                    ui.column(|ui| {
                                        ui.grow(1.0).gap(4.0);
                                        code(ui, "Image::from_rgba8(..)");
                                        note(
                                            ui,
                                            "Decode once into an `Arc<ImageData>` and pass the \
                                             same handle every frame. The atlas is keyed by the \
                                             image contents, so a repeated value costs one entry.",
                                        );
                                    });
                                });
                            },
                        );
                    });

                    // ------------------------------------------------------
                    // SVG
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "SVG", "Svg::new");
                                note(
                                    ui,
                                    "resvg parses and rasterizes at a requested size. The parsed \
                                     tree and the rasterized bitmap are both cached, so a logo \
                                     that never changes is rasterized once per size.",
                                );
                                ui.add(
                                    Slider::new(&scale, 96.0, 260.0)
                                        .step(4.0)
                                        .label("Render size")
                                        .width(Length::Percent(100.0)),
                                );
                                ui.row(|ui| {
                                    ui.width_full().gap(14.0).align_items(Align::Center);
                                    let side = scale.get();
                                    if let Ok(svg) = Svg::new(LOGO_SVG) {
                                        ui.add(
                                            svg.render_size(side as u32, side as u32)
                                                .size(side, side),
                                        );
                                    }
                                    ui.column(|ui| {
                                        ui.grow(1.0).gap(4.0);
                                        code(ui, format!("render_size({side:.0}, {side:.0})"));
                                        note(
                                            ui,
                                            "Bad SVG is a recoverable error, not a panic: \
                                             `Svg::new` returns `Result`.",
                                        );
                                    });
                                });
                            },
                        );
                    });
                });

                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    // ------------------------------------------------------
                    // QR
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::outlined()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "QR code", "QrCode::new");
                                note(
                                    ui,
                                    "The payload is a `State<String>`, so the code re-encodes as \
                                     you type. Encoding is cached per payload and error-correction \
                                     level; the bitmap follows the theme's contrast roles.",
                                );
                                ui.add(
                                    TextInput::new(&payload)
                                        .placeholder("Anything encodable")
                                        .width(Length::Percent(100.0)),
                                );
                                ui.row(|ui| {
                                    ui.gap(14.0).align_items(Align::Center);
                                    match QrCode::new(payload.get()) {
                                        Ok(code) => {
                                            ui.add(code.size(168).colors(
                                                theme.colors.on_surface,
                                                theme.colors.surface,
                                            ));
                                        }
                                        Err(error) => {
                                            ui.add(
                                                Label::new(error)
                                                    .font_size(11.0)
                                                    .color(theme.colors.error),
                                            );
                                        }
                                    }
                                    ui.column(|ui| {
                                        ui.grow(1.0).gap(3.0);
                                        micro(ui, "error correction M");
                                        code(ui, format!("{} bytes", payload.get().len()));
                                    });
                                });
                            },
                        );
                    });

                    // ------------------------------------------------------
                    // Markdown
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(2.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Markdown", "Markdown::new");
                                note(
                                    ui,
                                    "CommonMark and GFM through pulldown-cmark. Each block becomes \
                                     its own text node, so headings, lists and quotes lay out \
                                     like ordinary widgets.",
                                );
                                ui.add(Markdown::new(MARKETING_MD).font_size(12.5));
                            },
                        );
                    });
                });
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Media },
        options("Kasumi — media", 1180.0, 900.0),
    )
}
