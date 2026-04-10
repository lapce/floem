//! Retained paint artifact storage and replay.
//!
//! The retained display list stores per-element paint and post-paint recordings as
//! [`Scene`] values in local space. Retention happens at the element/stage
//! level: we rerecord only dirty elements and reuse unchanged retained scenes across
//! transform changes when the recorded content allows it.

use crate::text::GlyphRunRef;
use imaging::{
    BlurredRoundedRect, ClipRef, FillRef, GeometryRef, GroupRef, PaintSink, StrokeRef,
    record::{Clip, Command, Draw, Geometry, Glyph as ImagingGlyph, Scene, replay_transformed},
};
use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Size};
use rustc_hash::{FxHashMap, FxHashSet};
use std::mem;
use understory_box_tree::NodeFlags;

use crate::{
    BoxTree, ElementId,
    view::stacking::{StackingContextItem, collect_stacking_context_items_into},
};

const COMPOSED_SCENE_MIN_SUBTREE_SIZE: usize = 8;

/// Transform class describing when recorded content remains valid.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TransformClass {
    Exact,
    TranslateOnly,
    Orthonormal,
    Affine,
}

impl TransformClass {
    pub(crate) fn supports(self, diff: Self) -> bool {
        match self {
            Self::Exact => matches!(diff, Self::Exact),
            Self::TranslateOnly => matches!(diff, Self::Exact | Self::TranslateOnly),
            Self::Orthonormal => !matches!(diff, Self::Affine),
            Self::Affine => true,
        }
    }

    pub(crate) fn combine(self, other: Self) -> Self {
        use TransformClass::{Affine, Exact, Orthonormal, TranslateOnly};

        match (self, other) {
            (Exact, x) | (x, Exact) => x,
            (TranslateOnly, _) | (_, TranslateOnly) => TranslateOnly,
            (Orthonormal, _) | (_, Orthonormal) => Orthonormal,
            (Affine, Affine) => Affine,
        }
    }
}

pub(crate) fn transform_diff_class(original: Affine, current: Affine) -> TransformClass {
    if current == original {
        return TransformClass::Exact;
    }

    let o = original.as_coeffs();
    let c = current.as_coeffs();
    let same_linear = o[0] == c[0] && o[1] == c[1] && o[2] == c[2] && o[3] == c[3];
    if same_linear {
        TransformClass::TranslateOnly
    } else {
        TransformClass::Affine
    }
}

#[derive(Clone)]
pub(crate) struct ElementStage {
    pub scene: Scene,
    pub transform_class: TransformClass,
}

impl Default for ElementStage {
    fn default() -> Self {
        Self {
            scene: Scene::new(),
            transform_class: TransformClass::Affine,
        }
    }
}

