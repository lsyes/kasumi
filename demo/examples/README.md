# Kasumi examples

Every `.rs` file in this directory is an independent binary target. Run any of
them from the workspace root:

```sh
cargo run -p kasumi-demo --example <name>
```

Add `--release` for a smoother interactive run, or select the OpenGL backend
explicitly:

```sh
cargo run --release -p kasumi-demo --example <name> --no-default-features --features glow
```

`shared/mod.rs` is the only non-target file: a small module pulled in with
`#[path = "shared/mod.rs"] mod shared;`. It contains just page scaffolding and
label helpers — **no framework widget is wrapped there**, so every example shows
the real API at its call site.

Each example's top doc comment explains the concept and the APIs it uses. They
are meant to be copied: delete everything except the block you need.

## Start here

| Example | What it teaches |
|---|---|
| [`counter.rs`](counter.rs) | The minimal application: `App::view`, `ui.state`, `Response::clicked`. |
| [`state_and_ids.rs`](state_and_ids.rs) | Why values reset: positional ids, `ui.keyed`, conditional branches, TTL eviction. |
| [`custom_widget.rs`](custom_widget.rs) | `impl Widget`, `ui.clickable()`, custom `Output` types. |
| [`todo.rs`](todo.rs) | A complete small app: one `State<Vec<_>>`, keyed rows, derived counts. |

## Layout and text

| Example | What it teaches |
|---|---|
| [`layout.rs`](layout.rs) | `Justify`, `Align`, `grow/basis`, grid tracks and spans, `ui.stack`, `ui.wrap_row`, `absolute`. |
| [`typography.rs`](typography.rs) | The 15 M3 type roles, weights, families, `Label::width` and wrapping, per-cluster fallback, `ui.texts().measure`. |
| [`editor.rs`](editor.rs) | `TextEditor` + `SyntaxHighlighter`: multi-line editing, line numbers, auto-indent, bracket pairs, `Label::spans`, `TextAnalysis`/`TextLayout` IDE APIs. |
| [`lists.rs`](lists.rs) | `List`/`ListRow`, 10,000 virtualized rows, `ListView`, horizontal and bidirectional `ScrollView`. |
| [`virtual_scroll.rs`](virtual_scroll.rs) | `Ui::virtual_scroll` with variable-height rows and a prefix-sum offset table. |
| [`table.rs`](table.rs) | A 50,000-row data table with a sticky header, sorting and filtering. |
| [`charts.rs`](charts.rs) | Bars, sparklines, a heatmap and a concentric donut built only from containers. |

## Controls and forms

| Example | What it teaches |
|---|---|
| [`controls.rs`](controls.rs) | The five button emphases, `Switch`, `Checkbox`, `Radio`, `Slider`, `ProgressBar` and disabled states. |
| [`form.rs`](form.rs) | `TextInput` bound to `State<String>`, live validation, `ComboBox`, a confirmation dialog. |
| [`menus.rs`](menus.rs) | `MenuButton`, arbitrarily nested submenus, context menus, selection paths, tooltips. |
| [`navigation.rs`](navigation.rs) | `NavigationDrawer` in all four variants, sections and collapsible groups. |
| [`search.rs`](search.rs) | `SearchBar` inline results and the full-window search view. |
| [`dialogs.rs`](dialogs.rs) | `Dialog` scrims, Escape/scrim dismissal, `dismissible(false)`, dialog bodies. |

## Themes, media and motion

| Example | What it teaches |
|---|---|
| [`themes.rs`](themes.rs) | HCT dynamic color from a seed, scheme variants, contrast levels, every color role, shape/spacing tokens. |
| [`media.rs`](media.rs) | `Image` and `ImageFit`, resvg `Svg`, `QrCode`, CommonMark `Markdown`. |
| [`animation.rs`](animation.rs) | `animate`, `animate_with`, springs and easing curves, `ui.transition` presence, opacity/scale/translate. |
| [`input.rs`](input.rs) | `ui.input()`, `key_pressed`, `key_modifiers`, every `Response` flag, event-driven frames. |

## Platform and async

| Example | What it teaches |
|---|---|
| [`async_resources.rs`](async_resources.rs) | `ui.resource` / `resource_future_keyed`, loading/ready/error, stale-while-revalidate, reload and cancel. |
| [`windows.rs`](windows.rs) | Multi-window: one `App`, a `Runtime`/`StateStore` per window, shared data on `self`. |
| [`tray.rs`](tray.rs) | `TrayMenu` snapshots, `TrayEvent`s per frame, checkable items, `TrayStatus`. |

## Small applications

| Example | What it teaches |
|---|---|
| [`calculator.rs`](calculator.rs) | A keypad grid, a pure `State<Calc>` model, physical keyboard shortcuts. |
| [`todo.rs`](todo.rs) | Add / filter / toggle / delete, keyed rows, derived counts. |

## Verifying the examples

All targets are compile-checked with the rest of the workspace:

```sh
cargo build -p kasumi-demo --examples
```

On a machine with a display, each one can be run headlessly for a few frames
using the diagnostics the main demo supports:

```sh
KASUMI_STATS=1 KASUMI_EXIT_AFTER_FRAMES=6 \
    cargo run -q -p kasumi-demo --example counter
```

`KASUMI_STATS=1` prints per-frame node and layout counts, which is how the
virtualization examples demonstrate a flat node count. `KASUMI_EXIT_AFTER_FRAMES`
quits after N rendered frames.

## Environment

The examples need the same system fontconfig fonts and OpenGL 3.3 / Vulkan
adapter as the main demo. See the [root README](../../README.md#quick-start).
