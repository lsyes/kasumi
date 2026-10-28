# Kasumi

Kasumi is a Rust desktop UI skeleton built around an immediate-mode `view(ui)`
function and a retained node tree. User code declares only the current frame;
nodes, layout caches, interaction state and state handles are owned by the
framework and never appear in the public API.

This is a working prototype, not a published or production-ready framework.

![Kasumi demo](docs/screenshot.png)

## What is implemented

- A layered workspace: core value types, typed state, retained tree, taffy
  layout, text/paint, the `Ui` facade and a winit backend with selectable
  `glow` (OpenGL) or `wgpu` renderers.
- Flex, grid, stack, centered, wrapped and scrollable containers; labels,
  buttons, text fields, checkboxes, radios, sliders, combo boxes, dividers,
  progress bars, tooltips and context menus.
- RGBA images, resvg-backed SVG, CommonMark/GFM Markdown and QR-code widgets.
- A retained, spring-based scroll view with wheel, keyboard and scrollbar
  input, using a smoothed Firefox-style wheel response.
- Single-line text editing with mouse-drag selection, horizontal scrolling,
  word and line navigation, undo/redo, IME pre-edit and system clipboard
  Ctrl/Cmd+A/C/X/V support.
- Position-derived element ids, optional explicit keys for lists and
  conditional branches, duplicate-id detection in debug builds.
- A generational node arena. Nodes are reused across frames and recycled at
  frame end when they are not declared.
- Typed `State<T>` handles with TTL eviction, persistent state and automatic
  repaint requests.
- Incremental taffy layout: only style/content/child-list changes are pushed
  into taffy, and taffy's cache skips clean subtrees.
- Text shaping with cosmic-text, glyph rasterization with swash, a shelf-packed
  RGBA atlas and a single instanced OpenGL draw call for rectangles, glyphs and
  images.
- Event-driven rendering: idle windows do not run `view`, and pointer moves
  that stay inside the same hover target do not produce frames.
- Retained tween and spring animation channels, visual opacity/translation/
  scale transforms, reduced-motion-friendly presence transitions, and animated
  controls including combo popups, checks, radios, sliders and progress.
- An accessibility snapshot API and a small demo with counters, keyed rows,
  typography samples and animation.

## Requirements

- Linux with system fonts available through fontconfig. The default `glow`
  backend needs OpenGL 3.3; the `wgpu` backend needs a Vulkan, GLES, Metal or
  DX12 adapter supported by wgpu 30. The backend uses winit's X11 and Wayland
  features. The web is not wired up.
- Rust 1.88 or newer. The workspace uses edition 2024.

## Run the demo

```sh
cargo run --release -p kasumi-demo

# Select the wgpu backend without compiling glutin/glow.
cargo run --release -p kasumi-demo --no-default-features --features wgpu
```

The backend crates use mutually exclusive `glow` and `wgpu` features. Selecting
neither or both is a compile error, so embedders cannot accidentally pay for or
link both render paths. The winit event loop, input normalization, clipboard,
IME, animation clock, layout and paint layers are shared.

Diagnostics:

```sh
# Print GL information and per-frame layout/paint statistics.
KASUMI_STATS=1 cargo run --release -p kasumi-demo

# Render the overview without its startup animation, then quit after four
# frames. Useful for the static-frame measurements below.
KASUMI_STATS=1 KASUMI_DEMO_TARGET=0 KASUMI_EXIT_AFTER_FRAMES=4 \
    cargo run --release -p kasumi-demo

# Open another demo page: 0 overview, 1 typography, 2 layout, 3 form,
# 4 controls.
KASUMI_DEMO_PAGE=1 cargo run --release -p kasumi-demo
```

## Minimal example

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
                ui.add(
                    Label::new(count.get().to_string())
                        .font_size(28.0)
                        .family(FontFamily::Monospace),
                );
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

`view` is called once per rendered frame. The function returns nothing and
cannot return a borrow of the tree. The only values that can leave the frame are
`State<T>`, plain application data and the `Response` values returned by
widgets.

The same program is in
[demo/examples/counter.rs](demo/examples/counter.rs) and is compile-checked with
the rest of the workspace.

## Frame pipeline

1. The backend converts winit events into an `InputState` and computes the
   device scale and logical viewport.
2. `Runtime::frame` processes input against the previous layout. Hover, active
   and focus targets are resolved here. Hit testing against the previous frame
   is deliberate: a widget's current size is not known until `view` returns.
