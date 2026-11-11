# Kasumi

Kasumi is a Rust desktop UI skeleton built around an immediate-mode `view(ui)`
function and a retained node tree. User code declares only the current frame;
the framework owns nodes, layout caches, interaction state and state handles.

This is a working prototype, not a published or production-ready framework.

![Kasumi demo](docs/screenshot.png)

## Quick start

Requirements: Rust 1.88+ (edition 2024), desktop system fonts through
fontconfig, and either OpenGL 3.3 / GLES or a Vulkan, Metal or DX12 adapter
supported by wgpu 30. The web is not wired up.

```sh
cargo run --release -p kasumi-demo
cargo run -p kasumi-demo --example counter
cargo build -p kasumi-demo --examples
cargo test --workspace

# Open another demo page: 0 overview, 1 typography, 2 layout, 3 form,
# 4 controls, 5 surfaces, 6 async resources.
KASUMI_DEMO_PAGE=1 cargo run --release -p kasumi-demo

# Start in the baseline dark scheme; the sidebar toggles it per window.
KASUMI_DEMO_DARK=1 cargo run --release -p kasumi-demo
```

`kasumi-backend` has mutually exclusive `glow` and `wgpu` features. Cargo
manifests currently default to **`wgpu`**; select exactly one explicitly for
an embedder:

```toml
[dependencies]
kasumi = { path = "crates/kasumi" }
kasumi-backend = { path = "crates/kasumi-backend", default-features = false, features = ["wgpu"] }
```

```sh
# Select OpenGL 3.3 instead.
cargo run -p kasumi-demo --example counter --no-default-features --features glow
```

## Minimal app

```rust
use kasumi::prelude::*;
use kasumi::App;
use kasumi_backend::{run, WindowOptions};

struct Counter;

impl App for Counter {
    fn view(&mut self, ui: &mut Ui) {
        let count = ui.state(|| 0i32);

        ui.column(|ui| {
            ui.size_full().padding(24.0).gap(12.0);
            ui.add(Label::new("Counter").font_size(22.0).semibold());

            ui.row(|ui| {
                ui.gap(8.0);
                if ui.button("-").clicked() {
                    count.update(|value| *value -= 1);
                }
                ui.add(Label::new(count.get().to_string()).font_size(28.0));
                if ui.add(Button::new("+").primary()).clicked() {
                    count.update(|value| *value += 1);
                }
            });
        });
    }
}

fn main() {
    run(Counter, WindowOptions::default()).expect("kasumi event loop");
}
```

`view` is called once per rendered frame. Declarations are reconciled by
position-derived ids; use `ui.keyed(..)` for lists and conditional branches.
Widgets return a `Response` for this frame; cross-frame values live in
`State<T>` or application data. Hit testing uses the previous frame's layout,
so a newly appearing or resized control can be hit one frame late.

## What's included

- Material Design 3 baseline, seeded and custom themes; the 15-role type scale,
  shape/elevation/motion/state-layer tokens and per-window theme switching.
- Flex, grid, stack, centered, wrapped and scrollable containers; incremental
  taffy layout and text measurement.
- Buttons, fields, checkboxes, radios, switches, sliders, combo boxes, lists,
  menus, navigation drawers, cards, dialogs, tooltips and progress indicators.
- Multi-window applications and a cross-platform system tray.
- Async resources with loading / ready / error states, stale-while-revalidate
  reloads, cancellation and event-loop wakeups.
- Single-line and multi-line text editing, syntax highlighting, rich text,
  RGBA images, SVG, Markdown and QR codes.
- Retained scrolling and virtualization, including fixed-height `ListView` and
  application-managed `Ui::virtual_scroll` for variable-height rows.
- Tween / spring animation, presence transitions and paint-time visual
  transforms; event-driven rendering keeps idle windows asleep.

See [demo/examples/README.md](demo/examples/README.md) for the index of all 25
standalone examples. Each file is an independent binary and can be copied as a
starting point.

## Documentation

The root README is deliberately short. Full material has been split into:

| Document | Contents |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | frame pipeline, state and ids, layout, rendering, animation, performance, crate roles, testing and known limitations |
| [docs/GUIDE.md](docs/GUIDE.md) | widgets, forms, platform surfaces, M3 theme, lists / scrolling and text |
| [crates/kasumi/README.md](crates/kasumi/README.md) | facade-crate widget and theme notes |
| [crates/kasumi/src/widgets/README.md](crates/kasumi/src/widgets/README.md) | scrolling and list implementation notes |
| [crates/kasumi/src/theme/README.md](crates/kasumi/src/theme/README.md) | theme token implementation notes |
| [crates/kasumi-text/README.md](crates/kasumi-text/README.md) | experimental text backend, editor-grade APIs and benchmark |
| [crates/kasumi-layout/README.md](crates/kasumi-layout/README.md) | taffy mirror and layout notes |
| [crates/kasumi-tree/README.md](crates/kasumi-tree/README.md) | node arena, reconciliation and state notes |
| [crates/kasumi-backend/README.md](crates/kasumi-backend/README.md) | renderer, feature selection and performance notes |
| [demo/examples/README.md](demo/examples/README.md) | example gallery and headless verification commands |

`kasumi` is the facade crate. The other crates are implementation layers and
can change without notice.

## Diagnostics

```sh
KASUMI_STATS=1 cargo run --release -p kasumi-demo
KASUMI_STATS=1 KASUMI_DEMO_TARGET=0 KASUMI_EXIT_AFTER_FRAMES=4 \
    cargo run -q -p kasumi-demo --example counter
cargo check -p kasumi-demo --no-default-features --features wgpu
cargo bench -p kasumi --bench frames
```

`KASUMI_DEMO_PAGE`, `KASUMI_DEMO_DARK` and `KASUMI_DEMO_TARGET` select demo
pages and baseline schemes for measurements; see
[demo/examples/README.md](demo/examples/README.md).

## Known limitations

This prototype has no platform accessibility bridge, blurred shadows, linear
light color pipeline, GPU damage tracking, web backend, callback queue or
`view!` macro. Dialogs do not trap keyboard focus, transforms do not affect
hit testing, clipping is rectangular, and the glyph/image atlas is a fixed
2048×2048 texture. `State<T>` is single-threaded and each window has its own
state store. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#known-limitations)
for the complete list.

## License

Apache-2.0. See `LICENSE-APACHE`.

The glyph atlas and instanced sprite strategy are adapted from gpui, the UI
framework used by Zed (Apache-2.0). See `NOTICE`.
