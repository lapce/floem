//! Retained paint artifact storage and replay.
//!
//! The retained display list stores per-element paint and post-paint recordings as
//! [`ExtendedScene`] values in local space. Retention happens at the element/stage
//! level: we rerecord only dirty elements and reuse unchanged retained scenes across
//! transform changes when the recorded content allows it.

use std::sync::Arc;

use floem_renderer::text::GlyphRunRef;
use floem_renderer::{DisplayCommandExt, OwnedSvg, Svg};
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GeometryRef, GroupRef, PaintSink,
    StrokeRef,
    record::{Clip, Draw, ExtendedCommand, ExtendedScene, Geometry, Glyph as ImagingGlyph, replay_ext_transformed},
};
use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Size};
use peniko::BrushRef;
use rustc_hash::{FxHashMap, FxHashSet};
use understory_box_tree::NodeFlags;

use crate::{
    BoxTree, ElementId, Rasterizer as AppRasterizer,
    view::stacking::{StackingContextItem, collect_stacking_context_items_into},
};

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
    pub scene: ExtendedScene<DisplayCommandExt>,
    pub transform_class: TransformClass,
}

impl Default for ElementStage {
    fn default() -> Self {
        Self {
            scene: ExtendedScene::new(),
            transform_class: TransformClass::Affine,
        }
    }
}

impl ElementStage {
    pub(crate) fn set_scene(&mut self, scene: ExtendedScene<DisplayCommandExt>) {
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

const LARGE_CHILD_INDEX_THRESHOLD: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DisplayNodeSlot(usize);

#[derive(Clone, Default)]
struct ChildList {
    ordered: Vec<DisplayNodeSlot>,
    direct_lookup: Option<FxHashMap<ElementId, DisplayNodeSlot>>,
}

impl ChildList {
    fn new(children: Vec<DisplayNodeSlot>, nodes: &[Option<DisplayNode>]) -> Self {
        let direct_lookup = (children.len() > LARGE_CHILD_INDEX_THRESHOLD).then(|| {
            children
                .iter()
                .filter_map(|&slot| {
                    let node = nodes.get(slot.0)?.as_ref()?;
                    Some((node.element_id?, slot))
                })
                .collect()
        });
        Self {
            ordered: children,
            direct_lookup,
        }
    }

    fn direct_child(&self, id: ElementId, nodes: &[Option<DisplayNode>]) -> Option<DisplayNodeSlot> {
        if let Some(slot) = self
            .direct_lookup
            .as_ref()
            .and_then(|lookup| lookup.get(&id).copied())
        {
            return Some(slot);
        }

        self.ordered.iter().copied().find(|slot| {
            nodes
                .get(slot.0)
                .and_then(Option::as_ref)
                .is_some_and(|node| node.element_id == Some(id))
        })
    }
}

#[derive(Clone, Default)]
struct DisplayNode {
    element_id: Option<ElementId>,
    parent: Option<DisplayNodeSlot>,
    children: ChildList,
    display: ElementDisplayList,
}

#[derive(Default)]
pub struct RetainedDisplayList {
    roots: Vec<DisplayNodeSlot>,
    nodes: Vec<Option<DisplayNode>>,
    free_list: Vec<DisplayNodeSlot>,
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
        let mut existing = FxHashMap::default();
        for (index, node) in self.nodes.iter().enumerate() {
            let Some(node) = node.as_ref() else {
                continue;
            };
            let Some(element_id) = node.element_id else {
                continue;
            };
            existing.insert(element_id, DisplayNodeSlot(index));
        }

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
            return &mut self.node_mut(slot).expect("display list node missing").display;
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

        let paints_this_node = if is_drag_preview {
            true
        } else {
            box_tree
                .world_bounds(element_id.0)
                .is_none_or(|bounds| bounds.area() != 0.0)
        };

        let mut child_items: Vec<StackingContextItem> = Vec::new();
        collect_stacking_context_items_into(element_id, box_tree, &mut child_items);

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
            let node = self.node_mut(slot).expect("display list node missing");
            node.element_id = Some(element_id);
            node.parent = parent;
            node.children = children;
            if let Some(display) = inactive_display {
                node.display = display;
            }
            active_ids.insert(element_id);
            if is_new {
                newly_active_ids.insert(element_id);
            }
            out.push(slot);
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
        self.roots
            .iter()
            .copied()
            .find_map(|slot| self.find_slot_from(slot, id))
    }

    fn find_slot_from(&self, slot: DisplayNodeSlot, id: ElementId) -> Option<DisplayNodeSlot> {
        let node = self.node(slot)?;
        if node.element_id == Some(id) {
            return Some(slot);
        }
        if let Some(child) = node.children.direct_child(id, &self.nodes) {
            return Some(child);
        }
        node.children
            .ordered
            .iter()
            .copied()
            .find_map(|child| self.find_slot_from(child, id))
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
}

pub struct RecordingRenderer<'a> {
    scene: &'a mut ExtendedScene<DisplayCommandExt>,
}

impl<'a> RecordingRenderer<'a> {
    pub(crate) fn new(scene: &'a mut ExtendedScene<DisplayCommandExt>) -> Self {
        Self { scene }
    }
}

impl RecordingRenderer<'_> {
    pub fn draw_svg<'b>(
        &mut self,
        svg: Svg<'b>,
        rect: Rect,
        transform: Affine,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        let _ = self.scene.custom_command(DisplayCommandExt::DrawSvg {
            svg: OwnedSvg {
                tree: Arc::new(svg.tree.clone()),
                hash: Arc::from(svg.hash.to_vec()),
            },
            rect,
            transform,
            brush: brush.map(|brush| brush.into().to_owned()),
        });
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

pub(crate) fn replay_stage(
    stage: &ElementStage,
    renderer: &mut dyn AppRasterizer,
    base_transform: Affine,
    render_size: Size,
    _local_damage: Option<&[Rect]>,
) {
    let mut sink = SanitizingSink {
        inner: renderer,
        render_size,
    };
    replay_ext_transformed(&stage.scene, &mut sink, base_transform);
}

pub(crate) fn replay_view_clip(
    renderer: &mut dyn AppRasterizer,
    clip: RoundedRect,
    base_transform: Affine,
    render_size: Size,
) {
    let clip = constrain_infinite_rounded_rect(clip, base_transform, render_size);
    PaintSink::push_clip(
        renderer.paint_sink(),
        ClipRef::fill(clip).with_transform(base_transform),
    );
}

struct SanitizingSink<'a> {
    inner: &'a mut (dyn AppRasterizer + floem_renderer::CustomRasterizer),
    render_size: Size,
}

