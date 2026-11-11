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

## Frame integration, rich text and microbenchmarks

### Text

`kasumi-text` is the framework's text backend. It is a separate crate so the
font database, shaping caches and rasterizer can be reused and benchmarked
without the paint layer:

- `fontdb` discovers system fonts. Generic sans/serif/mono families are picked
  once from an ordered platform preference list; named families are resolved
  through fontdb's CSS-like query.
- Each paragraph is analyzed once with swash's Unicode tables. Bidi levels
  come from `unicode-bidi`, and paragraphs are split into runs with one level
  and one script.
- Swash's cluster shaper shapes each run. A per-cluster selector checks the
  primary face first and then a category-ordered fallback list (emoji, CJK,
  generic). Font bytes load lazily, and coverage decisions are cached per
  character, so repeated CJK or emoji runs do not rescan the font database.
- Line breaking uses `unicode-linebreak` (UAX #14). Bidi reordering applies
  UAX #9 rule L2 at cluster granularity; explicit newlines become separate
  paragraphs and are stacked. Glyphs inside a cluster stay in logical order
  for correct mark placement.
- Variable weight/italic axes and faux bold/italic are derived from swash's
  synthesis table, so a fallback face keeps the requested style.
- Swash rasterizes glyph outlines, color outlines and bitmap emoji. The
  renderer packs masks and color emoji into one RGBA atlas. Masks are uploaded
  as replicated alpha, color glyphs as RGBA.
- Glyphs are emitted as textured quads in the same instance stream as rects.
- The layout cache is keyed by text, family, weight, italic, size, line
  height, letter spacing, wrapping mode and available width. The cache holds up
  to 2048 layouts and clears as a whole when full.
- Glyphs are rasterized at `font_size * scale_factor`, quantized to quarter
  pixels, and cached in the atlas by face, glyph, device size and quarter-pixel
  subpixel phase. The outline absorbs the fractional pen position so each
  glyph quad can be snapped to whole device pixels and sampled 1:1, which
  avoids the soft, top-clipped edges caused by resampling the atlas.

The CJK sample in the demo exercises the system fallback chain; on this machine
it resolves to Noto Sans CJK through fontconfig. CJK punctuation (which has
Unicode script `Common`) is routed to the CJK fallback list as well, so a mixed
paragraph does not mix punctuation from an unrelated Latin face.

### Rich text, syntax highlighting and code editing

`kasumi-text` deliberately keeps colour out of the layout. Instead, every
`ShapedGlyph` carries the UTF-8 byte range of its source cluster, so a paint
pass can resolve a colour per glyph even after bidi visual reordering.
`kasumi-core::TextContent` holds a sorted `Vec<TextSpan>` (byte range + colour,
optional background and underline); `GlyphPrimitive` already has a per-instance
colour, so highlighting needs no renderer or shader change and **never invalidates
the layout cache** — the cache key contains only shaping attributes, not colour.

`Label::spans(..)` / `Label::highlight(..)` colour a paragraph in one layout
node, preserving word wrapping and bidi across span boundaries. `TextEditor`
uses the same mechanism for token colours, current-line bands, selection
rectangles and diagnostics.

```rust
use kasumi::prelude::*;

let source = ui.state(|| "fn main() { let x = 42; }\n".to_owned());
let (response, status) = ui.add(
    TextEditor::new(&source)
        .highlighter(SyntaxHighlighter::rust(SyntaxColors::dark()))
        .height(420.0)
        .line_numbers(true)
        .tab_width(4)
        .guide_column(80),
);
// status: { line, column, line_count, selection_len, changed }
```

`Highlighter` is a plain trait (`fn highlight(&self, text: &str) -> Vec<TextSpan>`),
so tree-sitter, an LSP token stream or a hand-written lexer can be plugged in.
The built-in `SyntaxHighlighter` covers Rust, C, Python, JSON and shell.

`TextEditor` supports: line-aware mouse/keyboard selection (double-click word,
triple-click line), two-axis scrolling that follows the caret, auto-indent,
bracket/quote auto-pairing, Tab/Shift+Tab indent, `Ctrl+/` toggle comment,
`Ctrl+D` duplicate line, `Alt+↑/↓` move lines, `Ctrl+B` matching bracket,
undo/redo, IME pre-edit, clipboard, visible whitespace and trailing-whitespace
highlights. See `demo/examples/editor.rs`.

#### Editor-grade text APIs

`kasumi-text` gained the primitives an editor needs beyond measurement:

- `TextLayout`: `caret_at(byte) -> CaretPosition { x, y, line, height }`,
  `byte_at_point(text, x, y)`, `selection_rects(..)`, `caret_rect(..)`,
  `line_of_byte`, `logical_line_of_byte`, `logical_line_range`,
  `position_at` / `byte_at_position`, `logical_clusters(line)`, and
  `char_index_of_byte` / `byte_of_char_index`.
- `ShapedLine` now carries `byte_start`, `byte_end` and `logical_line`, so soft
  wraps can be told apart from hard newlines; `TextCluster` carries `advance`,
  `level`, `whitespace` and `column_byte`.
- `TextAnalysis` (UAX #29 via `unicode-segmentation`): grapheme and word
  boundaries, logical line ranges, tab-stop expansion, indent columns and
  bracket-pair matching. `expand_tabs`, `column_of_byte`, `word_range_at` and
  friends are also available as one-shot helpers.

### Text microbenchmark

```sh
cargo run --release -p kasumi-text --example bench
```

The harness warms the shaper and then measures cache hits, unique Latin
layouts, unique mixed CJK/emoji/RTL layouts and glyph rasterization. These are
medians from the release build on the development machine (Loongson-3A6000,
Linux, loongarch64):

| Workload | cosmic-text path (old) | `kasumi-text` |
|---|---:|---:|
| Cached layout lookup | ~176 ns | ~156 ns |
| Unique Latin layout, 43 chars | ~60 µs | ~21 µs |
| Unique mixed CJK/emoji/RTL layout | ~205 µs | ~39 µs |
| Glyph rasterization, per glyph | ~45 µs | ~45 µs |

The old column was measured before the dependency was removed, with an
equivalent temporary harness against the previous `TextEngine`. Rasterization
is unchanged because both engines call swash; the gain comes from analysis,
fallback and line breaking running in-process without a second shaping pass.
Font discovery still costs about 50 ms when the process starts, before the
first frame.

Assembly was evaluated and deliberately deferred. Profiles show the remaining
cost is inside swash's outline engine and font cache, not in a vectorizable
loop owned by this crate; a hand-written kernel would be architecture-specific
and would not move the dominant term. The benchmark harness is kept so a future
`asm`/SIMD feature can be justified by a measured kernel rather than added
speculatively.
