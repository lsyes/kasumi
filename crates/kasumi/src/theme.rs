//! Material Design 3 theme tokens.
//!
//! The theme is plain data. It carries the M3 color roles, the 15 baseline
//! type-scale styles, shape, elevation, motion, state-layer opacities and a
//! small 4 dp spacing scale. Widgets copy the values they need into retained
//! node styles, so a theme change applies to nodes declared afterwards.
//!
//! `Theme::light` and `Theme::dark` use the official baseline scheme.
//! `Theme::from_seed` derives an approximate dynamic scheme in OKLCH space;
//! the official HCT algorithm is intentionally not reimplemented here.

use std::time::Duration;

use kasumi_core::{Color, Corners, FontFamily, FontWeight, TextStyle, TextWrap};

use crate::dynamic_color::{DynamicColorConfig, DynamicScheme};
use kasumi_tree::{Easing, MotionSpec};

/// Light or dark Material color scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }
}

/// Material Design 3 color roles.
///
/// Field names follow the `md.sys.color.*` tokens with hyphens replaced by
/// underscores. `background`/`on_background` are retained aliases of
/// `surface`/`on_surface`. `success` is a Kasumi extension because M3 has no
/// normative success role.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorScheme {
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub primary_fixed: Color,
    pub primary_fixed_dim: Color,
    pub on_primary_fixed: Color,
    pub on_primary_fixed_variant: Color,

    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub secondary_fixed: Color,
    pub secondary_fixed_dim: Color,
    pub on_secondary_fixed: Color,
    pub on_secondary_fixed_variant: Color,

    pub tertiary: Color,
    pub on_tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    pub tertiary_fixed: Color,
    pub tertiary_fixed_dim: Color,
    pub on_tertiary_fixed: Color,
    pub on_tertiary_fixed_variant: Color,

    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,

    pub surface: Color,
    pub on_surface: Color,
    pub surface_dim: Color,
    pub surface_bright: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub surface_variant: Color,
    pub on_surface_variant: Color,
    pub surface_tint: Color,

    pub background: Color,
    pub on_background: Color,

    pub outline: Color,
    pub outline_variant: Color,
    pub shadow: Color,
    pub scrim: Color,

    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub inverse_primary: Color,

    /// Kasumi extension for positive status.
    pub success: Color,
    pub on_success: Color,
    pub success_container: Color,
    pub on_success_container: Color,
}

impl ColorScheme {
    /// The Material 3 baseline light scheme.
    pub fn baseline_light() -> Self {
        Self::from_roles(
            (
                "#6750A4", "#FFFFFF", "#EADDFF", "#21005D", "#EADDFF", "#D0BCFF", "#21005D",
                "#4F378B",
            ),
            (
                "#625B71", "#FFFFFF", "#E8DEF8", "#1D192B", "#E8DEF8", "#CCC2DC", "#1D192B",
                "#4A4458",
            ),
            (
                "#7D5260", "#FFFFFF", "#FFD8E4", "#31111D", "#FFD8E4", "#EFB8C8", "#31111D",
                "#633B48",
            ),
            ("#B3261E", "#FFFFFF", "#F9DEDC", "#410E0B"),
            LightSurfaces {
                surface: "#FEF7FF",
                on_surface: "#1D1B20",
                surface_dim: "#DED8E1",
                surface_bright: "#FEF7FF",
                lowest: "#FFFFFF",
                low: "#F7F2FA",
                container: "#F3EDF7",
                high: "#ECE6F0",
                highest: "#E6E0E9",
                variant: "#E7E0EC",
                on_variant: "#49454F",
                outline: "#79747E",
                outline_variant: "#CAC4D0",
                inverse_surface: "#322F35",
                inverse_on_surface: "#F5EFF7",
                inverse_primary: "#D0BCFF",
            },
            ("#2E6B30", "#FFFFFF", "#B1F0B2", "#002106"),
        )
    }

