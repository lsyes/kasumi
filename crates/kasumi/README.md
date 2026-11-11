# kasumi

`kasumi` 是应用使用的门面 crate：`Ui`、`Widget`、内置控件、`Response`、`Theme` 与运行时。

组件细节见 [`docs/GUIDE.md`](../../docs/GUIDE.md)，跨层架构见 [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md)。

## Widgets and controls

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

## Theme and animation

### Material Design 3 theme

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