3. `view` runs with a `Ui`. Every declaration is reconciled by id into the
   retained tree. A new id allocates a node; a matching id reuses it.
4. `Tree::end_frame` commits child order, recycles nodes that were not touched
   and ends the state TTL window.
5. `LayoutEngine` updates the changed parts of its taffy mirror and runs layout.
   Absolute rectangles are written back to the nodes.
6. The paint pass walks the committed tree and appends rects and positioned
   glyphs to one `DrawList`, preserving z-order.
7. The backend uploads the draw list as one instance buffer and renders it with
   one shader. A frame is requested again only for animations, explicit
   repaint requests or input that changes what is on screen.

## Identity and reconciliation

The `Ui` holds an id scope and a sibling counter. A child id is
`mix(parent_scope, sibling_index)`. Containers become the scope for their
children. `ui.keyed(key, |ui| ...)` replaces the sibling-derived scope with
`hash(parent_scope, key)`, so reordering keyed items does not reparent their
subtrees.

State ids use a separate namespace. Adding or removing a `ui.state` call does
not shift widget ids.

Every declared node id is checked against a per-frame table that records which
widget declared it. A duplicate panics with the id, the frame number and both
origins. Duplicate ids are normally caused by an unkeyed conditional branch or
by reordering unkeyed siblings; wrapping the branch in `ui.keyed` fixes both
cases.

## State and interaction

`ui.state(|| value)` returns a cloneable `State<T>`. Handles can be stored on
application structs or captured by closures; they do not borrow the `Ui`. The
API is `get()` (requires `Clone`), `with()` for a zero-copy read, `set()` and
`update()`. All writes request a repaint. Since a write can happen after the
widgets that read the value were declared, a write schedules one follow-up
frame; that frame is also the point at which the rendered result observes the
new value. State ids use a namespace separate from node ids, so adding or
removing a state call does not shift widget ids.

Values that are not touched for 600 frames are evicted. `ui.state_persistent`
opts a value out of eviction; `Runtime::set_state_ttl_frames` changes the
window. An evicted handle re-runs its initializer on the next access.

Widgets return `Response`, which reports `hovered`, `pressed`, `clicked`,
`released`, `dragged`, `context_clicked`, `focused`, `changed` and the current
scroll offset. Hover and click resolution uses the previous frame's
rectangles, as described above. Tab order is the depth-first order of
focusable nodes in the current tree. Enter and Space activate focused buttons,
checkboxes, radios and combo boxes.

The text field keeps caret, selection and undo state on its retained node while
the text itself remains a `State<String>`. It supports Shift+arrows,
Ctrl/Cmd+arrows, Home/End, Delete/Backspace, Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z,
Ctrl/Cmd+A/C/X/V, mouse selection and IME composition. The backend owns the
system clipboard integration.

`Runtime::a11y_snapshot` produces a flat list of roles, labels, bounds, depth
and focus state from the retained tree. It is a data API only; there is no
platform integration yet.

## Layout

`kasumi-layout` mirrors the tree into a taffy tree. Flexbox and CSS grid are
both supported; `ui.grid(columns, ..)` uses equal fractional tracks and
`ui.grid_with(..)` accepts explicit `GridTrack` values. `ui.stack(..)` places
all children in one grid cell. Styles are only pushed for nodes whose style or
text content changed, and child lists are only pushed when the committed order
changed. Taffy's cache handles the rest.

Text nodes are measured through a `TextMeasure` trait implemented by
`kasumi-paint::TextEngine`. The paint pass shapes at the width recorded by the
layout measure pass, so the reserved height and the rendered lines agree.
Paragraphs that must wrap should set an explicit width through
`Label::width(...)` or a fixed-width parent; the demo's typography page does
this.

## Text

The text pipeline follows the split used by gpui:

- cosmic-text discovers system fonts, performs fallback and shapes lines.
- swash rasterizes glyph outlines and color glyphs.
- The renderer packs masks and color emoji into one RGBA atlas. Masks are
  uploaded as replicated alpha, color glyphs as RGBA.
- Glyphs are emitted as textured quads in the same instance stream as rects.
- The shaping cache is keyed by text, family, weight, italic, size, line
  height, letter spacing, wrapping mode and available width. The cache holds up
  to 2048 layouts and clears as a whole when full.
- Glyphs are rasterized at `font_size * scale_factor`, quantized to quarter
  pixels, and cached in the atlas by face, glyph and device size.

The CJK sample in the demo exercises the system fallback chain; on this machine
it resolves to Noto Sans CJK through fontconfig.

## Scrolling and rich content

