//! Font discovery, style matching and fallback.
//!
//! `fontdb` enumerates platform fonts and answers CSS-like style queries.
//! Font bytes are loaded lazily and cached; a face is only registered in the
//! public [`FontFaceId`] table when shaping or rasterization actually needs it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use fontdb::{Database, Family, Query, Stretch, Style as DbStyle, Weight as DbWeight};
use kasumi_core::{FontFaceId, FontFamily};
use swash::text::cluster::CharCluster;
use swash::{
    Attributes, CacheKey, FontRef, Metrics, Stretch as SwashStretch, Style as SwashStyle,
    Synthesis, Weight as SwashWeight,
};

use crate::shape::{FallbackCategory, cluster_covered, fallback_category, is_ignorable_char};

/// Immutable font data shared by every weight/style instance of a face.
pub(crate) struct FaceData {
    pub(crate) data: Arc<dyn AsRef<[u8]> + Send + Sync>,
    pub(crate) offset: u32,
    pub(crate) key: CacheKey,
    pub(crate) metrics: Metrics,
}

impl FaceData {
    pub(crate) fn font(&self) -> FontRef<'_> {
        FontRef {
            data: self.data.as_ref().as_ref(),
            offset: self.offset,
            key: self.key,
        }
    }
}

/// A face combined with the requested weight and style. Variable-font axes and
/// faux bold/italic are described by `synthesis`.
pub(crate) struct FaceInstance {
    pub(crate) face_id: FontFaceId,
    pub(crate) data: Arc<FaceData>,
    pub(crate) synthesis: Synthesis,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FaceKey {
    id: fontdb::ID,
    weight: u16,
    italic: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PrimaryKey {
    family: FontFamily,
    weight: u16,
    italic: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FallbackKey {
    weight: u16,
    italic: bool,
    category: FallbackCategory,
}

pub(crate) struct FontStore {
    db: Database,
    instances: Vec<Arc<FaceInstance>>,
    instance_ids: HashMap<FaceKey, FontFaceId>,
    data: HashMap<fontdb::ID, Option<Arc<FaceData>>>,
    primary: HashMap<PrimaryKey, FontFaceId>,
    fallback_lists: HashMap<FallbackKey, Vec<fontdb::ID>>,
    fallback_chars: HashMap<(u16, bool, char), Option<FontFaceId>>,
}

impl FontStore {
    pub(crate) fn new() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        Self::from_db(db)
    }

    pub(crate) fn from_db(mut db: Database) -> Self {
        configure_generic_families(&mut db);
        Self {
            db,
            instances: Vec::new(),
            instance_ids: HashMap::new(),
            data: HashMap::new(),
            primary: HashMap::new(),
            fallback_lists: HashMap::new(),
            fallback_chars: HashMap::new(),
        }
    }

    pub(crate) fn face_count(&self) -> usize {
        self.instances.len()
    }

    pub(crate) fn instance(&self, face: FontFaceId) -> Option<&Arc<FaceInstance>> {
        self.instances.get(face.0 as usize)
    }

    /// Resolves the primary face for a style request. Generic families are
    /// configured by `configure_generic_families`; a missing named family
    /// falls back to the generic sans-serif.
    pub(crate) fn primary(
        &mut self,
        family: &FontFamily,
        weight: u16,
        italic: bool,
    ) -> Option<FontFaceId> {
        let key = PrimaryKey {
            family: family.clone(),
            weight,
            italic,
        };
        if let Some(face) = self.primary.get(&key) {
            return Some(*face);
        }

        let db_style = if italic {
            DbStyle::Italic
        } else {
            DbStyle::Normal
        };
        let db_weight = DbWeight(weight);
        let mut families = match family {
            FontFamily::SansSerif => vec![Family::SansSerif],
            FontFamily::Serif => vec![Family::Serif],
            FontFamily::Monospace => vec![Family::Monospace],
            FontFamily::Named(name) => vec![Family::Name(name.as_str())],
        };
        let query = Query {
            families: &families,
            weight: db_weight,
            style: db_style,
            stretch: Stretch::Normal,
        };
        let mut id = self.db.query(&query);
        if id.is_none() && matches!(family, FontFamily::Named(_)) {
            families = vec![Family::SansSerif];
            id = self.db.query(&Query {
                families: &families,
                weight: db_weight,
                style: db_style,
                stretch: Stretch::Normal,
            });
        }

        if let Some(id) = id
            && let Some(face) = self.register(id, weight, italic)
        {
            self.primary.insert(key, face);
            return Some(face);
        }
        // A font database can contain entries that fail to parse, and a
        // caller-provided database may have no face matching the query.
        // Fall back to any loadable face rather than returning no text at all.
        let candidates: Vec<fontdb::ID> = self.db.faces().map(|face| face.id).collect();
        for id in candidates {
            if let Some(face) = self.register(id, weight, italic) {
                self.primary.insert(key, face);
                return Some(face);
            }
        }
        None
    }

    /// Chooses a face for a character cluster, maps the cluster to that
    /// face's nominal glyphs and returns the face id.
    pub(crate) fn select_for_cluster(
        &mut self,
        cluster: &mut CharCluster,
        primary: FontFaceId,
        weight: u16,
        italic: bool,
    ) -> Option<FontFaceId> {
        let face = self.choose_face(cluster, primary, weight, italic)?;
        let instance = self.instances.get(face.0 as usize)?.clone();
        let charmap = instance.data.font().charmap();
        // The partition shaper expects the selector to have mapped the
        // cluster's characters for the font it returns.
        cluster.map(|ch| charmap.map(ch));
        Some(face)
    }

    fn choose_face(
        &mut self,
        cluster: &CharCluster,
        primary: FontFaceId,
        weight: u16,
        italic: bool,
    ) -> Option<FontFaceId> {
        if let Some(instance) = self.instance(primary)
            && cluster_covered(instance, cluster)
        {
            return Some(primary);
        }

        let relevant: Vec<char> = cluster
            .chars()
            .iter()
            .filter(|ch| !ch.ignorable && ch.contributes_to_shaping && !is_ignorable_char(ch.ch))
            .map(|ch| ch.ch)
            .collect();
        if relevant.is_empty() {
            return Some(primary);
        }

        // Faces remembered for individual characters are the fastest path on
        // repeated CJK or emoji runs.
        let mut cached: Vec<FontFaceId> = Vec::new();
        for ch in &relevant {
            if let Some(Some(face)) = self.fallback_chars.get(&(weight, italic, *ch))
                && !cached.contains(face)
            {
                cached.push(*face);
            }
        }
        for face in cached {
            if let Some(instance) = self.instance(face).cloned()
                && chars_covered(&instance, &relevant)
            {
                return Some(face);
            }
        }

        let category = fallback_category(cluster);
        let candidates = self.fallback_ids(weight, italic, category);
        for id in candidates {
            if !self.chars_covered_by_id(id, &relevant) {
                continue;
            }
            if let Some(face) = self.register(id, weight, italic) {
                for ch in &relevant {
                    self.fallback_chars
                        .insert((weight, italic, *ch), Some(face));
                }
                return Some(face);
            }
        }

        self.fallback_chars
            .insert((weight, italic, relevant[0]), None);
        Some(primary)
    }

    fn chars_covered_by_id(&mut self, id: fontdb::ID, chars: &[char]) -> bool {
        let Some(data) = self.load_data(id) else {
            return false;
        };
        let font = data.font();
        let charmap = font.charmap();
        chars
            .iter()
            .all(|ch| is_ignorable_char(*ch) || charmap.map(*ch) != 0)
    }

    fn register(&mut self, id: fontdb::ID, weight: u16, italic: bool) -> Option<FontFaceId> {
        let key = FaceKey { id, weight, italic };
        if let Some(face) = self.instance_ids.get(&key) {
            return Some(*face);
        }
        let data = self.load_data(id)?;
        let requested = Attributes::new(
            SwashStretch::NORMAL,
            SwashWeight(weight),
            if italic {
                SwashStyle::Italic
            } else {
                SwashStyle::Normal
            },
        );
        let synthesis = data.font().attributes().synthesize(requested);
        let face_id = FontFaceId(self.instances.len() as u64);
        self.instances.push(Arc::new(FaceInstance {
            face_id,
            data,
            synthesis,
        }));
        self.instance_ids.insert(key, face_id);
        Some(face_id)
    }

    fn load_data(&mut self, id: fontdb::ID) -> Option<Arc<FaceData>> {
        if let Some(cached) = self.data.get(&id) {
            return cached.clone();
        }
        let loaded = self.load_data_inner(id);
        self.data.insert(id, loaded.clone());
        loaded
    }

    fn load_data_inner(&self, id: fontdb::ID) -> Option<Arc<FaceData>> {
        let (source, index) = {
            let face = self.db.face(id)?;
            (face.source.clone(), face.index)
        };
        let data: Arc<dyn AsRef<[u8]> + Send + Sync> = match source {
            fontdb::Source::Binary(data) => data,
            fontdb::Source::File(path) => Arc::new(std::fs::read(path).ok()?),
            fontdb::Source::SharedFile(_, data) => data,
        };
        let (offset, key, metrics) = {
            let font = FontRef::from_index(data.as_ref().as_ref(), index as usize)?;
            (font.offset, font.key, font.metrics(&[]))
        };
        Some(Arc::new(FaceData {
            data,
            offset,
            key,
            metrics,
        }))
    }

    fn fallback_ids(
        &mut self,
        weight: u16,
        italic: bool,
        category: FallbackCategory,
    ) -> Vec<fontdb::ID> {
        let key = FallbackKey {
            weight,
            italic,
            category,
        };
        if let Some(cached) = self.fallback_lists.get(&key) {
            return cached.clone();
        }

        // One face per family is enough: coverage is a property of the family
        // in practice, and trying every weight variant would multiply font
        // loads for no benefit. Pick the variant closest to the requested
        // style and weight.
        let mut best: HashMap<String, (u32, u8, u16, fontdb::ID)> = HashMap::new();
        for face in self.db.faces() {
            let Some((family, _)) = face.families.first() else {
                continue;
            };
            if family.is_empty() {
                continue;
            }
            let rank = family_rank(family, category);
            let style_penalty = if matches_style(face.style, italic) {
                0
            } else {
                1
            };
            let weight_distance = face.weight.0.abs_diff(weight);
            let candidate = (rank, style_penalty, weight_distance, face.id);
            best.entry(family.clone())
                .and_modify(|current| {
                    if (candidate.0, candidate.1, candidate.2) < (current.0, current.1, current.2) {
                        *current = candidate;
                    }
                })
                .or_insert(candidate);
        }

        let mut rows: Vec<(u32, u8, u16, String, fontdb::ID)> = best
            .into_iter()
            .map(|(family, (rank, style, distance, id))| (rank, style, distance, family, id))
            .collect();
        rows.sort_by(|a, b| (a.0, a.1, a.2, &a.3).cmp(&(b.0, b.1, b.2, &b.3)));
        let ids = rows.into_iter().map(|row| row.4).collect::<Vec<_>>();
        self.fallback_lists.insert(key, ids.clone());
        ids
    }
}

fn chars_covered(instance: &FaceInstance, chars: &[char]) -> bool {
    let charmap = instance.data.font().charmap();
    chars
        .iter()
        .all(|ch| is_ignorable_char(*ch) || charmap.map(*ch) != 0)
}

fn matches_style(style: DbStyle, italic: bool) -> bool {
    if italic {
        matches!(style, DbStyle::Italic | DbStyle::Oblique)
    } else {
        matches!(style, DbStyle::Normal)
    }
}

const EMOJI_FAMILIES: &[&str] = &[
    "Noto Color Emoji",
    "Apple Color Emoji",
    "Segoe UI Emoji",
    "Twemoji Mozilla",
    "EmojiOne Color",
    "Noto Emoji",
    "Symbola",
];

const CJK_FAMILIES: &[&str] = &[
    "Noto Sans CJK SC",
    "Noto Sans CJK TC",
    "Noto Sans CJK JP",
    "Noto Sans CJK KR",
    "Noto Serif CJK SC",
    "Noto Serif CJK TC",
    "Noto Serif CJK JP",
    "Noto Serif CJK KR",
    "Source Han Sans SC",
    "Source Han Sans",
    "Source Han Serif SC",
    "Noto Sans SC",
    "Noto Sans TC",
    "Noto Sans JP",
    "Noto Sans KR",
    "Noto Serif SC",
    "Noto Serif TC",
    "Noto Serif JP",
    "Noto Serif KR",
    "WenQuanYi Micro Hei",
    "WenQuanYi Zen Hei",
    "Droid Sans Fallback",
    "PingFang SC",
    "Microsoft YaHei",
    "Hiragino Sans GB",
];

const GENERIC_FAMILIES: &[&str] = &[
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
    "FreeSans",
    "Arial",
    "Segoe UI",
    "Helvetica Neue",
    "Helvetica",
    "Roboto",
    "Ubuntu",
    "Cantarell",
    "Inter",
    "Noto Sans Arabic",
    "Noto Naskh Arabic",
    "Noto Sans Hebrew",
    "Noto Sans Devanagari",
    "Noto Sans Thai",
    "Noto Sans Symbols",
    "Noto Serif",
    "DejaVu Serif",
    "Liberation Serif",
    "Times New Roman",
    "Noto Sans Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Cascadia Code",
    "JetBrains Mono",
    "Courier New",
];

fn family_rank(family: &str, category: FallbackCategory) -> u32 {
    let lists: &[&[&str]] = match category {
        FallbackCategory::Emoji => &[EMOJI_FAMILIES, GENERIC_FAMILIES],
        FallbackCategory::Cjk => &[CJK_FAMILIES, GENERIC_FAMILIES],
        FallbackCategory::Generic => &[GENERIC_FAMILIES, CJK_FAMILIES],
    };
    for (list_index, list) in lists.iter().enumerate() {
        if let Some(position) = list
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(family))
        {
            return list_index as u32 * 1_000 + position as u32;
        }
    }
    10_000
}

fn configure_generic_families(db: &mut Database) {
    let available: HashSet<String> = db
        .faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect();

    fn pick(available: &HashSet<String>, candidates: &[&str]) -> Option<String> {
        candidates
            .iter()
            .find(|candidate| available.contains(**candidate))
            .map(|candidate| (*candidate).to_owned())
    }

    if let Some(family) = pick(
        &available,
        &[
            "Inter",
            "Noto Sans",
            "DejaVu Sans",
            "Liberation Sans",
            "Cantarell",
            "Ubuntu",
            "Open Sans",
        ],
    ) {
        db.set_sans_serif_family(family);
    }
    if let Some(family) = pick(
        &available,
        &["Noto Serif", "DejaVu Serif", "Liberation Serif", "Georgia"],
    ) {
        db.set_serif_family(family);
    }
    if let Some(family) = pick(
        &available,
        &[
            "JetBrains Mono",
            "Noto Sans Mono",
            "DejaVu Sans Mono",
            "Liberation Mono",
            "Cascadia Code",
        ],
    ) {
        db.set_monospace_family(family);
    }
}
