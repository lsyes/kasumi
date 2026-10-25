use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use kasumi_core::{ImageData, ImageFit, Length};

use crate::Image;
use crate::response::Response;
use crate::ui::{Ui, Widget};
use crate::widgets::cache;

/// A vector illustration rasterized by resvg into an image texture.
#[derive(Clone)]
pub struct Svg {
    source: String,
    render_width: u32,
    render_height: u32,
    width: Option<Length>,
    height: Option<Length>,
}

impl Svg {
    pub fn new(source: impl Into<String>) -> Result<Self, String> {
        let source = source.into();
        resvg::usvg::Tree::from_str(&source, &resvg::usvg::Options::default())
            .map_err(|error| format!("invalid SVG: {error}"))?;
        Ok(Self {
            source,
            render_width: 512,
            render_height: 512,
            width: None,
            height: None,
        })
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let source = std::fs::read_to_string(path.as_ref()).map_err(|error| {
            format!("failed to read SVG `{}`: {error}", path.as_ref().display())
        })?;
        Self::new(source)
    }

    pub fn render_size(mut self, width: u32, height: u32) -> Self {
        self.render_width = width.clamp(16, 1024);
        self.render_height = height.clamp(16, 1024);
        self
    }

    pub fn size(mut self, width: impl Into<Length>, height: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self.height = Some(height.into());
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = Some(height.into());
        self
    }

    pub fn rasterize(&self) -> Result<ImageData, String> {
        let tree = resvg::usvg::Tree::from_str(&self.source, &resvg::usvg::Options::default())
            .map_err(|error| format!("invalid SVG: {error}"))?;
        let source_size = tree.size();
        let source_width = source_size.width().max(1.0);
        let source_height = source_size.height().max(1.0);
        let requested_width = self.render_width as f32;
        let requested_height = self.render_height as f32;
        let scale = (requested_width / source_width)
            .min(requested_height / source_height)
            .max(0.001);
        let width = (source_width * scale).round().clamp(1.0, 1024.0) as u32;
        let height = (source_height * scale).round().clamp(1.0, 1024.0) as u32;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| "failed to allocate SVG pixmap".to_owned())?;
        let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(
            (width as f32 - source_width * scale) * 0.5,
            (height as f32 - source_height * scale) * 0.5,
        );
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        let mut pixels = pixmap.take();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let alpha = u32::from(pixel[3]);
            if alpha > 0 && alpha < 255 {
                for channel in &mut pixel[..3] {
                    *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
                }
            }
        }
        Ok(ImageData::from_rgba8(width, height, pixels))
    }

    fn render_image(&self) -> Image {
        let mut hasher = DefaultHasher::new();
        self.source.hash(&mut hasher);
        self.render_width.hash(&mut hasher);
        self.render_height.hash(&mut hasher);
        let data = cache::image(hasher.finish(), || {
            self.rasterize()
                .unwrap_or_else(|_| ImageData::from_rgba8(1, 1, vec![0, 0, 0, 0]))
        });
        let mut image = Image::from_shared(data)
            .fit(ImageFit::Contain)
            .alt("SVG image");
        if let Some(width) = self.width {
            image = image.width(width);
        }
        if let Some(height) = self.height {
            image = image.height(height);
        }
        if self.width.is_none() && self.height.is_none() {
            image = image.size(
                Length::Px(self.render_width.min(512) as f32),
                Length::Px(self.render_height.min(512) as f32),
            );
        }
        image
    }
}

impl Widget for Svg {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        ui.add(self.render_image())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterizes_valid_svg_with_alpha() {
        let svg = Svg::new(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#ff0000"/></svg>"##,
        )
        .unwrap()
        .render_size(32, 32);
        let image = svg.rasterize().unwrap();
        assert_eq!((image.width(), image.height()), (32, 32));
        assert!(
            image
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] > 0)
        );
        assert!(
            image
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] > 200)
        );
    }

    #[test]
    fn rejects_invalid_svg() {
        assert!(Svg::new("<svg><broken>").is_err());
    }
}