impl ElementStage {
    pub(crate) fn set_scene(&mut self, scene: Scene) {
        self.transform_class = scene_transform_class(&scene);
        self.scene = scene;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ElementSnapshot {
    pub local_bounds: Rect,
    pub clip: Option<RoundedRect>,
    pub world_transform: Affine,
}

impl ElementSnapshot {
    pub(crate) fn from_box_tree(box_tree: &crate::BoxTree, element_id: ElementId) -> Self {
        Self {
            local_bounds: box_tree.local_bounds(element_id.0).unwrap_or_default(),
            clip: box_tree.local_clip(element_id.0).flatten(),
            world_transform: box_tree.world_transform(element_id.0).unwrap_or_default(),
        }
    }

    pub(crate) fn supports_reuse(self, current: Self) -> bool {
        self.local_bounds == current.local_bounds && self.clip == current.clip
    }
}

#[derive(Clone, Default)]
pub(crate) struct ElementDisplayList {
    pub paint: ElementStage,
    pub post: ElementStage,
    pub snapshot: Option<ElementSnapshot>,
}

pub(crate) struct DisplayListSync {
    pub active_ids: FxHashSet<ElementId>,
    pub newly_active_ids: FxHashSet<ElementId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DisplayNodeSlot(usize);

#[derive(Clone, Default)]
struct ChildList {
    ordered: Vec<DisplayNodeSlot>,
}

impl ChildList {
    fn new(children: Vec<DisplayNodeSlot>, _nodes: &[Option<DisplayNode>]) -> Self {
        Self { ordered: children }
    }
}

#[derive(Clone, Default)]
struct DisplayNode {
    element_id: Option<ElementId>,
    parent: Option<DisplayNodeSlot>,
    children: ChildList,
    display: ElementDisplayList,
    composed_scene: Scene,
    composed_dirty: bool,
    subtree_size: usize,
}

#[derive(Default)]
pub struct RetainedDisplayList {
    roots: Vec<DisplayNodeSlot>,
    nodes: Vec<Option<DisplayNode>>,
    free_list: Vec<DisplayNodeSlot>,
    slot_by_id: FxHashMap<ElementId, DisplayNodeSlot>,
    inactive_elements: FxHashMap<ElementId, ElementDisplayList>,
    active_count: usize,
}

impl RetainedDisplayList {
    pub(crate) fn sync_structure(
        &mut self,
        root: ElementId,
        box_tree: &BoxTree,
        dragging_preview: Option<ElementId>,
    ) -> DisplayListSync {
        let mut existing = mem::take(&mut self.slot_by_id);

        let mut roots = Vec::new();
        let mut active_ids = FxHashSet::default();
        let mut newly_active_ids = FxHashSet::default();

        self.sync_branch(
            root,
            None,
            false,
            dragging_preview,
            box_tree,
            &mut existing,
            &mut active_ids,
            &mut newly_active_ids,
            &mut roots,
        );

        if let Some(preview) = dragging_preview {
            self.sync_branch(
                preview,
                None,
                true,
                None,
                box_tree,
                &mut existing,
                &mut active_ids,
                &mut newly_active_ids,
                &mut roots,
            );
        }

        for (_, slot) in existing {
            self.free_slot(slot);
        }

        self.active_count = active_ids.len();
        self.roots = roots;
        DisplayListSync {
            active_ids,
            newly_active_ids,
        }
    }

    pub(crate) fn replay_step_count(&self) -> usize {
        self.active_count * 2
    }

    pub(crate) fn element_mut(&mut self, id: ElementId) -> &mut ElementDisplayList {
        if let Some(slot) = self.find_slot(id) {
            return &mut self
                .node_mut(slot)
                .expect("display list node missing")
                .display;
        }
        self.inactive_elements.entry(id).or_default()
    }

    pub(crate) fn element(&self, id: ElementId) -> Option<&ElementDisplayList> {
        if let Some(slot) = self.find_slot(id) {
            return Some(&self.node(slot)?.display);
        }
        self.inactive_elements.get(&id)
    }

    pub(crate) fn needs_stage_rerecord(&self, id: ElementId, snapshot: ElementSnapshot) -> bool {
        let Some(element) = self.element(id) else {
            return true;
        };
        let Some(previous) = element.snapshot else {
            return true;
        };

        if !previous.supports_reuse(snapshot) {
            return true;
        }

        let diff = transform_diff_class(previous.world_transform, snapshot.world_transform);
        !element.paint.transform_class.supports(diff)
            || !element.post.transform_class.supports(diff)
    }

    pub(crate) fn root_slots(&self) -> &[DisplayNodeSlot] {
        &self.roots
    }

    pub(crate) fn node_element_id(&self, slot: DisplayNodeSlot) -> Option<ElementId> {
        self.node(slot)?.element_id
    }

    pub(crate) fn child_slots(&self, slot: DisplayNodeSlot) -> Option<&[DisplayNodeSlot]> {
        Some(&self.node(slot)?.children.ordered)
    }

    pub(crate) fn mark_composed_dirty(&mut self, id: ElementId) {
        self.mark_composed_dirty_from_slot(self.find_slot(id));
    }

    pub(crate) fn ensure_composed_scene(&mut self, slot: DisplayNodeSlot) {
        if !self.slot_has_composed_scene(slot) {
            return;
        }

        let (dirty, child_slots) = match self.node(slot) {
            Some(node) => (node.composed_dirty, node.children.ordered.clone()),
            None => return,
        };
        if !dirty {
            return;
        }

        for child in child_slots.iter().copied() {
            if self.slot_has_composed_scene(child) {
                self.ensure_composed_scene(child);
            }
        }

        let mut composed = {
            let node = self.node_mut(slot).expect("display list node missing");
            let mut scene = mem::take(&mut node.composed_scene);
            scene.clear();
            scene
        };

        let snapshot = {
            let node = self.node(slot).expect("display list node missing");
            node.display.snapshot
        };
        if let Some(snapshot) = snapshot {
            self.append_node_contents_to_scene(slot, &mut composed, snapshot.world_transform);
        }

        let node = self.node_mut(slot).expect("display list node missing");
        node.composed_scene = composed;
        node.composed_dirty = false;
    }

    pub(crate) fn composed_scene(&self, slot: DisplayNodeSlot) -> Option<&Scene> {
        Some(&self.node(slot)?.composed_scene)
    }

    pub(crate) fn snapshot_for_slot(&self, slot: DisplayNodeSlot) -> Option<ElementSnapshot> {
        self.node(slot)?.display.snapshot
    }

    pub(crate) fn slot_has_composed_scene(&self, slot: DisplayNodeSlot) -> bool {
        let Some(node) = self.node(slot) else {
            return false;
        };
        if !node.caches_composed_scene() {
            return false;
        }
        let Some(parent) = node.parent else {
            return true;
        };
        !self
            .node(parent)
            .is_some_and(DisplayNode::caches_composed_scene)
    }

    fn sync_branch(
        &mut self,
        element_id: ElementId,
        parent: Option<DisplayNodeSlot>,
        is_drag_preview: bool,
        skip_element_id: Option<ElementId>,
        box_tree: &BoxTree,
        existing: &mut FxHashMap<ElementId, DisplayNodeSlot>,
        active_ids: &mut FxHashSet<ElementId>,
        newly_active_ids: &mut FxHashSet<ElementId>,
        out: &mut Vec<DisplayNodeSlot>,
    ) {
        if !is_drag_preview && Some(element_id) == skip_element_id {
            return;
        }

        if box_tree
            .flags(element_id.0)
            .is_none_or(|f| !f.contains(NodeFlags::VISIBLE))
        {
            return;
        }

        let mut child_items: Vec<StackingContextItem> = Vec::new();
        collect_stacking_context_items_into(element_id, box_tree, &mut child_items);

        let paints_this_node = if is_drag_preview {
            true
        } else {
            box_tree
                .world_bounds(element_id.0)
                .is_none_or(|bounds| bounds.area() != 0.0)
        };

        if paints_this_node {
            let (slot, is_new) = match existing.remove(&element_id) {
                Some(slot) => (slot, false),
                None => (self.alloc_slot(element_id), true),
            };
            let mut child_slots = Vec::with_capacity(child_items.len());
            for child in child_items {
                self.sync_branch(
                    child.element_id,
                    Some(slot),
                    is_drag_preview,
                    skip_element_id,
                    box_tree,
                    existing,
                    active_ids,
                    newly_active_ids,
                    &mut child_slots,
                );
            }

            let children = ChildList::new(child_slots, &self.nodes);
            let inactive_display = if is_new {
                self.inactive_elements.remove(&element_id)
            } else {
                None
            };
            let subtree_size = 1 + children
                .ordered
                .iter()
                .map(|child| {
                    self.node(*child)
                        .map(|child_node| child_node.subtree_size)
                        .unwrap_or(0)
                })
                .sum::<usize>();
            let node = self.node_mut(slot).expect("display list node missing");
            let structure_changed = node.parent != parent
                || node.children.ordered != children.ordered
                || node.subtree_size != subtree_size;
            node.element_id = Some(element_id);
            node.parent = parent;
            node.children = children;
            node.subtree_size = subtree_size;
            if let Some(display) = inactive_display {
                node.display = display;
            }
            if is_new || structure_changed {
                node.composed_dirty = true;
            }
            let parent_to_mark = if is_new || structure_changed {
                node.parent
            } else {
                None
            };
            self.slot_by_id.insert(element_id, slot);
            active_ids.insert(element_id);
            if is_new {
                newly_active_ids.insert(element_id);
            }
            out.push(slot);
            self.mark_composed_dirty_from_slot(parent_to_mark);
        } else {
            for child in child_items {
                self.sync_branch(
                    child.element_id,
                    parent,
                    is_drag_preview,
                    skip_element_id,
                    box_tree,
                    existing,
                    active_ids,
                    newly_active_ids,
                    out,
                );
            }
        }
    }

    fn find_slot(&self, id: ElementId) -> Option<DisplayNodeSlot> {
        self.slot_by_id.get(&id).copied()
    }

    fn alloc_slot(&mut self, element_id: ElementId) -> DisplayNodeSlot {
        if let Some(slot) = self.free_list.pop() {
            self.nodes[slot.0] = Some(DisplayNode {
                element_id: Some(element_id),
                ..DisplayNode::default()
            });
            return slot;
        }

        let slot = DisplayNodeSlot(self.nodes.len());
        self.nodes.push(Some(DisplayNode {
            element_id: Some(element_id),
            ..DisplayNode::default()
        }));
        slot
    }

    fn free_slot(&mut self, slot: DisplayNodeSlot) {
        if let Some(node) = self.nodes.get_mut(slot.0).and_then(Option::take) {
            if let Some(element_id) = node.element_id {
                self.slot_by_id.remove(&element_id);
            }
            if node.element_id.is_some() && self.active_count > 0 {
                self.active_count -= 1;
            }
            self.free_list.push(slot);
        }
    }

    fn node(&self, slot: DisplayNodeSlot) -> Option<&DisplayNode> {
        self.nodes.get(slot.0)?.as_ref()
    }

    fn node_mut(&mut self, slot: DisplayNodeSlot) -> Option<&mut DisplayNode> {
        self.nodes.get_mut(slot.0)?.as_mut()
    }

    fn mark_composed_dirty_from_slot(&mut self, mut current: Option<DisplayNodeSlot>) {
        while let Some(slot) = current {
            let Some(node) = self.node_mut(slot) else {
                break;
            };
            node.composed_dirty = true;
            current = node.parent;
        }
    }

    fn append_node_contents_to_scene(
        &self,
        slot: DisplayNodeSlot,
        scene: &mut Scene,
        scene_world_transform: Affine,
    ) {
        let Some(node) = self.node(slot) else {
            return;
        };
        let Some(snapshot) = node.display.snapshot else {
            return;
        };
        // The composed scene is flattened into a single coordinate space rooted at
        // `scene_world_transform`, so every descendant transform must be expressed
        // relative to that same anchor.
        let local_transform = scene_world_transform.inverse() * snapshot.world_transform;

        scene.reserve_like(&node.display.paint.scene);
        scene.reserve_like(&node.display.post.scene);
        if let Some(clip) = snapshot.clip {
            let _ = scene.push_clip(Clip::Fill {
                transform: local_transform,
                shape: Geometry::RoundedRect(clip),
                fill_rule: peniko::Fill::NonZero,
            });
        }
        scene.append_transformed(&node.display.paint.scene, local_transform);
        for child in &node.children.ordered {
            let Some(child_node) = self.node(*child) else {
                continue;
            };
            if self.slot_has_composed_scene(*child) && !child_node.composed_dirty {
                let Some(child_snapshot) = child_node.display.snapshot else {
                    continue;
                };
                let child_transform =
                    scene_world_transform.inverse() * child_snapshot.world_transform;
                scene.reserve_like(&child_node.composed_scene);
                scene.append_transformed(&child_node.composed_scene, child_transform);
            } else {
                self.append_node_contents_to_scene(*child, scene, scene_world_transform);
            }
        }
        scene.append_transformed(&node.display.post.scene, local_transform);
        if snapshot.clip.is_some() {
            scene.pop_clip();
        }
    }
}

impl DisplayNode {
    fn caches_composed_scene(&self) -> bool {
        self.subtree_size >= COMPOSED_SCENE_MIN_SUBTREE_SIZE
    }
}

#[doc(hidden)]
pub mod bench_support {
    use super::*;
    use crate::ViewId;
    use imaging::{PaintSink, record::replay};
    use peniko::Color;
    use std::collections::VecDeque;

    #[derive(Clone, Copy, Debug)]
    pub enum TreeShape {
        Deep,
        Broad,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum InvalidationDepth {
        Shallow,
        Deep,
    }

    pub struct SyntheticDisplayList {
        list: RetainedDisplayList,
        root_slot: DisplayNodeSlot,
        shallow_id: ElementId,
        deep_id: ElementId,
        scene_commands: usize,
        mutation_epoch: u32,
    }

    impl SyntheticDisplayList {
        pub fn new(node_count: usize, scene_commands: usize, shape: TreeShape) -> Self {
            assert!(node_count >= 1);
            let mut list = RetainedDisplayList::default();
            let (root_slot, root_id) = attach_node(
                &mut list,
                None,
                snapshot(Affine::IDENTITY),
                make_scene(scene_commands, 0),
            );

            let mut shallow_id = root_id;
            let mut deep_id = root_id;

            match shape {
                TreeShape::Deep => {
                    let mut parent = root_slot;
                    for idx in 1..node_count {
                        let offset = idx as f64;
                        let (slot, element_id) = attach_node(
                            &mut list,
                            Some(parent),
                            snapshot(Affine::translate((offset, offset * 0.25))),
                            make_scene(scene_commands, idx as u32),
                        );
                        if idx == 1 {
                            shallow_id = element_id;
                        }
                        deep_id = element_id;
                        parent = slot;
                    }
                }
                TreeShape::Broad => {
                    let mut frontier = VecDeque::from([root_slot]);
                    let mut parent = root_slot;
                    let mut remaining_at_parent = 8usize;
                    for idx in 1..node_count {
                        if remaining_at_parent == 0 {
                            parent = frontier.pop_front().expect("broad tree parent");
                            remaining_at_parent = 8;
                        }
                        let offset = idx as f64;
                        let (slot, element_id) = attach_node(
                            &mut list,
                            Some(parent),
                            snapshot(Affine::translate((offset * 0.5, offset * 0.125))),
                            make_scene(scene_commands, idx as u32),
                        );
                        if idx == 1 {
                            shallow_id = element_id;
                        }
                        deep_id = element_id;
                        frontier.push_back(slot);
                        remaining_at_parent -= 1;
                    }
                }
            }

            finalize_subtree_sizes(&mut list, root_slot);

            Self {
                list,
                root_slot,
                shallow_id,
                deep_id,
                scene_commands,
                mutation_epoch: node_count as u32,
            }
        }

        pub fn compose_root(&mut self) {
            self.list.ensure_composed_scene(self.root_slot);
        }

        pub fn invalidate(&mut self, depth: InvalidationDepth) {
            let element_id = match depth {
                InvalidationDepth::Shallow => self.shallow_id,
                InvalidationDepth::Deep => self.deep_id,
            };
            let slot = self.list.find_slot(element_id).expect("bench node");
            self.mutation_epoch = self.mutation_epoch.wrapping_add(1);
            let node = self.list.node_mut(slot).expect("bench node");
            node.display
                .paint
                .set_scene(make_scene(self.scene_commands, self.mutation_epoch));
            self.list.mark_composed_dirty(element_id);
        }

        pub fn mark_dirty(&mut self, depth: InvalidationDepth) {
            let element_id = match depth {
                InvalidationDepth::Shallow => self.shallow_id,
                InvalidationDepth::Deep => self.deep_id,
            };
            self.list.mark_composed_dirty(element_id);
        }

        pub fn replay_composed<S: PaintSink>(&mut self, sink: &mut S) {
            self.list.ensure_composed_scene(self.root_slot);
            if let Some(scene) = self.list.composed_scene(self.root_slot) {
                replay(scene, sink);
            }
        }
    }

    fn attach_node(
        list: &mut RetainedDisplayList,
        parent: Option<DisplayNodeSlot>,
        snapshot: ElementSnapshot,
        paint_scene: Scene,
    ) -> (DisplayNodeSlot, ElementId) {
        let element_id = ViewId::new().get_element_id();
        let slot = list.alloc_slot(element_id);
        {
            let node = list.node_mut(slot).expect("bench node");
            node.parent = parent;
            node.display.snapshot = Some(snapshot);
            node.display.paint.set_scene(paint_scene);
            node.display.post.set_scene(Scene::new());
            node.composed_dirty = true;
        }
        list.slot_by_id.insert(element_id, slot);
        if let Some(parent) = parent {
            list.node_mut(parent)
                .expect("bench parent")
                .children
                .ordered
                .push(slot);
        } else {
            list.roots.push(slot);
        }
        (slot, element_id)
    }

    fn finalize_subtree_sizes(list: &mut RetainedDisplayList, slot: DisplayNodeSlot) -> usize {
        let child_slots = list.child_slots(slot).expect("bench children").to_vec();
        let subtree_size = 1 + child_slots
            .into_iter()
            .map(|child| finalize_subtree_sizes(list, child))
            .sum::<usize>();
        list.node_mut(slot).expect("bench node").subtree_size = subtree_size;
        subtree_size
    }

    fn snapshot(world_transform: Affine) -> ElementSnapshot {
        ElementSnapshot {
            local_bounds: Rect::new(0.0, 0.0, 100.0, 40.0),
            clip: None,
            world_transform,
        }
    }

    fn make_scene(command_count: usize, seed: u32) -> Scene {
        let mut scene = Scene::new();
        for idx in 0..command_count {
            let offset = f64::from(seed) + idx as f64;
            let rect = Rect::new(offset, offset * 0.5, offset + 10.0, offset * 0.5 + 8.0);
            let color = if idx % 2 == 0 {
                Color::from_rgb8(0x33, 0x66, 0x99)
            } else {
                Color::from_rgb8(0xaa, 0x55, 0x22)
            };
            let draw = if idx % 3 == 0 {
                Draw::Stroke {
                    transform: Affine::translate((offset * 0.25, offset * 0.1)),
                    stroke: peniko::kurbo::Stroke::new(1.0),
                    brush: color.into(),
                    brush_transform: None,
                    shape: Geometry::Rect(rect),
                    composite: imaging::Composite::default(),
                }
            } else {
                Draw::Fill {
                    transform: Affine::translate((offset * 0.25, offset * 0.1)),
                    fill_rule: peniko::Fill::NonZero,
                    brush: color.into(),
                    brush_transform: None,
                    shape: Geometry::Rect(rect),
                    composite: imaging::Composite::default(),
                }
            };
            let _ = scene.draw(draw);
        }
        scene
    }
}

pub struct RecordingRenderer<'a> {
    scene: &'a mut Scene,
}

impl<'a> RecordingRenderer<'a> {
    pub(crate) fn new(scene: &'a mut Scene) -> Self {
        Self { scene }
    }
}

impl PaintSink for RecordingRenderer<'_> {
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        let _ = self.scene.push_clip(clip.to_owned());
    }

    fn pop_clip(&mut self) {
        self.scene.pop_clip();
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        PaintSink::push_group(self.scene, group);
    }

    fn pop_group(&mut self) {
        self.scene.pop_group();
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        let _ = self.scene.draw(draw.to_owned());
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        let _ = self.scene.draw(draw.to_owned());
    }

    fn glyph_run(&mut self, draw: GlyphRunRef<'_>, glyphs: &mut dyn Iterator<Item = ImagingGlyph>) {
        let _ = self.scene.draw(Draw::GlyphRun(draw.to_owned(glyphs)));
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        let _ = self.scene.draw(Draw::BlurredRoundedRect(draw));
    }
}

pub(crate) fn replay_scene(
    scene: &Scene,
    sink: &mut dyn PaintSink,
    base_transform: Affine,
    render_size: Size,
) {
    let mut sink = SanitizingSink {
        inner: sink,
        render_size,
    };
    replay_transformed(scene, &mut sink, base_transform);
}

pub(crate) fn replay_view_clip(
    sink: &mut dyn PaintSink,
    clip: RoundedRect,
    base_transform: Affine,
    render_size: Size,
) {
    let clip = constrain_infinite_rounded_rect(clip, base_transform, render_size);
    PaintSink::push_clip(sink, ClipRef::fill(clip).with_transform(base_transform));
}

struct SanitizingSink<'a> {
    inner: &'a mut dyn PaintSink,
    render_size: Size,
}