Scroll containers retain offset, target and velocity. Wheel input accumulates
into a critically damped spring; scrollbar drags update the offset directly.
Keyboard arrows, Page Up/Down and Home/End scroll focused viewports. Hit
testing and painting apply ancestor scroll offsets without re-running layout.

Images are decoded through `image` and uploaded once by content id. SVG is
rasterized through resvg/tiny-skia, Markdown is parsed with pulldown-cmark and
QR codes are generated from the `qrcode` encoder. All of these produce ordinary
RGBA atlas entries and remain in the same ordered draw list as text.

## Rendering

The backend exposes two mutually exclusive feature-selected renderers:

- `glow` (default): OpenGL 3.3 core through glutin and glow.
- `wgpu`: wgpu 30.0.1 with WGSL, explicitly enabling Vulkan, GLES, Metal and
  DX12. Cargo still trims target-specific APIs during a native build.

Both consume the same ordered `DrawList` and atlas allocations. The wgpu path
handles surface capability selection, resizing, suboptimal/outdated/lost
surfaces and bounded submission polling.

Scrolling remains available when a scroll view hides its scrollbar with
`ScrollView::scrollbars(false)`; only the rail and thumb paint calls are
suppressed.

One `#[repr(C)]` instance struct describes rectangles, glyphs and images. The
vertex shader expands four vertices per instance; the fragment shader either
evaluates a rounded-box signed distance field with border coverage, or samples
the atlas. Output is premultiplied alpha with `ONE, ONE_MINUS_SRC_ALPHA`
blending. Clipping is a per-instance rectangle test in the fragment shader, not
a scissor or a rounded clip.

Colors are stored as sRGB components and written to a non-sRGB framebuffer.
There is no linear-light color pipeline.

## Animation

Animation state is retained on nodes and driven by one runtime timeline, so
idle animations do not force unconditional continuous rendering.

```rust
let progress = ui.animate_with(
    "panel-open",
    1.0,
    MotionSpec::spring(Spring::new(0.24, 0.82)),
);

ui.transition("details", show_details, Transition::popover(), |ui| {
    ui.column(|ui| {
        ui.gap(8.0);
        ui.label("Animated content");
        ui.label(format!("{progress:.2}"));
    });
});

let switch_alpha = ui.transition_switch("tab", selected_tab, Transition::fade());
if let Some(visual) = ui.visual() {
    visual.opacity.snap(switch_alpha);
}
```

`Ui::animate` remains the simple tween API. `animate_with` supports named
channels, tween curves and critically/near-critically damped springs. The motion
types are re-exported from the facade prelude. A `Transition` controls initial
opacity, translation and scale; conditional content is retained during its exit
animation. `Ui::visual`, `opacity`, `scale`, `translate` and `transform_origin`
are available to custom widgets.

Visual transforms affect painting and do not mutate layout. The current
compound alpha and opacity are passed down the paint walk.

## Performance

Measured on the development machine (Linux, Mesa 26.1.6, AMD RX 5500 XT,
loongarch64) with the release build:

| Scene | Nodes | Primitives | Layout updates | Measures | CPU per frame |
|---|---:|---:|---:|---:|---:|
| Overview, static (`KASUMI_DEMO_TARGET=0`) | 113 | 29 rects, 735 glyphs | 0 | 0 | 0.19–0.28 ms |
| Layout page, static | 77 | 36 rects, 334 glyphs | 0 | 0 | 0.15–0.16 ms |
| Debug build, overview static | 113 | 29 rects, 735 glyphs | 0 | 0 | ~1.1–1.8 ms |

The first frame is not representative: it includes initial shaping, font
fallback and first-time rasterization of every glyph in the scene, and takes
roughly 200 ms release on this machine. Later static frames perform no layout
work. `cargo test` includes
`runtime::tests::static_frames_do_no_layout_work_and_reuse_nodes`, which
asserts zero style updates, zero child updates and zero measures on a settled
static frame.

The timing above covers input processing, reconcile, layout, scene building and
texture upload preparation. It does not include GPU execution or vsync. The
renderer still rebuilds and uploads the instance buffer on every rendered frame;
the savings come from not rendering idle frames at all.

### Reproducible benchmark

```sh
cargo bench -p kasumi --bench frames
```

The dependency-free harness in
[`crates/kasumi/benches/frames.rs`](crates/kasumi/benches/frames.rs) warms the
text cache, runs seven timed rounds and reports the median CPU time per frame.
It covers:

