//! Layout cache keyed by text, shaping attributes and available width.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::Arc;

use kasumi_core::{FontFamily, TextStyle, TextWrap};

use crate::TextLayout;

#[derive(Debug, Clone)]
struct CachedLayout {
    text: Arc<str>,
    style: TextStyle,
    width: Option<f32>,
    layout: Arc<TextLayout>,
}

/// FNV-1a. Layout keys are short and mostly compared cache hits; a
/// cryptographic hasher costs more than the lookup itself.
#[derive(Clone, Copy)]
struct TextHasher(u64);

impl Default for TextHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for TextHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

type LayoutBuckets = HashMap<u64, Vec<CachedLayout>, BuildHasherDefault<TextHasher>>;

#[derive(Default)]
pub(crate) struct LayoutCache {
    buckets: LayoutBuckets,
    len: usize,
}

impl LayoutCache {
    // The demo settles around a few hundred distinct layouts. A wholesale
    // clear is predictable and keeps the cache free of per-entry bookkeeping.
    const LIMIT: usize = 2048;

    fn hash(text: &str, style: &TextStyle, width: Option<f32>) -> u64 {
        let mut hasher = TextHasher::default();
        text.hash(&mut hasher);
        family_key(&style.family).hash(&mut hasher);
        style.weight.0.hash(&mut hasher);
        style.italic.hash(&mut hasher);
        style.size.to_bits().hash(&mut hasher);
        style.line_height.map(f32::to_bits).hash(&mut hasher);
        style.letter_spacing.to_bits().hash(&mut hasher);
        matches!(style.wrap, TextWrap::Word).hash(&mut hasher);
        width.map(f32::to_bits).hash(&mut hasher);
        hasher.finish()
    }

    pub(crate) fn get(
        &self,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
    ) -> Option<Arc<TextLayout>> {
        let hash = Self::hash(text, style, width);
        let bucket = self.buckets.get(&hash)?;
        bucket.iter().find_map(|entry| {
            if entry.text.as_ref() == text
                && shaping_eq(&entry.style, style)
                && entry.width.map(f32::to_bits) == width.map(f32::to_bits)
            {
                Some(entry.layout.clone())
            } else {
                None
            }
        })
    }

    pub(crate) fn insert(
        &mut self,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
        layout: Arc<TextLayout>,
    ) {
        if self.len >= Self::LIMIT {
            self.buckets.clear();
            self.len = 0;
        }
        let hash = Self::hash(text, style, width);
        let bucket = self.buckets.entry(hash).or_default();
        if bucket.len() >= 8 {
            bucket.remove(0);
            self.len -= 1;
        }
        bucket.push(CachedLayout {
            text: Arc::from(text),
            style: style.clone(),
            width,
            layout,
        });
        self.len += 1;
    }
}

/// Converts a family request to the data that actually affects shaping.
/// Keeping this separate from `hash` lets the cache avoid hashing the
/// generic-family discriminant and the named string in different ways.
fn family_key(family: &FontFamily) -> (&'static str, &str) {
    match family {
        FontFamily::SansSerif => ("sans-serif", ""),
        FontFamily::Serif => ("serif", ""),
        FontFamily::Monospace => ("monospace", ""),
        FontFamily::Named(name) => ("named", name),
    }
}

fn shaping_eq(a: &TextStyle, b: &TextStyle) -> bool {
    a.family == b.family
        && a.weight == b.weight
        && a.italic == b.italic
        && a.size.to_bits() == b.size.to_bits()
        && a.line_height.map(f32::to_bits) == b.line_height.map(f32::to_bits)
        && a.letter_spacing.to_bits() == b.letter_spacing.to_bits()
        && matches!(a.wrap, TextWrap::Word) == matches!(b.wrap, TextWrap::Word)
}