impl PaintSink for SanitizingSink<'_> {
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        let clip = sanitize_clip_ref(clip, self.render_size);
        self.inner.push_clip(clip.as_ref());
    }

    fn pop_clip(&mut self) {
        self.inner.pop_clip();
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        let clip = group
            .clip
            .map(|clip| sanitize_clip_ref(clip, self.render_size));
        let group = GroupRef {
            clip: clip.as_ref().map(Clip::as_ref),
            mask: group.mask.clone(),
            filters: group.filters,
            composite: group.composite,
        };
        self.inner.push_group(group);
    }

    fn pop_group(&mut self) {
        self.inner.pop_group();
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        self.inner.fill(draw);
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        self.inner.stroke(draw);
    }

    fn glyph_run(&mut self, draw: GlyphRunRef<'_>, glyphs: &mut dyn Iterator<Item = ImagingGlyph>) {
        self.inner.glyph_run(draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        self.inner.blurred_rounded_rect(draw);
    }
}

fn scene_transform_class(scene: &Scene) -> TransformClass {
    scene
        .commands()
        .iter()
        .map(|command| command_transform_class(scene, command))
        .fold(TransformClass::Exact, TransformClass::combine)
}

fn command_transform_class(scene: &Scene, command: &Command) -> TransformClass {
    match command {
        Command::PushContext(_) | Command::PopContext => TransformClass::Exact,
        Command::PushClip(_) | Command::PopClip | Command::PushGroup(_) | Command::PopGroup => {
            TransformClass::Affine
        }
        Command::Draw(id) => match scene.draw_op(*id) {
            Draw::Fill { .. } | Draw::Stroke { .. } => TransformClass::Affine,
            Draw::GlyphRun(_) | Draw::BlurredRoundedRect(_) => TransformClass::TranslateOnly,
        },
    }
}