    /// The Material 3 baseline dark scheme.
    pub fn baseline_dark() -> Self {
        Self::from_roles(
            (
                "#D0BCFF", "#381E72", "#4F378B", "#EADDFF", "#EADDFF", "#D0BCFF", "#21005D",
                "#4F378B",
            ),
            (
                "#CCC2DC", "#332D41", "#4A4458", "#E8DEF8", "#E8DEF8", "#CCC2DC", "#1D192B",
                "#4A4458",
            ),
            (
                "#EFB8C8", "#492532", "#633B48", "#FFD8E4", "#FFD8E4", "#EFB8C8", "#31111D",
                "#633B48",
            ),
            ("#F2B8B5", "#601410", "#8C1D18", "#F9DEDC"),
            LightSurfaces {
                surface: "#141218",
                on_surface: "#E6E0E9",
                surface_dim: "#141218",
                surface_bright: "#3B383E",
                lowest: "#0F0D13",
                low: "#1D1B20",
                container: "#211F26",
                high: "#2B2930",
                highest: "#36343B",
                variant: "#49454F",
                on_variant: "#CAC4D0",
                outline: "#938F99",
                outline_variant: "#49454F",
                inverse_surface: "#E6E0E9",
                inverse_on_surface: "#322F35",
                inverse_primary: "#6750A4",
            },
            ("#9CD49A", "#00390D", "#15521F", "#B1F0B2"),
        )
    }

    /// The surface-container role associated with an M3 elevation level.
    pub fn surface_at_elevation(&self, level: u8) -> Color {
        match level {
            0 => self.surface,
            1 => self.surface_container_low,
            2 => self.surface_container,
            3 => self.surface_container_high,
            _ => self.surface_container_highest,
        }
    }

    /// Disabled glyph/base color (`on-surface` at 38%).
    pub fn disabled_on_surface(&self) -> Color {
        self.on_surface.fade(0.38)
    }

    /// Disabled container fill (`on-surface` at 12%).
    pub fn disabled_container(&self) -> Color {
        self.on_surface.fade(0.12)
    }

    /// A standard state layer over `base`, with the given opacity.
    pub fn state_layer(&self, layer: Color, opacity: f32) -> Color {
        layer.fade(opacity.clamp(0.0, 1.0))
    }

    #[allow(clippy::too_many_arguments)]
    fn from_roles(
        primary: (&str, &str, &str, &str, &str, &str, &str, &str),
        secondary: (&str, &str, &str, &str, &str, &str, &str, &str),
        tertiary: (&str, &str, &str, &str, &str, &str, &str, &str),
        error: (&str, &str, &str, &str),
        surfaces: LightSurfaces,
        success: (&str, &str, &str, &str),
    ) -> Self {
        let surface = Color::hex(surfaces.surface);
        let on_surface = Color::hex(surfaces.on_surface);
        Self {
            primary: Color::hex(primary.0),
            on_primary: Color::hex(primary.1),
            primary_container: Color::hex(primary.2),
            on_primary_container: Color::hex(primary.3),
            primary_fixed: Color::hex(primary.4),
            primary_fixed_dim: Color::hex(primary.5),
            on_primary_fixed: Color::hex(primary.6),
            on_primary_fixed_variant: Color::hex(primary.7),
            secondary: Color::hex(secondary.0),
            on_secondary: Color::hex(secondary.1),
            secondary_container: Color::hex(secondary.2),
            on_secondary_container: Color::hex(secondary.3),
            secondary_fixed: Color::hex(secondary.4),
            secondary_fixed_dim: Color::hex(secondary.5),
            on_secondary_fixed: Color::hex(secondary.6),
            on_secondary_fixed_variant: Color::hex(secondary.7),
            tertiary: Color::hex(tertiary.0),
            on_tertiary: Color::hex(tertiary.1),
            tertiary_container: Color::hex(tertiary.2),
            on_tertiary_container: Color::hex(tertiary.3),
            tertiary_fixed: Color::hex(tertiary.4),
            tertiary_fixed_dim: Color::hex(tertiary.5),
            on_tertiary_fixed: Color::hex(tertiary.6),
            on_tertiary_fixed_variant: Color::hex(tertiary.7),
            error: Color::hex(error.0),
            on_error: Color::hex(error.1),
            error_container: Color::hex(error.2),
            on_error_container: Color::hex(error.3),
            surface,
            on_surface,
            surface_dim: Color::hex(surfaces.surface_dim),
            surface_bright: Color::hex(surfaces.surface_bright),
            surface_container_lowest: Color::hex(surfaces.lowest),
            surface_container_low: Color::hex(surfaces.low),
            surface_container: Color::hex(surfaces.container),
            surface_container_high: Color::hex(surfaces.high),
            surface_container_highest: Color::hex(surfaces.highest),
            surface_variant: Color::hex(surfaces.variant),
            on_surface_variant: Color::hex(surfaces.on_variant),
            surface_tint: Color::hex(primary.0),
            background: surface,
            on_background: on_surface,
            outline: Color::hex(surfaces.outline),
            outline_variant: Color::hex(surfaces.outline_variant),
            shadow: Color::BLACK,
            scrim: Color::BLACK,
            inverse_surface: Color::hex(surfaces.inverse_surface),
            inverse_on_surface: Color::hex(surfaces.inverse_on_surface),
            inverse_primary: Color::hex(surfaces.inverse_primary),
            success: Color::hex(success.0),
            on_success: Color::hex(success.1),
            success_container: Color::hex(success.2),
            on_success_container: Color::hex(success.3),
        }
    }
}

