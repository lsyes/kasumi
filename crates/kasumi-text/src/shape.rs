//! Unicode analysis, itemization and shaping.
//!
//! The pipeline is deliberately small:
//!
//! 1. analyze every character once (script, bidi class, boundaries);
//! 2. resolve bidi levels with `unicode-bidi` for non-ASCII paragraphs;
//! 3. split into runs with one level and one script;
//! 4. shape each run with swash, selecting a font per character cluster.
//!
//! swash's partition shaper lets the font selector run at cluster
//! granularity, so fallback does not require heuristically shaping every
//! candidate font. That is where most of the speedup over a conventional
//! "shape, detect missing glyphs, reshape" loop comes from.

use std::sync::Arc;

use kasumi_core::{FontFaceId, TextStyle};
use swash::shape::partition::SimpleShapeOptions;
use swash::shape::partition::{self, SelectedFont, Selector};
use swash::shape::{Direction, ShapeContext};
use swash::text::cluster::{Boundary, CharCluster, CharInfo, Token};
use swash::text::{Codepoint, Properties, Script, analyze};
use swash::{FontRef, Synthesis};
use unicode_bidi::ParagraphBidiInfo;

use crate::font::{FaceInstance, FontStore};

#[derive(Debug, Clone, Copy)]
pub(crate) struct RawGlyph {
    pub(crate) id: u16,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) is_mark: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RawCluster {
    pub(crate) face: FontFaceId,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) glyph_start: usize,
    pub(crate) glyph_end: usize,
    pub(crate) advance: f32,
    pub(crate) level: u8,
    pub(crate) whitespace: bool,
}

#[derive(Debug, Default)]
pub(crate) struct ShapedText {
    pub(crate) glyphs: Vec<RawGlyph>,
    pub(crate) clusters: Vec<RawCluster>,
}

#[derive(Clone, Copy)]
struct CharMeta {
    offset: usize,
    ch: char,
    properties: Properties,
    boundary: Boundary,
}

/// Shapes one paragraph. `base` is the paragraph's byte offset in the caller's
/// string; token offsets are absolute so callers can concatenate paragraphs.
pub(crate) fn shape_paragraph(
    fonts: &mut FontStore,
    context: &mut ShapeContext,
    text: &str,
    base: usize,
    primary: FontFaceId,
    style: &TextStyle,
    out: &mut ShapedText,
) {
    out.glyphs.clear();
    out.clusters.clear();
    if text.is_empty() {
        return;
    }

    let metas: Vec<CharMeta> = text
        .char_indices()
        .zip(analyze(text.chars()))
        .map(|((offset, ch), (properties, boundary))| CharMeta {
            offset,
            ch,
            properties,
            boundary,
        })
        .collect();

    // ASCII is by far the common case and cannot contain an RTL character.
    // Skipping the bidi pass also keeps the hot path free of temporary level
    // vectors for labels and code.
    let bidi = if text.is_ascii() {
        None
    } else {
        Some(ParagraphBidiInfo::new(text, None))
    };
    let levels = bidi.as_ref().map(|info| info.levels.as_slice());
    let size = style.size.max(1.0);

    let mut run_start = 0;
    let mut run_script = Script::Latin;
    let mut run_level = 0u8;
    let mut has_run = false;

    for (index, meta) in metas.iter().enumerate() {
        let script = meta.properties.script();
        let common = matches!(script, Script::Common | Script::Inherited | Script::Unknown);
        let level = levels
            .and_then(|levels| levels.get(meta.offset))
            .map(|level| level.number())
            .unwrap_or(0);

        if !has_run {
            has_run = true;
            run_start = index;
            run_script = if common { Script::Latin } else { script };
            run_level = level;
            continue;
        }

        if level != run_level || (!common && script != run_script) {
            shape_run(
                fonts,
                context,
                &metas[run_start..index],
                base,
                run_script,
                run_level,
                primary,
                size,
                style,
                out,
            );
            run_start = index;
            run_script = if common { Script::Latin } else { script };
            run_level = level;
        }
    }

    if has_run {
        shape_run(
            fonts,
            context,
            &metas[run_start..],
            base,
            run_script,
            run_level,
            primary,
            size,
            style,
            out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn shape_run(
    fonts: &mut FontStore,
    context: &mut ShapeContext,
    metas: &[CharMeta],
    base: usize,
    script: Script,
    level: u8,
    primary: FontFaceId,
    size: f32,
    style: &TextStyle,
    out: &mut ShapedText,
) {
    if metas.is_empty() {
        return;
    }
    let tokens: Vec<Token> = metas
        .iter()
        .map(|meta| Token {
            ch: meta.ch,
            offset: (base + meta.offset) as u32,
            len: meta.ch.len_utf8() as u8,
            info: CharInfo::new(meta.properties, meta.boundary),
            data: 0,
        })
        .collect();

    let direction = if level % 2 == 1 {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    };
    let options = SimpleShapeOptions {
        script,
        direction,
        size,
        ..SimpleShapeOptions::default()
    };
    let mut selector = ClusterSelector {
        fonts,
        primary,
        weight: style.weight.0,
        italic: style.italic,
    };

    partition::shape(context, &mut selector, &options, tokens, |font, shaper| {
        shaper.shape_with(|cluster| {
            let start = cluster.source.start as usize;
            let end = cluster.source.end as usize;
            let whitespace = cluster.info.is_whitespace();
            let glyph_start = out.glyphs.len();
            let mut advance = 0.0;
            for glyph in cluster.glyphs {
                advance += glyph.advance;
                // Missing glyphs and whitespace have no visible mask.
                // Their advance still contributes to the line width.
                if whitespace || glyph.id == 0 {
                    continue;
                }
                out.glyphs.push(RawGlyph {
                    id: glyph.id,
                    x: glyph.x,
                    y: glyph.y,
                    is_mark: glyph.info.is_mark(),
                });
            }
            // Swash positions an attached mark relative to
            // `base.x - base.advance` in both directions, but an RTL cluster
            // is still laid out with a positive pen advance. Add the cluster
            // advance back to marks so they overlay their base instead of
            // hanging to the left of it.
            if level % 2 == 1 {
                for glyph in &mut out.glyphs[glyph_start..] {
                    if glyph.is_mark {
                        glyph.x += advance;
                    }
                }
            }
            out.clusters.push(RawCluster {
                face: font.instance.face_id,
                start,
                end,
                glyph_start,
                glyph_end: out.glyphs.len(),
                advance,
                level,
                whitespace,
            });
        });
    });
}

struct SelectedFace {
    instance: Arc<FaceInstance>,
}

impl PartialEq for SelectedFace {
    fn eq(&self, other: &Self) -> bool {
        self.instance.face_id == other.instance.face_id
    }
}

impl SelectedFont for SelectedFace {
    fn font(&self) -> FontRef<'_> {
        self.instance.data.font()
    }

    fn synthesis(&self) -> Option<Synthesis> {
        let synthesis = self.instance.synthesis;
        synthesis.any().then_some(synthesis)
    }
}

struct ClusterSelector<'a> {
    fonts: &'a mut FontStore,
    primary: FontFaceId,
    weight: u16,
    italic: bool,
}

impl Selector for ClusterSelector<'_> {
    type SelectedFont = SelectedFace;

    fn select_font(&mut self, cluster: &mut CharCluster) -> Option<Self::SelectedFont> {
        let face =
            self.fonts
                .select_for_cluster(cluster, self.primary, self.weight, self.italic)?;
        let instance = self.fonts.instance(face)?.clone();
        Some(SelectedFace { instance })
    }
}

