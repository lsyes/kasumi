# Kasumi architecture

本文件汇总框架运行时、布局、渲染与限制。API 入口见根 README，逐组件细节见 `GUIDE.md`，文本后端见 `crates/kasumi-text/README.md`，示例索引见 `demo/examples/README.md`。

## 运行时与帧管线

### Frame pipeline

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

## 身份、状态与交互

### Identity and reconciliation

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

### State and interaction

`ui.state(|| value)` returns a cloneable `State<T>`. Handles can be stored on
application structs or captured by closures; they do not borrow the `Ui`. The
API is `get()` (requires `Clone`), `with()` for a zero-copy read, `set()` and
`update()`. All writes request a repaint. Since a write can happen after the
widgets that read the value were declared, a write schedules one follow-up
frame; that frame is also the point at which the rendered result observes the
new value. State ids use a namespace separate from node ids, so adding or
removing a state call does not shift widget ids.

Values whose `ui.state` call stops running for 600 frames are evicted.
Declaring a slot keeps it alive, so a value that is declared every frame but
read only occasionally (a draft, an id allocator) holds its value: eviction
follows declaration, not reads. `ui.state_persistent` opts a value out of
eviction; `Runtime::set_state_ttl_frames` changes the window. An evicted handle
re-runs its initializer on the next access.

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

## 布局

### Layout

`kasumi-layout` mirrors the tree into a taffy tree. Flexbox and CSS grid are
both supported; `ui.grid(columns, ..)` uses equal fractional tracks and
`ui.grid_with(..)` accepts explicit `GridTrack` values. `ui.stack(..)` places
all children in one grid cell. Styles are only pushed for nodes whose style or
text content changed, and child lists are only pushed when the committed order
changed. Taffy's cache handles the rest.

`ui.grow(factor)` sets the flex-grow factor. Flex items default to their
content width as the flex base, so proportional columns should also call
`ui.basis(0.0)` to distribute all free space strictly by the grow ratios.

Text nodes are measured through a `TextMeasure` trait implemented by
`kasumi-paint::TextEngine`. The paint pass shapes at the width recorded by the
layout measure pass, so the reserved height and the rendered lines agree.
Paragraphs that must wrap should set an explicit width through
`Label::width(...)` or a fixed-width parent; the demo's typography page does
this.

## 渲染

### Rendering

The backend exposes two mutually exclusive feature-selected renderers:

- `glow`: OpenGL 3.3 core through glutin and glow.
- `wgpu` (default): wgpu 30.0.1 with WGSL, explicitly enabling Vulkan, GLES, Metal and
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

## 动画

### Animation

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

## 性能与基准

### Performance

Measured on the development machine (Linux, Mesa 26.1.6, AMD RX 5500 XT,
loongarch64) with the release build:

| Scene | Nodes | Primitives | Layout updates | Measures | CPU per frame |
|---|---:|---:|---:|---:|---:|
| Overview, static (`KASUMI_DEMO_TARGET=0`) | 125 | 23 rects, 768 glyphs | 0 | 0 | 0.29–0.32 ms |
| Layout page, static | 89 | 36 rects, 363 glyphs | 0 | 0 | 0.18–0.20 ms |
| Debug build, overview static | 125 | 22 rects, 768 glyphs | 0 | 0 | ~0.35 ms |
| Debug build, controls page static | 208 | 44 rects, 1,283 glyphs | 0 | 0 | ~0.6 ms |

The first frame is not representative: it includes initial shaping and font
fallback for every label in the scene. With `kasumi-text` the release overview
reaches its first frame in roughly 5 ms of runtime CPU, and the text-heavy
typography page in roughly 13 ms; the previous cosmic-text path measured about
200 ms on this machine. Later static frames perform no layout work. `cargo test` includes
`runtime::tests::static_frames_do_no_layout_work_and_reuse_nodes`, which
asserts zero style updates, zero child updates and zero measures on a settled
static frame.

Debug builds use `opt-level = 1` for both workspace crates and dependencies,
which keeps first builds and rebuilds cheap while the demo stays interactive;
application code still runs with debug assertions, overflow checks and line
tables. With that profile the debug overview reaches its first frame in a few
milliseconds of CPU time on this machine, and the Controls page settles at
about 0.5 ms per frame instead of re-encoding its QR code every frame.

The timing above covers input processing, reconcile, layout, scene building and
texture upload preparation. It does not include GPU execution or vsync. The
renderer still rebuilds and uploads the instance buffer on every rendered frame;
the savings come from not rendering idle frames at all.

#### Reproducible benchmark

```sh
cargo bench -p kasumi --bench frames
```

The dependency-free harness in
[`../crates/kasumi/benches/frames.rs`](../crates/kasumi/benches/frames.rs) warms the
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

## Crate 分工与测试

### Crates

| Crate | Role |
|---|---|
| `kasumi-core` | Ids, geometry, colors, styles, input types, draw-list primitives. No dependencies. |
| `kasumi-state` | Type-erased state table, `State<T>`, TTL and repaint flag. |
| `kasumi-tree` | Generational node arena, reconcile/commit, hit testing, a11y snapshot. |
| `kasumi-layout` | taffy mirror, dirty tracking, text measurement trait, absolute rects. |
| `kasumi-text` | Experimental cross-platform font discovery, shaping, fallback, line breaking and rasterization backend, plus editor-grade layout geometry and UAX #29 text analysis. |
| `kasumi-paint` | Text service wrapper and the retained-to-draw-list paint pass. |
| `kasumi` | `Ui`, `Widget`, built-in widgets, `Response`, `Theme`, runtime and frame loop. |
| `kasumi-backend` | Shared winit input/clipboard/IME layer plus feature-selected glow or wgpu renderers. The only crate allowed to use `unsafe`. |
| `kasumi-demo` | The example application shown above. |

