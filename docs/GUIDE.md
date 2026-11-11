# Kasumi component guide

本文件汇总内置组件、主题、列表与文本的详细用法。每个主题都有 `demo/examples/` 下可直接运行的独立示例；入门与构建命令见根 README。

## Widgets, forms and platform surfaces

### Menus

`Menu` is declarative data, not callbacks: items, shortcuts, check marks and
arbitrarily nested submenus. The menu is retained under the anchor widget's id
and activation reports the path through the tree, so one `match` dispatches
every command:

```rust
let menu = Menu::new()
    .item(MenuItem::new("Open").shortcut("Ctrl+O"))
    .submenu("Export", Menu::new().item("PNG").item("SVG"))
    .separator()
    .item(MenuItem::new("Delete").enabled(false));

if let Some(selection) = ui.menu(&anchor, menu) {
    match selection.path.as_slice() {
        [0] => open(),
        [1, 0] => export_png(),
        [1, 1] => export_svg(),
        _ => {}
    }
}
```

`ui.context_menu(&response, menu)` opens the same menu at the pointer on a
right click, and `MenuButton::new("File", menu)` packages the button and the
menu into one widget.

Submenus cascade to the right of their parent row. Hovering a parent opens it
after `Menu::hover_delay` (400 ms by default, matching Material), clicking it
toggles the level immediately, and `→` / `←` or `Enter` / `Space` / `Escape`
drive the same state from the keyboard. Panels are placed from a root overlay
so they paint above page content, and a full-window scrim swallows presses that
land outside the menu; the menu closes in that frame and the press does not
reach the widget underneath. A menu whose parent row disappears is dropped by
`Tree::end_frame`, so nothing leaks.

Menus use the M3 menu tokens: surface-container at elevation level 2, an
extra-small (4 dp) corner radius, 48 dp rows with 16 dp padding, a leading
check-mark column when any row is checkable, right-aligned shortcut hints, and
a 1 dp outline-variant border. Panels enter with a fade and an 80 %→100 % scale
spring from their top edge, the Material menu entrance.

### Combo boxes

`ComboBox` is the M3 exposed dropdown menu. The popup now follows the Material
motion rather than a generic spring: opening scales the panel from 0.8 to 1.0
with a fast spatial spring from its **top edge** — the edge that meets the
field — while a 150 ms emphasized-accelerate tween drives the close. The panel
stays mounted for the exit animation but its rows stop accepting input the
moment the popup closes, and it flips above the field when there is no room
below, in which case the scale origin follows to the bottom edge.

Rows are 48 dp with 8 dp vertical panel padding, a 24 dp leading column
reserved for the selected row's check mark, and `secondary-container` on the
selected row. The trailing chevron cross-fades between the down and up glyphs
instead of swapping at a threshold, which reads as Material's icon rotation
without transforming shaped text.

Keyboard support matches a native select: `Enter`/`Space` open the popup,
`↑`/`↓` move the selection live, `Home`/`End` jump to the ends, and `Escape`
closes. Clicking anywhere outside the field and its panel dismisses it.

### Navigation drawers and rails

`NavigationDrawer` describes destinations once and renders them as a
permanent sidebar, a modal drawer or a rail:

```rust
let nav = NavigationDrawer::new()
    .header(NavHeader::new("kasumi").subtitle("0.3.0"))
    .item(NavItem::new("overview", "Overview").icon("◉"))
    .section("Data", |section| {
        section
            .item(NavItem::new("lists", "Lists"))
            .item(NavItem::new("search", "Search"))
    })
    .item(
        NavItem::new("settings", "Settings")
            .child(NavItem::new("theme", "Theme")),
    )
    .footer(NavItem::new("theme-mode", "Dark theme").icon("◐"))
    .selected(&current);

if let Some(key) = ui.navigation(&nav).selected {
    navigate(key);
}
```

Items are identified by a stable key, and the drawer reports the activated
key instead of running callbacks. Sections render a `title-small` heading (or a
rule in rail mode), the footer is pinned below the scrolling list, and a
parent item toggles its group without navigating. Nested groups keep the flat
Material row metrics and gain one `12 dp` step of indentation per level, so a
deep tree never shrinks its rows; a group's chevron animates with the same
spring as the row state.

Passing `.selected(key)` paints the active indicator and expands the
selection's ancestors on the first frame, so a selection restored from
persisted state is visible immediately.

