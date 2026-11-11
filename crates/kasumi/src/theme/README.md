# theme implementation notes

M3 token 数据及动态颜色实现。完整主题指南见 [`docs/GUIDE.md`](../../../../docs/GUIDE.md)。

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
