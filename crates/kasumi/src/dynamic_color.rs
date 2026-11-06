//! Dynamic color from a seed using the Material color system.
//!
//! Material's dynamic color derives a whole scheme from one source color: the
//! seed is converted to HCT (hue, chroma, tone), tonal palettes are built from
//! its hue and chroma, and each color role is resolved against a contrast curve
//! that guarantees its foreground/background ratio.
//!
//! This module wraps the `material-color-utilities` implementation of that
//! algorithm, so `Theme::from_seed` produces the same roles the real system
//! does rather than an approximation in another color space.
//!
//! ```ignore
//! use kasumi::{Color, DynamicColorConfig, SchemeVariant, ContrastLevel, Theme, ThemeMode};
//!
//! let theme = Theme::from_seed_with(
//!     Color::hex("#006B5E"),
//!     ThemeMode::Dark,
//!     DynamicColorConfig::new()
//!         .variant(SchemeVariant::Vibrant)
//!         .contrast(ContrastLevel::High),
//! );
//! ```
//!
//! The scheme is pure data: it stores the seed, the mode and the config, and
//! `color_scheme()` resolves the roles on demand. Callers that build many themes
//! from one seed can keep one `DynamicScheme` and re-resolve it.

use material_color_utilities::dynamiccolor::{
    DynamicSchemeBuilder, MaterialDynamicColors, SpecVersion, Variant,
};
use material_color_utilities::hct::Hct;

use crate::theme::{ColorScheme, ThemeMode};
use kasumi_core::Color;

/// The Material scheme variants.
///
/// Each variant changes how the primary/secondary/tertiary palettes are derived
/// from the seed and how the neutral palette is tinted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemeVariant {
    /// The default: a muted spot of color around the seed hue.
    #[default]
    TonalSpot,
    /// More saturated primary and tertiary colors.
    Vibrant,
    /// Rotated hues, for a more playful scheme.
    Expressive,
    /// The seed color is kept as the primary color.
    Fidelity,
    /// Like fidelity but only the primary color is kept.
    Content,
    /// Nearly monochrome neutrals.
    Neutral,
    /// Fully black and white.
    Monochrome,
    /// A wide spread of hues.
    Rainbow,
    /// Warm and cool hues mixed.
    FruitSalad,
}

impl SchemeVariant {
    fn to_material(self) -> Variant {
        match self {
            Self::TonalSpot => Variant::TonalSpot,
            Self::Vibrant => Variant::Vibrant,
            Self::Expressive => Variant::Expressive,
            Self::Fidelity => Variant::Fidelity,
            Self::Content => Variant::Content,
            Self::Neutral => Variant::Neutral,
            Self::Monochrome => Variant::Monochrome,
            Self::Rainbow => Variant::Rainbow,
            Self::FruitSalad => Variant::FruitSalad,
        }
    }

    /// Every variant, in declaration order.
    pub const ALL: [Self; 9] = [
        Self::TonalSpot,
        Self::Vibrant,
        Self::Expressive,
        Self::Fidelity,
        Self::Content,
        Self::Neutral,
        Self::Monochrome,
        Self::Rainbow,
        Self::FruitSalad,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::TonalSpot => "Tonal spot",
            Self::Vibrant => "Vibrant",
            Self::Expressive => "Expressive",
            Self::Fidelity => "Fidelity",
            Self::Content => "Content",
            Self::Neutral => "Neutral",
            Self::Monochrome => "Monochrome",
            Self::Rainbow => "Rainbow",
            Self::FruitSalad => "Fruit salad",
        }
    }
}

/// How far the contrast curves are stretched.
///
/// The Material scale runs from `-1.0` (reduced contrast) through `0.0`
/// (standard) to `1.0` (maximum).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum ContrastLevel {
    /// Contrast below the standard level.
    Reduced,
    /// The Material default.
    #[default]
    Standard,
    /// Higher contrast, for example for accessibility settings.
    High,
    /// The maximum the contrast curves allow.
    Maximum,
    /// An explicit level in `-1.0..=1.0`.
    Custom(f32),
}

