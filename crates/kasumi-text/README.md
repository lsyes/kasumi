# kasumi-text

An experimental cross-platform text backend for Kasumi. It replaces the
cosmic-text shaping layer with a small UI-oriented engine built on `fontdb`,
`swash`, `unicode-bidi` and `unicode-linebreak`.

Status: experimental, versioned with the framework. The public API may change.

## What it does

```text
text + TextStyle + max width
        │
        ├─ swash Unicode analysis ── script, boundaries, emoji class
        ├─ unicode-bidi ──────────── paragraph levels (non-ASCII only)
        ├─ itemize ───────────────── one script + bidi level per run
        ├─ swash cluster shaper ──── primary face, then ordered fallback
        ├─ unicode-linebreak ─────── UAX #14 line opportunities
        ├─ UAX #9 L2 ─────────────── visual cluster order per line
        └─ TextLayout { glyphs, lines, clusters, size }
                                  │
                                  └─ swash rasterizer → RasterizedGlyph
```

- Font discovery and style matching: `fontdb`. Faces load lazily from the
  platform font database; a face is registered only when shaping or
  rasterization needs it.
- Fallback: a per-cluster selector tests the primary face first, then a list
  ordered for the cluster's category (emoji, CJK, generic). Coverage results
  are cached per character. CJK punctuation is treated as CJK even though its
  Unicode script is `Common`.
- Style: variable-font weight/italic axes and faux bold/italic come from
  swash's `Attributes::synthesize`; the same synthesis is applied to shaping and
  rasterization.
- Layout: paragraphs split on `\n` keep absolute byte offsets, so caret and
  selection mapping work across explicit newlines. Wrapping uses UAX #14;
  trailing whitespace is kept in the cluster map but excluded from line width.
- Bidi: runs are shaped in logical order with the direction implied by their
  level, then reordered by UAX #9 L2 at cluster granularity. Glyphs inside a
  cluster remain in logical order for mark placement. Swash reports attached
  marks relative to the base advance in RTL runs, so the cluster advance is
  added back to keep marks over their base; positions match HarfBuzz for the
  fonts checked.
- Rasterization: swash outline, color-outline and bitmap sources; the renderer
  receives an alpha mask or RGBA pixels.

## API

`TextSystem` takes `&mut self` for layout and rasterization. Synchronization is
the caller's responsibility; `kasumi-paint` wraps it in one mutex and exposes a
frame-scoped session.

```rust
use kasumi_core::TextStyle;
use kasumi_text::TextSystem;

let mut text = TextSystem::new();
let style = TextStyle::default().size(16.0);
let layout = text.layout("Hello, 世界", &style, Some(320.0));

for glyph in &layout.glyphs {
    let mask = text.rasterize(glyph.face, glyph.glyph_id, glyph.font_size, 1.0);
}
```

`TextSystem::with_database` accepts a caller-provided `fontdb::Database` for
tests or applications that load their own fonts.

## Scope

Implemented: horizontal text, word wrap, explicit newlines, generic and named
families, weight/italic requests, CJK and emoji fallback, mixed-direction runs,
alpha and color glyphs, fractional glyph offsets, caret/selection cluster maps.

Not implemented yet: vertical writing, arbitrary OpenType feature selection,
arbitrary variable axes, PostScript-name face selection, a convenient
single-file font registration API, RGB subpixel masks, and shaping across
fallback boundaries (fallback is per cluster).

## Benchmark

```sh
cargo run --release -p kasumi-text --example bench
```

Release medians on the development machine (Loongson-3A6000, Linux,
loongarch64):

| Workload | cosmic-text path | `kasumi-text` |
|---|---:|---:|
| Cached layout lookup | ~176 ns | ~156 ns |
| Unique Latin layout, 43 chars | ~60 µs | ~21 µs |
| Unique mixed CJK/emoji/RTL layout | ~205 µs | ~39 µs |
| Glyph rasterization, per glyph | ~45 µs | ~45 µs |

The rasterization path is shared swash code, so it is unchanged. The gain comes
from cluster-level fallback, single-pass Unicode itemization and avoidable
per-frame work.

## On assembly

No hand-written assembly is included. Profiling the new path leaves the
remaining hot time in swash's outline engine and font loading; there is no
crate-owned loop where an architecture-specific kernel would beat the compiler
by a meaningful margin. Adding one now would also compromise the
cross-platform story (the development machine is loongarch64) for no measured
gain. The benchmark exists so a future `asm` or SIMD feature can target a
concrete kernel — for example mask expansion or color-glyph swizzling — when it
shows up as a bottleneck.