fn sanitize_clip_ref(clip: ClipRef<'_>, render_size: Size) -> Clip {
    match clip {
        ClipRef::Fill {
            transform,
            shape,
            fill_rule,
        } => Clip::Fill {
            transform,
            shape: sanitize_clip_geometry(shape, transform, render_size),
            fill_rule,
        },
        ClipRef::Stroke {
            transform,
            shape,
            stroke,
        } => Clip::Stroke {
            transform,
            shape: sanitize_clip_geometry(shape, transform, render_size),
            stroke: stroke.clone(),
        },
    }
}

fn sanitize_clip_geometry(
    shape: GeometryRef<'_>,
    transform: Affine,
    render_size: Size,
) -> Geometry {
    match shape {
        GeometryRef::Rect(rect) => {
            Geometry::Rect(constrain_infinite_rect(rect, transform, render_size))
        }
        GeometryRef::RoundedRect(rect) => Geometry::RoundedRect(constrain_infinite_rounded_rect(
            rect,
            transform,
            render_size,
        )),
        GeometryRef::Path(path) => Geometry::Path(path.clone()),
        GeometryRef::OwnedPath(path) => Geometry::Path(path),
    }
}

fn constrain_infinite_rounded_rect(
    rect: RoundedRect,
    transform: Affine,
    render_size: Size,
) -> RoundedRect {
    let constrained = constrain_infinite_rect(rect.rect(), transform, render_size);
    RoundedRect::from_rect(constrained, rect.radii())
}