struct LightSurfaces {
    surface: &'static str,
    on_surface: &'static str,
    surface_dim: &'static str,
    surface_bright: &'static str,
    lowest: &'static str,
    low: &'static str,
    container: &'static str,
    high: &'static str,
    highest: &'static str,
    variant: &'static str,
    on_variant: &'static str,
    outline: &'static str,
    outline_variant: &'static str,
    inverse_surface: &'static str,
    inverse_on_surface: &'static str,
    inverse_primary: &'static str,
}

/// Material 3 type scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Typography {
    pub display_large: TextStyle,
    pub display_medium: TextStyle,
    pub display_small: TextStyle,
    pub headline_large: TextStyle,
    pub headline_medium: TextStyle,
    pub headline_small: TextStyle,
    pub title_large: TextStyle,
    pub title_medium: TextStyle,
    pub title_small: TextStyle,
    pub body_large: TextStyle,
    pub body_medium: TextStyle,
    pub body_small: TextStyle,
    pub label_large: TextStyle,
    pub label_medium: TextStyle,
    pub label_small: TextStyle,
}

impl Typography {
    pub fn new(family: FontFamily, color: Color) -> Self {
        let style = |size: f32, line_height: f32, weight: FontWeight, tracking: f32| -> TextStyle {
            TextStyle {
                family: family.clone(),
                weight,
                size,
                line_height: Some(line_height),
                letter_spacing: tracking,
                color,
                wrap: TextWrap::Word,
                ..TextStyle::default()
            }
        };
        Self {
            display_large: style(57.0, 64.0, FontWeight::NORMAL, -0.25),
            display_medium: style(45.0, 52.0, FontWeight::NORMAL, 0.0),
            display_small: style(36.0, 44.0, FontWeight::NORMAL, 0.0),
            headline_large: style(32.0, 40.0, FontWeight::NORMAL, 0.0),
            headline_medium: style(28.0, 36.0, FontWeight::NORMAL, 0.0),
            headline_small: style(24.0, 32.0, FontWeight::NORMAL, 0.0),
            title_large: style(22.0, 28.0, FontWeight::NORMAL, 0.0),
            title_medium: style(16.0, 24.0, FontWeight::MEDIUM, 0.15),
            title_small: style(14.0, 20.0, FontWeight::MEDIUM, 0.1),
            body_large: style(16.0, 24.0, FontWeight::NORMAL, 0.5),
            body_medium: style(14.0, 20.0, FontWeight::NORMAL, 0.25),
            body_small: style(12.0, 16.0, FontWeight::NORMAL, 0.4),
            label_large: style(14.0, 20.0, FontWeight::MEDIUM, 0.1),
            label_medium: style(12.0, 16.0, FontWeight::MEDIUM, 0.5),
            label_small: style(11.0, 16.0, FontWeight::MEDIUM, 0.5),
        }
    }

    /// Rebuilds every role with the same metrics and a new color.
    pub fn with_color(&self, color: Color) -> Self {
        Self {
            display_large: self.display_large.clone().color(color),
            display_medium: self.display_medium.clone().color(color),
            display_small: self.display_small.clone().color(color),
            headline_large: self.headline_large.clone().color(color),
            headline_medium: self.headline_medium.clone().color(color),
            headline_small: self.headline_small.clone().color(color),
            title_large: self.title_large.clone().color(color),
            title_medium: self.title_medium.clone().color(color),
            title_small: self.title_small.clone().color(color),
            body_large: self.body_large.clone().color(color),
            body_medium: self.body_medium.clone().color(color),
            body_small: self.body_small.clone().color(color),
            label_large: self.label_large.clone().color(color),
            label_medium: self.label_medium.clone().color(color),
            label_small: self.label_small.clone().color(color),
        }
    }
}

/// M3 shape scale in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeScale {
    pub none: f32,
    pub extra_small: f32,
    pub small: f32,
    pub medium: f32,
    pub large: f32,
    pub extra_large: f32,
    pub full: f32,
}