The visual treatment follows the M3 drawer and rail tokens: 56 dp rows with a
full-radius `secondary-container` indicator inside a 12 dp inset, icons and
labels in `on-secondary-container` when selected and `on-surface-variant`
otherwise, a 16 dp corner radius on the modal drawer's trailing edge, and a
96 dp rail (`NavVariant::Rail`) that centers icons and keeps labels in the
accessibility tree only. The drawer's list scrolls without a persistent
scrollbar, which is what Material specifies; `NavigationDrawer::scrollbars(true)`
opts back in.

The demo's sidebar is now one `NavigationDrawer` with two sections, a
collapsible group and a theme-mode footer entry.

### Cards and dialogs

`Ui::card(Card::new(), |ui| ..)` declares a themed column container. `Card`
selects the elevated, filled or outlined treatment and controls padding, gap,
size and radius. Elevated cards use a tonal surface plus a hairline border;
the renderer does not implement shadows yet. A clickable card gets a pointer
cursor, keyboard focus and a previous-frame hover tint.

`Ui::dialog(Dialog::new(&open).title(..), |ui| ..)` renders a modal through the
root overlay layer while the `State<bool>` is true. The widget paints a
full-window scrim that captures pointer and wheel input before the page below
and centers a card. It closes on Escape or a scrim click when `dismissible` is
enabled. Keyboard focus is not trapped; that is deliberately left to the
application, and dialog bodies are ordinary immediate-mode content.

Dialogs are real retained nodes only while open. Closing the state removes the
overlay on the next frame, so no hidden dialog tree keeps consuming layout or
paint time.

### Multiple windows

The winit backend can run many windows against one shared `App`:

```rust
fn view(&mut self, ui: &mut Ui) {
    if ui.window().is_primary() {
        if ui.add(Button::new("Inspect")).clicked() {
            ui.open_window(
                WindowRequest::new("Inspector").size(560.0, 700.0),
            );
        }
    } else {
        // The same application object, rendering a different window.
        let key = ui.window().key;
        if ui.add(Button::new("Close")).clicked() {
            ui.close_window(key);
        }
    }
}
```

Each window owns a `Runtime`, retained tree, layout engine, GPU surface and
`StateStore`. That isolation is intentional: a `State<T>` created in the main
window is not shared with a secondary window. Share cross-window data through
application fields (for example an `Arc<Mutex<_>>`) or the window's
`App::view` itself. `Ui::windows()` returns a snapshot sorted by `WindowKey`
with title, logical size, scale factor, primary/focused flags. Window actions
are executed by the backend after the current frame is presented; a newly
opened window first appears on a later frame. Closing the last window exits
the event loop.

The single-window `run(app, WindowOptions)` entry point is unchanged; it uses
`WindowKey::MAIN` and the same multi-window machinery internally.

### System tray

`WindowOptions::tray` registers a tray icon before the first frame:

```rust
let menu = TrayMenu::new("Kasumi")
    .item(TrayMenuItem::new("show", "Show Kasumi"))
    .separator()
    .item(
        TrayMenuItem::new("pages", "Open page")
            .children(pages.iter().map(|page| TrayMenuItem::new(page.key, page.title))),
    )
    .item(TrayMenuItem::new("theme", "Dark theme").checkbox(true))
    .separator()
    .item(TrayMenuItem::new("quit", "Quit"))
    .on_activate("show");

run(App, WindowOptions { tray: Some(menu), ..Default::default() });
```

Activations arrive in `view` through `Ui::tray_events()`, once each, exactly
like pointer or keyboard input — so the application stays a single `view`
function with no callback plumbing:

```rust
for event in ui.tray_events().to_vec() {
    if let TrayEvent::Activated { id } = event {
        match id.as_str() {
            "theme" => toggle_theme(),
            "quit" => quit(),
            other => navigate(other),
        }
    }
}
```

`Ui::set_tray_menu(menu)` replaces the menu; the backend applies it after the
frame. Checkable entries keep their state across replacements, and
`Ui::tray_status()` reports `Active`, `Unavailable(reason)` or `Disabled` so an
application can adapt when the desktop has no status area.

The three platforms share `TrayMenu`/`TrayMenuItem`/`TrayEvent` from the facade
crate and each own a small implementation in `kasumi-backend`:

