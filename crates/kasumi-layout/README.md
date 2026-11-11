# kasumi-layout

taffy 镜像、脏标记与文本测量 trait 的实现层。

## Layout

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

See also: [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) · [`docs/GUIDE.md`](../../docs/GUIDE.md).
