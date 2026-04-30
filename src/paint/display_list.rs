//! Retained paint artifact storage and replay.
//!
//! The retained display list stores per-element paint and post-paint recordings as
//! [`Scene`] values in local space. Retention happens at the element/stage
//! level: we rerecord only dirty elements and reuse unchanged retained scenes across
//! transform changes when the recorded content allows it.

use crate::text::GlyphRunRef;
use imaging::{
    BlurredRoundedRect, Brush, ClipRef, FillRef, Filter, GeometryRef, GroupRef, PaintSink,
    StrokeRef,
    record::{
        Clip, Command, Draw, DrawId, Geometry, Glyph as ImagingGlyph, Scene, replay_transformed,
    },
};
use peniko::{
    BlendMode, Fill,
    kurbo::{Affine, Point, Rect, RoundedRect, Shape as _, Size},
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::mem;
use understory_box_tree::NodeFlags;

use crate::{
    BoxTree, ElementId,
    effects::{
        ColorEffect, CompositorEffect, EffectComposite, EffectFilter, EffectGroupRef, SourceEffect,
    },
    external_surface::ExternalSurfaceId,
    paint::composition::{
        CompositionItem, CompositionKey, CompositionPlan, ExternalSurfaceLayer, PaintStage,
        SceneExternalImage, SceneLayer,
    },
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
    pub color_effects: Vec<ColorEffectCommand>,
    pub transform_class: TransformClass,
    pub content_revision: u64,
    stack_index: StageStackIndex,
}

impl Default for ElementStage {
    fn default() -> Self {
        Self {
            scene: Scene::new(),
            color_effects: Vec::new(),
            transform_class: TransformClass::Affine,
            content_revision: 0,
            stack_index: StageStackIndex::default(),
        }
    }
}

impl ElementStage {
    pub(crate) fn set_scene(&mut self, scene: Scene, color_effects: Vec<ColorEffectCommand>) {
        self.transform_class = scene_transform_class(&scene);
        self.stack_index = StageStackIndex::build(&scene);
        self.stack_index.apply_effect_commands(&color_effects);
        self.scene = scene;
        self.color_effects = color_effects;
        self.content_revision = self.content_revision.wrapping_add(1);
    }
}

#[derive(Clone, Default)]
struct StageStackIndex {
    group_depth_before_command: Vec<u16>,
    clip_depth_before_command: Vec<u16>,
    effect_depth_before_command: Vec<u16>,
}

impl StageStackIndex {
    fn build(scene: &Scene) -> Self {
        let mut group_depth = 0u16;
        let mut clip_depth = 0u16;
        let mut group_depth_before_command = Vec::with_capacity(scene.commands().len() + 1);
        let mut clip_depth_before_command = Vec::with_capacity(scene.commands().len() + 1);
        let effect_depth_before_command = vec![0; scene.commands().len() + 1];

        for command in scene.commands() {
            group_depth_before_command.push(group_depth);
            clip_depth_before_command.push(clip_depth);
            match command {
                Command::PushGroup(_) => group_depth = group_depth.saturating_add(1),
                Command::PopGroup => group_depth = group_depth.saturating_sub(1),
                Command::PushClip(_) => clip_depth = clip_depth.saturating_add(1),
                Command::PopClip => clip_depth = clip_depth.saturating_sub(1),
                Command::PushContext(_) | Command::PopContext | Command::Draw(_) => {}
            }
        }

        group_depth_before_command.push(group_depth);
        clip_depth_before_command.push(clip_depth);

        Self {
            group_depth_before_command,
            clip_depth_before_command,
            effect_depth_before_command,
        }
    }

    fn apply_effect_commands(&mut self, effects: &[ColorEffectCommand]) {
        self.effect_depth_before_command.fill(0);
        let mut events = effects.iter().collect::<Vec<_>>();
        events.sort_by_key(|event| event.command_index);
        let mut event_index = 0usize;
        let mut depth = 0u16;
        for command_index in 0..self.effect_depth_before_command.len() {
            while event_index < events.len() && events[event_index].command_index == command_index {
                match &events[event_index].kind {
                    ColorEffectCommandKind::Push(_) => depth = depth.saturating_add(1),
                    ColorEffectCommandKind::Pop => depth = depth.saturating_sub(1),
                }
                event_index += 1;
            }
            self.effect_depth_before_command[command_index] = depth;
        }
    }

    fn has_active_group_or_clip(&self, command_index: usize) -> bool {
        let group_depth = self
            .group_depth_before_command
            .get(command_index)
            .copied()
            .unwrap_or(0);
        let clip_depth = self
            .clip_depth_before_command
            .get(command_index)
            .copied()
            .unwrap_or(0);
        let effect_depth = self
            .effect_depth_before_command
            .get(command_index)
            .copied()
            .unwrap_or(0);
        group_depth > 0 || clip_depth > 0 || effect_depth > 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColorEffectCommand {
    pub command_index: usize,
    pub kind: ColorEffectCommandKind,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ColorEffectCommandKind {
    Push(CompositorEffect),
    Pop,
}

enum EffectGroupClose {
    Group,
    ColorEffect,
}

#[doc(hidden)]
pub struct StageRecorder {
    scene: Scene,
    color_effects: Vec<ColorEffectCommand>,
    effect_group_stack: Vec<Vec<EffectGroupClose>>,
}

impl StageRecorder {
    pub(crate) fn from_stage(stage: &mut ElementStage) -> Self {
        let scene = mem::take(&mut stage.scene);
        Self {
            scene,
            color_effects: mem::take(&mut stage.color_effects),
            effect_group_stack: Vec::new(),
        }
    }

    pub(crate) fn finish(self, stage: &mut ElementStage) {
        debug_assert!(
            self.effect_group_stack.is_empty(),
            "unbalanced Floem effect groups"
        );
        stage.set_scene(self.scene, self.color_effects);
    }

    pub(crate) fn clear(&mut self) {
        self.scene.clear();
        self.color_effects.clear();
        self.effect_group_stack.clear();
    }

    pub fn source_effect_rect(&mut self, rect: Rect, effect: SourceEffect) {
        self.push_effect_group(
            crate::effects::group_ref().with_filters(&[EffectFilter::SourceEffect(effect)]),
        );
        self.fill(FillRef::new(
            GeometryRef::Rect(rect),
            peniko::Color::TRANSPARENT,
        ));
        self.pop_effect_group();
    }

    pub fn push_color_effect(&mut self, effect: ColorEffect) {
        let filters = [EffectFilter::ColorEffect(effect)];
        self.push_effect_group(crate::effects::group_ref().with_filters(&filters));
    }

    pub fn pop_color_effect(&mut self) {
        self.pop_effect_group();
    }

    pub fn push_effect_group(&mut self, group: EffectGroupRef<'_>) {
        let has_compositor_effect = group.filters.iter().any(|filter| {
            matches!(
                filter,
                EffectFilter::ColorEffect(_) | EffectFilter::SourceEffect(_)
            )
        });
        let composite = match group.composite {
            EffectComposite::Imaging(composite) => composite,
            EffectComposite::Shader(effect) => {
                panic!(
                    "shader composite effects require a backdrop render pass and are not implemented yet: {effect:?}"
                )
            }
        };
        let imaging_filters = if has_compositor_effect {
            Vec::new()
        } else {
            group
                .filters
                .iter()
                .filter_map(|filter| match filter {
                    EffectFilter::Imaging(filter) => Some(*filter),
                    EffectFilter::ColorEffect(_) | EffectFilter::SourceEffect(_) => None,
                })
                .collect::<Vec<_>>()
        };
        let erased_group = GroupRef::new()
            .with_filters(&imaging_filters)
            .with_composite(composite);
        let erased_group = if let Some(clip) = group.clip {
            erased_group.with_clip(clip)
        } else {
            erased_group
        };
        let erased_group = if let Some(mask) = group.mask {
            GroupRef {
                clip: erased_group.clip,
                mask: Some(mask),
                filters: erased_group.filters,
                composite: erased_group.composite,
            }
        } else {
            erased_group
        };
        PaintSink::push_group(&mut self.scene, erased_group);
        let mut close_ops = vec![EffectGroupClose::Group];

        if has_compositor_effect {
            for filter in group.filters {
                let effect = match filter {
                    EffectFilter::Imaging(filter) => CompositorEffect::Color(
                        compositor_effect_for_imaging_filter(*filter).unwrap_or_else(|| {
                            panic!(
                                "cannot preserve ordered mixed filter chain with unsupported Imaging filter: {filter:?}"
                            )
                        }),
                    ),
                    EffectFilter::ColorEffect(effect) => CompositorEffect::Color(effect.clone()),
                    EffectFilter::SourceEffect(effect) => CompositorEffect::Source(effect.clone()),
                };
                self.color_effects.push(ColorEffectCommand {
                    command_index: self.current_command_index(),
                    kind: ColorEffectCommandKind::Push(effect),
                });
                close_ops.push(EffectGroupClose::ColorEffect);
                PaintSink::push_group(&mut self.scene, GroupRef::new());
                close_ops.push(EffectGroupClose::Group);
            }
        }

        self.effect_group_stack.push(close_ops);
    }

    pub fn pop_effect_group(&mut self) {
        let close_ops = self
            .effect_group_stack
            .pop()
            .expect("unbalanced Floem effect group pop");
        for op in close_ops.into_iter().rev() {
            match op {
                EffectGroupClose::Group => self.scene.pop_group(),
                EffectGroupClose::ColorEffect => self.pop_compositor_color_effect(),
            }
        }
    }

    fn pop_compositor_color_effect(&mut self) {
        self.color_effects.push(ColorEffectCommand {
            command_index: self.current_command_index(),
            kind: ColorEffectCommandKind::Pop,
        });
    }

    fn current_command_index(&self) -> usize {
        self.scene.commands().len()
    }
}

impl imaging::ImagingSceneSink for StageRecorder {
    fn imaging_scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }
}

fn compositor_effect_for_imaging_filter(filter: Filter) -> Option<ColorEffect> {
    match filter {
        Filter::Blur {
            std_deviation_x,
            std_deviation_y,
        } => {
            let id = u64::from(std_deviation_x.to_bits())
                | (u64::from(std_deviation_y.to_bits()) << 32) ^ 0xB10E_0000_0000_0000;
            Some(
                ColorEffect::wgsl(
                    crate::effects::ColorEffectId(id),
                    format!(
                        r#"
let radius = vec2<f32>({std_deviation_x:?}, {std_deviation_y:?});
let texel = vec2<f32>(1.0 / frame.target_width, 1.0 / frame.target_height);
var acc = textureSample(input_texture, input_sampler, uv) * 0.227027;
acc += textureSample(input_texture, input_sampler, uv + texel * vec2<f32>( radius.x, 0.0)) * 0.1945946;
acc += textureSample(input_texture, input_sampler, uv + texel * vec2<f32>(-radius.x, 0.0)) * 0.1945946;
acc += textureSample(input_texture, input_sampler, uv + texel * vec2<f32>(0.0,  radius.y)) * 0.1216216;
acc += textureSample(input_texture, input_sampler, uv + texel * vec2<f32>(0.0, -radius.y)) * 0.1216216;
acc += textureSample(input_texture, input_sampler, uv + texel * vec2<f32>( radius.x,  radius.y)) * 0.035135;
acc += textureSample(input_texture, input_sampler, uv + texel * vec2<f32>(-radius.x,  radius.y)) * 0.035135;
acc += textureSample(input_texture, input_sampler, uv + texel * vec2<f32>( radius.x, -radius.y)) * 0.035135;
acc += textureSample(input_texture, input_sampler, uv + texel * vec2<f32>(-radius.x, -radius.y)) * 0.035135;
return acc;
"#
                    ),
                )
                .with_label("imaging blur filter"),
            )
        }
        _ => None,
    }
}

impl PaintSink<EffectFilter, EffectComposite> for StageRecorder {
    fn push_context(&mut self, context: imaging::ContextRef<'_>) {
        self.scene.push_context(context.label, context.source);
    }

    fn pop_context(&mut self) {
        self.scene.pop_context();
    }

    fn push_clip(&mut self, clip: ClipRef<'_>) {
        let clip = clip.to_owned();
        self.scene.push_clip(clip);
    }

    fn pop_clip(&mut self) {
        self.scene.pop_clip();
    }

    fn push_group(&mut self, group: EffectGroupRef<'_>) {
        self.push_effect_group(group);
    }

    fn pop_group(&mut self) {
        self.pop_effect_group();
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        PaintSink::fill(&mut self.scene, draw);
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        PaintSink::stroke(&mut self.scene, draw);
    }

    fn glyph_run(&mut self, draw: GlyphRunRef<'_>, glyphs: &mut dyn Iterator<Item = ImagingGlyph>) {
        PaintSink::glyph_run(&mut self.scene, draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        PaintSink::blurred_rounded_rect(&mut self.scene, draw);
    }

    fn scene_picture(&mut self, picture: &imaging::ScenePicture, transform: Affine) {
        PaintSink::scene_picture(&mut self.scene, picture, transform);
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
            clip: box_tree.clipped_local_clip(element_id.0),
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

    pub(crate) fn lower_composition_plan(&self, effective_scale: f64) -> CompositionPlan {
        let mut chunk_index = 0;
        let mut external_occurrence = 0;
        let mut chunks = Vec::new();
        for slot in &self.roots {
            self.lower_slot_into_chunks(
                *slot,
                &mut chunks,
                &mut chunk_index,
                &mut external_occurrence,
                effective_scale,
                0,
            );
        }
        let mut plan = chunks_into_composition_plan(chunks);
        mark_promoted_scene_layers(&mut plan);
        plan
    }

    fn lower_slot_into_chunks(
        &self,
        slot: DisplayNodeSlot,
        chunks: &mut Vec<LoweredChunk>,
        chunk_index: &mut u32,
        external_occurrence: &mut u32,
        effective_scale: f64,
        active_stack_depth: usize,
    ) {
        let Some(node) = self.node(slot) else {
            return;
        };
        let Some(element_id) = node.element_id else {
            return;
        };
        let Some(snapshot) = node.display.snapshot else {
            return;
        };

        let active_stack_depth = if let Some(clip) = snapshot.clip {
            chunks.push(LoweredChunk::PushClip(ClipScope {
                clip,
                transform: snapshot.world_transform,
                bounds: snapshot.local_bounds,
            }));
            active_stack_depth + 1
        } else {
            active_stack_depth
        };

        self.lower_stage_into_plan(
            element_id,
            PaintStage::Paint,
            &node.display.paint,
            snapshot,
            chunks,
            chunk_index,
            external_occurrence,
            effective_scale,
            active_stack_depth,
        );

        for child in &node.children.ordered {
            self.lower_slot_into_chunks(
                *child,
                chunks,
                chunk_index,
                external_occurrence,
                effective_scale,
                active_stack_depth,
            );
        }

        self.lower_stage_into_plan(
            element_id,
            PaintStage::Post,
            &node.display.post,
            snapshot,
            chunks,
            chunk_index,
            external_occurrence,
            effective_scale,
            active_stack_depth,
        );

        if snapshot.clip.is_some() {
            chunks.push(LoweredChunk::PopClip);
        }
    }

    fn lower_stage_into_plan(
        &self,
        _element_id: ElementId,
        _stage_kind: PaintStage,
        stage: &ElementStage,
        snapshot: ElementSnapshot,
        chunks: &mut Vec<LoweredChunk>,
        chunk_index: &mut u32,
        external_occurrence: &mut u32,
        effective_scale: f64,
        active_stack_depth: usize,
    ) {
        if stage.scene.is_empty() {
            return;
        }

        let command_count = stage.scene.commands().len();
        let mut range_start = 0;
        let mut effect_index = 0usize;

        for command_index in 0..=command_count {
            while effect_index < stage.color_effects.len()
                && stage.color_effects[effect_index]
                    .command_index
                    .min(command_count)
                    == command_index
            {
                push_scene_range(
                    chunks,
                    chunk_index,
                    stage,
                    range_start,
                    command_index,
                    effective_scale,
                    snapshot,
                );
                range_start = command_index;
                match &stage.color_effects[effect_index].kind {
                    ColorEffectCommandKind::Push(effect) => {
                        chunks.push(LoweredChunk::PushColorEffect(effect.clone()));
                    }
                    ColorEffectCommandKind::Pop => chunks.push(LoweredChunk::PopColorEffect),
                }
                effect_index += 1;
            }

            let Some(command) = stage.scene.commands().get(command_index) else {
                continue;
            };
            if active_stack_depth == 0
                && !stage.stack_index.has_active_group_or_clip(command_index)
                && let Some(external) = promotable_external_image_fill(
                    &stage.scene,
                    command,
                    *external_occurrence,
                    effective_scale,
                    snapshot.world_transform,
                )
            {
                push_scene_range(
                    chunks,
                    chunk_index,
                    stage,
                    range_start,
                    command_index,
                    effective_scale,
                    snapshot,
                );
                chunks.push(LoweredChunk::External(external));
                *external_occurrence += 1;
                range_start = command_index + 1;
            }
        }

        push_scene_range(
            chunks,
            chunk_index,
            stage,
            range_start,
            command_count,
            effective_scale,
            snapshot,
        );
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

#[derive(Clone)]
enum LoweredChunk {
    PushClip(ClipScope),
    PopClip,
    PushColorEffect(CompositorEffect),
    PopColorEffect,
    Scene(LoweredSceneChunk),
    External(ExternalSurfaceLayer),
}

#[derive(Clone)]
struct ClipScope {
    clip: RoundedRect,
    transform: Affine,
    bounds: Rect,
}

#[derive(Clone)]
struct LoweredSceneChunk {
    scene: Scene,
    external_images: Vec<SceneExternalImage>,
    color_effects: Vec<CompositorEffect>,
    content_revision: u64,
    transform: Affine,
    bounds: Rect,
}

fn chunks_into_composition_plan(chunks: Vec<LoweredChunk>) -> CompositionPlan {
    let mut plan = CompositionPlan::new();
    let mut pending = SceneRunBuilder::default();
    let mut run_index = 0u32;

    for chunk in chunks {
        match chunk {
            LoweredChunk::PushClip(clip) => pending.push_clip(clip),
            LoweredChunk::PopClip => pending.pop_clip(),
            LoweredChunk::PushColorEffect(effect) => {
                pending.flush(&mut plan, &mut run_index);
                pending.push_color_effect(effect);
            }
            LoweredChunk::PopColorEffect => {
                pending.flush(&mut plan, &mut run_index);
                pending.pop_color_effect();
            }
            LoweredChunk::Scene(scene) => pending.push(scene),
            LoweredChunk::External(external) => {
                pending.flush(&mut plan, &mut run_index);
                plan.items.push(CompositionItem::ExternalSurface(external));
            }
        }
    }

    pending.flush(&mut plan, &mut run_index);
    plan
}

#[derive(Default)]
struct SceneRunBuilder {
    scene: Scene,
    external_images: Vec<SceneExternalImage>,
    color_effects: Vec<CompositorEffect>,
    content_revision: u64,
    bounds: Option<Rect>,
    content_chunks: usize,
}

impl SceneRunBuilder {
    fn push_clip(&mut self, clip: ClipScope) {
        push_snapshot_clip(&mut self.scene, clip.clip, clip.transform, clip.bounds);
        let clip_bounds = transform_rect_bbox(clip.transform, clip.bounds);
        self.bounds = Some(match self.bounds {
            Some(bounds) => union_rects(bounds, clip_bounds),
            None => clip_bounds,
        });
    }

    fn pop_clip(&mut self) {
        self.scene.pop_clip();
    }

    fn push_color_effect(&mut self, effect: CompositorEffect) {
        self.color_effects.push(effect);
    }

    fn pop_color_effect(&mut self) {
        let _ = self.color_effects.pop();
    }

    fn push(&mut self, chunk: LoweredSceneChunk) {
        debug_assert_eq!(self.color_effects, chunk.color_effects);
        self.scene.reserve_like(&chunk.scene);
        self.scene.append_transformed(&chunk.scene, chunk.transform);
        self.external_images.extend(chunk.external_images);
        self.content_revision = self.content_revision.wrapping_add(chunk.content_revision);
        self.content_chunks += 1;
        let chunk_bounds = transform_rect_bbox(chunk.transform, chunk.bounds);
        self.bounds = Some(match self.bounds {
            Some(bounds) => union_rects(bounds, chunk_bounds),
            None => chunk_bounds,
        });
    }

    fn flush(&mut self, plan: &mut CompositionPlan, run_index: &mut u32) {
        if self.content_chunks == 0 {
            self.scene.clear();
            self.bounds = None;
            return;
        }

        let source_scene = mem::take(&mut self.scene);
        let content_bounds =
            scene_command_range_bounds(&source_scene, 0, source_scene.commands().len());
        let bounds = match (self.bounds, content_bounds) {
            (Some(bounds), Some(content_bounds)) => union_rects(bounds, content_bounds),
            (Some(bounds), None) => bounds,
            (None, Some(content_bounds)) => content_bounds,
            (None, None) => Rect::ZERO,
        };
        let mut scene = Scene::new();
        scene.reserve_like(&source_scene);
        scene.append_transformed(&source_scene, Affine::translate((-bounds.x0, -bounds.y0)));
        let local_content_bounds = content_bounds.map(|rect| rect - bounds.origin().to_vec2());
        plan.items.push(CompositionItem::Scene(SceneLayer {
            key: CompositionKey::SceneRun {
                run_index: *run_index,
            },
            scene,
            external_images: mem::take(&mut self.external_images),
            color_effects: self.color_effects.clone(),
            content_revision: self.content_revision,
            transform: Affine::translate(bounds.origin().to_vec2()),
            clip: None,
            bounds: Rect::from_origin_size(Point::ZERO, bounds.size()),
            content_bounds: local_content_bounds,
            opacity: 1.0,
            promoted: false,
        }));
        self.content_revision = 0;
        self.bounds = None;
        self.content_chunks = 0;
        *run_index += 1;
    }
}

fn push_scene_range(
    chunks: &mut Vec<LoweredChunk>,
    chunk_index: &mut u32,
    stage: &ElementStage,
    start: usize,
    end: usize,
    effective_scale: f64,
    snapshot: ElementSnapshot,
) {
    let Some(scene) = scene_command_range(&stage.scene, start, end) else {
        return;
    };
    let external_images =
        external_images_in_command_range(&stage.scene, start, end, effective_scale);
    chunks.push(LoweredChunk::Scene(LoweredSceneChunk {
        scene,
        external_images,
        color_effects: active_color_effects_at(stage, start),
        content_revision: stage.content_revision,
        transform: snapshot.world_transform,
        bounds: snapshot.local_bounds,
    }));
    *chunk_index += 1;
}

fn push_snapshot_clip(scene: &mut Scene, clip: RoundedRect, transform: Affine, bounds: Rect) {
    let clip = constrain_infinite_rounded_rect(clip, Affine::IDENTITY, bounds.size());
    let _ = scene.push_clip(Clip::Fill {
        transform,
        shape: Geometry::RoundedRect(clip),
        fill_rule: Fill::NonZero,
    });
}

fn active_color_effects_at(stage: &ElementStage, command_index: usize) -> Vec<CompositorEffect> {
    let mut effects = Vec::new();
    for command in &stage.color_effects {
        if command.command_index > command_index {
            break;
        }
        match &command.kind {
            ColorEffectCommandKind::Push(effect) => effects.push(effect.clone()),
            ColorEffectCommandKind::Pop => {
                let _ = effects.pop();
            }
        }
    }
    effects
}

fn external_images_in_command_range(
    scene: &Scene,
    start: usize,
    end: usize,
    effective_scale: f64,
) -> Vec<SceneExternalImage> {
    scene.commands()[start..end]
        .iter()
        .flat_map(|command| external_images_in_command(scene, command, effective_scale))
        .collect()
}

fn promotable_external_image_fill(
    scene: &Scene,
    command: &Command,
    occurrence: u32,
    effective_scale: f64,
    transform: Affine,
) -> Option<ExternalSurfaceLayer> {
    let Command::Draw(draw_id) = command else {
        return None;
    };
    let Draw::Fill {
        transform: draw_transform,
        fill_rule,
        brush,
        brush_transform,
        shape,
        composite,
    } = scene.draw_op(*draw_id)
    else {
        return None;
    };
    if *fill_rule != Fill::NonZero
        || brush_transform.is_some()
        || composite.blend != BlendMode::default()
    {
        return None;
    }
    let Geometry::Rect(rect) = shape else {
        return None;
    };
    let Brush::Image(image) = brush else {
        return None;
    };
    if image.sampler.x_extend != peniko::Extend::Pad
        || image.sampler.y_extend != peniko::Extend::Pad
    {
        return None;
    }
    let imaging::Image::External(external) = image.image else {
        return None;
    };
    let surface_id = ExternalSurfaceId::from_image_id(external.id)?;
    let promoted_rect = transform_rect_bbox(*draw_transform, *rect);
    let scale = effective_scale.max(f64::EPSILON);
    let source_size = Size::new(
        f64::from(external.width) / scale,
        f64::from(external.height) / scale,
    );
    let opacity = (composite.alpha * image.sampler.alpha).clamp(0.0, 1.0);
    Some(ExternalSurfaceLayer {
        key: CompositionKey::ExternalSurface {
            surface_id,
            occurrence,
        },
        surface_id,
        rect: promoted_rect,
        source_size,
        transform,
        clip: None,
        opacity,
    })
}

fn external_images_in_command(
    scene: &Scene,
    command: &Command,
    effective_scale: f64,
) -> Vec<SceneExternalImage> {
    let Command::Draw(draw_id) = command else {
        return Vec::new();
    };
    match scene.draw_op(*draw_id) {
        Draw::Fill { brush, .. } | Draw::Stroke { brush, .. } => {
            let Some(bounds) = draw_op_bounds(scene, *draw_id) else {
                return Vec::new();
            };
            external_images_in_brush(brush, bounds, effective_scale)
                .into_iter()
                .collect()
        }
        Draw::GlyphRun(run) => glyph_run_bounds(run)
            .and_then(|bounds| external_images_in_brush(&run.brush, bounds, effective_scale))
            .into_iter()
            .collect(),
        Draw::BlurredRoundedRect(_) | Draw::ScenePicture { .. } => Vec::new(),
    }
}

fn external_images_in_brush(
    brush: &Brush,
    rect: Rect,
    effective_scale: f64,
) -> Option<SceneExternalImage> {
    let Brush::Image(image) = brush else {
        return None;
    };
    let imaging::Image::External(external) = image.image else {
        return None;
    };
    let surface_id = ExternalSurfaceId::from_image_id(external.id)?;
    let scale = effective_scale.max(f64::EPSILON);
    Some(SceneExternalImage {
        image_id: external.id,
        surface_id,
        rect,
        source_size: Size::new(
            f64::from(external.width) / scale,
            f64::from(external.height) / scale,
        ),
    })
}

fn mark_promoted_scene_layers(plan: &mut CompositionPlan) {
    let mut earlier_external_bounds = Vec::new();
    for item in &mut plan.items {
        match item {
            CompositionItem::ExternalSurface(layer) => {
                earlier_external_bounds.push(layer_visible_bounds(
                    layer.rect,
                    layer.transform,
                    layer.clip,
                ));
            }
            CompositionItem::Scene(layer) => {
                let scene_bounds = layer_visible_bounds(
                    layer.content_bounds.unwrap_or(layer.bounds),
                    layer.transform,
                    layer.clip,
                );
                layer.promoted = earlier_external_bounds
                    .iter()
                    .any(|external_bounds| rects_overlap(*external_bounds, scene_bounds));
            }
        }
    }
}

fn layer_visible_bounds(rect: Rect, transform: Affine, clip: Option<RoundedRect>) -> Rect {
    let transformed_rect = transform_rect_bbox(transform, rect);
    if let Some(clip) = clip {
        return intersect_rects(
            transformed_rect,
            transform_rect_bbox(transform, clip.rect()),
        );
    }
    transformed_rect
}

fn transform_rect_bbox(transform: Affine, rect: Rect) -> Rect {
    let p0 = transform * rect.origin();
    let p1 = transform * Point::new(rect.x1, rect.y0);
    let p2 = transform * Point::new(rect.x0, rect.y1);
    let p3 = transform * Point::new(rect.x1, rect.y1);
    Rect::new(
        p0.x.min(p1.x).min(p2.x).min(p3.x),
        p0.y.min(p1.y).min(p2.y).min(p3.y),
        p0.x.max(p1.x).max(p2.x).max(p3.x),
        p0.y.max(p1.y).max(p2.y).max(p3.y),
    )
}

fn intersect_rects(a: Rect, b: Rect) -> Rect {
    Rect::new(
        a.x0.max(b.x0),
        a.y0.max(b.y0),
        a.x1.min(b.x1),
        a.y1.min(b.y1),
    )
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.x0 < a.x1
        && a.y0 < a.y1
        && b.x0 < b.x1
        && b.y0 < b.y1
        && a.x0 < b.x1
        && b.x0 < a.x1
        && a.y0 < b.y1
        && b.y0 < a.y1
}

fn scene_command_range_bounds(scene: &Scene, start: usize, end: usize) -> Option<Rect> {
    let mut bounds = None;
    for command in &scene.commands()[start..end] {
        let Command::Draw(draw_id) = command else {
            continue;
        };
        let draw_bounds = draw_op_bounds(scene, *draw_id)?;
        bounds = Some(match bounds {
            Some(bounds) => union_rects(bounds, draw_bounds),
            None => draw_bounds,
        });
    }
    bounds
}

fn draw_op_bounds(scene: &Scene, draw_id: DrawId) -> Option<Rect> {
    match scene.draw_op(draw_id) {
        Draw::Fill {
            transform, shape, ..
        } => Some(transform_rect_bbox(*transform, geometry_bounds(shape))),
        Draw::Stroke {
            transform,
            stroke,
            shape,
            ..
        } => Some(expand_rect(
            transform_rect_bbox(*transform, geometry_bounds(shape)),
            stroke.width * 0.5,
        )),
        Draw::BlurredRoundedRect(draw) => Some(expand_rect(
            transform_rect_bbox(draw.transform, draw.rect),
            draw.std_dev * 3.0,
        )),
        Draw::ScenePicture { transform, picture } => {
            Some(transform_rect_bbox(*transform, picture.bounds()))
        }
        Draw::GlyphRun(run) => glyph_run_bounds(run),
    }
}

fn glyph_run_bounds(run: &imaging::record::GlyphRun) -> Option<Rect> {
    if run.glyphs.is_empty() {
        return None;
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let advance_pad = f64::from(run.font_size) * 0.75;
    let ascent = f64::from(run.font_size);
    let descent = f64::from(run.font_size) * 0.25;

    for glyph in &run.glyphs {
        let x = f64::from(glyph.x);
        let y = f64::from(glyph.y);
        min_x = min_x.min(x);
        min_y = min_y.min(y - ascent);
        max_x = max_x.max(x + advance_pad);
        max_y = max_y.max(y + descent);
    }

    Some(transform_rect_bbox(
        run.transform,
        Rect::new(min_x, min_y, max_x, max_y),
    ))
}

fn geometry_bounds(geometry: &Geometry) -> Rect {
    match geometry {
        Geometry::Rect(rect) => *rect,
        Geometry::RoundedRect(rect) => rect.rect(),
        Geometry::Path(path) => path.bounding_box(),
    }
}

fn expand_rect(rect: Rect, amount: f64) -> Rect {
    Rect::new(
        rect.x0 - amount,
        rect.y0 - amount,
        rect.x1 + amount,
        rect.y1 + amount,
    )
}

fn union_rects(a: Rect, b: Rect) -> Rect {
    Rect::new(
        a.x0.min(b.x0),
        a.y0.min(b.y0),
        a.x1.max(b.x1),
        a.y1.max(b.y1),
    )
}

fn scene_command_range(scene: &Scene, start: usize, end: usize) -> Option<Scene> {
    if start >= end {
        return None;
    }

    let mut out = Scene::new();
    let mut active_commands = Vec::new();
    for command in &scene.commands()[..start] {
        match command {
            Command::PushContext(_) | Command::PushClip(_) | Command::PushGroup(_) => {
                active_commands.push(command);
            }
            Command::PopContext | Command::PopClip | Command::PopGroup => {
                let _ = active_commands.pop();
            }
            Command::Draw(_) => {}
        }
    }
    for command in &active_commands {
        append_scene_command(scene, &mut out, command);
    }
    let mut open_commands = active_commands;
    for command in &scene.commands()[start..end] {
        append_scene_command(scene, &mut out, command);
        match command {
            Command::PushContext(_) | Command::PushClip(_) | Command::PushGroup(_) => {
                open_commands.push(command);
            }
            Command::PopContext | Command::PopClip | Command::PopGroup => {
                let _ = open_commands.pop();
            }
            Command::Draw(_) => {}
        }
    }
    for command in open_commands.iter().rev() {
        match command {
            Command::PushContext(_) => out.pop_context(),
            Command::PushClip(_) => out.pop_clip(),
            Command::PushGroup(_) => out.pop_group(),
            Command::PopContext | Command::PopClip | Command::PopGroup | Command::Draw(_) => {}
        }
    }
    (!out.is_empty()).then_some(out)
}

fn append_scene_command(source: &Scene, dest: &mut Scene, command: &Command) {
    match *command {
        Command::PushContext(id) => {
            let context = source.context(id);
            dest.push_context(context.as_ref(source).label, context.as_ref(source).source);
        }
        Command::PopContext => dest.pop_context(),
        Command::PushClip(id) => {
            dest.push_clip(source.clip(id).clone());
        }
        Command::PopClip => dest.pop_clip(),
        Command::PushGroup(id) => {
            let group = source.group(id).as_ref_with(source);
            PaintSink::push_group(dest, group);
        }
        Command::PopGroup => dest.pop_group(),
        Command::Draw(id) => {
            dest.draw(source.draw_op(id).clone());
        }
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
            node.display.paint.set_scene(
                make_scene(self.scene_commands, self.mutation_epoch),
                Vec::new(),
            );
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
            node.display.paint.set_scene(paint_scene, Vec::new());
            node.display.post.set_scene(Scene::new(), Vec::new());
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
            Draw::ScenePicture { .. } => TransformClass::Affine,
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
    use imaging::{Composite, Filter, ImageBrush};
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

    fn external_image_fill_draw(
        surface_id: crate::external_surface::ExternalSurfaceId,
        rect: Rect,
        effective_scale: f64,
    ) -> Draw {
        let width = (rect.width() * effective_scale).ceil().max(1.0) as u32;
        let height = (rect.height() * effective_scale).ceil().max(1.0) as u32;
        let image = imaging::ExternalImage::new(
            surface_id.image_id(),
            width,
            height,
            peniko::ImageAlphaType::AlphaPremultiplied,
        );
        Draw::Fill {
            transform: Affine::IDENTITY,
            fill_rule: Fill::NonZero,
            brush: Brush::Image(ImageBrush::from(image)),
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

    #[test]
    fn stage_recorder_preserves_order_for_mixed_imaging_and_color_effect_filters() {
        let mut stage = ElementStage::default();
        let mut recorder = StageRecorder::from_stage(&mut stage);
        let effect = ColorEffect::wgsl(crate::effects::ColorEffectId(71), "return color;");
        let filters = [
            EffectFilter::Imaging(Filter::Blur {
                std_deviation_x: 2.0,
                std_deviation_y: 2.0,
            }),
            EffectFilter::ColorEffect(effect.clone()),
            EffectFilter::Imaging(Filter::Blur {
                std_deviation_x: 5.0,
                std_deviation_y: 5.0,
            }),
        ];

        recorder.push_effect_group(crate::effects::group_ref().with_filters(&filters));
        recorder.fill(FillRef::new(
            GeometryRef::Rect(Rect::new(0.0, 0.0, 10.0, 10.0)),
            Color::BLACK,
        ));
        recorder.pop_effect_group();
        recorder.finish(&mut stage);

        assert!(matches!(
            stage.scene.commands(),
            [
                Command::PushGroup(_),
                Command::PushGroup(_),
                Command::PushGroup(_),
                Command::PushGroup(_),
                Command::Draw(_),
                Command::PopGroup,
                Command::PopGroup,
                Command::PopGroup,
                Command::PopGroup,
            ]
        ));

        let Command::PushGroup(final_group) = stage.scene.commands()[0] else {
            panic!("expected final property group");
        };
        assert!(stage.scene.group(final_group).filters.is_empty());

        assert_eq!(stage.color_effects.len(), 6);
        assert_eq!(
            stage
                .color_effects
                .iter()
                .map(|command| command.command_index)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 6, 7, 8]
        );
        assert!(matches!(
            &stage.color_effects[0].kind,
            ColorEffectCommandKind::Push(CompositorEffect::Color(generated)) if generated.id != effect.id
        ));
        assert_eq!(
            stage.color_effects[1],
            ColorEffectCommand {
                command_index: 2,
                kind: ColorEffectCommandKind::Push(CompositorEffect::Color(effect.clone())),
            }
        );
        assert!(matches!(
            &stage.color_effects[2].kind,
            ColorEffectCommandKind::Push(CompositorEffect::Color(generated)) if generated.id != effect.id
        ));
        assert!(
            stage.color_effects[3..]
                .iter()
                .all(|command| matches!(command.kind, ColorEffectCommandKind::Pop))
        );
    }

    #[test]
    fn source_effect_rect_records_placeholder_draw_inside_source_effect() {
        let mut stage = ElementStage::default();
        let mut recorder = StageRecorder::from_stage(&mut stage);
        let effect = crate::effects::SourceEffect::wgsl(
            crate::effects::ShaderEffectId(9),
            "return vec4<f32>(uv, 0.0, 1.0);",
        );

        recorder.source_effect_rect(Rect::new(1.0, 2.0, 11.0, 22.0), effect.clone());
        recorder.finish(&mut stage);

        assert!(matches!(
            stage.scene.commands(),
            [
                Command::PushGroup(_),
                Command::PushGroup(_),
                Command::Draw(_),
                Command::PopGroup,
                Command::PopGroup,
            ]
        ));
        assert_eq!(stage.color_effects.len(), 2);
        assert!(matches!(
            &stage.color_effects[0].kind,
            ColorEffectCommandKind::Push(CompositorEffect::Source(recorded)) if recorded == &effect
        ));
        assert!(matches!(
            stage.color_effects[1].kind,
            ColorEffectCommandKind::Pop
        ));
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
            node.display.paint.set_scene(paint_scene, Vec::new());
            node.display.post.set_scene(post_scene, Vec::new());
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
        stage.set_scene(scene, Vec::new());

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
        stage.set_scene(scene, Vec::new());

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
        stage.set_scene(scene, Vec::new());

        assert_eq!(stage.transform_class, TransformClass::TranslateOnly);
    }

    #[test]
    fn lower_composition_plan_promotes_simple_external_image_fill() {
        let surface_id = crate::external_surface::ExternalSurfaceId::test_new(7);
        let mut list = RetainedDisplayList::default();
        let rect = Rect::new(4.0, 5.0, 24.0, 25.0);
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::translate((10.0, 20.0)), None),
            scene_with_draw(external_image_fill_draw(surface_id, rect, 2.0)),
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(2.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::ExternalSurface(layer) = &plan.items[0] else {
            panic!("expected promoted external image fill");
        };
        assert_eq!(layer.surface_id, surface_id);
        assert_eq!(layer.rect, rect);
        assert_eq!(layer.source_size, rect.size());
        assert_eq!(layer.transform, Affine::translate((10.0, 20.0)));
        assert_eq!(
            layer.key,
            CompositionKey::ExternalSurface {
                surface_id,
                occurrence: 0,
            }
        );
    }

    #[test]
    fn lower_composition_plan_keeps_repeated_external_image_fills_as_distinct_layers() {
        let surface_id = crate::external_surface::ExternalSurfaceId::test_new(9);
        let mut scene = Scene::new();
        let _ = scene.draw(external_image_fill_draw(
            surface_id,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            1.0,
        ));
        let _ = scene.draw(external_image_fill_draw(
            surface_id,
            Rect::new(20.0, 20.0, 30.0, 30.0),
            1.0,
        ));
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::IDENTITY, None),
            scene,
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        let placements = plan
            .items
            .iter()
            .filter_map(|item| match item {
                CompositionItem::ExternalSurface(layer) => Some(layer),
                CompositionItem::Scene(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(placements.len(), 2);
        assert_eq!(
            placements[0].key,
            CompositionKey::ExternalSurface {
                surface_id,
                occurrence: 0
            }
        );
        assert_eq!(
            placements[1].key,
            CompositionKey::ExternalSurface {
                surface_id,
                occurrence: 1
            }
        );
    }

    #[test]
    fn lower_composition_plan_coalesces_adjacent_scene_layers_across_elements() {
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::IDENTITY, None),
            scene_with_draw(fill_draw(Rect::new(0.0, 0.0, 10.0, 10.0), Affine::IDENTITY)),
            Scene::new(),
        );
        let (_label_slot, _label_id) = attach_node(
            &mut list,
            Some(root_slot),
            snapshot(Affine::translate((20.0, 0.0)), None),
            scene_with_draw(fill_draw(Rect::new(0.0, 0.0, 8.0, 8.0), Affine::IDENTITY)),
            Scene::new(),
        );
        let (_canvas_slot, _canvas_id) = attach_node(
            &mut list,
            Some(root_slot),
            snapshot(Affine::translate((0.0, 20.0)), None),
            scene_with_draw(fill_draw(Rect::new(0.0, 0.0, 12.0, 12.0), Affine::IDENTITY)),
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::Scene(layer) = &plan.items[0] else {
            panic!("expected coalesced scene layer");
        };
        assert_eq!(layer.key, CompositionKey::SceneRun { run_index: 0 });
        assert_eq!(layer.scene.commands().len(), 3);
        assert_eq!(layer.transform, Affine::IDENTITY);
        assert_eq!(layer.bounds, Rect::new(0.0, 0.0, 120.0, 120.0));
        assert_eq!(layer.content_bounds, Some(Rect::new(0.0, 0.0, 28.0, 32.0)));
        assert!(layer.external_images.is_empty());
        assert!(!layer.promoted);
    }

    #[test]
    fn lower_composition_plan_coalesces_paint_children_and_post_into_one_scene_run() {
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::IDENTITY, None),
            scene_with_draw(fill_draw(Rect::new(0.0, 0.0, 10.0, 10.0), Affine::IDENTITY)),
            scene_with_draw(fill_draw(
                Rect::new(40.0, 0.0, 50.0, 10.0),
                Affine::IDENTITY,
            )),
        );
        let (_child_slot, _child_id) = attach_node(
            &mut list,
            Some(root_slot),
            snapshot(Affine::translate((20.0, 0.0)), None),
            scene_with_draw(fill_draw(Rect::new(0.0, 0.0, 10.0, 10.0), Affine::IDENTITY)),
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::Scene(layer) = &plan.items[0] else {
            panic!("expected one scene run");
        };
        assert_eq!(layer.key, CompositionKey::SceneRun { run_index: 0 });
        assert_eq!(layer.scene.commands().len(), 3);
        assert_eq!(layer.transform, Affine::IDENTITY);
        assert_eq!(layer.bounds, Rect::new(0.0, 0.0, 120.0, 100.0));
        assert_eq!(layer.content_bounds, Some(Rect::new(0.0, 0.0, 50.0, 10.0)));
        assert!(layer.external_images.is_empty());
    }

    #[test]
    fn lower_composition_plan_scene_run_target_is_local_to_content_bounds() {
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::translate((40.0, 30.0)), None),
            scene_with_draw(fill_draw(
                Rect::new(10.0, 20.0, 30.0, 50.0),
                Affine::IDENTITY,
            )),
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::Scene(layer) = &plan.items[0] else {
            panic!("expected one scene run");
        };
        assert_eq!(layer.transform, Affine::translate((40.0, 30.0)));
        assert_eq!(layer.bounds, Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(
            layer.content_bounds,
            Some(Rect::new(10.0, 20.0, 30.0, 50.0))
        );
        assert_eq!(
            scene_command_range_bounds(&layer.scene, 0, layer.scene.commands().len()),
            Some(Rect::new(10.0, 20.0, 30.0, 50.0))
        );
    }

    #[test]
    fn lower_composition_plan_replays_snapshot_clip_inside_scene_run() {
        let clip = RoundedRect::from_rect(Rect::new(5.0, 6.0, 25.0, 26.0), 3.0);
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::translate((10.0, 20.0)), Some(clip)),
            scene_with_draw(fill_draw(
                Rect::new(0.0, 0.0, 100.0, 100.0),
                Affine::IDENTITY,
            )),
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::Scene(layer) = &plan.items[0] else {
            panic!("expected one scene run");
        };
        assert_eq!(layer.clip, None);
        assert!(matches!(
            layer.scene.commands(),
            [Command::PushClip(_), Command::Draw(_), Command::PopClip]
        ));
        let Command::PushClip(clip_id) = layer.scene.commands()[0] else {
            panic!("expected clip");
        };
        let Clip::Fill {
            transform, shape, ..
        } = layer.scene.clip(clip_id)
        else {
            panic!("expected fill clip");
        };
        assert_eq!(*transform, Affine::IDENTITY);
        assert_eq!(*shape, Geometry::RoundedRect(clip));
    }

    #[test]
    fn lower_composition_plan_sanitizes_infinite_snapshot_clip_inside_scene_run() {
        let clip =
            RoundedRect::from_rect(Rect::new(f64::NEG_INFINITY, 6.0, f64::INFINITY, 26.0), 3.0);
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::translate((10.0, 20.0)), Some(clip)),
            scene_with_draw(fill_draw(
                Rect::new(0.0, 0.0, 100.0, 100.0),
                Affine::IDENTITY,
            )),
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::Scene(layer) = &plan.items[0] else {
            panic!("expected one scene run");
        };
        assert_eq!(layer.clip, None);
        let Command::PushClip(clip_id) = layer.scene.commands()[0] else {
            panic!("expected clip");
        };
        let Clip::Fill {
            transform, shape, ..
        } = layer.scene.clip(clip_id)
        else {
            panic!("expected fill clip");
        };
        assert_eq!(*transform, Affine::IDENTITY);
        assert_eq!(
            *shape,
            Geometry::RoundedRect(RoundedRect::from_rect(
                Rect::new(0.0, 6.0, 100.0, 26.0),
                3.0
            ))
        );
    }

    #[test]
    fn lower_composition_plan_applies_sanitized_ancestor_scroll_clip_to_child_scene_run() {
        let clip =
            RoundedRect::from_rect(Rect::new(f64::NEG_INFINITY, 6.0, f64::INFINITY, 26.0), 3.0);
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::translate((10.0, 20.0)), Some(clip)),
            Scene::new(),
            Scene::new(),
        );
        let (_child_slot, _child_id) = attach_node(
            &mut list,
            Some(root_slot),
            snapshot(Affine::translate((15.0, 30.0)), None),
            scene_with_draw(fill_draw(Rect::new(0.0, 0.0, 20.0, 20.0), Affine::IDENTITY)),
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::Scene(layer) = &plan.items[0] else {
            panic!("expected one scene run");
        };
        assert_eq!(layer.clip, None);
        assert!(matches!(
            layer.scene.commands(),
            [Command::PushClip(_), Command::Draw(_), Command::PopClip]
        ));
        let Command::PushClip(clip_id) = layer.scene.commands()[0] else {
            panic!("expected ancestor clip");
        };
        let Clip::Fill {
            transform, shape, ..
        } = layer.scene.clip(clip_id)
        else {
            panic!("expected fill clip");
        };
        assert_eq!(*transform, Affine::IDENTITY);
        assert_eq!(
            *shape,
            Geometry::RoundedRect(RoundedRect::from_rect(
                Rect::new(0.0, 6.0, 100.0, 26.0),
                3.0
            ))
        );
    }

    #[test]
    fn lower_composition_plan_flattens_external_surface_under_ancestor_clip() {
        let surface_id = crate::external_surface::ExternalSurfaceId::test_new(19);
        let clip = RoundedRect::from_rect(Rect::new(0.0, 0.0, 50.0, 50.0), 0.0);
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::IDENTITY, Some(clip)),
            Scene::new(),
            Scene::new(),
        );
        let (_child_slot, _child_id) = attach_node(
            &mut list,
            Some(root_slot),
            snapshot(Affine::translate((10.0, 10.0)), None),
            {
                let mut scene = Scene::new();
                let _ = scene.draw(fill_draw(Rect::new(0.0, 0.0, 20.0, 20.0), Affine::IDENTITY));
                let _ = scene.draw(external_image_fill_draw(
                    surface_id,
                    Rect::new(20.0, 0.0, 40.0, 20.0),
                    1.0,
                ));
                scene
            },
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::Scene(layer) = &plan.items[0] else {
            panic!("expected flattened scene run");
        };
        assert_eq!(layer.external_images.len(), 1);
        assert_eq!(layer.external_images[0].surface_id, surface_id);
        assert!(matches!(
            layer.scene.commands(),
            [
                Command::PushClip(_),
                Command::Draw(_),
                Command::Draw(_),
                Command::PopClip
            ]
        ));
    }

    #[test]
    fn lower_composition_plan_coalesces_cube_like_scene_runs_around_external_surface() {
        let surface_id = crate::external_surface::ExternalSurfaceId::test_new(10);
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::IDENTITY, None),
            scene_with_draw(fill_draw(
                Rect::new(0.0, 0.0, 100.0, 40.0),
                Affine::IDENTITY,
            )),
            Scene::new(),
        );
        let (_label_slot, _label_id) = attach_node(
            &mut list,
            Some(root_slot),
            snapshot(Affine::translate((0.0, 50.0)), None),
            scene_with_draw(fill_draw(Rect::new(0.0, 0.0, 80.0, 20.0), Affine::IDENTITY)),
            Scene::new(),
        );
        let (_canvas_slot, _canvas_id) = attach_node(
            &mut list,
            Some(root_slot),
            snapshot(Affine::translate((0.0, 100.0)), None),
            {
                let mut scene = Scene::new();
                let _ = scene.draw(fill_draw(
                    Rect::new(0.0, 0.0, 100.0, 100.0),
                    Affine::IDENTITY,
                ));
                let _ = scene.draw(external_image_fill_draw(
                    surface_id,
                    Rect::new(20.0, 20.0, 80.0, 80.0),
                    1.0,
                ));
                let _ = scene.draw(fill_draw(
                    Rect::new(10.0, 10.0, 90.0, 90.0),
                    Affine::IDENTITY,
                ));
                scene
            },
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 3);
        let CompositionItem::Scene(before) = &plan.items[0] else {
            panic!("expected scene before external surface");
        };
        let CompositionItem::ExternalSurface(external) = &plan.items[1] else {
            panic!("expected external surface");
        };
        let CompositionItem::Scene(after) = &plan.items[2] else {
            panic!("expected scene after external surface");
        };
        assert_eq!(before.key, CompositionKey::SceneRun { run_index: 0 });
        assert_eq!(after.key, CompositionKey::SceneRun { run_index: 1 });
        assert_eq!(external.clip, None);

        let scene_command_counts = plan
            .items
            .iter()
            .filter_map(|item| match item {
                CompositionItem::Scene(layer) => Some(layer.scene.commands().len()),
                CompositionItem::ExternalSurface(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(scene_command_counts, vec![3, 1]);
    }

    #[test]
    fn lower_composition_plan_does_not_put_snapshot_clip_on_direct_external_layer() {
        let surface_id = crate::external_surface::ExternalSurfaceId::test_new(17);
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::translate((10.0, 20.0)), None),
            scene_with_draw(external_image_fill_draw(
                surface_id,
                Rect::new(4.0, 5.0, 24.0, 25.0),
                1.0,
            )),
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::ExternalSurface(layer) = &plan.items[0] else {
            panic!("expected direct external surface");
        };
        assert_eq!(layer.surface_id, surface_id);
        assert_eq!(layer.transform, Affine::translate((10.0, 20.0)));
        assert_eq!(layer.clip, None);
    }

    #[test]
    fn lower_composition_plan_flattens_group_spanning_external_surface() {
        let surface_id = crate::external_surface::ExternalSurfaceId::test_new(18);
        let mut scene = Scene::new();
        let _ = scene.push_group(imaging::record::Group::default());
        let _ = scene.draw(fill_draw(Rect::new(0.0, 0.0, 20.0, 20.0), Affine::IDENTITY));
        let _ = scene.draw(external_image_fill_draw(
            surface_id,
            Rect::new(20.0, 0.0, 40.0, 20.0),
            1.0,
        ));
        let _ = scene.draw(fill_draw(
            Rect::new(40.0, 0.0, 60.0, 20.0),
            Affine::IDENTITY,
        ));
        scene.pop_group();

        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::IDENTITY, None),
            scene,
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::Scene(layer) = &plan.items[0] else {
            panic!("expected flattened scene layer");
        };
        assert_eq!(layer.external_images.len(), 1);
        assert_eq!(layer.external_images[0].surface_id, surface_id);
        assert!(matches!(
            layer.scene.commands(),
            [
                Command::PushGroup(_),
                Command::Draw(_),
                Command::Draw(_),
                Command::Draw(_),
                Command::PopGroup,
            ]
        ));
        let Command::Draw(external_draw_id) = layer.scene.commands()[2] else {
            panic!("expected inserted external image draw");
        };
        let Draw::Fill { brush, .. } = layer.scene.draw_op(external_draw_id) else {
            panic!("expected fill draw");
        };
        assert!(
            matches!(brush, Brush::Image(image) if matches!(image.image, imaging::Image::External(_)))
        );
    }

    #[test]
    fn lower_composition_plan_keeps_each_external_surface_as_split_boundary() {
        let first_surface = crate::external_surface::ExternalSurfaceId::test_new(14);
        let second_surface = crate::external_surface::ExternalSurfaceId::test_new(15);
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::IDENTITY, None),
            {
                let mut scene = Scene::new();
                let _ = scene.draw(fill_draw(Rect::new(0.0, 0.0, 10.0, 10.0), Affine::IDENTITY));
                let _ = scene.draw(external_image_fill_draw(
                    first_surface,
                    Rect::new(12.0, 0.0, 18.0, 10.0),
                    1.0,
                ));
                let _ = scene.draw(fill_draw(
                    Rect::new(20.0, 0.0, 30.0, 10.0),
                    Affine::IDENTITY,
                ));
                let _ = scene.draw(external_image_fill_draw(
                    second_surface,
                    Rect::new(32.0, 0.0, 38.0, 10.0),
                    1.0,
                ));
                let _ = scene.draw(fill_draw(
                    Rect::new(40.0, 0.0, 50.0, 10.0),
                    Affine::IDENTITY,
                ));
                scene
            },
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 5);
        assert!(matches!(plan.items[0], CompositionItem::Scene(_)));
        assert!(matches!(plan.items[1], CompositionItem::ExternalSurface(_)));
        assert!(matches!(plan.items[2], CompositionItem::Scene(_)));
        assert!(matches!(plan.items[3], CompositionItem::ExternalSurface(_)));
        assert!(matches!(plan.items[4], CompositionItem::Scene(_)));
    }

    #[test]
    fn lower_composition_plan_flattens_active_clip_spanning_external_surface() {
        let surface_id = crate::external_surface::ExternalSurfaceId::test_new(16);
        let clip = RoundedRect::from_rect(Rect::new(0.0, 0.0, 50.0, 50.0), 0.0);
        let mut scene = Scene::new();
        let _ = scene.push_clip(Clip::Fill {
            transform: Affine::IDENTITY,
            shape: Geometry::RoundedRect(clip),
            fill_rule: Fill::NonZero,
        });
        let _ = scene.draw(fill_draw(Rect::new(4.0, 4.0, 12.0, 12.0), Affine::IDENTITY));
        let _ = scene.draw(external_image_fill_draw(
            surface_id,
            Rect::new(14.0, 14.0, 30.0, 30.0),
            1.0,
        ));
        let _ = scene.draw(fill_draw(
            Rect::new(20.0, 20.0, 28.0, 28.0),
            Affine::IDENTITY,
        ));
        scene.pop_clip();

        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::translate((10.0, 20.0)), None),
            scene,
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);

        let CompositionItem::Scene(layer) = &plan.items[0] else {
            panic!("expected flattened scene");
        };
        assert_eq!(layer.transform, Affine::translate((10.0, 20.0)));
        assert_eq!(layer.clip, None);
        assert_eq!(layer.external_images.len(), 1);
        assert_eq!(layer.external_images[0].surface_id, surface_id);
        let [
            Command::PushClip(clip_id),
            Command::Draw(_),
            Command::Draw(_),
            Command::Draw(_),
            Command::PopClip,
        ] = layer.scene.commands()
        else {
            panic!("expected clipped flattened scene with inserted external image");
        };
        assert!(matches!(layer.scene.clip(*clip_id), Clip::Fill { .. }));
        let Command::Draw(external_draw_id) = layer.scene.commands()[2] else {
            panic!("expected inserted external image draw");
        };
        let Draw::Fill { brush, .. } = layer.scene.draw_op(external_draw_id) else {
            panic!("expected fill draw");
        };
        assert!(
            matches!(brush, Brush::Image(image) if matches!(image.image, imaging::Image::External(_)))
        );
    }

    #[test]
    fn lower_composition_plan_promotes_only_scene_chunks_overlapping_earlier_external_surface() {
        let surface_id = crate::external_surface::ExternalSurfaceId::test_new(11);
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::IDENTITY, None),
            {
                let mut scene = Scene::new();
                let _ = scene.draw(fill_draw(Rect::new(0.0, 0.0, 10.0, 10.0), Affine::IDENTITY));
                let _ = scene.draw(external_image_fill_draw(
                    surface_id,
                    Rect::new(20.0, 20.0, 40.0, 40.0),
                    1.0,
                ));
                let _ = scene.draw(fill_draw(
                    Rect::new(100.0, 100.0, 110.0, 110.0),
                    Affine::IDENTITY,
                ));
                scene
            },
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        let scene_promotions = plan
            .items
            .iter()
            .filter_map(|item| match item {
                CompositionItem::Scene(layer) => Some(layer.promoted),
                CompositionItem::ExternalSurface(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(scene_promotions, vec![false, false]);
    }

    #[test]
    fn lower_composition_plan_promotes_scene_chunk_overlapping_earlier_external_surface() {
        let surface_id = crate::external_surface::ExternalSurfaceId::test_new(12);
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::IDENTITY, None),
            {
                let mut scene = Scene::new();
                let _ = scene.draw(fill_draw(Rect::new(0.0, 0.0, 10.0, 10.0), Affine::IDENTITY));
                let _ = scene.draw(external_image_fill_draw(
                    surface_id,
                    Rect::new(20.0, 20.0, 40.0, 40.0),
                    1.0,
                ));
                let _ = scene.draw(fill_draw(
                    Rect::new(25.0, 25.0, 35.0, 35.0),
                    Affine::IDENTITY,
                ));
                scene
            },
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        let scene_promotions = plan
            .items
            .iter()
            .filter_map(|item| match item {
                CompositionItem::Scene(layer) => Some(layer.promoted),
                CompositionItem::ExternalSurface(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(scene_promotions, vec![false, true]);
    }

    #[test]
    fn lower_composition_plan_flattens_snapshot_clip_spanning_external_surface() {
        let surface_id = crate::external_surface::ExternalSurfaceId::test_new(13);
        let clip = RoundedRect::from_rect(Rect::new(100.0, 100.0, 120.0, 120.0), 0.0);
        let mut list = RetainedDisplayList::default();
        let (root_slot, _root_id) = attach_node(
            &mut list,
            None,
            snapshot(Affine::IDENTITY, Some(clip)),
            {
                let mut scene = Scene::new();
                let _ = scene.draw(fill_draw(Rect::new(0.0, 0.0, 10.0, 10.0), Affine::IDENTITY));
                let _ = scene.draw(external_image_fill_draw(
                    surface_id,
                    Rect::new(20.0, 20.0, 40.0, 40.0),
                    1.0,
                ));
                let _ = scene.draw(fill_draw(
                    Rect::new(25.0, 25.0, 35.0, 35.0),
                    Affine::IDENTITY,
                ));
                scene
            },
            Scene::new(),
        );
        finalize_subtree_sizes(&mut list, root_slot);

        let plan = list.lower_composition_plan(1.0);
        assert_eq!(plan.items.len(), 1);
        let CompositionItem::Scene(layer) = &plan.items[0] else {
            panic!("expected flattened scene");
        };
        assert_eq!(layer.external_images.len(), 1);
        assert_eq!(layer.external_images[0].surface_id, surface_id);
        assert_eq!(layer.clip, None);
        assert!(!layer.promoted);
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
            .set_scene(updated_child_scene, Vec::new());
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