/// Returns true when a cluster contains at least one character that maps to a
/// real glyph in `instance`. Variation selectors, ZWJ controls and other
/// default ignorables are skipped: fonts do not map them, but they must not
/// force a fallback either.
pub(crate) fn cluster_covered(instance: &FaceInstance, cluster: &CharCluster) -> bool {
    let charmap = instance.data.font().charmap();
    for ch in cluster.chars() {
        if ch.ignorable || !ch.contributes_to_shaping || is_ignorable_char(ch.ch) {
            continue;
        }
        if charmap.map(ch.ch) == 0 {
            return false;
        }
    }
    true
}

/// Characters that participate in shaping but never have a nominal glyph.
pub(crate) fn is_ignorable_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{200b}'..='\u{200f}'
                | '\u{2060}'..='\u{206f}'
                | '\u{fe00}'..='\u{fe0f}'
                | '\u{e0100}'..='\u{e01ef}'
        )
}

/// The fallback order depends on what kind of text we are looking for. CJK and
/// emoji fonts are frequently not the first faces in a system database, and
/// scanning hundreds of unrelated families per character would erase the
/// benefit of cluster-level selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FallbackCategory {
    Generic,
    Cjk,
    Emoji,
}

pub(crate) fn fallback_category(cluster: &CharCluster) -> FallbackCategory {
    let Some(ch) = cluster
        .chars()
        .iter()
        .find(|ch| !ch.ignorable && !is_ignorable_char(ch.ch))
        .map(|ch| ch.ch)
    else {
        return FallbackCategory::Generic;
    };
    let properties = ch.properties();
    if properties.is_emoji() || properties.is_extended_pictographic() || is_emoji_range(ch) {
        return FallbackCategory::Emoji;
    }
    // Fullwidth punctuation and CJK symbols are Common script even though
    // they belong next to Han text. Treat them as CJK so a mixed paragraph
    // does not fall back to an unrelated Latin face for its punctuation.
    if is_cjk_range(ch) {
        return FallbackCategory::Cjk;
    }
    match properties.script() {
        Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul | Script::Bopomofo => {
            FallbackCategory::Cjk
        }
        _ => FallbackCategory::Generic,
    }
}

fn is_emoji_range(ch: char) -> bool {
    matches!(ch as u32, 0x1f000..=0x1faff | 0x2600..=0x27bf)
}

fn is_cjk_range(ch: char) -> bool {
    matches!(
        ch as u32,
        0x2e80..=0x2fff
            | 0x3000..=0x303f
            | 0x3100..=0x312f
            | 0x31a0..=0x31bf
            | 0x3400..=0x4dbf
            | 0x4e00..=0x9fff
            | 0xf900..=0xfaff
            | 0xfe30..=0xfe4f
            | 0xff00..=0xffef
            | 0x20000..=0x2fa1f
    )
}