| Platform | Backend | Notes |
|---|---|---|
| Linux | `ksni` over D-Bus | StatusNotifierItem; no GTK or libappindicator. Needs a `StatusNotifierWatcher` (KDE, XFCE, Cinnamon, Budgie, waybar); stock GNOME needs the AppIndicator extension. |
| Windows | `Shell_NotifyIconW` | Hidden top-level window and `TrackPopupMenu`, so the icon survives an Explorer restart; `WM_APP` callback posts to the event loop. |
| macOS | `NSStatusBar` + `NSMenu` | AppKit menus are main-thread only, so the status item is created during `resumed`; each item targets a small `NSObject` subclass holding a closure. |

The Linux service runs on a worker thread and wakes the winit loop through the
same `EventLoopProxy` the async resources use, so an idle window renders one
frame per activation rather than polling. The tray is torn down on exit, which
unregisters the D-Bus object and removes the status item.

### Async resources

`Ui::resource_keyed(key, loader)` returns a cloneable `Resource<T>` and starts
`loader` on a small background executor the first time that key is declared.
The UI thread never blocks. Position-based `Ui::resource(loader)` exists too,
but keyed resources survive conditional branches and reordered siblings.

```rust
let users = ui.resource_keyed("users", || {
    api::fetch_users().map_err(|error| error.to_string())
});
match users.status() {
    ResourceStatus::Loading if users.has_value() => {
        ui.label("Refreshing…");
    }
    ResourceStatus::Loading => {
        ui.label("Loading…");
    }
    ResourceStatus::Failed => {
        if let Some(error) = users.error() {
            ui.label(format!("Failed: {error}"));
        }
    }
    ResourceStatus::Ready => { /* users.with(|users| ..) */ }
    ResourceStatus::Idle => {}
}
if ui.button("Retry").clicked() {
    users.reload();
}
```

A reload keeps the previous value available while `status()` is `Loading`, so
UIs can show stale data during a refresh. `cancel()` invalidates an in-flight
load and clears the value; `set(value)` installs an optimistic value. Loaders
must be `Fn() -> Result<T, E> + Send + Sync + 'static` because a resource can be
retried, and the error must implement `Display` so it can be flattened into a
`ResourceError` and moved back to the UI thread. Values are `Send + 'static`.

`Ui::resource_future_keyed` accepts a future factory instead. The shared
executor polls the future with a thread-unpark waker. It is a plain executor,
not an I/O reactor: futures that depend on tokio or another reactor must bring
that runtime with them.

`ResourceStore` caches handles by id and expires slots that have not been
declared for the resource TTL (600 frames by default,
`Runtime::set_resource_ttl_frames`). External clones stay valid after
eviction; declaring the slot again starts a fresh load.

Completion signaling is event-driven. Each runtime installs a wake callback
through `set_resource_wake_callback`; the winit backend wraps an
`EventLoopProxy` so a worker finishing while every window is idle wakes the
event loop and schedules exactly one redraw. No polling timer is involved.

### Switches

`Switch` is the M3 toggle bound to `State<bool>`, optionally with a label and a
supporting line:

```rust
ui.switch(&enabled);

ui.add(
    Switch::new(&notifications)
        .label("Notifications")
        .supporting("Push alerts for new comments"),
);
```

The widget resolves its geometry and colors once per frame and stores them on
the track node; the paint layer replays them. That keeps the theme crate out of
`kasumi-paint` and keeps the switch independent of the glyph atlas.

Geometry and colors follow the M3 tokens:

| Part | Unselected | Selected |
|---|---|---|
| Track | 52×32, `surface-variant`, 2 dp `outline` | 52×32, `primary`, no border |
| Thumb | 16 dp, `outline` | 24 dp, `on-primary`, 4 dp from the trailing edge |
| Pressed thumb | 28 dp, flush with the leading edge | 28 dp, flush with the trailing edge |
| State layer | 40 dp `on-surface` | 40 dp `primary` |
| Icons | optional 16 dp | optional 16 dp `on-primary-container` |

The thumb travels between an 8 dp leading inset and a 4 dp trailing inset as
the 250 ms standard-curve animation runs, and grows from 16 to 28 dp while
pressed, toward the edge it is heading to. The state layer is hover 8 % and
press 10 %, and a disabled switch fades to the 38 % disabled content role.

Clicking anywhere in the row toggles it, Space toggles it when focused, and the
arrow keys move the thumb (with `keyboard_arrows_move_thumb`, on by default)
so a switch keeps the desktop convention instead of behaving like a checkbox.
`Response::changed` reports the toggle.

