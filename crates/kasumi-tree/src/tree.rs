//! The retained node tree and its per-frame reconciliation.
//!
//! The UI layer declares nodes by id during `view`; this module matches those
//! declarations against the previous frame, commits child order at the end of
//! the frame and recycles nodes that were not touched.

use std::collections::HashMap;

use kasumi_core::{Id, Point, Rect, Size, Style, TextContent, Vec2};

use crate::arena::{Arena, NodeId};
use crate::node::{Node, NodeKind, Sense};

/// Interactive rail width. The painted thumb is narrower, but the whole rail
/// remains easy to grab.
const SCROLLBAR_THICKNESS: f32 = 12.0;
const SCROLLBAR_MIN_THUMB: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarGeometry {
    pub container: Id,
    pub axis: ScrollAxis,
    /// Outer hit area, including the empty part of the rail.
    pub track: Rect,
    /// Full-length rail geometry.
    pub rail: Rect,
    /// Draggable handle.
    pub thumb: Rect,
    /// Scrollable content extent for this axis.
    pub range: f32,
    /// Pointer position along the thumb's axis when it was grabbed.
    pub grab_offset: f32,
}

/// Stops an accidental parent/child cycle before it can overflow the stack.
const MAX_TREE_DEPTH: usize = 512;

pub struct Tree {
    arena: Arena<Node>,
    by_id: HashMap<Id, NodeId>,
    root: NodeId,
    frame: u64,
    viewport: Size,
    removed: Vec<NodeId>,
    dangling_parents: Vec<NodeId>,
    layout_generation: u64,
    created_last_frame: usize,
}

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}

impl Tree {
    pub fn new() -> Self {
        let mut arena = Arena::new();
        let mut root_node = Node::new(Id::ROOT, NodeKind::Root);
        root_node.style = Style::default();
        root_node.style.width = kasumi_core::Length::Percent(100.0);
        root_node.style.height = kasumi_core::Length::Percent(100.0);
        root_node.style.direction = kasumi_core::FlexDirection::Column;
        root_node.style.align_items = kasumi_core::Align::Stretch;
        let root = arena.insert(root_node);
        let mut by_id = HashMap::new();
        by_id.insert(Id::ROOT, root);
        Self {
            arena,
            by_id,
            root,
            frame: 0,
            viewport: Size::new(800.0, 600.0),
            removed: Vec::new(),
            dangling_parents: Vec::new(),
            layout_generation: 1,
            created_last_frame: 0,
        }
    }

