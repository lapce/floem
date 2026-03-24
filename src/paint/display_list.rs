//! Retained paint artifact storage and replay.
//!
//! This module holds Floem's retained display-list representation. The current design
//! is intentionally moving away from a purely "record a flat command stream per
//! element and replay it blindly" model toward a more Blink-like paint-artifact model:
//! element-local recording, explicit property state, chunking, and eventually
//! selective replay/layerization/compositing.
//!
//! ## Current architecture
//!
//! The core retained object is [`RetainedDisplayList`]. It stores:
//! - A frame-wide paint order of [`PaintOrPost`] entries, which preserves traversal
//!   order and the split between pre-child paint and post-child paint.
//! - Per-element retained stages in [`ElementDisplayList`].
//! - For each stage, a chunked representation in [`ElementStage`].
//!
//! Each [`ElementStage`] is recorded in the element's local coordinate space. Recording
//! local geometry is important because it makes artifacts reusable across transform
//! changes such as scrolling or ancestor movement. The stage is then compiled into:
//! - [`PaintChunk`]s, which are adjacent runs of draw work sharing the same property
//!   state.
//! - A [`PaintPropertyTree`], which currently tracks transform, clip, effect, and
//!   scroll state ids referenced by those chunks.
//!
//! The chunk/property split is the key architectural step away from the older model.
//! The old flat command list encoded state transitions directly in the command stream.
//! The current structure instead treats chunks as "draw work under property state X",
//! which is much closer to the way Blink's paint chunks and property trees work.
//!
//! ## Recording model
//!
//! [`RecordingRenderer`] is the adapter used during paint recording. Views still paint
//! through a familiar imperative API (`fill`, `stroke`, `draw_glyphs`, `draw_img`,
//! `draw_svg`, `push_layer`, etc.), but the retained layer no longer treats that
//! imperative stream as the final artifact format.
//!
//! Recording proceeds in two steps:
//! 1. Views emit [`DisplayCommand`]s in local space.
//! 2. [`ElementStage::set_commands`] compiles those commands into chunks plus property
//!    state.
//!
//! At the moment, this compilation does a few useful things:
//! - Infers [`TransformClass`] from the actual recorded content instead of assuming
//!   every stage is always affine-sensitive.
//! - Coalesces adjacent draw commands that share the same property state.
//! - Tracks chunk bounds and simple chunk metadata.
//! - Moves stage-local clips out of the replay command stream and into clip property
//!   ids.
//!
//! ## Clip model
//!
//! Clip handling is deliberately split in two:
//! - Stage-local clips are represented in the property tree and applied by clip id
//!   during replay.
//! - View-owned overflow clips are replayed by traversal code in `paint/mod.rs` so
//!   they remain active across descendant element replay.
//!
//! This split exists because stage-local clip transitions are local to one retained
//! artifact, while overflow clips affect the replay of descendant artifacts and cannot
//! be modeled as a purely local stage concern.
//!
//! Another important detail is that the retained snapshot only uses the element's
//! intrinsic local clip as artifact identity. We explicitly do **not** use the
//! ancestor-accumulated effective clip as a rerecord key anymore. That avoids the
//! old bug where scrolling changed the accumulated clip and invalidated reusable
//! artifacts even though the local recording itself was unchanged.
//!
//! Some local clips can extend to infinity on visible axes. Those clips are still
//! represented locally for identity purposes, but replay sanitizes them against the
//! current render surface before sending them to the renderer. This keeps retained
//! reuse decoupled from accumulated clip state without ever handing infinite clip
//! geometry to raster backends.
//!
//! ## Replay model
//!
//! Replay happens in [`replay_stage`]. The stage walks chunks, diffs the currently
//! applied clip chain against the chunk's target [`ClipNodeId`], and mutates renderer
//! state to match before replaying the chunk's draw commands.
//!
//! A few optimizations are already in place:
//! - Redundant `set_transform` calls are skipped.
//! - Redundant `set_z_index` calls are skipped.
//! - Stage-local clips are diffed by property id rather than replaying recorded
//!   `PushClip` / `PopClip` commands.
//! - Chunk metadata and bounds are available for future scheduler/compositor work.
//!
//! However, replay is still fundamentally full-frame/full-scene today. Backends such
//! as Vello and tiny-skia rebuild their scene/recording from scratch on every frame.
//! That means chunk-level damage filtering is currently staged but not active as a
//! drawing optimization, because skipping a chunk in a fresh frame would simply make
//! that content disappear.
//!
//! ## Retention and invalidation
//!
//! [`ElementSnapshot`] captures the element-local state used to decide whether an
//! artifact can be reused:
//! - local bounds
//! - local clip
//! - world transform
//!
//! Retention is refined by [`TransformClass`], which describes what transform changes
//! are safe for a recorded artifact. This is especially useful for scroll/content
//! reuse where translation-only movement should not force rerecord.
//!
//! The retained subtree optimization in `paint/mod.rs` builds on top of this by
//! allowing transform boundaries to mark subtrees as reusable under certain transform
//! changes, while still replaying them in the correct frame order.
//!
//! ## Current optimizations
//!
//! The main optimizations already implemented in this module are:
//! - Element-local retained recording.
//! - Stage compilation into chunks.
//! - Property-tree ids for transforms, clips, and effects.
//! - Transform sensitivity inference.
//! - Adjacent draw coalescing by property state.
//! - Chunk bounds and metadata collection.
//! - Property-driven stage-local clip replay.
//! - Infinite clip sanitization at replay time.
//!
//! These changes are mostly architectural. They make later performance work possible
//! without forcing a second format rewrite.
//!
//! ## Where this is going
//!
//! The intended direction is a true retained paint artifact system that can support:
//! - Chunk-level damage-driven replay.
//! - Layer promotion based on chunk/property metadata.
//! - Tiling and partial raster.
//! - Parallel paint artifact construction.
//! - Parallel raster/composite for independent tiles or layers.
//! - Stronger separation between paint, raster, and compositing.
//!
//! In practical terms, the major remaining steps are:
//! 1. Introduce a real retained surface/compositor model so undamaged content can
//!    survive across frames.
//! 2. Promote damage filtering from "artifact metadata" to "actual replay/raster
//!    decision making".
//! 3. Expand property trees so clip/effect/transform/scroll state is not just
//!    recorded, but also reusable across layer/tile boundaries.
//! 4. Teach chunk metadata to drive layerization decisions, especially for blur,
//!    text, images, and isolated compositing effects.
//! 5. Add explicit raster/composite invalidation so rerecord, reraster, and replay
//!    become separate decisions rather than one combined paint decision.
//! 6. Eventually move from a single-threaded replay model toward chunk/tile/layer
//!    scheduling that can exploit parallelism safely.
//!
//! Until those steps land, this module should be read as a staging layer for the
//! future architecture: the artifact format is becoming compositor-friendly before
//! the compositor itself exists.