- complete static frames with 100 and 500 keyed rows;
- a frame with 250 live `State` entries;
- a clean 1,501-node layout pass;
- a layout with one dirty text width among 1,501 nodes;
- scene painting with cached text shaping.

Representative results from a release build on the current development machine
(Loongson-3A6000, Linux) are:

| Benchmark | CPU per frame |
|---|---:|
| `runtime/static_100_rows` | 0.148 ms |
| `runtime/static_500_rows` | 0.323 ms |
| `runtime/state_250_entries` | 0.087 ms |
| `layout/clean_1501_nodes` | 19 ns |
| `layout/one_dirty_label_1501_nodes` | 0.260 ms |
| `paint/cached_text_1501_nodes` | 0.427 ms |

These timings exclude GPU execution and are machine-dependent. The benchmark
is intended as a repeatable regression check rather than a cross-device score.

## Crates

| Crate | Role |
|---|---|
| `kasumi-core` | Ids, geometry, colors, styles, input types, draw-list primitives. No dependencies. |
| `kasumi-state` | Type-erased state table, `State<T>`, TTL and repaint flag. |
| `kasumi-tree` | Generational node arena, reconcile/commit, hit testing, a11y snapshot. |
| `kasumi-layout` | taffy mirror, dirty tracking, text measurement trait, absolute rects. |
| `kasumi-paint` | cosmic-text/swash text engine and the retained-to-draw-list paint pass. |
| `kasumi` | `Ui`, `Widget`, built-in widgets, `Response`, `Theme`, runtime and frame loop. |
| `kasumi-backend` | Shared winit input/clipboard/IME layer plus feature-selected glow or wgpu renderers. The only crate allowed to use `unsafe`. |
| `kasumi-demo` | The example application shown above. |

`kasumi` is the facade crate. The other crates are implementation layers and
can change without notice.

## Testing

```sh
cargo test --workspace
```

```sh
cargo check -p kasumi-demo --no-default-features --features wgpu
```

The suite covers geometry and color parsing, id generation, state typing and
TTL eviction, arena generation checks, reconcile and hit testing, flex/grid/
stack layout, clean-frame behavior, text shaping and caching, hover/press/drag
semantics, context overlays, smooth scrolling, text selection/clipboard/undo,
keyboard focus and animation scheduling. Backend tests validate that the WGSL
scene shader parses with the exact wgpu/Naga version in the lockfile.

## Known limitations

- Hit testing uses the previous frame's layout. A control that changes size or
  appears under the pointer can be hit one frame late.
- Large-list virtualization is not implemented yet. Scroll views lay out all
  declared children.
- Text input is single-line. Multi-line editing, password masking and
  platform-native selection handles are not implemented.
- SVG and QR rasters are cached by their full input. File-backed `Image`
  builders decode when constructed, so keep `ImageData` or a shared image
  handle in application state for expensive assets.
- Accessibility is a snapshot API (`Runtime::a11y_snapshot`). There is no
  AccessKit or platform a11y bridge.
- The optional `view!` macro crate is not included. Method chains are the
  complete API.
- No callback queue (`ui.on_click`), reactive dependency tracking or
  `ui.local` state fast path.
- One window per process.
- The glyph/image atlas is a single fixed 2048×2048 texture. Images are capped
  at 1024 px on their longest edge; when the atlas fills, new entries are
  skipped rather than causing a multi-page draw.
- A text node's wrap width is the width used by its last measure pass. Changing
  a text node's final width without a new measure pass can produce overflow
  instead of reflow.
- `State<T>` is single-threaded (`Rc`-based) and is not `Send` or `Sync`.
  Mutating state outside `view` requires an explicit repaint path from the
  embedder.
- State values are evicted after 600 untouched frames unless they are declared
  with `ui.state_persistent`. An evicted value resets to its initializer.
- Clipping is rectangular. Rounded corners on a clipped parent do not clip
  children to the rounded shape.
- Transforms are paint-time only and do not affect hit testing or layout.
  Exiting transition content is retained but no longer interactive.
- Colors are not managed in linear light.
- Every rendered frame rebuilds and uploads the instance buffer; there is no GPU
  damage tracking or retained scene cache.
- winit is not vendored. The backend uses the crates.io releases.

## License

Apache-2.0. See `LICENSE-APACHE`.

The text pipeline is adapted from gpui's cosmic-text/swash split and glyph atlas
approach. gpui is part of Zed and is licensed under Apache-2.0. See `NOTICE`.
The extracted gpui reference checkout in `gpui/` is not part of this project and
is ignored by Git.