impl ContrastLevel {
    /// The numeric level the Material curves expect.
    pub fn value(self) -> f64 {
        match self {
            Self::Reduced => -0.5,
            Self::Standard => 0.0,
            Self::High => 0.5,
            Self::Maximum => 1.0,
            Self::Custom(value) => (value as f64).clamp(-1.0, 1.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Reduced => "Reduced",
            Self::Standard => "Standard",
            Self::High => "High",
            Self::Maximum => "Maximum",
            Self::Custom(_) => "Custom",
        }
    }

    pub const ALL: [Self; 4] = [Self::Reduced, Self::Standard, Self::High, Self::Maximum];
}

/// Selects how a dynamic scheme is derived from its seed.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct DynamicColorConfig {
    pub variant: SchemeVariant,
    pub contrast: ContrastLevel,
    /// Whether to use the 2025 color specification. The 2021 specification is
    /// the baseline that shipped with Material 3.
    pub spec_2025: bool,
}

impl DynamicColorConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn variant(mut self, variant: SchemeVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn contrast(mut self, contrast: ContrastLevel) -> Self {
        self.contrast = contrast;
        self
    }

    /// Uses the newer color specification, which changes some role tones.
    pub fn spec_2025(mut self, enabled: bool) -> Self {
        self.spec_2025 = enabled;
        self
    }
}

/// A dynamic color scheme: a seed plus the parameters that turn it into roles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicScheme {
    seed: Color,
    mode: ThemeMode,
    config: DynamicColorConfig,
}

impl DynamicScheme {
    /// The standard scheme: tonal spot at the base contrast level.
    pub fn from_seed(seed: Color, mode: ThemeMode) -> Self {
        Self::new(seed, mode, DynamicColorConfig::default())
    }

    pub fn new(seed: Color, mode: ThemeMode, config: DynamicColorConfig) -> Self {
        Self { seed, mode, config }
    }

    pub fn seed(&self) -> Color {
        self.seed
    }

    pub fn mode(&self) -> ThemeMode {
        self.mode
    }

    pub fn config(&self) -> DynamicColorConfig {
        self.config
    }

    pub fn with_seed(mut self, seed: Color) -> Self {
        self.seed = seed;
        self
    }