impl Default for ShapeScale {
    fn default() -> Self {
        Self {
            none: 0.0,
            extra_small: 4.0,
            small: 8.0,
            medium: 12.0,
            large: 16.0,
            extra_large: 28.0,
            full: 999.0,
        }
    }
}

impl ShapeScale {
    pub fn corners(self, radius: f32) -> Corners {
        Corners::all(radius)
    }
}

/// M3 elevation levels in dp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElevationScale {
    pub level0: f32,
    pub level1: f32,
    pub level2: f32,
    pub level3: f32,
    pub level4: f32,
    pub level5: f32,
}

impl Default for ElevationScale {
    fn default() -> Self {
        Self {
            level0: 0.0,
            level1: 1.0,
            level2: 3.0,
            level3: 6.0,
            level4: 8.0,
            level5: 12.0,
        }
    }
}

impl ElevationScale {
    pub fn dp(self, level: u8) -> f32 {
        match level {
            0 => self.level0,
            1 => self.level1,
            2 => self.level2,
            3 => self.level3,
            4 => self.level4,
            _ => self.level5,
        }
    }
}

/// State-layer opacities from the M3 interaction specs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StateLayerOpacity {
    pub hover: f32,
    pub focus: f32,
    pub pressed: f32,
    pub dragged: f32,
    pub disabled_content: f32,
    pub disabled_container: f32,
}

impl Default for StateLayerOpacity {
    fn default() -> Self {
        Self {
            hover: 0.08,
            focus: 0.10,
            pressed: 0.10,
            dragged: 0.16,
            disabled_content: 0.38,
            disabled_container: 0.12,
        }
    }
}

/// M3 duration tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurationScale {
    pub short1: Duration,
    pub short2: Duration,
    pub short3: Duration,
    pub short4: Duration,
    pub medium1: Duration,
    pub medium2: Duration,
    pub medium3: Duration,
    pub medium4: Duration,
    pub long1: Duration,
    pub long2: Duration,
    pub long3: Duration,
    pub long4: Duration,
    pub extra_long1: Duration,
    pub extra_long2: Duration,
    pub extra_long3: Duration,
    pub extra_long4: Duration,
}

impl Default for DurationScale {
    fn default() -> Self {
        Self {
            short1: Duration::from_millis(50),
            short2: Duration::from_millis(100),
            short3: Duration::from_millis(150),
            short4: Duration::from_millis(200),
            medium1: Duration::from_millis(250),
            medium2: Duration::from_millis(300),
            medium3: Duration::from_millis(350),
            medium4: Duration::from_millis(400),
            long1: Duration::from_millis(450),
            long2: Duration::from_millis(500),
            long3: Duration::from_millis(550),
            long4: Duration::from_millis(600),
            extra_long1: Duration::from_millis(700),
            extra_long2: Duration::from_millis(800),
            extra_long3: Duration::from_millis(900),
            extra_long4: Duration::from_millis(1000),
        }
    }
}

/// M3 easing tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionEasing {
    pub linear: Easing,
    pub standard: Easing,
    pub standard_decelerate: Easing,
    pub standard_accelerate: Easing,
    pub emphasized: Easing,
    pub emphasized_decelerate: Easing,
    pub emphasized_accelerate: Easing,
}

impl Default for MotionEasing {
    fn default() -> Self {
        Self {
            linear: Easing::Linear,
            standard: Easing::M3_STANDARD,
            standard_decelerate: Easing::M3_DECELERATE,
            standard_accelerate: Easing::M3_ACCELERATE,
            emphasized: Easing::M3_EMPHASIZED,
            emphasized_decelerate: Easing::CubicBezier {
                x1: 0.05,
                y1: 0.7,
                x2: 0.1,
                y2: 1.0,
            },
            emphasized_accelerate: Easing::CubicBezier {
                x1: 0.3,
                y1: 0.0,
                x2: 0.8,
                y2: 0.15,
            },
        }
    }
}

/// Duration and easing tokens together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionScale {
    pub duration: DurationScale,
    pub easing: MotionEasing,
}

impl Default for MotionScale {
    fn default() -> Self {
        Self {
            duration: DurationScale::default(),
            easing: MotionEasing::default(),
        }
    }
}

impl MotionScale {
    pub fn tween(self, duration: Duration, easing: Easing) -> MotionSpec {
        MotionSpec::tween(duration, easing)
    }

    /// Short emphasized state change, suitable for selection and popovers.
    pub fn emphasized_short(self) -> MotionSpec {
        MotionSpec::tween(self.duration.short4, self.easing.emphasized_decelerate)
    }

