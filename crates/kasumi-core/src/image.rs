//! CPU-side RGBA images shared with the renderer.
//!
//! Images stay in ordinary memory until the backend uploads them to its
//! texture atlas. The stable content id lets the renderer avoid uploading the
//! same image every frame.

use std::fmt;
use std::sync::Arc;

/// How an image is fitted into the rectangle reserved by layout.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFit {
    /// Preserve aspect ratio and fit the whole image inside the box.
    #[default]
    Contain,
    /// Preserve aspect ratio and fill the box, cropping the overflow.
    Cover,
    /// Stretch the image to the box.
    Fill,
    /// Draw at the image's logical pixel size, centered in the box.
    Original,
}

/// Straight-alpha, row-major RGBA8 image data.
#[derive(Clone)]
pub struct ImageData {
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
    id: u64,
}

impl ImageData {
    /// Creates image data from tightly packed, top-to-bottom RGBA8 pixels.
    ///
    /// # Panics
    ///
    /// Panics when either dimension is zero or when the byte count does not
    /// equal `width * height * 4`. Invalid image data is a programming error at
    /// this boundary; file and SVG decoders report their own recoverable errors.
    pub fn from_rgba8(width: u32, height: u32, pixels: impl Into<Arc<[u8]>>) -> Self {
        assert!(width > 0 && height > 0, "image dimensions must be non-zero");
        let pixels = pixels.into();
        assert_eq!(
            pixels.len(),
            width as usize * height as usize * 4,
            "RGBA image byte count does not match its dimensions"
        );
        let id =
            fnv1a(&pixels) ^ u64::from(width).rotate_left(24) ^ u64::from(height).rotate_left(48);
        Self {
            width,
            height,
            pixels,
            id,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Stable id derived from dimensions and pixel content.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Approximate memory held by the pixel buffer without cloning it.
    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }
}

impl PartialEq for ImageData {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.width == other.width
            && self.height == other.height
            && self.pixels == other.pixels
    }
}

impl fmt::Debug for ImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageData")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("id", &format_args!("{:#018x}", self.id))
            .field("bytes", &self.pixels.len())
            .finish()
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_id_is_stable_and_data_is_checked() {
        let a = ImageData::from_rgba8(1, 1, vec![1, 2, 3, 255]);
        let b = ImageData::from_rgba8(1, 1, vec![1, 2, 3, 255]);
        let c = ImageData::from_rgba8(1, 1, vec![1, 2, 4, 255]);
        assert_eq!(a, b);
        assert_ne!(a.id(), c.id());
        assert_eq!(a.width(), 1);
        assert_eq!(a.byte_len(), 4);
    }

    #[test]
    #[should_panic(expected = "byte count")]
    fn rejects_wrong_buffer_length() {
        let _ = ImageData::from_rgba8(2, 2, vec![0; 3]);
    }
}
