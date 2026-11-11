# widgets implementation notes

内置 `Widget` 实现与滚动 / 列表内容。完整组件指南见 [`docs/GUIDE.md`](../../../../docs/GUIDE.md)。

## Scrolling and rich content

Scroll containers retain a rendered offset and a pending destination. Wheel
input follows Chromium's scroll-offset animation: each notch adds 120 logical
pixels to the target, and the rendered offset eases toward it with an
ease-in-out cubic Bézier whose duration runs from 200 ms for a single notch
down to 100 ms for long targets. Further notches retarget the running segment
from its current position and velocity, so rapid input accumulates instead of
restarting. High-resolution pixel deltas apply immediately because the device
already produced smooth motion, and scrollbar drags update the offset directly
while cancelling any wheel animation. Keyboard arrows, Page Up/Down and
Home/End scroll focused viewports with the same eased segment. Hit testing and
painting apply ancestor scroll offsets without re-running layout.

Wheel routing stops at the nearest scrollable ancestor that has overflow on the
requested axis. Once that viewport captures the event it keeps it at its edges
instead of chaining the gesture into the page behind it, so scrolling a nested
list cannot move the surrounding page. A nested viewport whose content fits has
no scroll range and is skipped, leaving the page scrollable through it.

### Search bar and search view

`SearchBar` is the M3 pill-shaped field with the leading magnifier, an inline
results popup and an optional trailing clear button:

```rust
let search = SearchBar::new(&query)
    .placeholder("Search commands")
    .results([
        SearchResult::new("Overview").description("the starting page"),
        SearchResult::new("Lists"),
    ]);

let outcome = ui.search(&search);
if let Some(title) = outcome.activated {
    run_command(title);
}
```

Results are filtered case-insensitively against both the title and the
description. `↑`/`↓` move the highlight, `Enter` activates it, a click on a row
does the same, and `Escape` closes the popup and clears the field. A press
outside the field and its results dismisses the popup; pressing inside keeps it
mounted until release, which is what lets a row's own click fire.

The field reuses the text-input editing pipeline (`apply_edits`), so selection,
undo, IME pre-edit and clipboard shortcuts behave exactly as they do in a
`TextInput`, without duplicating that code.

`SearchBar::view(true)` renders the full-window Material *search view*: a
`surface-container-high` sheet over the page with the field docked at the top
and the results as a list below. It is a root overlay that swallows clicks
behind it.

Layout follows the M3 search tokens: a 56 dp bar with a full-radius shape,
`surface-container-high` container, 24 dp icons, `body-large` input text and
56 dp result rows. Gaps between the bar and the popup, and the popup's
`extra-large` corners, match the menu surface.

### Material lists

`List` renders the three Material row variants from declarative data:

```rust
let list = List::new()
    .header("This week")
    .row(ListRow::new("Design review")
        .supporting("Today, 09:30 · 4 comments")
        .leading_avatar("DR")
        .trailing("09:30"))
    .divider()
    .row(ListRow::new("Deploy pipeline")
        .supporting("main · 3 commits ahead")
        .leading_icon("↑")
        .trailing_icon("›"))
    .row(ListRow::new("Release notes")
        .supporting("A longer description")
        .height(ListRowHeight::ThreeLine));

if let Some(index) = ui.list(&list).clicked {
    open(index);
}
```

Rows resolve to 56, 72 or 88 dp. A supporting line promotes an unspecified
one-line row to two-line, which is what the guidelines call for, while an
explicit `height` always wins. The leading slot takes a 24 dp glyph or a 40 dp
circular avatar, the trailing slot a caption and/or a glyph, and the text
column keeps title and supporting text in one aligned block. Selection paints
the `secondary-container` container with `on-secondary-container` content;
disabled rows fade to 38 % and ignore presses.

`clicked` reports the row index *without* counting headers or dividers, so it
always matches `List::row_count()` indexing.

`List::virtualize(true)` declares only the elements that intersect the
scroll window. Row heights vary between elements, so the window is found by
binary search over the prefix offsets rather than by division; a 10,000-row
mixed list declares a few dozen nodes. The unfiltered path stays eager because
short lists keep local widget state across frames that way.

### Virtualized lists

`ScrollView::virtual_content(width, height)` declares the full scrollable
extent without laying out children outside the viewport. Pair it with
`Ui::virtual_scroll`, whose closure receives a `ScrollWindow` describing the
retained offset, the viewport from the previous layout pass and a clamped
`row_range` helper. Only the declared children exist in the tree, while the
scrollbar and wheel range still use the full extent.

`ListView::new(row_count, row_height, |ui, index| ..)` packages the common
fixed-height case. Rows are absolutely positioned at `index * row_height`
inside one content node; the list adds an overscan band (240 logical pixels by
default) so a fast scroll does not reveal an empty frame while the next layout
runs. Because off-screen rows are recycled immediately, local widget state in a
row is not retained across virtual eviction; lift shared values into
`State<T>`. Clipped nested scroll content is excluded from an ancestor's
scrollable overflow, so a 10,000-row virtual list nested inside a page does not
make the page believe it is 10,000 rows tall.

```rust
ui.add(
    ListView::new(rows.len(), 48.0, |ui, index| {
        ui.row(|ui| {
            ui.width_full().height_full().padding_xy(16.0, 0.0);
            ui.label(&rows[index].title);
        });
    })
    .size(Length::Percent(100.0), 320.0),
);
```

Programmatic scrolling uses `Ui::scroll_to(&response, Point::new(x, y))` or
`Ui::scroll_by(&response, Vec2::new(dx, dy))`. The latter keeps the smooth
wheel animation; the former cancels it and clamps to the retained range.

Images are decoded through `image` and uploaded once by content id. SVG is
rasterized through resvg/tiny-skia, Markdown is parsed with pulldown-cmark and
QR codes are generated from the `qrcode` encoder. All of these produce ordinary
RGBA atlas entries and remain in the same ordered draw list as text.