use std::sync::Arc;

use floem_renderer::text::{Glyph, GlyphRunRef};
use floem_renderer::{Renderer as FloemRenderer, Svg, usvg};
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GroupRef, PaintSink, StrokeRef,
    record::{
        Clip, Draw, ExtendedScene, Geometry, Glyph as ImagingGlyph, GlyphRun, Group,
        replay_ext_transformed,
    },
};
use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Shape, Size};
use peniko::{BrushRef, Fill};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{ElementId, paint::PaintOrPost};

/// Transform class describing when recorded content remains valid.
#[allow(dead_code)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CompositorPromotionHint {
    ScrollContent,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompositorLayerCandidate {
    pub promotion_hint: CompositorPromotionHint,
    pub bounds: Rect,
    pub clip: Option<RoundedRect>,
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
pub(crate) struct OwnedSvg {
    pub tree: Arc<usvg::Tree>,
    pub hash: Arc<[u8]>,
}

#[derive(Clone)]
pub(crate) enum DisplayCommandExt {
    DrawSvg {
        svg: OwnedSvg,
        rect: Rect,
        transform: Affine,
        brush: Option<peniko::Brush>,
    },
}

impl imaging::record::CustomCommand for DisplayCommandExt {
    fn prepend_transform(&self, prefix: Affine) -> Self {
        match self {
            Self::DrawSvg {
                svg,
                rect,
                transform,
                brush,
            } => Self::DrawSvg {
                svg: svg.clone(),
                rect: *rect,
                transform: prefix * *transform,
                brush: brush.clone(),
            },
        }
    }
}

#[derive(Clone)]
pub(crate) enum DisplayCommand {
    PushClip {
        clip: Clip,
    },
    PopClip,
    PushGroup {
        group: Group,
    },
    PopGroup,
    Draw {
        draw: Draw,
    },
    DrawSvg {
        svg: OwnedSvg,
        rect: Rect,
        transform: Affine,
        brush: Option<peniko::Brush>,
    },
}

#[derive(Clone)]
pub(crate) struct ElementStage {
    pub chunks: Vec<PaintChunk>,
    pub property_tree: PaintPropertyTree,
    pub transform_class: TransformClass,
    pub layer_candidate: Option<CompositorLayerCandidate>,
}

impl Default for ElementStage {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            property_tree: PaintPropertyTree::default(),
            transform_class: TransformClass::Affine,
            layer_candidate: None,
        }
    }
}