pub(crate) fn constrain_rect_to_render_bounds(
    rect: Rect,
    transform: Affine,
    render_size: Size,
) -> Rect {
    if rect.x0.is_finite() && rect.x1.is_finite() && rect.y0.is_finite() && rect.y1.is_finite() {
        return rect;
    }

    let viewport = Rect::from_origin_size(Point::ZERO, render_size);
    let inverse = transform.inverse();
    let local_viewport = inverse.transform_rect_bbox(viewport);

    Rect::new(
        if rect.x0.is_finite() {
            rect.x0
        } else {
            local_viewport.x0
        },
        if rect.y0.is_finite() {
            rect.y0
        } else {
            local_viewport.y0
        },
        if rect.x1.is_finite() {
            rect.x1
        } else {
            local_viewport.x1
        },
        if rect.y1.is_finite() {
            rect.y1
        } else {
            local_viewport.y1
        },
    )
}

fn constrain_infinite_rect(rect: Rect, transform: Affine, render_size: Size) -> Rect {
    constrain_rect_to_render_bounds(rect, transform, render_size)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::ViewId;
    use imaging::Composite;
    use peniko::{Color, Fill};

    fn fill_draw(rect: Rect, transform: Affine) -> Draw {
        Draw::Fill {
            transform,
            fill_rule: Fill::NonZero,
            brush: Color::BLACK.into(),
            brush_transform: None,
            shape: Geometry::Rect(rect),
            composite: Composite::default(),
        }
    }

    fn fill_draw_with_color(rect: Rect, transform: Affine, color: Color) -> Draw {
        Draw::Fill {
            transform,
            fill_rule: Fill::NonZero,
            brush: color.into(),
            brush_transform: None,
            shape: Geometry::Rect(rect),
            composite: Composite::default(),
        }
    }

    fn scene_with_draw(draw: Draw) -> Scene {
        let mut scene = Scene::new();
        let _ = scene.draw(draw);
        scene
    }

    fn scene_with_clip_and_draw(clip: RoundedRect, clip_transform: Affine, draw: Draw) -> Scene {
        let mut scene = Scene::new();
        let _ = scene.push_clip(Clip::Fill {
            transform: clip_transform,
            shape: Geometry::RoundedRect(clip),
            fill_rule: Fill::NonZero,
        });
        let _ = scene.draw(draw);
        scene.pop_clip();
        scene
    }

    fn snapshot(world_transform: Affine, clip: Option<RoundedRect>) -> ElementSnapshot {
        ElementSnapshot {
            local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
            clip,
            world_transform,
        }
    }

    fn attach_node(
        list: &mut RetainedDisplayList,
        parent: Option<DisplayNodeSlot>,
        snapshot: ElementSnapshot,
        paint_scene: Scene,
        post_scene: Scene,
    ) -> (DisplayNodeSlot, ElementId) {
        let element_id = ViewId::new().get_element_id();
        let slot = list.alloc_slot(element_id);
        {
            let node = list.node_mut(slot).expect("node");
            node.parent = parent;
            node.display.snapshot = Some(snapshot);
            node.display.paint.set_scene(paint_scene);
            node.display.post.set_scene(post_scene);
            node.composed_dirty = true;
        }
        list.slot_by_id.insert(element_id, slot);
        if let Some(parent) = parent {
            list.node_mut(parent)
                .expect("parent")
                .children
                .ordered
                .push(slot);
        } else {
            list.roots.push(slot);
        }
        (slot, element_id)
    }

    fn finalize_subtree_sizes(list: &mut RetainedDisplayList, slot: DisplayNodeSlot) -> usize {
        let children = list.child_slots(slot).expect("children").to_vec();
        let subtree_size = 1 + children
            .into_iter()
            .map(|child| finalize_subtree_sizes(list, child))
            .sum::<usize>();
        let node = list.node_mut(slot).expect("node");
        node.subtree_size = subtree_size;
        subtree_size
    }

    fn make_cached_root_with_fillers(
        root_snapshot: ElementSnapshot,
        root_paint: Scene,
        root_post: Scene,
        filler_count: usize,
    ) -> (RetainedDisplayList, DisplayNodeSlot, ElementId) {
        let mut list = RetainedDisplayList::default();
        let (root_slot, root_id) =
            attach_node(&mut list, None, root_snapshot, root_paint, root_post);
        for i in 0..filler_count {
            let filler_transform = Affine::translate((100.0 + i as f64, 200.0 + i as f64));
            let filler_snapshot = snapshot(filler_transform, None);
            let _ = attach_node(
                &mut list,
                Some(root_slot),
                filler_snapshot,
                Scene::new(),
                Scene::new(),
            );
        }
        finalize_subtree_sizes(&mut list, root_slot);
        (list, root_slot, root_id)
    }

    #[test]
    fn stage_stores_scene_directly() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut stage = ElementStage::default();
        let mut scene = Scene::new();
        let _ = scene.draw(fill_draw(rect, Affine::IDENTITY));
        let _ = scene.draw(Draw::Stroke {
            transform: Affine::IDENTITY,
            stroke: peniko::kurbo::Stroke::new(1.0),
            brush: Color::BLACK.into(),
            brush_transform: None,
            shape: Geometry::Rect(rect),
            composite: Composite::default(),
        });
        stage.set_scene(scene);

        assert_eq!(stage.scene.commands().len(), 2);
        assert_eq!(stage.transform_class, TransformClass::Affine);
    }

    #[test]
    fn clip_commands_are_preserved_in_scene() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut stage = ElementStage::default();
        let mut scene = Scene::new();
        let _ = scene.push_clip(Clip::Fill {
            transform: Affine::IDENTITY,
            shape: Geometry::Rect(rect),
            fill_rule: Fill::NonZero,
        });
        let _ = scene.draw(fill_draw(rect, Affine::IDENTITY));
        scene.pop_clip();
        stage.set_scene(scene);

        assert_eq!(stage.scene.commands().len(), 3);
    }

    #[test]
    fn transformed_glyph_or_blur_downgrades_retention() {
        let mut stage = ElementStage::default();
        let mut scene = Scene::new();
        let _ = scene.draw(Draw::BlurredRoundedRect(imaging::BlurredRoundedRect {
            transform: Affine::IDENTITY,
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            color: Color::BLACK,
            radius: 4.0,
            std_dev: 6.0,
            composite: Composite::default(),
        }));
        stage.set_scene(scene);

        assert_eq!(stage.transform_class, TransformClass::TranslateOnly);
    }

    #[test]
    fn infinite_clip_is_constrained_to_render_bounds() {
        let constrained = constrain_infinite_rect(
            Rect::new(f64::NEG_INFINITY, 10.0, f64::INFINITY, 20.0),
            Affine::IDENTITY,
            Size::new(200.0, 100.0),
        );

        assert_eq!(constrained.x0, 0.0);
        assert_eq!(constrained.x1, 200.0);
        assert_eq!(constrained.y0, 10.0);
        assert_eq!(constrained.y1, 20.0);
    }

    #[test]
    fn composed_scene_flattens_transforms_and_clips() {
        let root_clip = RoundedRect::from_rect(Rect::new(0.0, 0.0, 40.0, 30.0), 4.0);
        let child_clip = RoundedRect::from_rect(Rect::new(1.0, 2.0, 9.0, 12.0), 2.0);
        let root_snapshot = snapshot(Affine::translate((10.0, 20.0)), Some(root_clip));
        let root_paint = scene_with_draw(fill_draw(
            Rect::new(0.0, 0.0, 5.0, 5.0),
            Affine::translate((1.0, 2.0)),
        ));
        let (mut list, root_slot, _) =
            make_cached_root_with_fillers(root_snapshot, root_paint, Scene::new(), 6);

        let child_snapshot = snapshot(Affine::translate((15.0, 27.0)), Some(child_clip));
        let child_scene = scene_with_clip_and_draw(
            child_clip,
            Affine::translate((0.5, 1.5)),
            fill_draw(Rect::new(0.0, 0.0, 3.0, 4.0), Affine::translate((3.0, 4.0))),
        );
        let _ = attach_node(
            &mut list,
            Some(root_slot),
            child_snapshot,
            child_scene,
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        assert!(list.slot_has_composed_scene(root_slot));
        list.ensure_composed_scene(root_slot);

        let mut expected = Scene::new();
        let _ = expected.push_clip(Clip::Fill {
            transform: Affine::IDENTITY,
            shape: Geometry::RoundedRect(root_clip),
            fill_rule: Fill::NonZero,
        });
        let _ = expected.draw(fill_draw(
            Rect::new(0.0, 0.0, 5.0, 5.0),
            Affine::translate((1.0, 2.0)),
        ));
        let _ = expected.push_clip(Clip::Fill {
            transform: Affine::translate((5.0, 7.0)),
            shape: Geometry::RoundedRect(child_clip),
            fill_rule: Fill::NonZero,
        });
        let _ = expected.push_clip(Clip::Fill {
            transform: Affine::translate((5.5, 8.5)),
            shape: Geometry::RoundedRect(child_clip),
            fill_rule: Fill::NonZero,
        });
        let _ = expected.draw(fill_draw(
            Rect::new(0.0, 0.0, 3.0, 4.0),
            Affine::translate((8.0, 11.0)),
        ));
        expected.pop_clip();
        expected.pop_clip();
        expected.pop_clip();

        assert_eq!(list.composed_scene(root_slot), Some(&expected));
    }

    #[test]
    fn composed_scene_avoids_nested_cached_subtrees_and_keeps_correct_transform() {
        let root_snapshot = snapshot(Affine::translate((50.0, 60.0)), None);
        let root_paint = scene_with_draw(fill_draw(
            Rect::new(0.0, 0.0, 2.0, 2.0),
            Affine::translate((1.0, 1.0)),
        ));
        let (mut list, root_slot, _) =
            make_cached_root_with_fillers(root_snapshot, root_paint, Scene::new(), 6);

        let child_snapshot = snapshot(Affine::translate((70.0, 90.0)), None);
        let child_paint = scene_with_draw(fill_draw(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            Affine::translate((2.0, 3.0)),
        ));
        let (child_slot, _) = attach_node(
            &mut list,
            Some(root_slot),
            child_snapshot,
            child_paint,
            Scene::new(),
        );
        for i in 0..7 {
            let grandchild_snapshot =
                snapshot(Affine::translate((75.0 + i as f64, 95.0 + i as f64)), None);
            let grandchild_scene = if i == 0 {
                scene_with_draw(fill_draw(
                    Rect::new(0.0, 0.0, 1.0, 1.0),
                    Affine::translate((4.0, 5.0)),
                ))
            } else {
                Scene::new()
            };
            let _ = attach_node(
                &mut list,
                Some(child_slot),
                grandchild_snapshot,
                grandchild_scene,
                Scene::new(),
            );
        }
        finalize_subtree_sizes(&mut list, root_slot);

        assert!(list.slot_has_composed_scene(root_slot));
        assert!(!list.slot_has_composed_scene(child_slot));
        list.ensure_composed_scene(root_slot);

        let mut expected = Scene::new();
        let _ = expected.draw(fill_draw(
            Rect::new(0.0, 0.0, 2.0, 2.0),
            Affine::translate((1.0, 1.0)),
        ));
        let _ = expected.draw(fill_draw(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            Affine::translate((22.0, 33.0)),
        ));
        let _ = expected.draw(fill_draw(
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Affine::translate((29.0, 40.0)),
        ));

        assert_eq!(list.composed_scene(root_slot), Some(&expected));
    }

    #[test]
    fn composed_scene_rebuilds_when_child_stage_changes() {
        let root_snapshot = snapshot(Affine::translate((10.0, 10.0)), None);
        let (mut list, root_slot, _) =
            make_cached_root_with_fillers(root_snapshot, Scene::new(), Scene::new(), 6);

        let child_snapshot = snapshot(Affine::translate((15.0, 17.0)), None);
        let child_scene = scene_with_draw(fill_draw_with_color(
            Rect::new(0.0, 0.0, 2.0, 2.0),
            Affine::translate((1.0, 1.0)),
            Color::BLACK,
        ));
        let (child_slot, child_id) = attach_node(
            &mut list,
            Some(root_slot),
            child_snapshot,
            child_scene,
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        list.ensure_composed_scene(root_slot);
        let first = list
            .composed_scene(root_slot)
            .cloned()
            .expect("composed scene");

        let updated_child_scene = scene_with_draw(fill_draw_with_color(
            Rect::new(0.0, 0.0, 3.0, 3.0),
            Affine::translate((2.0, 2.0)),
            Color::WHITE,
        ));
        list.node_mut(child_slot)
            .expect("child node")
            .display
            .paint
            .set_scene(updated_child_scene);
        list.mark_composed_dirty(child_id);
        list.ensure_composed_scene(root_slot);

        let mut expected = Scene::new();
        let _ = expected.draw(fill_draw_with_color(
            Rect::new(0.0, 0.0, 3.0, 3.0),
            Affine::translate((7.0, 9.0)),
            Color::WHITE,
        ));

        assert_ne!(list.composed_scene(root_slot), Some(&first));
        assert_eq!(list.composed_scene(root_slot), Some(&expected));
    }
}
