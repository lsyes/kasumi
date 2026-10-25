//! Colors are stored as non-premultiplied sRGB components in `0.0..=1.0`.
//!
//! The skeleton does not perform a linear-light color pipeline; the renderer
//! writes these components to a non-sRGB framebuffer. This keeps blending
//! predictable across the GL configs we can obtain on Linux and matches what
//! most simple toolkits do. A linear pipeline can be added without touching
//! the UI layer.

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::rgba8(r, g, b, 255)
    }

    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    /// Parses `#RRGGBB`, `#RRGGBBAA`, `RRGGBB` or `RRGGBBAA`.
    pub fn hex(hex: &str) -> Self {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        let parse = |range: std::ops::Range<usize>| -> u8 {
            u8::from_str_radix(&hex[range], 16).unwrap_or_else(|_| {
                panic!("invalid color literal {hex:?}: expected hexadecimal RGB or RGBA")
            })
        };
        match hex.len() {
            6 => Self::rgba8(parse(0..2), parse(2..4), parse(4..6), 255),
            8 => Self::rgba8(parse(0..2), parse(2..4), parse(4..6), parse(6..8)),
            _ => panic!("invalid color literal {hex:?}: expected 6 or 8 hexadecimal digits"),
        }
    }

    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    pub fn with_alpha8(self, a: u8) -> Self {
        self.with_alpha(a as f32 / 255.0)
    }

    pub const fn opaque(self) -> Self {
        self.with_alpha(1.0)
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn to_rgba8(self) -> [u8; 4] {
        [
            (self.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.b.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    pub fn lighten(self, amount: f32) -> Self {
        self.lerp(Self::WHITE, amount)
    }

    pub fn darken(self, amount: f32) -> Self {
        self.lerp(Self::BLACK, amount)
    }

    /// Multiplies the alpha channel, leaving the color untouched.
    pub fn fade(self, factor: f32) -> Self {
        self.with_alpha(self.a * factor.clamp(0.0, 1.0))
    }

    /// Premultiplied components, as expected by the renderer's blend state.
    pub fn premultiplied(self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex() {
        assert_eq!(Color::hex("#ff8000"), Color::rgb8(255, 128, 0));
        assert_eq!(Color::hex("11223344"), Color::rgba8(17, 34, 51, 68));
    }

    #[test]
    fn fade_and_lerp() {
        let c = Color::WHITE.fade(0.5);
        assert_eq!(c.a, 0.5);
        assert_eq!(
            Color::BLACK.lerp(Color::WHITE, 0.5),
            Color::rgb(0.5, 0.5, 0.5)
        );
    }
}
