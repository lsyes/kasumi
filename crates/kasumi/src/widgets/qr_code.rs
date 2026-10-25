use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use kasumi_core::{Color, ImageData, ImageFit, Length};
use qrcode::{EcLevel, QrCode as Encoder};

use crate::Image;
use crate::response::Response;
use crate::ui::{Ui, Widget};
use crate::widgets::cache;

/// A QR code rendered as a crisp RGBA image.
#[derive(Clone)]
pub struct QrCode {
    encoder: Encoder,
    payload: Vec<u8>,
    ec_level: EcLevel,
    size: u32,
    quiet_zone: u32,
    dark: Color,
    light: Color,
}

impl QrCode {
    pub fn new(data: impl AsRef<[u8]>) -> Result<Self, String> {
        let encoder = Encoder::with_error_correction_level(data.as_ref(), EcLevel::M)
            .map_err(|error| format!("failed to encode QR code: {error}"))?;
        Ok(Self {
            encoder,
            payload: data.as_ref().to_vec(),
            ec_level: EcLevel::M,
            size: 180,
            quiet_zone: 4,
            dark: Color::BLACK,
            light: Color::WHITE,
        })
    }

    pub fn error_correction(data: impl AsRef<[u8]>, level: EcLevel) -> Result<Self, String> {
        let encoder = Encoder::with_error_correction_level(data.as_ref(), level)
            .map_err(|error| format!("failed to encode QR code: {error}"))?;
        Ok(Self {
            encoder,
            payload: data.as_ref().to_vec(),
            ec_level: level,
            size: 180,
            quiet_zone: 4,
            dark: Color::BLACK,
            light: Color::WHITE,
        })
    }

    pub fn size(mut self, size: u32) -> Self {
        self.size = size.clamp(32, 1024);
        self
    }

    pub fn colors(mut self, dark: Color, light: Color) -> Self {
        self.dark = dark;
        self.light = light;
        self
    }

    pub fn image_data(&self) -> ImageData {
        (*self.image_data_shared()).clone()
    }

    fn image_data_shared(&self) -> Arc<ImageData> {
        let mut hasher = DefaultHasher::new();
        self.payload.hash(&mut hasher);
        format!("{:?}", self.ec_level).hash(&mut hasher);
        self.size.hash(&mut hasher);
        self.quiet_zone.hash(&mut hasher);
        self.dark.to_rgba8().hash(&mut hasher);
        self.light.to_rgba8().hash(&mut hasher);
        cache::image(hasher.finish(), || self.rasterize_image())
    }

    fn rasterize_image(&self) -> ImageData {
        let modules = self.encoder.to_colors();
        let side = self.encoder.width() as u32;
        let total_modules = side + self.quiet_zone * 2;
        let module = (self.size / total_modules).max(1);
        let pixels_side = total_modules * module;
        let dark = self.dark.to_rgba8();
        let light = self.light.to_rgba8();
        let mut pixels = vec![0u8; (pixels_side * pixels_side * 4) as usize];

        for y in 0..side {
            for x in 0..side {
                let module_color = if modules[(y * side + x) as usize] == qrcode::Color::Dark {
                    dark
                } else {
                    light
                };
                for py in 0..module {
                    for px in 0..module {
                        let target_x = (x + self.quiet_zone) * module + px;
                        let target_y = (y + self.quiet_zone) * module + py;
                        let offset = ((target_y * pixels_side + target_x) * 4) as usize;
                        pixels[offset..offset + 4].copy_from_slice(&module_color);
                    }
                }
            }
        }
        ImageData::from_rgba8(pixels_side, pixels_side, pixels)
    }
}

impl Widget for QrCode {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let data = self.image_data_shared();
        let side = data.width().min(512) as f32;
        ui.add(
            Image::from_shared(data)
                .alt("QR code")
                .fit(ImageFit::Contain)
                .size(Length::Px(side), Length::Px(side)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_crisp_quiet_zone() {
        let code = QrCode::new("https://example.com")
            .unwrap()
            .size(180)
            .colors(Color::BLACK, Color::WHITE);
        let image = code.image_data();
        assert!(image.width() >= 100);
        assert_eq!(image.width(), image.height());
        assert!(
            image
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] == 0)
        );
        assert!(
            image
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] == 255)
        );
    }
}