    /// Medium standard transition, suitable for expanding containers.
    pub fn standard_medium(self) -> MotionSpec {
        MotionSpec::tween(self.duration.medium2, self.easing.standard)
    }
}

/// A small 4 dp spacing scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacingScale {
    pub none: f32,
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
}

impl Default for SpacingScale {
    fn default() -> Self {
        Self {
            none: 0.0,
            xs: 4.0,
            sm: 8.0,
            md: 12.0,
            lg: 16.0,
            xl: 24.0,
            xxl: 32.0,
        }
    }
}

/// Material Design 3 theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub colors: ColorScheme,
    pub typography: Typography,
    pub shape: ShapeScale,
    pub elevation: ElevationScale,
    pub state: StateLayerOpacity,
    pub motion: MotionScale,
    pub spacing: SpacingScale,
    pub mode: ThemeMode,
    pub font_family: FontFamily,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
    /// The baseline light theme.
    pub fn light() -> Self {
        Self::from_scheme(ColorScheme::baseline_light(), ThemeMode::Light)
    }

    /// The baseline dark theme.
    pub fn dark() -> Self {
        Self::from_scheme(ColorScheme::baseline_dark(), ThemeMode::Dark)
    }

    /// Builds a theme around an explicit color scheme.
    pub fn from_scheme(colors: ColorScheme, mode: ThemeMode) -> Self {
        let font_family = FontFamily::SansSerif;
        let typography = Typography::new(font_family.clone(), colors.on_surface);
        Self {
            colors,
            typography,
            shape: ShapeScale::default(),
            elevation: ElevationScale::default(),
            state: StateLayerOpacity::default(),
            motion: MotionScale::default(),
            spacing: SpacingScale::default(),
            mode,
            font_family,
        }
    }

    /// Derives a theme from a seed color with the Material algorithm.
    pub fn from_seed(seed: Color, mode: ThemeMode) -> Self {
        Self::from_dynamic(DynamicScheme::from_seed(seed, mode))
    }

    /// Derives a theme from a seed with an explicit variant and contrast.
    pub fn from_seed_with(seed: Color, mode: ThemeMode, config: DynamicColorConfig) -> Self {
        Self::from_dynamic(DynamicScheme::new(seed, mode, config))
    }

    /// Resolves every role of an already-configured dynamic scheme.
    pub fn from_dynamic(scheme: DynamicScheme) -> Self {
        Self::from_scheme(scheme.color_scheme(), scheme.mode())
    }

    pub fn is_dark(&self) -> bool {
        self.mode.is_dark()
    }

    /// Replaces the color scheme and re-colors the generated typography.
    pub fn with_colors(mut self, colors: ColorScheme) -> Self {
        self.typography = self.typography.with_color(colors.on_surface);
        self.colors = colors;
        self
    }

    /// Rebuilds the type scale with a different family, keeping the colors.
    pub fn with_font_family(mut self, family: impl Into<FontFamily>) -> Self {
        self.font_family = family.into();
        self.typography = Typography::new(self.font_family.clone(), self.colors.on_surface);
        self
    }

    /// Switches between the light and dark baseline schemes.
    pub fn toggled(&self) -> Self {
        match self.mode {
            ThemeMode::Light => Self::dark(),
            ThemeMode::Dark => Self::light(),
        }
    }

    /// The default body style, used by labels that do not request a role.
    pub fn text_style(&self) -> TextStyle {
        self.typography.body_medium.clone()
    }

    /// A title style resized to an explicit size. Prefer the named type-scale
    /// roles for new code; this helper exists for content-derived sizes.
    pub fn heading_style(&self, size: f32) -> TextStyle {
        let mut style = self.typography.title_large.clone();
        style.size = size;
        style.line_height = Some(size * 1.3);
        style
    }

    /// The surface color for an elevation level.
    pub fn surface_at_elevation(&self, level: u8) -> Color {
        self.colors.surface_at_elevation(level)
    }

    /// Resolves a state-layer opacity for a named interaction state.
    pub fn state_layer_opacity(&self, state: InteractionState) -> f32 {
        match state {
            InteractionState::Hover => self.state.hover,
            InteractionState::Focus => self.state.focus,
            InteractionState::Pressed => self.state.pressed,
            InteractionState::Dragged => self.state.dragged,
        }
    }
}

