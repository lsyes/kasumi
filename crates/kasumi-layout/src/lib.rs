//! Taffy-backed layout with retained nodes and incremental invalidation.
//!
//! The taffy tree mirrors the retained node tree. Only nodes whose style,
//! content or child list changed are updated in taffy; taffy's own cache then
//! skips clean subtrees.

#![forbid(unsafe_code)]

use std::collections::HashMap;

use kasumi_core::{
    Align, FlexDirection, GridTrack, Justify, LayoutMode, Length, Point, Position, Rect, Size,
    Style, TextStyle,
};
use kasumi_tree::{NodeId, NodeKind, Tree};
use taffy::style_helpers::{auto, fr, line, max_content, min_content, span};
use taffy::{
    AlignItems, AlignSelf, AvailableSpace, Dimension, Display, FlexDirection as TaffyDirection,
    FlexWrap, GridTemplateComponent, JustifyContent, LengthPercentage, LengthPercentageAuto,
    NodeId as TaffyId, Overflow, Style as TaffyStyle, TaffyTree, compute_leaf_layout,
};

/// Implemented by the paint layer. Kept as a trait so layout has no dependency
/// on text shaping.
pub trait TextMeasure {
    fn measure_text(&self, text: &str, style: &TextStyle, max_width: Option<f32>) -> Size;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LayoutStats {
    pub nodes: usize,
    pub style_updates: usize,
    pub child_updates: usize,
    pub measures: usize,
    /// False when the layout generation and viewport are unchanged, so the
    /// previous absolute rectangles are still valid.
    pub computed: bool,
}

pub struct LayoutEngine {
    taffy: TaffyTree<NodeId>,
    nodes: HashMap<NodeId, TaffyId>,
    children: Vec<TaffyId>,
    rects: Vec<(NodeId, Rect)>,
    stale: Vec<NodeId>,
    known_node_count: usize,
    root: Option<TaffyId>,
    viewport: Size,
    applied_generation: u64,
    stats: LayoutStats,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            nodes: HashMap::new(),
            children: Vec::new(),
            rects: Vec::new(),
            stale: Vec::new(),
            known_node_count: 0,
            root: None,
            viewport: Size::ZERO,
            applied_generation: 0,
            stats: LayoutStats::default(),
        }
    }

    pub fn stats(&self) -> LayoutStats {
        self.stats
    }

    /// Drops the taffy mirror for nodes recycled by `Tree::end_frame`.
    pub fn remove_nodes(&mut self, removed: &[NodeId]) {
        let mut removed_count = 0;
        for node_id in removed {
            if let Some(taffy_id) = self.nodes.remove(node_id) {
                let _ = self.taffy.remove(taffy_id);
                removed_count += 1;
            }
        }
        self.known_node_count = self.known_node_count.saturating_sub(removed_count);
    }

    pub fn compute(
        &mut self,
        tree: &mut Tree,
        viewport: Size,
        measure: &dyn TextMeasure,
    ) -> LayoutStats {
        self.stats = LayoutStats::default();

        let root = tree.root();
        let root_taffy = self.nodes.get(&root).copied();
        if self.root == root_taffy
            && self.viewport == viewport
            && self.applied_generation == tree.layout_generation()
        {
            self.stats.nodes = tree.node_count();
            return self.stats;
        }

        // Callers are expected to use `remove_nodes`, but detect a changed
        // arena size here as a safe fallback for direct embedders.
        if self.known_node_count != tree.node_count() {
            self.stale.clear();
            self.stale.extend(
                self.nodes
                    .keys()
                    .copied()
                    .filter(|node| tree.get(*node).is_none()),
            );
            for node_id in &self.stale {
                if let Some(taffy_id) = self.nodes.remove(node_id) {
                    let _ = self.taffy.remove(taffy_id);
                }
            }
        }

        // 1. Create nodes that are new to taffy.
        for (node_id, node) in tree.iter() {
            if self.nodes.contains_key(&node_id) {
                continue;
            }
            let parent_kind = node
                .parent
                .and_then(|parent| tree.get(parent))
                .map(|node| node.kind);
            let style = to_taffy_style(node.kind, &node.style, parent_kind);
            let taffy_id = self
                .taffy
                .new_leaf_with_context(style, node_id)
                .expect("taffy node allocation failed");
            self.nodes.insert(node_id, taffy_id);
        }

        // 2. Push style, content and child changes directly from the arena.
        for (node_id, node) in tree.iter() {
            let taffy_id = self.nodes[&node_id];
            self.stats.nodes += 1;

            if node.style_dirty || node.content_dirty {
                let parent_kind = node
                    .parent
                    .and_then(|parent| tree.get(parent))
                    .map(|node| node.kind);
                let style = to_taffy_style(node.kind, &node.style, parent_kind);
                self.taffy
                    .set_style(taffy_id, style)
                    .expect("taffy set_style failed");
                self.stats.style_updates += 1;
            }

            if node.children_dirty {
                self.children.clear();
                self.children.extend(
                    node.children
                        .iter()
                        .filter_map(|child| self.nodes.get(child).copied()),
                );
                self.taffy
                    .set_children(taffy_id, &self.children)
                    .expect("taffy set_children failed");
                self.stats.child_updates += 1;
            }
        }

        // 3. The root gets an explicit viewport size. Updating only on resize
        //    preserves its layout cache.
        let root_taffy = self.nodes[&root];
        if self.viewport != viewport {
            self.viewport = viewport;
            let node = tree.get(root).expect("root disappeared");
            let mut style = to_taffy_style(node.kind, &node.style, None);
            style.size = taffy::Size {
                width: Dimension::length(viewport.width),
                height: Dimension::length(viewport.height),
            };
            self.taffy
                .set_style(root_taffy, style)
                .expect("taffy root set_style failed");
            self.taffy
                .mark_dirty(root_taffy)
                .expect("taffy root mark_dirty failed");
        } else if self.root != Some(root_taffy) {
            self.taffy
                .mark_dirty(root_taffy)
                .expect("taffy root mark_dirty failed");
        }
        self.root = Some(root_taffy);

        // 4. Run layout. The measure closure is only invoked for dirty leaves.
        let mut measures = 0usize;
        self.taffy
            .compute_layout_with_measure(
                root_taffy,
                taffy::Size {
                    width: AvailableSpace::Definite(viewport.width),
                    height: AvailableSpace::Definite(viewport.height),
                },
                |inputs, _taffy_id, context, taffy_style| {
                    let Some(node_id) = context.map(|id| *id) else {
                        return taffy::LayoutOutput::HIDDEN;
                    };
                    let Some(node) = tree.get(node_id) else {
                        return taffy::LayoutOutput::HIDDEN;
                    };
                    let Some(content) = node.text.as_ref() else {
                        return compute_leaf_layout(
                            inputs,
                            taffy_style,
                            |_, _| 0.0,
                            |known_dimensions, _available| taffy::Size {
                                width: known_dimensions.width.unwrap_or(0.0),
                                height: known_dimensions.height.unwrap_or(0.0),
                            },
                        );
                    };

                    let definite_width = match inputs.available_space.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    };
                    let width_hint = inputs
                        .known_dimensions
                        .width
                        .or(definite_width)
                        .map(|width| width.max(1.0));
                    measures += 1;
                    node.text_shape_width.set(width_hint);
                    let measured = measure.measure_text(&content.text, &content.style, width_hint);
                    compute_leaf_layout(
                        inputs,
                        taffy_style,
                        |_, _| 0.0,
                        |known_dimensions, _available| taffy::Size {
                            width: known_dimensions.width.unwrap_or(measured.width),
                            height: known_dimensions.height.unwrap_or(measured.height),
                        },
                    )
                },
            )
            .expect("taffy layout failed");
        self.stats.measures = measures;

        // 5. Convert taffy's parent-relative locations into absolute rects.
        self.apply_rects(tree, root);
        tree.update_scroll_metrics();

        // 6. A successful pass consumes all dirty flags.
        for (_, node) in tree.iter_mut() {
            node.style_dirty = false;
            node.content_dirty = false;
            node.children_dirty = false;
            node.layout_dirty = false;
        }

        self.applied_generation = tree.layout_generation();
        self.known_node_count = tree.node_count();
        self.stats.computed = true;
        self.stats
    }

    fn apply_rects(&mut self, tree: &mut Tree, root: NodeId) {
        self.rects.clear();
        collect_rects(
            &self.taffy,
            &self.nodes,
            tree,
            root,
            Point::ZERO,
            0,
            &mut self.rects,
        );
        for &(node_id, rect) in &self.rects {
            if let Some(node) = tree.get_mut(node_id) {
                node.rect = rect;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_rects(
    taffy: &TaffyTree<NodeId>,
    nodes: &HashMap<NodeId, TaffyId>,
    tree: &Tree,
    node_id: NodeId,
    parent_origin: Point,
    depth: usize,
    out: &mut Vec<(NodeId, Rect)>,
) {
    if depth > 512 {
        return;
    }
    let Some(taffy_id) = nodes.get(&node_id).copied() else {
        return;
    };
    let layout = match taffy.layout(taffy_id) {
        Ok(layout) => *layout,
        Err(_) => return,
    };
    let origin = Point::new(
        parent_origin.x + layout.location.x,
        parent_origin.y + layout.location.y,
    );
    out.push((
        node_id,
        Rect::new(
            origin,
            Size::new(layout.size.width.max(0.0), layout.size.height.max(0.0)),
        ),
    ));

    let Some(node) = tree.get(node_id) else {
        return;
    };
    for child in &node.children {
        collect_rects(taffy, nodes, tree, *child, origin, depth + 1, out);
    }
}

fn lp(value: f32) -> LengthPercentage {
    LengthPercentage::length(value)
}

fn lpa(value: f32) -> LengthPercentageAuto {
    LengthPercentageAuto::length(value)
}

fn dimension(value: Length) -> Dimension {
    match value {
        Length::Auto => Dimension::auto(),
        Length::Px(px) => Dimension::length(px),
        Length::Percent(pct) => Dimension::percent(pct / 100.0),
    }
}

fn length_percentage_auto(value: Length) -> LengthPercentageAuto {
    match value {
        Length::Auto => LengthPercentageAuto::auto(),
        Length::Px(px) => LengthPercentageAuto::length(px),
        Length::Percent(pct) => LengthPercentageAuto::percent(pct / 100.0),
    }
}

fn to_taffy_style(kind: NodeKind, style: &Style, parent_kind: Option<NodeKind>) -> TaffyStyle {
    let direction = match style.direction {
        FlexDirection::Row => TaffyDirection::Row,
        FlexDirection::Column => TaffyDirection::Column,
        FlexDirection::RowReverse => TaffyDirection::RowReverse,
        FlexDirection::ColumnReverse => TaffyDirection::ColumnReverse,
    };
    let align_items = match style.align_items {
        Align::Start => AlignItems::FLEX_START,
        Align::Center => AlignItems::CENTER,
        Align::End => AlignItems::FLEX_END,
        Align::Stretch => AlignItems::STRETCH,
        Align::Auto => AlignItems::STRETCH,
    };
    let align_self = match style.align_self {
        Align::Start => Some(AlignSelf::FLEX_START),
        Align::Center => Some(AlignSelf::CENTER),
        Align::End => Some(AlignSelf::FLEX_END),
        Align::Stretch => Some(AlignSelf::STRETCH),
        Align::Auto => None,
    };
    let justify_content = match style.justify {
        Justify::Start => JustifyContent::FLEX_START,
        Justify::Center => JustifyContent::CENTER,
        Justify::End => JustifyContent::FLEX_END,
        Justify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
        Justify::SpaceAround => JustifyContent::SPACE_AROUND,
        Justify::SpaceEvenly => JustifyContent::SPACE_EVENLY,
    };
    let grid_columns: Vec<GridTemplateComponent<String>> = style
        .grid_columns
        .iter()
        .copied()
        .map(grid_component)
        .collect();
    let grid_rows: Vec<GridTemplateComponent<String>> = style
        .grid_rows
        .iter()
        .copied()
        .map(grid_component)
        .collect();
    let is_stack = kind == NodeKind::Stack;
    let mut result = TaffyStyle {
        display: if style.hidden {
            Display::None
        } else if style.layout == LayoutMode::Grid || is_stack {
            Display::Grid
        } else {
            Display::Flex
        },
        flex_direction: direction,
        flex_wrap: if style.wrap {
            FlexWrap::Wrap
        } else {
            FlexWrap::NoWrap
        },
        size: taffy::Size {
            width: dimension(style.width),
            height: dimension(style.height),
        },
        min_size: taffy::Size {
            width: length_percentage_auto(style.min_width),
            height: length_percentage_auto(style.min_height),
        },
        max_size: taffy::Size {
            width: length_percentage_auto(style.max_width),
            height: length_percentage_auto(style.max_height),
        },
        padding: taffy::Rect {
            left: lp(style.padding.left),
            right: lp(style.padding.right),
            top: lp(style.padding.top),
            bottom: lp(style.padding.bottom),
        },
        margin: taffy::Rect {
            left: lpa(style.margin.left),
            right: lpa(style.margin.right),
            top: lpa(style.margin.top),
            bottom: lpa(style.margin.bottom),
        },
        gap: taffy::Size {
            width: lp(style.gap),
            height: lp(style.gap),
        },
        align_items,
        align_content: if style.layout == LayoutMode::Grid || is_stack {
            taffy::AlignContent::START
        } else {
            taffy::AlignContent::NORMAL
        },
        align_self,
        justify_content,
        flex_grow: style.flex_grow,
        flex_shrink: style.flex_shrink,
        position: match style.position {
            Position::Relative => taffy::Position::Relative,
            Position::Absolute => taffy::Position::Absolute,
        },
        inset: taffy::Rect {
            left: length_percentage_auto(style.inset.left),
            right: length_percentage_auto(style.inset.right),
            top: length_percentage_auto(style.inset.top),
            bottom: length_percentage_auto(style.inset.bottom),
        },
        overflow: taffy::Point {
            x: if style.clip {
                Overflow::Clip
            } else {
                Overflow::Visible
            },
            y: if style.clip {
                Overflow::Clip
            } else {
                Overflow::Visible
            },
        },
        grid_template_columns: if is_stack {
            vec![fr(1.0)]
        } else {
            grid_columns
        },
        grid_template_rows: if is_stack { vec![fr(1.0)] } else { grid_rows },
        grid_auto_rows: vec![auto()],
        grid_auto_columns: vec![auto()],
        grid_column: if style.grid_column_span > 1 {
            span(style.grid_column_span)
        } else {
            Default::default()
        },
        grid_row: if style.grid_row_span > 1 {
            span(style.grid_row_span)
        } else {
            Default::default()
        },
        ..Default::default()
    };
    if parent_kind == Some(NodeKind::Stack) && style.position == Position::Relative {
        result.grid_row = line(1);
        result.grid_column = line(1);
    }
    if kind == NodeKind::Text {
        // Text is a measured leaf; keeping shrink at 1 lets a fixed-width
        // parent clip it rather than overflowing the row.
        result.flex_shrink = 1.0;
    }
    result
}

fn grid_component(track: GridTrack) -> GridTemplateComponent<String> {
    match track {
        GridTrack::Auto => auto(),
        GridTrack::MinContent => min_content(),
        GridTrack::MaxContent => max_content(),
        GridTrack::Px(value) => {
            taffy::style_helpers::length::<f32, GridTemplateComponent<String>>(value)
        }
        GridTrack::Percent(value) => {
            taffy::style_helpers::percent::<f32, GridTemplateComponent<String>>(value / 100.0)
        }
        GridTrack::Fr(value) => fr::<f32, GridTemplateComponent<String>>(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasumi_core::{Id, Style};

    struct FixedMeasure;

    impl TextMeasure for FixedMeasure {
        fn measure_text(&self, text: &str, style: &TextStyle, _max_width: Option<f32>) -> Size {
            Size::new(
                text.chars().count() as f32 * style.size * 0.5,
                style.size * 1.4,
            )
        }
    }

    #[test]
    fn lays_out_a_column() {
        let mut tree = Tree::new();
        tree.begin_frame(Size::new(200.0, 200.0));
        let column = tree.ensure(Id::from_raw(1), NodeKind::Container);
        tree.set_style(column, Style::column().with_size_full());
        tree.declare_child(tree.root(), column);
        let a = tree.ensure(Id::from_raw(2), NodeKind::Container);
        tree.set_style(a, Style::default().with_height(40.0));
        tree.declare_child(column, a);
        let b = tree.ensure(Id::from_raw(3), NodeKind::Container);
        tree.set_style(b, Style::default().with_height(60.0));
        tree.declare_child(column, b);
        tree.end_frame();

        let mut engine = LayoutEngine::new();
        engine.compute(&mut tree, Size::new(200.0, 200.0), &FixedMeasure);
        assert_eq!(
            tree.rect_of(Id::from_raw(1)),
            Rect::from_xywh(0.0, 0.0, 200.0, 200.0)
        );
        assert_eq!(
            tree.rect_of(Id::from_raw(2)),
            Rect::from_xywh(0.0, 0.0, 200.0, 40.0)
        );
        assert_eq!(
            tree.rect_of(Id::from_raw(3)),
            Rect::from_xywh(0.0, 40.0, 200.0, 60.0)
        );
    }

    #[test]
    fn clean_frame_does_no_work() {
        let mut tree = Tree::new();
        tree.begin_frame(Size::new(100.0, 100.0));
        let a = tree.ensure(Id::from_raw(1), NodeKind::Container);
        tree.set_style(a, Style::default().with_size_full());
        tree.declare_child(tree.root(), a);
        tree.end_frame();
        let mut engine = LayoutEngine::new();
        engine.compute(&mut tree, Size::new(100.0, 100.0), &FixedMeasure);

        tree.begin_frame(Size::new(100.0, 100.0));
        let a = tree.ensure(Id::from_raw(1), NodeKind::Container);
        tree.declare_child(tree.root(), a);
        tree.end_frame();
        let stats = engine.compute(&mut tree, Size::new(100.0, 100.0), &FixedMeasure);
        assert!(!stats.computed);
        assert_eq!(stats.style_updates, 0);
        assert_eq!(stats.child_updates, 0);
        assert_eq!(stats.measures, 0);
        assert_eq!(stats.nodes, 2);

        tree.begin_frame(Size::new(100.0, 100.0));
        let a = tree.ensure(Id::from_raw(1), NodeKind::Container);
        tree.set_style(a, Style::default().with_height(50.0));
        tree.declare_child(tree.root(), a);
        tree.end_frame();
        let stats = engine.compute(&mut tree, Size::new(100.0, 100.0), &FixedMeasure);
        assert!(stats.computed);
        assert_eq!(stats.style_updates, 1);
    }

    #[test]
    fn lays_out_grid_tracks_in_rows() {
        let mut tree = Tree::new();
        tree.begin_frame(Size::new(200.0, 160.0));
        let grid = tree.ensure(Id::from_raw(1), NodeKind::Container);
        tree.set_style(
            grid,
            Style::grid([GridTrack::fr(1.0), GridTrack::fr(1.0)])
                .with_width(200.0)
                .with_height(160.0),
        );
        tree.declare_child(tree.root(), grid);
        for index in 0..4 {
            let child = tree.ensure(Id::from_raw(10 + index), NodeKind::Container);
            tree.set_style(child, Style::default().with_height(40.0));
            tree.declare_child(grid, child);
        }
        tree.end_frame();

        let mut engine = LayoutEngine::new();
        engine.compute(&mut tree, Size::new(200.0, 160.0), &FixedMeasure);
        assert_eq!(
            tree.rect_of(Id::from_raw(10)),
            Rect::from_xywh(0.0, 0.0, 100.0, 40.0)
        );
        assert_eq!(
            tree.rect_of(Id::from_raw(11)),
            Rect::from_xywh(100.0, 0.0, 100.0, 40.0)
        );
        assert_eq!(
            tree.rect_of(Id::from_raw(12)),
            Rect::from_xywh(0.0, 40.0, 100.0, 40.0)
        );
        assert_eq!(
            tree.rect_of(Id::from_raw(13)),
            Rect::from_xywh(100.0, 40.0, 100.0, 40.0)
        );
    }

    #[test]
    fn stack_children_share_a_cell() {
        let mut tree = Tree::new();
        tree.begin_frame(Size::new(120.0, 80.0));
        let stack = tree.ensure(Id::from_raw(1), NodeKind::Stack);
        tree.set_style(stack, Style::default().with_size_full());
        tree.declare_child(tree.root(), stack);
        for index in 0..2 {
            let child = tree.ensure(Id::from_raw(10 + index), NodeKind::Container);
            tree.set_style(child, Style::default().with_width(40.0).with_height(20.0));
            tree.declare_child(stack, child);
        }
        tree.end_frame();

        let mut engine = LayoutEngine::new();
        engine.compute(&mut tree, Size::new(120.0, 80.0), &FixedMeasure);
        assert_eq!(
            tree.rect_of(Id::from_raw(10)),
            tree.rect_of(Id::from_raw(11))
        );
    }
}
