# kasumi-backend

winit 输入 / 剪贴板 / IME 与互斥的 glow / wgpu 渲染后端。

## Rendering

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

## Performance

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

### Reproducible benchmark

```sh
cargo bench -p kasumi --bench frames
```

The dependency-free harness in
[`../kasumi/benches/frames.rs`](../kasumi/benches/frames.rs) warms the
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

See also: [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) · [`docs/GUIDE.md`](../../docs/GUIDE.md).
