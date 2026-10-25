use std::path::Path;

use kasumi_core::{CursorIcon, ImageData, ImageFit, Length, Style};
use kasumi_tree::NodeKind;

use crate::response::Response;
use crate::ui::{Ui, Widget};

const MAX_TEXTURE_EDGE: u32 = 1024;

/// An RGBA image widget. File-backed constructors decode once when the builder
/// is created; store the resulting `ImageData` in application state when an
/// image should not be decoded during each frame.
#[derive(Clone)]
pub struct Image {
    data: std::sync::Arc<ImageData>,
    fit: ImageFit,
    width: Option<Length>,
    height: Option<Length>,
    radius: Option<f32>,
    alt: Option<String>,
}

impl Image {
    pub fn new(data: ImageData) -> Self {
        Self {
            data: std::sync::Arc::new(data),
            fit: ImageFit::Contain,
            width: None,
            height: None,
            radius: None,
            alt: None,
        }
    }

    pub fn from_shared(data: std::sync::Arc<ImageData>) -> Self {
        Self {
            data,
            fit: ImageFit::Contain,
            width: None,
            height: None,
            radius: None,
            alt: None,
        }
    }

    pub fn from_rgba8(width: u32, height: u32, pixels: impl Into<std::sync::Arc<[u8]>>) -> Self {
        Self::new(ImageData::from_rgba8(width, height, pixels))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let decoded = image::load_from_memory(bytes)
            .map_err(|error| format!("failed to decode image: {error}"))?;
        Ok(Self::new(rgba_image_data(decoded)?))
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let bytes = std::fs::read(path.as_ref()).map_err(|error| {
            format!(
                "failed to read image `{}`: {error}",
                path.as_ref().display()
            )
        })?;
        Self::from_bytes(&bytes)
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
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

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius.max(0.0));
        self
    }

    pub fn alt(mut self, alt: impl Into<String>) -> Self {
        self.alt = Some(alt.into());
        self
    }
}

impl Widget for Image {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let id = ui.next_id("image");
        let natural_width = self.data.width().min(512) as f32;
        let natural_height =
            natural_width * self.data.height() as f32 / self.data.width().max(1) as f32;
        let width = self.width.unwrap_or(Length::Px(natural_width));
        let height = self.height.unwrap_or(Length::Px(natural_height));
        let node = ui.adopt(
            id,
            NodeKind::Image,
            Style {
                width,
                height,
                clip: true,
                radius: kasumi_core::Corners::all(self.radius.unwrap_or(0.0)),
                cursor: CursorIcon::Default,
                ..Style::default()
            },
        );
        if let Some(node) = ui.cx.tree.get_mut(node) {
            node.image = Some(self.data);
            node.image_fit = self.fit;
        }
        if let Some(alt) = self.alt {
            ui.cx.tree.set_accessibility_label(node, alt);
        }
        ui.response_of(id)
    }
}

pub(crate) fn rgba_image_data(decoded: image::DynamicImage) -> Result<ImageData, String> {
    let mut rgba = decoded.to_rgba8();
    if rgba.width() > MAX_TEXTURE_EDGE || rgba.height() > MAX_TEXTURE_EDGE {
        let scale = (MAX_TEXTURE_EDGE as f32 / rgba.width().max(rgba.height()) as f32).min(1.0);
        let width = ((rgba.width() as f32 * scale).round() as u32).max(1);
        let height = ((rgba.height() as f32 * scale).round() as u32).max(1);
        rgba = image::imageops::resize(&rgba, width, height, image::imageops::FilterType::Lanczos3);
    }
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return Err("decoded image has zero dimensions".to_owned());
    }
    Ok(ImageData::from_rgba8(width, height, rgba.into_raw()))
}