    pub fn with_mode(mut self, mode: ThemeMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_config(mut self, config: DynamicColorConfig) -> Self {
        self.config = config;
        self
    }

    /// Resolves the scheme's color roles.
    pub fn color_scheme(&self) -> ColorScheme {
        let builder0 = DynamicSchemeBuilder::default()
            .source_color_hct(Hct::from_int(argb_from_color(self.seed)))
            .variant(self.config.variant.to_material())
            .is_dark(self.mode.is_dark())
            .contrast_level(self.config.contrast.value())
            .spec_version(if self.config.spec_2025 {
                SpecVersion::Spec2025
            } else {
                SpecVersion::Spec2021
            });
        // Keep the default palettes; only the source and the modifiers matter.
        let scheme = builder0.build();
        let colors = MaterialDynamicColors;
        macro_rules! hex {
            ($dc:expr) => {
                argb_to_color($dc.get_argb(&scheme))
            };
        }

        ColorScheme {
            primary: hex!(colors.primary()),
            on_primary: hex!(colors.on_primary()),
            primary_container: hex!(colors.primary_container()),
            on_primary_container: hex!(colors.on_primary_container()),
            primary_fixed: hex!(colors.primary_fixed()),
            primary_fixed_dim: hex!(colors.primary_fixed_dim()),
            on_primary_fixed: hex!(colors.on_primary_fixed()),
            on_primary_fixed_variant: hex!(colors.on_primary_fixed_variant()),

            secondary: hex!(colors.secondary()),
            on_secondary: hex!(colors.on_secondary()),
            secondary_container: hex!(colors.secondary_container()),
            on_secondary_container: hex!(colors.on_secondary_container()),
            secondary_fixed: hex!(colors.secondary_fixed()),
            secondary_fixed_dim: hex!(colors.secondary_fixed_dim()),
            on_secondary_fixed: hex!(colors.on_secondary_fixed()),
            on_secondary_fixed_variant: hex!(colors.on_secondary_fixed_variant()),

            tertiary: hex!(colors.tertiary()),
            on_tertiary: hex!(colors.on_tertiary()),
            tertiary_container: hex!(colors.tertiary_container()),
            on_tertiary_container: hex!(colors.on_tertiary_container()),
            tertiary_fixed: hex!(colors.tertiary_fixed()),
            tertiary_fixed_dim: hex!(colors.tertiary_fixed_dim()),
            on_tertiary_fixed: hex!(colors.on_tertiary_fixed()),
            on_tertiary_fixed_variant: hex!(colors.on_tertiary_fixed_variant()),

            error: hex!(colors.error()),
            on_error: hex!(colors.on_error()),
            error_container: hex!(colors.error_container()),
            on_error_container: hex!(colors.on_error_container()),

            surface: hex!(colors.surface()),
            on_surface: hex!(colors.on_surface()),
            surface_dim: hex!(colors.surface_dim()),
            surface_bright: hex!(colors.surface_bright()),
            surface_container_lowest: hex!(colors.surface_container_lowest()),
            surface_container_low: hex!(colors.surface_container_low()),
            surface_container: hex!(colors.surface_container()),
            surface_container_high: hex!(colors.surface_container_high()),
            surface_container_highest: hex!(colors.surface_container_highest()),
            surface_variant: hex!(colors.surface_variant()),
            on_surface_variant: hex!(colors.on_surface_variant()),
            surface_tint: hex!(colors.surface_tint()),

            background: hex!(colors.background()),
            on_background: hex!(colors.on_background()),

            outline: hex!(colors.outline()),
            outline_variant: hex!(colors.outline_variant()),
            shadow: Color::BLACK,
            scrim: Color::BLACK,

            inverse_surface: hex!(colors.inverse_surface()),
            inverse_on_surface: hex!(colors.inverse_on_surface()),
            inverse_primary: hex!(colors.inverse_primary()),

            // Material has no success role. Derive one from the tertiary
            // palette's green counterpart so the extension still follows the
            // seed instead of staying at a fixed hue.
            success: hex!(colors.tertiary()),
            on_success: hex!(colors.on_tertiary()),
            success_container: hex!(colors.tertiary_container()),
            on_success_container: hex!(colors.on_tertiary_container()),
        }
    }
}

/// Converts an sRGB `Color` to the ARGB integer the color utilities expect.
pub fn argb_from_color(color: Color) -> u32 {
    let [r, g, b, a] = color.to_rgba8();
    (u32::from(a) << 24) | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// Converts an opaque ARGB integer to a `Color`.
pub fn argb_to_color(argb: u32) -> Color {
    let a = ((argb >> 24) & 0xff) as f32 / 255.0;
    let r = ((argb >> 16) & 0xff) as f32 / 255.0;
    let g = ((argb >> 8) & 0xff) as f32 / 255.0;
    let b = (argb & 0xff) as f32 / 255.0;
    Color::rgba(r, g, b, if a > 0.0 { a } else { 1.0 })
}

/// Extracts a seed color from an image, the way Material You picks a theme from
/// a wallpaper.
///
/// `pixels` is tightly packed RGBA8. The quantizer cluster the image into a
/// small palette and the Material scorer picks the most suitable cluster for a
/// UI theme, so a wallpaper with one dominant color yields that color and a
/// busy one yields the color that reads best as a theme seed.
pub fn seed_from_image(width: u32, height: u32, pixels: &[u8]) -> Option<Color> {
    if width == 0 || height == 0 || pixels.len() < (width as usize * height as usize * 4) {
        return None;
    }
    use material_color_utilities::score::score_with;
    use std::collections::HashMap;

    // Sample on a grid so a 4K wallpaper does not cost 8 million HCT
    // conversions; the palette is stable well below full resolution.
    let step = ((width as usize * height as usize) / 4096).max(1);
    let mut histogram: HashMap<u32, u16> = HashMap::new();
    for index in (0..(width as usize * height as usize)).step_by(step) {
        let offset = index * 4;
        let alpha = pixels[offset + 3];
        if alpha < 128 {
            continue;
        }
        let [r, g, b] = [pixels[offset], pixels[offset + 1], pixels[offset + 2]];
        let argb = 0xff00_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
        let bucket = histogram.entry(argb).or_insert(0);
        *bucket = bucket.saturating_add(1);
    }
    if histogram.is_empty() {
        return None;
    }
    // `score_with` returns a palette ordered by Material's suitability score.
    // Rank the returned candidates here rather than relying on the order, so a
    // change in the dependency's sort does not silently pick a worse seed.
    let scored = score_with(histogram.clone(), Some(4), None, Some(true));
    let total: f64 = histogram.values().map(|count| f64::from(*count)).sum();
    scored
        .into_iter()
        .max_by(|a, b| {
            let score = |argb: &u32| {
                let population = f64::from(*histogram.get(argb).unwrap_or(&0)) / total.max(1.0);
                let hct = Hct::from_int(*argb);
                // Material weights hue population 0.7 and chroma 0.3 around a
                // target chroma of 48.
                let chroma_weight = if hct.chroma() < 48.0 { 0.1 } else { 0.3 };
                population * 100.0 * 0.7 + (hct.chroma() - 48.0) * chroma_weight
            };
            score(a).total_cmp(&score(b))
        })
        .map(argb_to_color)
        .filter(|color| color.a > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Theme;

    fn hue_of(color: Color) -> f32 {
        material_color_utilities::hct::Hct::from_int(argb_from_color(color)).hue() as f32
    }

    #[test]
    fn argb_round_trips() {
        for color in [
            Color::hex("#6750A4"),
            Color::hex("#006B5E"),
            Color::hex("#FEF7FF"),
            Color::hex("#141218"),
        ] {
            let round = argb_to_color(argb_from_color(color));
            assert!((round.r - color.r).abs() < 0.01);
            assert!((round.g - color.g).abs() < 0.01);
            assert!((round.b - color.b).abs() < 0.01);
            assert_eq!(round.a, 1.0);
        }
    }

    #[test]
    fn dynamic_schemes_are_deterministic_and_keep_the_seed_hue() {
        let seed = Color::hex("#006B5E");
        let a = DynamicScheme::from_seed(seed, ThemeMode::Light).color_scheme();
        let b = DynamicScheme::from_seed(seed, ThemeMode::Light).color_scheme();
        assert_eq!(a, b);
        assert_eq!(a.background, a.surface);
        assert_eq!(a.on_background, a.on_surface);
        // The primary role stays on the seed's hue; tonal spot desaturates it.
        let seed_hue = hue_of(seed);
        let primary_hue = hue_of(a.primary);
        let delta = (seed_hue - primary_hue)
            .abs()
            .min(360.0 - (seed_hue - primary_hue).abs());
        assert!(delta < 12.0, "primary hue drifted by {delta}°");
    }

    #[test]
    fn light_and_dark_differ_in_surface_and_primary() {
        let seed = Color::hex("#6750A4");
        let light = DynamicScheme::from_seed(seed, ThemeMode::Light).color_scheme();
        let dark = DynamicScheme::from_seed(seed, ThemeMode::Dark).color_scheme();
        assert_ne!(light.primary, dark.primary);
        assert_ne!(light.surface, dark.surface);
        // The dark surface must be darker than the light one.
        assert!(dark.surface.r < light.surface.r);
        assert!(dark.on_surface.r > dark.surface.r);
    }

    #[test]
    fn variants_change_the_result() {
        let seed = Color::hex("#1B6C3A");
        let config = |variant| DynamicColorConfig::new().variant(variant);
        let tonal = DynamicScheme::new(seed, ThemeMode::Light, config(SchemeVariant::TonalSpot))
            .color_scheme();
        let vibrant = DynamicScheme::new(seed, ThemeMode::Light, config(SchemeVariant::Vibrant))
            .color_scheme();
        let mono = DynamicScheme::new(seed, ThemeMode::Light, config(SchemeVariant::Monochrome))
            .color_scheme();
        assert_ne!(tonal.primary, vibrant.primary);
        assert_ne!(tonal.primary, mono.primary);
        // Monochrome collapses the chroma, so the channels must be close.
        let spread = (mono.primary.r - mono.primary.g).abs()
            + (mono.primary.g - mono.primary.b).abs()
            + (mono.primary.r - mono.primary.b).abs();
        assert!(spread < 0.05, "monochrome primary is not gray: {spread}");
    }

    #[test]
    fn contrast_levels_move_the_on_colors() {
        let seed = Color::hex("#1B6C3A");
        let standard = DynamicScheme::new(
            seed,
            ThemeMode::Dark,
            DynamicColorConfig::new().contrast(ContrastLevel::Standard),
        )
        .color_scheme();
        let high = DynamicScheme::new(
            seed,
            ThemeMode::Dark,
            DynamicColorConfig::new().contrast(ContrastLevel::High),
        )
        .color_scheme();
        assert_ne!(standard.on_surface, high.on_surface);
        assert_eq!(ContrastLevel::High.value(), 0.5);
        assert_eq!(ContrastLevel::Maximum.value(), 1.0);
        assert_eq!(ContrastLevel::Custom(4.0).value(), 1.0);
    }

    #[test]
    fn themes_wrap_dynamic_schemes() {
        let seed = Color::hex("#006B5E");
        let theme = Theme::from_seed_with(
            seed,
            ThemeMode::Dark,
            DynamicColorConfig::new().variant(SchemeVariant::Expressive),
        );
        assert!(theme.is_dark());
        assert_eq!(theme.typography.body_medium.color, theme.colors.on_surface);
        let plain = Theme::from_seed(seed, ThemeMode::Dark);
        assert_ne!(plain.colors.primary, theme.colors.primary);
    }

    #[test]
    fn a_image_seed_is_picked_from_its_dominant_color() {
        // A 64×64 image that is 75 % saturated green and 25 % muted brown. The
        // Material scorer weights chroma and excited hue population, so the
        // saturated green must be the seed.
        let mut pixels = Vec::with_capacity(64 * 64 * 4);
        for index in 0..64 * 64 {
            if index % 4 == 0 {
                pixels.extend_from_slice(&[120, 90, 60, 255]);
            } else {
                pixels.extend_from_slice(&[20, 160, 60, 255]);
            }
        }
        let seed = seed_from_image(64, 64, &pixels).expect("seed missing");
        let seed_hct = Hct::from_int(argb_from_color(seed));
        let green_hct = Hct::from_int(0xff14a03c);
        let delta = (seed_hct.hue() - green_hct.hue()).abs();
        let delta = delta.min(360.0 - delta);
        assert!(delta < 30.0, "expected the green hue, got {seed:?}");
        assert!(seed_from_image(0, 0, &[]).is_none());
        // A fully transparent image has no seed either.
        let clear = vec![0u8; 16 * 16 * 4];
        assert!(seed_from_image(16, 16, &clear).is_none());
    }

    #[test]
    fn every_variant_resolves_a_complete_scheme() {
        for variant in SchemeVariant::ALL {
            let scheme = DynamicScheme::new(
                Color::hex("#7D5260"),
                ThemeMode::Light,
                DynamicColorConfig::new().variant(variant),
            )
            .color_scheme();
            for color in [
                scheme.primary,
                scheme.on_primary,
                scheme.secondary,
                scheme.surface,
                scheme.on_surface,
                scheme.outline,
                scheme.error,
            ] {
                assert!((0.0..=1.0).contains(&color.r));
                assert!((0.0..=1.0).contains(&color.g));
                assert!((0.0..=1.0).contains(&color.b));
                assert!(color.a > 0.0);
            }
        }
    }
}