`kasumi` is the facade crate. The other crates are implementation layers and
can change without notice.

### Testing

```sh
cargo test --workspace
```

```sh
cargo check -p kasumi-demo --no-default-features --features wgpu
```

The suite covers geometry and color parsing, id generation, state typing and
TTL eviction, resource loading/futures/cancellation/cache eviction, arena
generation checks, reconcile and hit testing, flex/grid/stack layout,
clean-frame behavior, text shaping, pixel tracking and caching, hover/press/drag
semantics, context overlays, smooth scrolling, text selection/clipboard/undo,
keyboard focus, cards, dialogs, window requests, Material color/type/motion
tokens and animation scheduling. Backend tests validate that the WGSL
scene shader parses with the exact wgpu/Naga version in the lockfile.

### Known limitations

- Hit testing uses the previous frame's layout. A control that changes size or
  appears under the pointer can be hit one frame late.
- `ListView` and `Ui::virtual_scroll` use fixed row heights. `List::virtualize(true)`
  supports its 56/72/88 dp rows via prefix-offset search. Custom variable-height
  data outside `List` needs an application-managed `Ui::virtual_scroll` offset table.
- `TextInput` is single-line; `TextEditor` provides multi-line editing. Password
  masking, platform-native selection handles and bidirectional *caret* movement
  (RTL text renders correctly, but editing is LTR-oriented) are not implemented.
  Syntax highlighting colours byte ranges only; it does not restyle font weight,
  family or size within one paragraph.
- `kasumi-text` targets horizontal UI text. Vertical writing, user-selectable
  OpenType features beyond the script defaults, and arbitrary variable-font
  axes are not exposed yet. Fallback is chosen per shaped cluster, so a
  fallback boundary can interrupt a complex-script cluster. Glyph masks are
  alpha-only; subpixel RGB rasterization is not used.
- SVG parse trees, QR encodings and Markdown block lists are cached by their
  full input for up to 16 entries per UI thread; their rasters use a separate
  content-addressed cache. File-backed `Image` builders still decode when
  constructed, so keep `ImageData` or a shared image handle in application
  state for expensive assets.
- Accessibility is a snapshot API (`Runtime::a11y_snapshot`). There is no
  AccessKit or platform a11y bridge.
- The tray uses a themed icon name on Linux and a letter on macOS rather than
  shipping artwork; `Tray` has no `set_icon` yet. Windows registers no icon
  image either.
- Dialog focus is not trapped inside the modal. Tabbing can still reach page
  controls; manage focus explicitly if a flow requires a hard trap.
- Multi-window state stores are per-window, and window commands are applied
  after the frame that emitted them. A newly opened window is visible to other
  windows on the following frame.
- Future-backed resources use a poll-and-park executor, not an async I/O
  reactor. Bring a reactor inside the future if the work needs one.
- Material elevation is tonal, not shadow-based: the paint layer has no blurred
  shadow primitive yet.
- Dynamic color depends on `material-color-utilities`, which has no stable
  release yet; the demo pins `1.0.0-dev.18`. `seed_from_image` samples the image and uses the public Material scorer; the
  dependency does not expose its Wu quantizer, so it ranks the sampled colors
  rather than running full Wu quantization first.
- The default typeface is the platform sans-serif; the M3 metrics and tracking
  are applied, but Roboto is not bundled.
- Resource loaders are not cancelled while blocking: `cancel` and `reload`
  invalidate stale results by generation, but a worker already blocked in a
  synchronous call still runs to completion in the background.
- The optional `view!` macro crate is not included. Method chains are the
  complete API.
- No callback queue (`ui.on_click`), reactive dependency tracking or
  `ui.local` state fast path.
- The glyph/image atlas is a single fixed 2048×2048 texture. Images are capped
  at 1024 px on their longest edge; when the atlas fills, new entries are
  skipped rather than causing a multi-page draw.
- A text node's wrap width is the width used by its last measure pass. Changing
  a text node's final width without a new measure pass can produce overflow
  instead of reflow.
- `State<T>` is single-threaded (`Rc`-based) and is not `Send` or `Sync`.
  Mutating state outside `view` requires an explicit repaint path from the
  embedder.
- State values are evicted after 600 frames in which their `ui.state` call did
  not run, unless they are declared with `ui.state_persistent`. Reading a value
  is not what keeps it alive: an evicted value resets to its initializer, so
  lift anything that must survive a conditional subtree into
  `ui.state_persistent`.
- Clipping is rectangular. Rounded corners on a clipped parent do not clip
  children to the rounded shape.
- Transforms are paint-time only and do not affect hit testing or layout.
  Exiting transition content is retained but no longer interactive.
- Colors are not managed in linear light.
- Every rendered frame rebuilds and uploads the instance buffer; there is no GPU
  damage tracking or retained scene cache.
- winit is not vendored. The backend uses the crates.io releases.