## Material Design 3 theme

`Theme` is plain data, not a global singleton. Build one of the baseline
schemes, derive one from a seed color, or assemble a custom scheme:

```rust
use kasumi::{ColorScheme, Theme, ThemeMode};

let light = Theme::light();
let dark = Theme::dark();
let seeded = Theme::from_seed(Color::hex("#006B5E"), ThemeMode::Dark);
let custom = Theme::from_scheme(ColorScheme::baseline_dark(), ThemeMode::Dark)
    .with_font_family(FontFamily::Named("Noto Sans".into()));
```

Every color role follows the `md.sys.color.*` naming used by the
[Material 3 color system](https://m3.material.io/styles/color/overview):
primary, secondary, tertiary, error and their on/container/fixed variants;
surface, surface-dim/bright and the five surface-container roles; on-surface,
outline, outline-variant, inverse roles, scrim and shadow. `background` and
`on-background` are aliases of the surface roles for older code. `success` is
a Kasumi extension because M3 does not define a normative success role.

`Theme::from_seed` runs the real Material algorithm through
[`material-color-utilities`](https://crates.io/crates/material-color-utilities):
the seed becomes an HCT color, the tonal palettes are derived from its hue and
chroma, and every role is resolved through its contrast curve. Pick a strategy
and a contrast level with `Theme::from_seed_with`:

```rust
use kasumi::{Color, ContrastLevel, DynamicColorConfig, SchemeVariant};

let theme = Theme::from_seed_with(
    Color::hex("#006B5E"),
    ThemeMode::Dark,
    DynamicColorConfig::new()
        .variant(SchemeVariant::Vibrant)
        .contrast(ContrastLevel::High),
);
```

`DynamicScheme` is the same value without the surrounding typography: it holds
the seed, the mode and the config, and `color_scheme()` resolves the roles on
demand. `seed_from_image(width, height, rgba)` picks a seed from an image the
way Material You picks one from a wallpaper: the pixels are sampled into a color histogram and the Material scorer ranks
those colors. The function returns `Option<Color>`; the dependency does not
expose its Wu quantizer through the public API.

The type scale contains all 15 baseline roles from the
[M3 typography tokens](https://m3.material.io/styles/typography/type-scale-tokens):
display large/medium/small, headline, title, body and label. Each role carries
the official size, line height, weight and tracking (expressed in logical
pixels and converted to em at shaping time). Apply a role with
`Label::style(theme.typography.headline_medium.clone())`; per-label overrides
still win. Typography is generated with the on-surface color and can be
recolored with `Typography::with_color`.

Shape, elevation, motion and state layers are exposed alongside the colors:

```rust
theme.shape.extra_large;        // 28 dp
theme.elevation.level3;         // 6 dp
theme.surface_at_elevation(2);  // surface-container
theme.motion.duration.short4;   // 200 ms
theme.motion.easing.emphasized; // cubic-bezier(0.2, 0, 0, 1)
theme.state.hover;              // 0.08
```

The paint layer has no shadow primitive yet, so elevated components use tonal
surface containers instead of real shadows. `Card` maps to surface-container-low
(elevated), surface-container-highest (filled) and surface + outline
(outlined). Dialogs use surface-container-high with a 28 dp radius and a 32%
scrim. Buttons expose filled, tonal, outlined, text and elevated emphasis plus
the legacy `primary`/`secondary`/`subtle` aliases. Menus, tooltips, text fields,
checkboxes, radios, sliders, progress indicators and scrollbars all read the
same roles.

`Ui::set_theme(theme)` changes the active theme for the current window. Widgets
declared after the call use the new tokens immediately, and a follow-up frame
repaints the nodes declared before it. Each window has its own theme, so a
secondary inspector can stay light while the main window is dark.

The demo's typography page renders the type scale and the color roles; the
surfaces page shows button variants, shape tokens and elevation tiles. Toggle
`KASUMI_DEMO_DARK=1` or press the sidebar button to inspect both schemes.

## Scrolling, lists and rich content

### Scrolling and rich content

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

#### Search bar and search view

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

#### Material lists

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

#### Virtualized lists

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

## Text backend

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

#### Rich text, syntax highlighting and code editing

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

##### Editor-grade text APIs

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

#### Text microbenchmark

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