impl ElementStage {
    pub(crate) fn set_commands(
        &mut self,
        commands: Vec<DisplayCommand>,
        layer_candidate: Option<CompositorLayerCandidate>,
    ) {
        let (chunks, property_tree) = chunk_display_commands(commands);
        self.chunks = chunks;
        self.property_tree = property_tree;
        self.layer_candidate = layer_candidate.clone();
        self.transform_class = if self.chunks.is_empty() {
            TransformClass::Affine
        } else {
            self.chunks
                .iter()
                .map(|chunk| chunk.transform_class)
                .fold(TransformClass::Exact, TransformClass::combine)
        };

        if let Some(layer_candidate) = layer_candidate {
            for chunk in &mut self.chunks {
                chunk.metadata.promotion_hint = Some(layer_candidate.promotion_hint);
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn chunk_indices_for_damage(&self, damage: &[Rect]) -> Vec<usize> {
        self.chunks
            .iter()
            .enumerate()
            .filter_map(|(index, chunk)| {
                chunk
                    .intersects_damage(damage, &self.property_tree)
                    .then_some(index)
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PaintPropertyState {
    pub z_index: i32,
    pub transform_id: TransformNodeId,
    pub clip_id: ClipNodeId,
    pub effect_id: EffectNodeId,
    pub scroll_id: ScrollNodeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintChunkKind {
    Boundary,
    Draw,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct TransformNodeId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct ClipNodeId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct EffectNodeId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct ScrollNodeId(pub u32);

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct TransformNode {
    pub parent: Option<TransformNodeId>,
    pub transform: Affine,
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct ClipNode {
    pub parent: Option<ClipNodeId>,
    pub transform_id: TransformNodeId,
    pub clip: Clip,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct EffectNode {
    pub parent: Option<EffectNodeId>,
    pub blend: peniko::BlendMode,
    pub alpha: f32,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollNode {
    pub parent: Option<ScrollNodeId>,
    pub translation: Affine,
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct PaintPropertyTree {
    pub transforms: Vec<TransformNode>,
    pub clips: Vec<ClipNode>,
    pub effects: Vec<EffectNode>,
    pub scrolls: Vec<ScrollNode>,
}

impl Default for PaintPropertyTree {
    fn default() -> Self {
        Self {
            transforms: vec![TransformNode {
                parent: None,
                transform: Affine::IDENTITY,
            }],
            clips: vec![ClipNode {
                parent: None,
                transform_id: TransformNodeId(0),
                clip: Clip::Fill {
                    transform: Affine::IDENTITY,
                    shape: Geometry::Rect(Rect::ZERO),
                    fill_rule: Fill::NonZero,
                },
            }],
            effects: vec![EffectNode {
                parent: None,
                blend: peniko::BlendMode::default(),
                alpha: 1.0,
            }],
            scrolls: vec![ScrollNode {
                parent: None,
                translation: Affine::IDENTITY,
            }],
        }
    }
}

#[derive(Clone)]
pub(crate) struct PaintChunk {
    pub kind: PaintChunkKind,
    pub properties: PaintPropertyState,
    pub commands: ExtendedScene<DisplayCommandExt>,
    pub bounds: Option<Rect>,
    pub metadata: PaintChunkMetadata,
    pub transform_class: TransformClass,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PaintChunkMetadata {
    pub has_text: bool,
    pub has_raster_image: bool,
    pub has_vector_image: bool,
    pub has_blur: bool,
    pub requires_layer: bool,
    pub promotion_hint: Option<CompositorPromotionHint>,
}

impl PaintChunkMetadata {
    fn merge(self, other: Self) -> Self {
        Self {
            has_text: self.has_text || other.has_text,
            has_raster_image: self.has_raster_image || other.has_raster_image,
            has_vector_image: self.has_vector_image || other.has_vector_image,
            has_blur: self.has_blur || other.has_blur,
            requires_layer: self.requires_layer || other.requires_layer,
            promotion_hint: self.promotion_hint.or(other.promotion_hint),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ElementSnapshot {
    pub local_bounds: Rect,
    pub clip: Option<RoundedRect>,
    pub world_transform: Affine,
    pub promotion_hint: Option<CompositorPromotionHint>,
}

impl ElementSnapshot {
    pub(crate) fn from_box_tree(box_tree: &crate::BoxTree, element_id: ElementId) -> Self {
        Self {
            local_bounds: box_tree.local_bounds(element_id.0).unwrap_or_default(),
            clip: box_tree.local_clip(element_id.0).flatten(),
            world_transform: box_tree.world_transform(element_id.0).unwrap_or_default(),
            promotion_hint: box_tree.compositor_promotion_hint(element_id.0),
        }
    }

    pub(crate) fn supports_reuse(self, current: Self) -> bool {
        self.local_bounds == current.local_bounds
            && self.clip == current.clip
            && self.promotion_hint == current.promotion_hint
    }

    pub(crate) fn layer_candidate(self) -> Option<CompositorLayerCandidate> {
        Some(CompositorLayerCandidate {
            promotion_hint: self.promotion_hint?,
            bounds: self.local_bounds,
            clip: self.clip,
        })
    }
}

impl PaintChunk {
    #[allow(dead_code)]
    pub(crate) fn intersects_damage(
        &self,
        damage: &[Rect],
        property_tree: &PaintPropertyTree,
    ) -> bool {
        let Some(bounds) = self.visible_bounds(property_tree) else {
            return true;
        };
        damage
            .iter()
            .any(|rect| rect.intersect(bounds).area() > 0.0)
    }

    fn visible_bounds(&self, property_tree: &PaintPropertyTree) -> Option<Rect> {
        clip_bounds_for_id(property_tree, self.properties.clip_id)
            .map(|clip_bounds| bounds_intersection(self.bounds, Some(clip_bounds)))
            .unwrap_or(self.bounds)
    }
}

#[derive(Clone, Default)]
pub(crate) struct ElementDisplayList {
    pub paint: ElementStage,
    pub post: ElementStage,
    pub snapshot: Option<ElementSnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PromotedLayerCandidate {
    pub element_id: ElementId,
    pub candidate: CompositorLayerCandidate,
    pub snapshot: ElementSnapshot,
    pub z_index: i32,
}

#[derive(Default)]
pub struct RetainedDisplayList {
    paint_order: Vec<PaintOrPost>,
    elements: FxHashMap<ElementId, ElementDisplayList>,
}

impl RetainedDisplayList {
    pub(crate) fn set_paint_order(&mut self, paint_order: Vec<PaintOrPost>) {
        self.paint_order = paint_order;
    }

    pub(crate) fn paint_order(&self) -> &[PaintOrPost] {
        &self.paint_order
    }

    pub(crate) fn element_mut(&mut self, id: ElementId) -> &mut ElementDisplayList {
        self.elements.entry(id).or_default()
    }

    pub(crate) fn element(&self, id: ElementId) -> Option<&ElementDisplayList> {
        self.elements.get(&id)
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

    pub(crate) fn retain_only(&mut self, ids: &FxHashSet<ElementId>) {
        self.elements.retain(|id, _| ids.contains(id));
    }

    pub(crate) fn promoted_layer_candidates(&self) -> Vec<PromotedLayerCandidate> {
        self.elements
            .iter()
            .filter_map(|(&element_id, element)| {
                let snapshot = element.snapshot?;
                let candidate = element
                    .paint
                    .layer_candidate
                    .clone()
                    .or_else(|| snapshot.layer_candidate())?;
                let z_index = element
                    .paint
                    .chunks
                    .first()
                    .map(|chunk| chunk.properties.z_index)
                    .unwrap_or_default();

                Some(PromotedLayerCandidate {
                    element_id,
                    candidate,
                    snapshot,
                    z_index,
                })
            })
            .collect()
    }
}

pub struct RecordingRenderer<'a> {
    commands: &'a mut Vec<DisplayCommand>,
}

impl<'a> RecordingRenderer<'a> {
    pub(crate) fn new(commands: &'a mut Vec<DisplayCommand>) -> Self {
        Self { commands }
    }

    fn record_draw(&mut self, draw: Draw) {
        self.commands.push(DisplayCommand::Draw { draw });
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
        self.commands.push(DisplayCommand::DrawSvg {
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
        self.commands.push(DisplayCommand::PushClip {
            clip: clip.to_owned(),
        });
    }

    fn pop_clip(&mut self) {
        self.commands.push(DisplayCommand::PopClip);
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        self.commands.push(DisplayCommand::PushGroup {
            group: group.to_owned(),
        });
    }

    fn pop_group(&mut self) {
        self.commands.push(DisplayCommand::PopGroup);
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        self.record_draw(draw.to_owned());
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        self.record_draw(draw.to_owned());
    }

    fn glyph_run(&mut self, draw: GlyphRunRef<'_>, glyphs: &mut dyn Iterator<Item = ImagingGlyph>) {
        self.record_draw(Draw::GlyphRun(draw.to_owned(glyphs)));
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        self.record_draw(Draw::BlurredRoundedRect(draw));
    }
}

struct ReplayRenderer<'a, R> {
    renderer: &'a mut R,
    render_size: Size,
    current_transform: &'a mut Option<Affine>,
}

impl<R> PaintSink for ReplayRenderer<'_, R>
where
    R: FloemRenderer,
{
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        let ClipRef::Fill {
            transform, shape, ..
        } = clip
        else {
            return;
        };
        set_transform_if_needed(self.renderer, transform, self.current_transform);
        match sanitize_clip_geometry(&shape.to_owned(), transform, self.render_size) {
            Geometry::Rect(rect) => self.renderer.clip(&rect),
            Geometry::RoundedRect(rect) => self.renderer.clip(&rect),
            Geometry::Path(path) => self.renderer.clip(&path),
        }
    }

    fn pop_clip(&mut self) {
        self.renderer.clear_clip();
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        let Some(ClipRef::Fill {
            transform, shape, ..
        }) = group.clip
        else {
            return;
        };
        set_transform_if_needed(self.renderer, Affine::IDENTITY, self.current_transform);
        match sanitize_clip_geometry(&shape.to_owned(), transform, self.render_size) {
            Geometry::Rect(rect) => self.renderer.push_layer(
                group.composite.blend,
                group.composite.alpha,
                transform,
                &rect,
            ),
            Geometry::RoundedRect(rect) => self.renderer.push_layer(
                group.composite.blend,
                group.composite.alpha,
                transform,
                &rect,
            ),
            Geometry::Path(path) => self.renderer.push_layer(
                group.composite.blend,
                group.composite.alpha,
                transform,
                &path,
            ),
        }
    }

    fn pop_group(&mut self) {
        self.renderer.pop_layer();
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        set_transform_if_needed(self.renderer, draw.transform, self.current_transform);
        match draw.shape {
            imaging::GeometryRef::Rect(rect) => self.renderer.fill(&rect, draw.brush, 0.0),
            imaging::GeometryRef::RoundedRect(rect) => self.renderer.fill(&rect, draw.brush, 0.0),
            imaging::GeometryRef::Path(path) => self.renderer.fill(path, draw.brush, 0.0),
            imaging::GeometryRef::OwnedPath(path) => self.renderer.fill(&path, draw.brush, 0.0),
        }
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        set_transform_if_needed(self.renderer, draw.transform, self.current_transform);
        match draw.shape {
            imaging::GeometryRef::Rect(rect) => {
                self.renderer.stroke(&rect, draw.brush, draw.stroke)
            }
            imaging::GeometryRef::RoundedRect(rect) => {
                self.renderer.stroke(&rect, draw.brush, draw.stroke)
            }
            imaging::GeometryRef::Path(path) => self.renderer.stroke(path, draw.brush, draw.stroke),
            imaging::GeometryRef::OwnedPath(path) => {
                self.renderer.stroke(&path, draw.brush, draw.stroke)
            }
        }
    }

    fn glyph_run(
        &mut self,
        draw: imaging::GlyphRunRef<'_>,
        glyphs: &mut dyn Iterator<Item = ImagingGlyph>,
    ) {
        let run_ref = GlyphRunRef {
            font: draw.font,
            transform: draw.transform,
            glyph_transform: draw.glyph_transform,
            font_size: draw.font_size,
            hint: draw.hint,
            normalized_coords: draw.normalized_coords,
            style: draw.style,
            brush: draw.brush,
            composite: draw.composite,
        };
        set_transform_if_needed(self.renderer, Affine::IDENTITY, self.current_transform);
        self.renderer.draw_glyphs(
            Point::ZERO,
            &run_ref,
            glyphs.map(|glyph| Glyph {
                id: glyph.id,
                style_index: 0,
                x: glyph.x,
                y: glyph.y,
                advance: 0.0,
            }),
        );
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        set_transform_if_needed(self.renderer, draw.transform, self.current_transform);
        let shape = draw.rect.to_rounded_rect(draw.radius);
        self.renderer.fill(&shape, draw.color, draw.std_dev);
    }
}

impl<R> CustomPaintSink<DisplayCommandExt> for ReplayRenderer<'_, R>
where
    R: FloemRenderer,
{
    fn custom(&mut self, command: &DisplayCommandExt) {
        let DisplayCommandExt::DrawSvg {
            svg,
            rect,
            transform,
            brush,
        } = command;
        set_transform_if_needed(self.renderer, *transform, self.current_transform);
        self.renderer.draw_svg(
            Svg {
                tree: svg.tree.as_ref(),
                hash: svg.hash.as_ref(),
            },
            *rect,
            brush.as_ref(),
        );
    }
}

pub(crate) fn replay_stage(
    stage: &ElementStage,
    renderer: &mut impl FloemRenderer,
    base_transform: Affine,
    render_size: Size,
    local_damage: Option<&[Rect]>,
) {
    let mut current_transform = None;
    let mut current_clip_stack: Vec<ClipNodeId> = Vec::new();
    // This stays wired through the replay path even though full-scene replay is still active.
    // Once the renderer/compositor can preserve undamaged content across frames, the stage can
    // switch from "replay every chunk" to "replay only intersecting chunks" without changing the
    // artifact format again.
    let chunk_indices = local_damage.map(|damage| stage.chunk_indices_for_damage(damage));

    for (index, chunk) in stage.chunks.iter().enumerate() {
        if let Some(indices) = &chunk_indices
            && !indices.contains(&index)
        {
            continue;
        }
        apply_clip_state(
            renderer,
            &stage.property_tree,
            chunk.properties.clip_id,
            base_transform,
            render_size,
            &mut current_transform,
            &mut current_clip_stack,
        );
        let mut replay = ReplayRenderer {
            renderer,
            render_size,
            current_transform: &mut current_transform,
        };
        replay_ext_transformed(&chunk.commands, &mut replay, base_transform);
    }

    apply_clip_state(
        renderer,
        &stage.property_tree,
        ClipNodeId(0),
        base_transform,
        render_size,
        &mut current_transform,
        &mut current_clip_stack,
    );
}

pub(crate) fn replay_view_clip(
    renderer: &mut impl FloemRenderer,
    clip: RoundedRect,
    base_transform: Affine,
    render_size: Size,
) {
    let final_transform = base_transform;
    renderer.set_transform(final_transform);
    let clip = constrain_infinite_rounded_rect(clip, final_transform, render_size);
    renderer.clip(&clip);
}

fn chunk_display_commands(commands: Vec<DisplayCommand>) -> (Vec<PaintChunk>, PaintPropertyTree) {
    let mut chunks = Vec::new();
    let mut properties = PaintPropertyState::default();
    let mut property_tree = PaintPropertyTree::default();
    let mut transform_intern = FxHashMap::default();
    transform_intern.insert(transform_key(Affine::IDENTITY), TransformNodeId(0));
    let mut clip_stack: Vec<ClipNodeId> = Vec::new();
    let mut effect_stack: Vec<EffectNodeId> = vec![EffectNodeId(0)];

    for command in commands {
        match command {
            DisplayCommand::PushClip { clip } => {
                let transform_id = intern_transform(
                    clip_transform(&clip),
                    &mut property_tree,
                    &mut transform_intern,
                );
                let clip_id = ClipNodeId(property_tree.clips.len() as u32);
                property_tree.clips.push(ClipNode {
                    parent: clip_stack.last().copied(),
                    transform_id,
                    clip,
                });
                clip_stack.push(clip_id);
                properties.clip_id = clip_id;
            }
            DisplayCommand::PopClip => {
                clip_stack.pop();
                properties.clip_id = clip_stack.last().copied().unwrap_or_default();
            }
            DisplayCommand::PushGroup { group } => {
                let effect_id = EffectNodeId(property_tree.effects.len() as u32);
                property_tree.effects.push(EffectNode {
                    parent: effect_stack.last().copied(),
                    blend: group.composite.blend,
                    alpha: group.composite.alpha,
                });
                effect_stack.push(effect_id);
                properties.effect_id = effect_id;
                push_boundary_chunk(
                    &mut chunks,
                    properties,
                    command_transform_class(&DisplayCommand::PushGroup {
                        group: group.clone(),
                    }),
                    DisplayCommand::PushGroup { group },
                );
            }
            DisplayCommand::PopGroup => {
                effect_stack.pop();
                properties.effect_id = effect_stack.last().copied().unwrap_or_default();
                push_boundary_chunk(
                    &mut chunks,
                    properties,
                    command_transform_class(&DisplayCommand::PopGroup),
                    DisplayCommand::PopGroup,
                );
            }
            command => {
                properties.transform_id = intern_transform(
                    command_affine(&command),
                    &mut property_tree,
                    &mut transform_intern,
                );
                let transform_class = command_transform_class(&command);
                let bounds = command_bounds(&command);
                let metadata = command_metadata(&command);
                match chunks.last_mut() {
                    Some(PaintChunk {
                        kind: PaintChunkKind::Draw,
                        properties: chunk_properties,
                        commands: chunk_commands,
                        bounds: chunk_bounds,
                        metadata: chunk_metadata,
                        transform_class: chunk_transform_class,
                    }) if *chunk_properties == properties => {
                        *chunk_transform_class = chunk_transform_class.combine(transform_class);
                        *chunk_bounds = union_rects(*chunk_bounds, bounds);
                        *chunk_metadata = chunk_metadata.merge(metadata);
                        record_scene_command(chunk_commands, command);
                    }
                    _ => chunks.push(PaintChunk {
                        kind: PaintChunkKind::Draw,
                        properties,
                        commands: replay_scene([command]),
                        bounds,
                        metadata,
                        transform_class,
                    }),
                }
            }
        }
    }

    (chunks, property_tree)
}

fn push_boundary_chunk(
    chunks: &mut Vec<PaintChunk>,
    properties: PaintPropertyState,
    transform_class: TransformClass,
    command: DisplayCommand,
) {
    chunks.push(PaintChunk {
        kind: PaintChunkKind::Boundary,
        properties,
        commands: replay_scene([command]),
        bounds: None,
        metadata: PaintChunkMetadata::default(),
        transform_class,
    });
}

fn replay_scene(
    commands: impl IntoIterator<Item = DisplayCommand>,
) -> ExtendedScene<DisplayCommandExt> {
    let mut scene = ExtendedScene::new();
    for command in commands {
        record_scene_command(&mut scene, command);
    }
    scene
}

fn record_scene_command(scene: &mut ExtendedScene<DisplayCommandExt>, command: DisplayCommand) {
    match command {
        DisplayCommand::PushClip { clip } => {
            let _ = scene.push_clip(clip);
        }
        DisplayCommand::PopClip => scene.pop_clip(),
        DisplayCommand::PushGroup { group } => {
            let _ = scene.push_group(group);
        }
        DisplayCommand::PopGroup => scene.pop_group(),
        DisplayCommand::Draw { draw } => {
            let _ = scene.draw(draw);
        }
        DisplayCommand::DrawSvg {
            svg,
            rect,
            transform,
            brush,
        } => {
            let _ = scene.custom_command(DisplayCommandExt::DrawSvg {
                svg,
                rect,
                transform,
                brush,
            });
        }
    }
}

fn command_transform_class(command: &DisplayCommand) -> TransformClass {
    match command {
        DisplayCommand::PushClip { .. }
        | DisplayCommand::PopClip
        | DisplayCommand::PushGroup { .. }
        | DisplayCommand::PopGroup => TransformClass::Affine,
        DisplayCommand::Draw { draw } => match draw {
            Draw::Fill { .. } | Draw::Stroke { .. } => TransformClass::Affine,
            Draw::GlyphRun(_) | Draw::BlurredRoundedRect(_) => TransformClass::TranslateOnly,
        },
        DisplayCommand::DrawSvg { .. } => TransformClass::TranslateOnly,
    }
}

fn command_bounds(command: &DisplayCommand) -> Option<Rect> {
    match command {
        DisplayCommand::PushClip { .. }
        | DisplayCommand::PopClip
        | DisplayCommand::PushGroup { .. }
        | DisplayCommand::PopGroup => None,
        DisplayCommand::Draw { draw, .. } => draw_bounds(draw),
        DisplayCommand::DrawSvg { rect, .. } => Some(*rect),
    }
}

fn draw_bounds(draw: &Draw) -> Option<Rect> {
    match draw {
        Draw::Fill { shape, .. } => Some(geometry_bounds(shape)),
        Draw::Stroke { shape, stroke, .. } => {
            let bounds = geometry_bounds(shape);
            let inset = stroke.width / 2.0;
            Some(bounds.inflate(inset, inset))
        }
        Draw::GlyphRun(run) => glyph_run_bounds(run),
        Draw::BlurredRoundedRect(rect) => {
            Some(rect.rect.inflate(rect.std_dev * 3.0, rect.std_dev * 3.0))
        }
    }
}

fn geometry_bounds(geometry: &Geometry) -> Rect {
    match geometry {
        Geometry::Rect(rect) => *rect,
        Geometry::RoundedRect(rect) => rect.rect(),
        Geometry::Path(path) => path.bounding_box(),
    }
}

fn glyph_run_bounds(run: &GlyphRun) -> Option<Rect> {
    let mut glyphs = run.glyphs.iter();
    let first = glyphs.next()?;
    let mut rect = Rect::new(
        first.x as f64,
        (first.y - run.font_size) as f64,
        (first.x + run.font_size) as f64,
        first.y as f64,
    );
    for glyph in glyphs {
        rect = rect.union(Rect::new(
            glyph.x as f64,
            (glyph.y - run.font_size) as f64,
            (glyph.x + run.font_size) as f64,
            glyph.y as f64,
        ));
    }
    Some(rect)
}

fn command_metadata(command: &DisplayCommand) -> PaintChunkMetadata {
    match command {
        DisplayCommand::PushClip { .. } | DisplayCommand::PopClip | DisplayCommand::PopGroup => {
            PaintChunkMetadata::default()
        }
        DisplayCommand::PushGroup { .. } => PaintChunkMetadata {
            requires_layer: true,
            ..PaintChunkMetadata::default()
        },
        DisplayCommand::Draw { draw, .. } => match draw {
            Draw::Fill { .. } | Draw::Stroke { .. } => PaintChunkMetadata::default(),
            Draw::GlyphRun(_) => PaintChunkMetadata {
                has_text: true,
                ..PaintChunkMetadata::default()
            },
            Draw::BlurredRoundedRect(_) => PaintChunkMetadata {
                has_blur: true,
                ..PaintChunkMetadata::default()
            },
        },
        DisplayCommand::DrawSvg { .. } => PaintChunkMetadata {
            has_vector_image: true,
            ..PaintChunkMetadata::default()
        },
    }
}

fn union_rects(lhs: Option<Rect>, rhs: Option<Rect>) -> Option<Rect> {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => Some(lhs.union(rhs)),
        (Some(lhs), None) => Some(lhs),
        (None, Some(rhs)) => Some(rhs),
        (None, None) => None,
    }
}

fn bounds_intersection(lhs: Option<Rect>, rhs: Option<Rect>) -> Option<Rect> {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => {
            let intersection = lhs.intersect(rhs);
            (intersection.area() > 0.0).then_some(intersection)
        }
        (Some(lhs), None) => Some(lhs),
        (None, Some(rhs)) => Some(rhs),
        (None, None) => None,
    }
}

fn clip_bounds_for_id(property_tree: &PaintPropertyTree, clip_id: ClipNodeId) -> Option<Rect> {
    if clip_id == ClipNodeId(0) {
        return None;
    }

    let mut current = Some(clip_id);
    let mut bounds = None;
    while let Some(id) = current {
        if id == ClipNodeId(0) {
            break;
        }
        let node = property_tree.clips.get(id.0 as usize)?;
        bounds = bounds_intersection(bounds, clip_node_bounds(node));
        current = node.parent;
    }
    bounds
}

fn clip_node_bounds(node: &ClipNode) -> Option<Rect> {
    match &node.clip {
        Clip::Fill { shape, .. } | Clip::Stroke { shape, .. } => Some(geometry_bounds(shape)),
    }
}

fn command_affine(command: &DisplayCommand) -> Affine {
    match command {
        DisplayCommand::PopClip | DisplayCommand::PopGroup => Affine::IDENTITY,
        DisplayCommand::PushClip { clip } => clip_transform(clip),
        DisplayCommand::PushGroup { .. } => Affine::IDENTITY,
        DisplayCommand::Draw { draw, .. } => match draw {
            Draw::Fill { transform, .. } | Draw::Stroke { transform, .. } => *transform,
            Draw::GlyphRun(run) => run.transform,
            Draw::BlurredRoundedRect(rect) => rect.transform,
        },
        DisplayCommand::DrawSvg { transform, .. } => *transform,
    }
}

fn clip_transform(clip: &Clip) -> Affine {
    match clip {
        Clip::Fill { transform, .. } => *transform,
        Clip::Stroke { transform, .. } => *transform,
    }
}

fn intern_transform(
    transform: Affine,
    property_tree: &mut PaintPropertyTree,
    intern: &mut FxHashMap<[u64; 6], TransformNodeId>,
) -> TransformNodeId {
    let key = transform_key(transform);
    if let Some(id) = intern.get(&key).copied() {
        return id;
    }

    let id = TransformNodeId(property_tree.transforms.len() as u32);
    property_tree.transforms.push(TransformNode {
        parent: Some(TransformNodeId(0)),
        transform,
    });
    intern.insert(key, id);
    id
}

fn transform_key(transform: Affine) -> [u64; 6] {
    transform.as_coeffs().map(f64::to_bits)
}

fn replay_clip_node(
    renderer: &mut impl FloemRenderer,
    clip_node: &ClipNode,
    property_tree: &PaintPropertyTree,
    base_transform: Affine,
    render_size: Size,
    current_transform: &mut Option<Affine>,
) {
    let Clip::Fill { shape, .. } = &clip_node.clip else {
        return;
    };
    let Some(transform) = property_tree
        .transforms
        .get(clip_node.transform_id.0 as usize)
        .map(|node| node.transform)
    else {
        return;
    };
    let final_transform = base_transform * transform;
    set_transform_if_needed(renderer, final_transform, current_transform);
    match sanitize_clip_geometry(shape, final_transform, render_size) {
        Geometry::Rect(rect) => renderer.clip(&rect),
        Geometry::RoundedRect(rect) => renderer.clip(&rect),
        Geometry::Path(path) => renderer.clip(&path),
    }
}

fn apply_clip_state(
    renderer: &mut impl FloemRenderer,
    property_tree: &PaintPropertyTree,
    target_clip_id: ClipNodeId,
    base_transform: Affine,
    render_size: Size,
    current_transform: &mut Option<Affine>,
    current_clip_stack: &mut Vec<ClipNodeId>,
) {
    // Stage-local clips are now driven by property ids instead of recorded Push/Pop commands.
    // Replay diffs the active clip chain against the target chunk state and mutates the renderer
    // clip stack to match.
    let target_stack = clip_chain(property_tree, target_clip_id);
    let shared_prefix = current_clip_stack
        .iter()
        .zip(target_stack.iter())
        .take_while(|(lhs, rhs)| lhs == rhs)
        .count();

    for _ in shared_prefix..current_clip_stack.len() {
        renderer.clear_clip();
    }
    current_clip_stack.truncate(shared_prefix);

    for clip_id in target_stack.into_iter().skip(shared_prefix) {
        let Some(node) = property_tree.clips.get(clip_id.0 as usize) else {
            continue;
        };
        replay_clip_node(
            renderer,
            node,
            property_tree,
            base_transform,
            render_size,
            current_transform,
        );
        current_clip_stack.push(clip_id);
    }
}

fn clip_chain(property_tree: &PaintPropertyTree, clip_id: ClipNodeId) -> Vec<ClipNodeId> {
    let mut chain = Vec::new();
    let mut current = Some(clip_id);

    while let Some(id) = current {
        if id == ClipNodeId(0) {
            break;
        }
        chain.push(id);
        current = property_tree
            .clips
            .get(id.0 as usize)
            .and_then(|node| node.parent);
    }

    chain.reverse();
    chain
}

fn set_transform_if_needed(
    renderer: &mut impl FloemRenderer,
    transform: Affine,
    current_transform: &mut Option<Affine>,
) {
    if current_transform != &Some(transform) {
        renderer.set_transform(transform);
        *current_transform = Some(transform);
    }
}

fn sanitize_clip_geometry(shape: &Geometry, transform: Affine, render_size: Size) -> Geometry {
    match shape {
        Geometry::Rect(rect) => {
            Geometry::Rect(constrain_infinite_rect(*rect, transform, render_size))
        }
        Geometry::RoundedRect(rect) => Geometry::RoundedRect(constrain_infinite_rounded_rect(
            *rect,
            transform,
            render_size,
        )),
        Geometry::Path(path) => Geometry::Path(path.clone()),
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
    use peniko::Color;

    #[test]
    fn stage_groups_adjacent_draws_with_matching_properties() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut stage = ElementStage::default();
        stage.set_commands(
            vec![
                DisplayCommand::Draw {
                    draw: Draw::Fill {
                        transform: Affine::IDENTITY,
                        fill_rule: Fill::NonZero,
                        brush: Color::BLACK.into(),
                        brush_transform: None,
                        shape: Geometry::Rect(rect),
                        composite: Composite::default(),
                    },
                },
                DisplayCommand::Draw {
                    draw: Draw::Stroke {
                        transform: Affine::IDENTITY,
                        stroke: peniko::kurbo::Stroke::new(1.0),
                        brush: Color::BLACK.into(),
                        brush_transform: None,
                        shape: Geometry::Rect(rect),
                        composite: Composite::default(),
                    },
                },
            ],
            None,
        );

        assert_eq!(stage.chunks.len(), 1);
        assert_eq!(stage.chunks[0].kind, PaintChunkKind::Draw);
        assert_eq!(stage.chunks[0].properties.z_index, 0);
        assert_eq!(stage.chunks[0].commands.commands().len(), 2);
        assert_eq!(stage.transform_class, TransformClass::Affine);
    }

    #[test]
    fn stage_tracks_clip_state_without_boundary_chunks() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut stage = ElementStage::default();
        stage.set_commands(
            vec![
                DisplayCommand::PushClip {
                    clip: Clip::Fill {
                        transform: Affine::IDENTITY,
                        shape: Geometry::Rect(rect),
                        fill_rule: Fill::NonZero,
                    },
                },
                DisplayCommand::Draw {
                    draw: Draw::Fill {
                        transform: Affine::IDENTITY,
                        fill_rule: Fill::NonZero,
                        brush: Color::BLACK.into(),
                        brush_transform: None,
                        shape: Geometry::Rect(rect),
                        composite: Composite::default(),
                    },
                },
                DisplayCommand::PopClip,
            ],
            None,
        );

        assert_eq!(stage.chunks.len(), 1);
        assert_eq!(stage.chunks[0].kind, PaintChunkKind::Draw);
        assert_ne!(stage.chunks[0].properties.clip_id, ClipNodeId(0));
        assert_eq!(stage.property_tree.clips.len(), 2);
    }

    #[test]
    fn stage_splits_draw_chunks_on_transform_state() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut stage = ElementStage::default();
        stage.set_commands(
            vec![
                DisplayCommand::Draw {
                    draw: Draw::Fill {
                        transform: Affine::IDENTITY,
                        fill_rule: Fill::NonZero,
                        brush: Color::BLACK.into(),
                        brush_transform: None,
                        shape: Geometry::Rect(rect),
                        composite: Composite::default(),
                    },
                },
                DisplayCommand::Draw {
                    draw: Draw::Fill {
                        transform: Affine::translate((5.0, 0.0)),
                        fill_rule: Fill::NonZero,
                        brush: Color::BLACK.into(),
                        brush_transform: None,
                        shape: Geometry::Rect(rect),
                        composite: Composite::default(),
                    },
                },
            ],
            None,
        );

        assert_eq!(stage.chunks.len(), 2);
        assert_ne!(
            stage.chunks[0].properties.transform_id,
            stage.chunks[1].properties.transform_id
        );
        assert_eq!(stage.property_tree.transforms.len(), 2);
    }

    #[test]
    fn blurred_draws_downgrade_stage_transform_retention() {
        let mut stage = ElementStage::default();
        stage.set_commands(
            vec![DisplayCommand::Draw {
                draw: Draw::BlurredRoundedRect(imaging::BlurredRoundedRect {
                    transform: Affine::IDENTITY,
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    color: Color::BLACK,
                    radius: 4.0,
                    std_dev: 6.0,
                    composite: Composite::default(),
                }),
            }],
            None,
        );

        assert_eq!(stage.transform_class, TransformClass::TranslateOnly);
        assert!(stage.chunks[0].metadata.has_blur);
        assert!(stage.chunks[0].bounds.is_some());
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