impl PaintSink for SanitizingSink<'_> {
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        let clip = sanitize_clip_ref(clip, self.render_size);
        self.inner.paint_sink().push_clip(clip.as_ref());
    }

    fn pop_clip(&mut self) {
        self.inner.paint_sink().pop_clip();
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        let clip = group.clip.map(|clip| sanitize_clip_ref(clip, self.render_size));
        let group = GroupRef {
            clip: clip.as_ref().map(Clip::as_ref),
            mask: group.mask.clone(),
            filters: group.filters,
            composite: group.composite,
        };
        self.inner.paint_sink().push_group(group);
    }

    fn pop_group(&mut self) {
        self.inner.paint_sink().pop_group();
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        self.inner.paint_sink().fill(draw);
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        self.inner.paint_sink().stroke(draw);
    }

    fn glyph_run(&mut self, draw: GlyphRunRef<'_>, glyphs: &mut dyn Iterator<Item = ImagingGlyph>) {
        self.inner.paint_sink().glyph_run(draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        self.inner.paint_sink().blurred_rounded_rect(draw);
    }
}

impl CustomPaintSink<DisplayCommandExt> for SanitizingSink<'_> {
    fn custom(&mut self, command: &DisplayCommandExt) {
        self.inner.custom_paint_sink().custom(command);
    }
}

fn scene_transform_class(scene: &ExtendedScene<DisplayCommandExt>) -> TransformClass {
    scene
        .commands()
        .iter()
        .map(|command| command_transform_class(scene, command))
        .fold(TransformClass::Exact, TransformClass::combine)
}

fn command_transform_class(
    scene: &ExtendedScene<DisplayCommandExt>,
    command: &ExtendedCommand,
) -> TransformClass {
    match command {
        ExtendedCommand::PushClip(_)
        | ExtendedCommand::PopClip
        | ExtendedCommand::PushGroup(_)
        | ExtendedCommand::PopGroup => TransformClass::Affine,
        ExtendedCommand::Draw(id) => match scene.draw_op(*id) {
            Draw::Fill { .. } | Draw::Stroke { .. } => TransformClass::Affine,
            Draw::GlyphRun(_) | Draw::BlurredRoundedRect(_) => TransformClass::TranslateOnly,
        },
        ExtendedCommand::Custom(id) => match scene.custom(*id) {
            DisplayCommandExt::DrawSvg { .. } => TransformClass::TranslateOnly,
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

fn sanitize_clip_geometry(shape: GeometryRef<'_>, transform: Affine, render_size: Size) -> Geometry {
    match shape {
        GeometryRef::Rect(rect) => Geometry::Rect(constrain_infinite_rect(rect, transform, render_size)),
        GeometryRef::RoundedRect(rect) => {
            Geometry::RoundedRect(constrain_infinite_rounded_rect(rect, transform, render_size))
        }
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

    #[test]
    fn stage_stores_scene_directly() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut stage = ElementStage::default();
        let mut scene = ExtendedScene::new();
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
        let mut scene = ExtendedScene::new();
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
        let mut scene = ExtendedScene::new();
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
}