    pub fn begin_frame(&mut self, viewport: Size) -> NodeId {
        self.frame += 1;
        self.viewport = viewport;
        self.created_last_frame = 0;
        self.removed.clear();
        self.touch_existing(self.root);
        self.root_node_mut().last_seen = self.frame;
        self.root
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn viewport(&self) -> Size {
        self.viewport
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn root_node(&self) -> &Node {
        self.arena.get(self.root).expect("root node missing")
    }

    pub fn root_node_mut(&mut self) -> &mut Node {
        self.arena.get_mut(self.root).expect("root node missing")
    }

    pub fn node_count(&self) -> usize {
        self.arena.len()
    }

    pub fn removed_last_frame(&self) -> usize {
        self.removed.len()
    }

    pub fn created_last_frame(&self) -> usize {
        self.created_last_frame
    }

    /// Monotonically increasing token bumped whenever layout-affecting state
    /// changes. Layout uses it to skip a whole pass when nothing changed.
    pub fn layout_generation(&self) -> u64 {
        self.layout_generation
    }

    fn mark_layout_dirty(&mut self) {
        self.layout_generation = self.layout_generation.wrapping_add(1);
    }

    pub fn get(&self, node: NodeId) -> Option<&Node> {
        self.arena.get(node)
    }

    pub fn get_mut(&mut self, node: NodeId) -> Option<&mut Node> {
        self.arena.get_mut(node)
    }

    pub fn find(&self, id: Id) -> Option<NodeId> {
        self.by_id.get(&id).copied()
    }

    pub fn node_by_id(&self, id: Id) -> Option<&Node> {
        self.find(id).and_then(|node| self.get(node))
    }

    pub fn node_by_id_mut(&mut self, id: Id) -> Option<&mut Node> {
        self.find(id).and_then(|node| self.get_mut(node))
    }

    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.arena.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (NodeId, &mut Node)> {
        self.arena.iter_mut()
    }

    /// Returns the existing node for `id` or creates a new one. The node is
    /// marked as seen in the current frame and its per-frame scratch is reset.
    pub fn ensure(&mut self, id: Id, kind: NodeKind) -> NodeId {
        if let Some(&node_id) = self.by_id.get(&id) {
            let kind_changed = self
                .arena
                .get(node_id)
                .is_some_and(|node| node.kind != kind);
            if kind_changed {
                if let Some(node) = self.arena.get_mut(node_id) {
                    node.kind = kind;
                    node.style_dirty = true;
                    node.content_dirty = true;
                    node.layout_dirty = true;
                }
                self.mark_layout_dirty();
            }
            self.touch_existing(node_id);
            return node_id;
        }

        let mut node = Node::new(id, kind);
        node.last_seen = self.frame;
        node.scratch_frame = self.frame;
        let node_id = self.arena.insert(node);
        self.by_id.insert(id, node_id);
        self.created_last_frame += 1;
        self.mark_layout_dirty();
        node_id
    }

    fn touch_existing(&mut self, node_id: NodeId) {
        let frame = self.frame;
        if let Some(node) = self.arena.get_mut(node_id) {
            node.last_seen = frame;
            if node.scratch_frame != frame {
                node.scratch_frame = frame;
                node.scratch_children.clear();
            }
            // A node can only be edited during view after it is touched, so
            // clearing here is safe and keeps `Response::changed` frame-local.
            node.changed = false;
        }
    }

    /// Records `child` in `parent`'s declared child list for this frame.
    pub fn declare_child(&mut self, parent: NodeId, child: NodeId) {
        if parent == child {
            panic!("kasumi: a node cannot be its own child");
        }
        let frame = self.frame;
        {
            let node = self
                .arena
                .get_mut(child)
                .unwrap_or_else(|| panic!("kasumi: declared child {child} is not alive"));
            node.parent = Some(parent);
        }
        let parent_node = self
            .arena
            .get_mut(parent)
            .unwrap_or_else(|| panic!("kasumi: declared parent {parent} is not alive"));
        if parent_node.scratch_frame != frame {
            parent_node.scratch_frame = frame;
            parent_node.scratch_children.clear();
        }
        debug_assert!(
            !parent_node.scratch_children.contains(&child),
            "kasumi: node {child} declared twice under {parent} in the same frame"
        );
        parent_node.scratch_children.push(child);
    }

    pub fn set_style(&mut self, node_id: NodeId, style: Style) {
        if let Some(node) = self.arena.get_mut(node_id)
            && node.style != style
        {
            node.style = style;
            node.style_dirty = true;
            node.layout_dirty = true;
            self.mark_layout_dirty();
        }
    }

    pub fn with_style(&mut self, node_id: NodeId, f: impl FnOnce(&mut Style)) {
        let Some(node) = self.arena.get_mut(node_id) else {
            return;
        };
        let mut next = node.style.clone();
        f(&mut next);
        if next != node.style {
            node.style = next;
            node.style_dirty = true;
            node.layout_dirty = true;
            self.mark_layout_dirty();
        }
    }

    pub fn set_text(&mut self, node_id: NodeId, text: TextContent) {
        if let Some(node) = self.arena.get_mut(node_id)
            && node.text.as_ref() != Some(&text)
        {
            node.text = Some(text);
            node.content_dirty = true;
            node.style_dirty = true;
            node.layout_dirty = true;
            self.mark_layout_dirty();
        }
    }

    pub fn set_sense(&mut self, node_id: NodeId, sense: Sense) {
        if let Some(node) = self.arena.get_mut(node_id) {
            node.sense = sense;
        }
    }

    /// Advances smooth hover transitions, optionally targeting button nodes
    /// only. This stays on the node so paint has no hidden frame state.
    pub fn tick_hover(&mut self, hovered: Option<Id>, buttons_only: bool, dt: f32) -> bool {
        let mut animating = false;
        let step = (dt.max(0.0) * 12.0).min(1.0);
        for (_, node) in self.arena.iter_mut() {
            if buttons_only && node.kind != NodeKind::Button {
                continue;
            }
            let target = if hovered == Some(node.id) { 1.0 } else { 0.0 };
            let delta = (target - node.hover_progress) * step;
            if delta.abs() > 0.001 {
                node.hover_progress += delta;
                animating = true;
            } else {
                node.hover_progress = target;
            }
        }
        animating
    }

    pub fn set_accessibility_label(&mut self, node_id: NodeId, label: impl Into<String>) {
        if let Some(node) = self.arena.get_mut(node_id) {
            node.accessibility_label = Some(label.into());
        }
    }

    pub fn set_a11y_hidden(&mut self, node_id: NodeId, hidden: bool) {
        if let Some(node) = self.arena.get_mut(node_id) {
            node.a11y_hidden = hidden;
        }
    }

    pub fn set_hidden(&mut self, node_id: NodeId, hidden: bool) {
        if let Some(node) = self.arena.get_mut(node_id)
            && node.style.hidden != hidden
        {
            node.style.hidden = hidden;
            node.style_dirty = true;
            node.layout_dirty = true;
            self.mark_layout_dirty();
        }
    }

    /// Commits child order, recycles unseen nodes and returns the ids that were
    /// removed. The returned slice remains valid until the next frame starts.
    pub fn end_frame(&mut self) -> &[NodeId] {
        let frame = self.frame;
        let mut layout_changed = false;

        // 1. Commit declared children for every node touched this frame.
        for (_, node) in self.arena.iter_mut() {
            if node.last_seen != frame || node.scratch_frame != frame {
                continue;
            }
            if node.children != node.scratch_children {
                node.children.clear();
                node.children.extend_from_slice(&node.scratch_children);
                node.children_dirty = true;
                node.layout_dirty = true;
                layout_changed = true;
            }
            node.scratch_children.clear();
        }
        if layout_changed {
            self.mark_layout_dirty();
        }

        // 2. Recycled nodes are exactly those not touched this frame. The root
        //    is touched in `begin_frame`.
        self.removed.clear();
        for (node_id, node) in self.arena.iter() {
            if node.last_seen != frame {
                self.removed.push(node_id);
            }
        }
        for node_id in &self.removed {
            let id = self
                .arena
                .get(*node_id)
                .expect("removed node disappeared before recycling")
                .id;
            self.arena.remove(*node_id);
            self.by_id.remove(&id);
        }
        if !self.removed.is_empty() {
            self.mark_layout_dirty();
        }

        // 3. Any surviving child whose parent disappeared must have been
        //    re-parented during view. Clear dangling links defensively.
        self.dangling_parents.clear();
        for (node_id, node) in self.arena.iter() {
            if let Some(parent) = node.parent
                && !self.arena.contains(parent)
            {
                self.dangling_parents.push(node_id);
            }
        }
        for node_id in &self.dangling_parents {
            if let Some(node) = self.arena.get_mut(*node_id) {
                node.parent = None;
            }
        }

        &self.removed
    }

    pub fn focusables(&self) -> Vec<Id> {
        let mut result = Vec::new();
        self.collect_focusables(self.root, 0, &mut result);
        result
    }

    fn collect_focusables(&self, node_id: NodeId, depth: usize, out: &mut Vec<Id>) {
        if depth > MAX_TREE_DEPTH {
            return;
        }
        let Some(node) = self.arena.get(node_id) else {
            return;
        };
        if node.style.hidden || node.a11y_hidden {
            return;
        }
        if node.sense.is_focusable() {
            out.push(node.id);
        }
        for child in &node.children {
            self.collect_focusables(*child, depth + 1, out);
        }
    }

    /// Hit-tests against the committed layout from the previous frame.
    ///
    /// Children are visited back-to-front, so the last-painted node wins. A
    /// hit returns the deepest hoverable node, which lets a button own hits
    /// over its label child.
    pub fn hit_test(&self, point: Point) -> Option<Id> {
        let viewport = Rect::new(Point::ZERO, self.viewport);
        self.hit_test_recursive(self.root, point, viewport, Vec2::ZERO, 0)
    }

    fn hit_test_recursive(
        &self,
        node_id: NodeId,
        point: Point,
        clip: Rect,
        offset: Vec2,
        depth: usize,
    ) -> Option<Id> {
        if depth > MAX_TREE_DEPTH {
            return None;
        }
        let node = self.arena.get(node_id)?;
        if node.style.hidden {
            return None;
        }
        let rect = node.rect.translate(offset);
        let child_offset = if node.style.scroll_x || node.style.scroll_y {
            Vec2::new(
                offset.x
                    - if node.style.scroll_x {
                        node.scroll.offset.x
                    } else {
                        0.0
                    },
                offset.y
                    - if node.style.scroll_y {
                        node.scroll.offset.y
                    } else {
                        0.0
                    },
            )
        } else {
            offset
        };
        let clip = if node.style.clip {
            clip.intersect(rect)
        } else {
            clip
        };
        if !clip.contains(point) {
            return None;
        }
        for child in node
            .children
            .iter()
            .rev()
            .filter(|child| {
                self.arena
                    .get(**child)
                    .is_some_and(|node| node.kind == NodeKind::Overlay)
            })
            .chain(node.children.iter().rev().filter(|child| {
                self.arena
                    .get(**child)
                    .is_none_or(|node| node.kind != NodeKind::Overlay)
            }))
        {
            if let Some(id) = self.hit_test_recursive(*child, point, clip, child_offset, depth + 1)
            {
                return Some(id);
            }
        }
        if node.sense.is_hoverable() && rect.contains(point) {
            Some(node.id)
        } else {
            None
        }
    }

    /// Returns the node rect (absolute coordinates in the window).
    pub fn rect_of(&self, id: Id) -> Rect {
        self.node_by_id(id)
            .map(|node| node.rect)
            .unwrap_or_default()
    }

    /// Returns the laid-out rect after applying all ancestor scroll offsets.
    pub fn visual_rect_of(&self, id: Id) -> Rect {
        let Some(mut node_id) = self.find(id) else {
            return Rect::ZERO;
        };
        let mut rect = self.get(node_id).map(|node| node.rect).unwrap_or_default();
        while let Some(parent_id) = self.get(node_id).and_then(|node| node.parent) {
            let Some(parent) = self.get(parent_id) else {
                break;
            };
            if parent.style.scroll_x {
                rect.origin.x -= parent.scroll.offset.x;
            }
            if parent.style.scroll_y {
                rect.origin.y -= parent.scroll.offset.y;
            }
            node_id = parent_id;
        }
        rect
    }

    pub fn ancestor_or_self_has_kind(&self, id: Id, kind: NodeKind) -> bool {
        let mut current = self.find(id);
        let mut depth = 0usize;
        while let Some(node_id) = current {
            if depth > MAX_TREE_DEPTH {
                return false;
            }
            let Some(node) = self.get(node_id) else {
                return false;
            };
            if node.kind == kind {
                return true;
            }
            current = node.parent;
            depth += 1;
        }
        false
    }

    /// True when `id` is `ancestor` or one of its descendants.
    pub fn is_within(&self, id: Id, ancestor: Id) -> bool {
        let mut current = self.find(id);
        let mut depth = 0usize;
        while let Some(node_id) = current {
            if depth > MAX_TREE_DEPTH {
                return false;
            }
            let Some(node) = self.get(node_id) else {
                return false;
            };
            if node.id == ancestor {
                return true;
            }
            current = node.parent;
            depth += 1;
        }
        false
    }

    /// Finds the nearest scroll container ancestor of `id`.
    pub fn nearest_scrollable(&self, id: Id, axis: ScrollAxis) -> Option<Id> {
        let mut current = self.find(id);
        let mut depth = 0usize;
        while let Some(node_id) = current {
            if depth > MAX_TREE_DEPTH {
                return None;
            }
            let node = self.get(node_id)?;
            let enabled = match axis {
                ScrollAxis::Horizontal => node.style.scroll_x,
                ScrollAxis::Vertical => node.style.scroll_y,
            };
            if enabled {
                return Some(node.id);
            }
            current = node.parent;
            depth += 1;
        }
        None
    }

    /// Updates a scroll container's target. Returns the container id when the
    /// pointer was over a scrollable subtree.
    pub fn scroll_by_at(&mut self, point: Point, delta: Vec2, now: f64) -> Option<Id> {
        let hit = self.hit_test(point);
        let mut current = hit.and_then(|id| self.find(id));
        let mut fallback = None;
        while let Some(node_id) = current {
            let Some(node) = self.arena.get(node_id) else {
                break;
            };
            if node.style.scroll_x || node.style.scroll_y {
                fallback = Some(node.id);
                break;
            }
            current = node.parent;
        }
        let hit = hit.or(fallback)?;
        let mut changed = None;
        let horizontal = self
            .nearest_scrollable(hit, ScrollAxis::Horizontal)
            .filter(|_| delta.x.abs() > f32::EPSILON);
        let vertical = self
            .nearest_scrollable(hit, ScrollAxis::Vertical)
            .filter(|_| delta.y.abs() > f32::EPSILON);

        if let Some(id) = horizontal
            && let Some(node) = self.node_by_id_mut(id)
        {
            node.scroll.target.x += delta.x;
            node.scroll.last_time = now;
            changed = Some(id);
        }
        if let Some(id) = vertical
            && let Some(node) = self.node_by_id_mut(id)
        {
            node.scroll.target.y += delta.y;
            node.scroll.last_time = now;
            changed = Some(id);
        }
        changed
    }

    /// Adds a scroll delta to a specific container. Only axes enabled by its
    /// style are changed.
    pub fn scroll_by(&mut self, id: Id, delta: Vec2, now: f64) -> bool {
        let Some(node) = self.node_by_id_mut(id) else {
            return false;
        };
        let mut changed = false;
        if node.style.scroll_x && delta.x.abs() > f32::EPSILON {
            node.scroll.target.x += delta.x;
            changed = true;
        }
        if node.style.scroll_y && delta.y.abs() > f32::EPSILON {
            node.scroll.target.y += delta.y;
            changed = true;
        }
        if changed {
            node.scroll.last_time = now;
        }
        changed
    }

    /// Updates a scroll viewport by its node identity. The outermost node id
    /// belongs to the stable component; the inner viewport may be an
    /// implementation detail, so callers can resolve it without knowing the
    /// child-id scheme.
    pub fn scroll_by_node(&mut self, node_id: NodeId, delta: Vec2, now: f64) -> bool {
        let Some(id) = self.get(node_id).map(|node| node.id) else {
            return false;
        };
        self.scroll_by(id, delta, now)
    }

    /// Resolves the nearest scrollable ancestor or self for a pointer hit.
    /// Unlike hit testing, this does not depend on the deepest hoverable node
    /// having its own scroll state.
    pub fn scrollable_at(&self, point: Point, axis: ScrollAxis) -> Option<Id> {
        self.nearest_scrollable(self.hit_test(point)?, axis)
    }

    /// Advances retained scroll springs. Returns true while any scroll view is
    /// still moving and therefore needs another frame.
    pub fn tick_scroll(&mut self, now: f64) -> bool {
        let mut animating = false;
        for (_, node) in self.arena.iter_mut() {
            if !(node.style.scroll_x || node.style.scroll_y) {
                continue;
            }
            let viewport = node.rect.size;
            node.scroll.clamp_to(viewport);
            if !node.scroll.smooth {
                node.scroll.offset = node.scroll.target;
                node.scroll.velocity = Point::ZERO;
                node.scroll.last_time = now;
                continue;
            }

            let elapsed = if node.scroll.last_time > 0.0 {
                (now - node.scroll.last_time).clamp(0.0, 0.05) as f32
            } else {
                1.0 / 60.0
            };
            node.scroll.last_time = now;
            if elapsed > 0.0 {
                advance_scroll(&mut node.scroll, elapsed);
            }
            if node.scroll.is_animating() {
                animating = true;
            } else {
                node.scroll.offset = node.scroll.target;
                node.scroll.velocity = Point::ZERO;
            }
        }
        animating
    }

    /// Recomputes natural extents for scroll containers after layout.
    pub fn update_scroll_metrics(&mut self) {
        let mut metrics = Vec::new();
        for (node_id, node) in self.arena.iter() {
            if !(node.style.scroll_x || node.style.scroll_y) {
                continue;
            }
            let mut width = node.rect.width();
            let mut height = node.rect.height();
            let mut pending = node.children.clone();
            while let Some(child_id) = pending.pop() {
                if let Some(child) = self.arena.get(child_id) {
                    pending.extend_from_slice(&child.children);
                    if child.kind != NodeKind::Overlay {
                        width = width.max(child.rect.right() - node.rect.left());
                        height = height.max(child.rect.bottom() - node.rect.top());
                    }
                }
            }
            if let Some(parent_id) = node.parent
                && let Some(parent) = self.arena.get(parent_id)
                && parent.style.clip
            {
                // The visible viewport is the parent's inner box. Measure
                // content from that origin so the parent's scrollbar and
                // clipping do not leak into the content extent.
                width = width.max(node.rect.width());
                height = height.max(node.rect.height());
            }
            metrics.push((
                node_id,
                Size::new(width, height),
                node.style.scroll_x,
                node.style.scroll_y,
                node.style.padding,
                node.rect.size,
            ));
        }
        for (node_id, size, scroll_x, scroll_y, padding, viewport) in metrics {
            if let Some(node) = self.arena.get_mut(node_id) {
                node.scroll.content_size = Size::new(
                    size.width + if scroll_x { padding.right } else { 0.0 },
                    size.height + if scroll_y { padding.bottom } else { 0.0 },
                );
                node.scroll.clamp_to(viewport);
            }
        }
    }

    pub fn scrollbar_at(&self, point: Point) -> Option<ScrollbarGeometry> {
        let viewport = Rect::new(Point::ZERO, self.viewport);
        self.scrollbar_recursive(self.root, point, viewport, Vec2::ZERO, 0)
    }

    fn scrollbar_recursive(
        &self,
        node_id: NodeId,
        point: Point,
        clip: Rect,
        offset: Vec2,
        depth: usize,
    ) -> Option<ScrollbarGeometry> {
        if depth > MAX_TREE_DEPTH {
            return None;
        }
        let node = self.arena.get(node_id)?;
        if node.style.hidden {
            return None;
        }
        let rect = node.rect.translate(offset);
        let clip = if node.style.clip {
            clip.intersect(rect)
        } else {
            clip
        };
        if !clip.contains(point) {
            return None;
        }
        let child_offset = Vec2::new(
            offset.x
                - if node.style.scroll_x {
                    node.scroll.offset.x
                } else {
                    0.0
                },
            offset.y
                - if node.style.scroll_y {
                    node.scroll.offset.y
                } else {
                    0.0
                },
        );
        for child in node
            .children
            .iter()
            .rev()
            .filter(|child| {
                self.arena
                    .get(**child)
                    .is_some_and(|node| node.kind == NodeKind::Overlay)
            })
            .chain(node.children.iter().rev().filter(|child| {
                self.arena
                    .get(**child)
                    .is_none_or(|node| node.kind != NodeKind::Overlay)
            }))
        {
            if let Some(found) =
                self.scrollbar_recursive(*child, point, clip, child_offset, depth + 1)
            {
                return Some(found);
            }
        }
        scrollbar_geometry(node, rect, point)
    }

    /// Sets a scrollbar drag position directly from the pointer location.
    pub fn drag_scrollbar(&mut self, geometry: ScrollbarGeometry, point: Point) {
        let Some(node) = self.node_by_id_mut(geometry.container) else {
            return;
        };
        let value = match geometry.axis {
            ScrollAxis::Horizontal => {
                let travel = (geometry.rail.width() - geometry.thumb.width()).max(1.0);
                let t = ((point.x - geometry.grab_offset - geometry.rail.left()) / travel)
                    .clamp(0.0, 1.0);
                t * geometry.range
            }
            ScrollAxis::Vertical => {
                let travel = (geometry.rail.height() - geometry.thumb.height()).max(1.0);
                let t = ((point.y - geometry.grab_offset - geometry.rail.top()) / travel)
                    .clamp(0.0, 1.0);
                t * geometry.range
            }
        };
        match geometry.axis {
            ScrollAxis::Horizontal => {
                node.scroll.offset.x = value;
                node.scroll.target.x = value;
                node.scroll.velocity.x = 0.0;
            }
            ScrollAxis::Vertical => {
                node.scroll.offset.y = value;
                node.scroll.target.y = value;
                node.scroll.velocity.y = 0.0;
            }
        }
    }

    /// Finds the first text descendant of a node. Used for accessible names.
    pub fn label_of(&self, node_id: NodeId) -> Option<String> {
        let mut labels = Vec::new();
        self.collect_text(node_id, 0, &mut labels);
        if labels.is_empty() {
            None
        } else {
            Some(labels.join(" "))
        }
    }

    fn collect_text(&self, node_id: NodeId, depth: usize, out: &mut Vec<String>) {
        if depth > 64 {
            return;
        }
        let Some(node) = self.arena.get(node_id) else {
            return;
        };
        if let Some(text) = node.text.as_ref()
            && !text.text.is_empty()
        {
            out.push(text.text.clone());
        }
        for child in &node.children {
            self.collect_text(*child, depth + 1, out);
        }
    }

    #[cfg(debug_assertions)]
    pub fn assert_consistent(&self) {
        for (node_id, node) in self.arena.iter() {
            if let Some(parent) = node.parent {
                assert!(
                    self.arena.contains(parent),
                    "node {node_id} points at dead parent {parent}"
                );
                let parent_node = self.arena.get(parent).expect("checked above");
                assert!(
                    parent_node.children.contains(&node_id),
                    "node {node_id} is not present in its parent's children"
                );
            }
            for child in &node.children {
                assert!(
                    self.arena.contains(*child),
                    "node {node_id} has dead child {child}"
                );
            }
        }
    }
}

fn advance_scroll(scroll: &mut crate::node::ScrollState, elapsed: f32) {
    const STIFFNESS: f32 = 520.0;
    const DAMPING: f32 = 42.0;
    const SUBSTEP: f32 = 1.0 / 240.0;

    let mut remaining = elapsed;
    while remaining > 0.0 {
        let dt = remaining.min(SUBSTEP);
        let ax = (scroll.target.x - scroll.offset.x) * STIFFNESS - scroll.velocity.x * DAMPING;
        let ay = (scroll.target.y - scroll.offset.y) * STIFFNESS - scroll.velocity.y * DAMPING;
        scroll.velocity.x += ax * dt;
        scroll.velocity.y += ay * dt;
        scroll.offset.x += scroll.velocity.x * dt;
        scroll.offset.y += scroll.velocity.y * dt;
        remaining -= dt;
    }

    if (scroll.target.x - scroll.offset.x).abs() < 0.08 && scroll.velocity.x.abs() < 0.8 {
        scroll.offset.x = scroll.target.x;
        scroll.velocity.x = 0.0;
    }
    if (scroll.target.y - scroll.offset.y).abs() < 0.08 && scroll.velocity.y.abs() < 0.8 {
        scroll.offset.y = scroll.target.y;
        scroll.velocity.y = 0.0;
    }
}

fn scrollbar_geometry(node: &Node, rect: Rect, point: Point) -> Option<ScrollbarGeometry> {
    if node.style.scroll_y && node.scroll.content_size.height > rect.height() + 0.5 {
        let track = Rect::from_xywh(
            rect.right() - SCROLLBAR_THICKNESS,
            rect.top() + 4.0,
            SCROLLBAR_THICKNESS,
            (rect.height() - 8.0).max(1.0),
        );
        if track.contains(point) {
            let range = node.scroll.content_size.height - rect.height();
            let thumb_height = (track.height() * rect.height() / node.scroll.content_size.height)
                .clamp(SCROLLBAR_MIN_THUMB, track.height());
            let travel = track.height() - thumb_height;
            let t = if range > 0.0 {
                (node.scroll.offset.y / range).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let rail = Rect::from_xywh(track.left() + 2.0, track.top(), 8.0, track.height());
            let thumb = Rect::from_xywh(
                rail.left() + 1.0,
                track.top() + travel * t,
                6.0,
                thumb_height,
            );
            return Some(ScrollbarGeometry {
                container: node.id,
                axis: ScrollAxis::Vertical,
                track,
                thumb,
                rail,
                range,
                grab_offset: (point.y - thumb.top()).clamp(0.0, thumb.height()),
            });
        }
    }

    if node.style.scroll_x && node.scroll.content_size.width > rect.width() + 0.5 {
        let track = Rect::from_xywh(
            rect.left() + 4.0,
            rect.bottom() - SCROLLBAR_THICKNESS,
            (rect.width() - 8.0).max(1.0),
            SCROLLBAR_THICKNESS,
        );
        if track.contains(point) {
            let range = node.scroll.content_size.width - rect.width();
            let thumb_width = (track.width() * rect.width() / node.scroll.content_size.width)
                .clamp(SCROLLBAR_MIN_THUMB, track.width());
            let travel = track.width() - thumb_width;
            let t = if range > 0.0 {
                (node.scroll.offset.x / range).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let rail = Rect::from_xywh(track.left(), track.top() + 2.0, track.width(), 8.0);
            let thumb = Rect::from_xywh(
                track.left() + travel * t,
                rail.top() + 1.0,
                thumb_width,
                6.0,
            );
            return Some(ScrollbarGeometry {
                container: node.id,
                axis: ScrollAxis::Horizontal,
                track,
                thumb,
                rail,
                range,
                grab_offset: (point.x - thumb.left()).clamp(0.0, thumb.width()),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuse_and_recycle() {
        let mut tree = Tree::new();
        tree.begin_frame(Size::new(100.0, 100.0));
        let a = tree.ensure(Id::from_raw(10), NodeKind::Container);
        tree.declare_child(tree.root(), a);
        tree.end_frame();
        assert_eq!(tree.node_count(), 2);

        tree.begin_frame(Size::new(100.0, 100.0));
        let b = tree.ensure(Id::from_raw(10), NodeKind::Container);
        tree.declare_child(tree.root(), b);
        tree.end_frame();
        assert_eq!(a, b);
        assert_eq!(tree.node_count(), 2);

        tree.begin_frame(Size::new(100.0, 100.0));
        let removed = tree.end_frame();
        assert_eq!(removed.len(), 1);
        assert_eq!(tree.node_count(), 1);
    }

    #[test]
    fn child_order_is_committed() {
        let mut tree = Tree::new();
        tree.begin_frame(Size::new(10.0, 10.0));
        let a = tree.ensure(Id::from_raw(1), NodeKind::Container);
        let b = tree.ensure(Id::from_raw(2), NodeKind::Container);
        tree.declare_child(tree.root(), b);
        tree.declare_child(tree.root(), a);
        tree.end_frame();
        assert_eq!(tree.root_node().children, vec![b, a]);
    }

    #[test]
    fn hit_test_prefers_deepest_interactive_node() {
        let mut tree = Tree::new();
        tree.begin_frame(Size::new(100.0, 100.0));
        let button = tree.ensure(Id::from_raw(1), NodeKind::Button);
        tree.set_sense(button, Sense::click());
        tree.declare_child(tree.root(), button);
        let label = tree.ensure(Id::from_raw(2), NodeKind::Text);
        tree.declare_child(button, label);
        tree.end_frame();
        let root = tree.root();
        tree.get_mut(root).unwrap().rect = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        tree.get_mut(button).unwrap().rect = Rect::from_xywh(10.0, 10.0, 50.0, 20.0);
        tree.get_mut(label).unwrap().rect = Rect::from_xywh(12.0, 12.0, 20.0, 16.0);
        assert_eq!(tree.hit_test(Point::new(20.0, 20.0)), Some(Id::from_raw(1)));
        assert_eq!(tree.hit_test(Point::new(99.0, 99.0)), None);
    }
}