/// Interaction states that produce a state layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionState {
    Hover,
    Focus,
    Pressed,
    Dragged,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_schemes_use_material_roles() {
        let light = ColorScheme::baseline_light();
        assert_eq!(light.primary, Color::hex("#6750A4"));
        assert_eq!(light.surface, Color::hex("#FEF7FF"));
        assert_eq!(light.surface_container_lowest, Color::hex("#FFFFFF"));
        assert_eq!(light.on_surface_variant, Color::hex("#49454F"));
        let dark = ColorScheme::baseline_dark();
        assert_eq!(dark.primary, Color::hex("#D0BCFF"));
        assert_eq!(dark.surface, Color::hex("#141218"));
        assert_ne!(light.primary, dark.primary);
    }

    #[test]
    fn type_scale_matches_m3_metrics() {
        let theme = Theme::light();
        assert_eq!(theme.typography.body_medium.size, 14.0);
        assert_eq!(theme.typography.body_medium.line_height, Some(20.0));
        assert_eq!(theme.typography.display_large.size, 57.0);
        assert_eq!(theme.typography.display_large.line_height, Some(64.0));
        assert_eq!(theme.typography.title_medium.weight, FontWeight::MEDIUM);
        assert_eq!(theme.typography.label_small.size, 11.0);
        assert_eq!(theme.typography.body_medium.letter_spacing, 0.25);
    }

    #[test]
    fn dark_theme_recolors_generated_typography() {
        let theme = Theme::dark();
        assert_eq!(theme.typography.body_medium.color, theme.colors.on_surface);
        assert!(theme.is_dark());
        assert_eq!(theme.toggled().mode, ThemeMode::Light);
    }

    #[test]
    fn seeded_themes_are_deterministic_and_complete() {
        let seed = Color::hex("#006B5E");
        let a = Theme::from_seed(seed, ThemeMode::Light);
        let b = Theme::from_seed(seed, ThemeMode::Light);
        assert_eq!(a, b);
        assert_eq!(a.colors.background, a.colors.surface);
        assert_eq!(a.colors.on_background, a.colors.on_surface);
        assert_ne!(a.colors.primary, a.colors.surface);
        assert_ne!(a.colors.primary, a.colors.error);
        // The dynamic path resolves the same roles `from_dynamic` exposes.
        let again = Theme::from_dynamic(DynamicScheme::from_seed(seed, ThemeMode::Light));
        assert_eq!(again.colors, a.colors);
        for color in [
            a.colors.primary,
            a.colors.primary_container,
            a.colors.secondary,
            a.colors.tertiary,
            a.colors.surface,
            a.colors.surface_container_highest,
            a.colors.on_surface,
            a.colors.error,
        ] {
            assert!((0.0..=1.0).contains(&color.r));
            assert!((0.0..=1.0).contains(&color.g));
            assert!((0.0..=1.0).contains(&color.b));
            assert_eq!(color.a, 1.0);
        }
    }

    #[test]
    fn elevation_maps_to_surface_containers() {
        let theme = Theme::light();
        assert_eq!(theme.surface_at_elevation(0), theme.colors.surface);
        assert_eq!(
            theme.surface_at_elevation(2),
            theme.colors.surface_container
        );
        assert_ne!(theme.surface_at_elevation(1), theme.surface_at_elevation(5));
        assert_eq!(theme.elevation.dp(5), 12.0);
        assert_eq!(theme.shape.medium, 12.0);
        assert_eq!(theme.shape.extra_large, 28.0);
    }

    #[test]
    fn motion_tokens_use_m3_curves_and_durations() {
        let theme = Theme::light();
        assert_eq!(theme.motion.duration.short4, Duration::from_millis(200));
        assert_eq!(
            theme.motion.duration.extra_long4,
            Duration::from_millis(1000)
        );
        let sampled = theme.motion.easing.standard.sample(0.5);
        assert!(sampled > 0.0 && sampled < 1.0);
        assert_eq!(theme.motion.easing.standard.sample(0.0), 0.0);
        assert_eq!(theme.motion.easing.standard.sample(1.0), 1.0);
    }

    #[test]
    fn state_layers_are_visible_over_transparent_bases() {
        let theme = Theme::light();
        let overlay = theme
            .colors
            .state_layer(theme.colors.primary, theme.state.hover);
        assert!(overlay.a > 0.0);
        assert!((overlay.r - theme.colors.primary.r).abs() < 0.001);
        let opaque = theme
            .colors
            .state_layer(theme.colors.on_primary, theme.state.pressed);
        assert!(opaque.a > 0.0);
    }
}
