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

use floem_renderer::text::{Glyph, GlyphRunProps};
use floem_renderer::{Img, Renderer as FloemRenderer, Svg, usvg};
use imaging::{
    Clip, Composite, Draw, FillRule, Geometry, Glyph as SceneGlyph, GlyphRun, Group, StrokeStyle,
};
use peniko::BrushRef;
use peniko::kurbo::{Affine, BezPath, Circle, Line, Point, Rect, RoundedRect, Shape, Size, Stroke};
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

#[derive(Clone, Debug)]
pub(crate) enum ShapeHint {
    Rect(Rect),
    RoundedRect(RoundedRect),
    Line(Line),
    Circle(Circle),
}

#[derive(Clone)]
pub(crate) struct OwnedSvg {
    pub tree: Arc<usvg::Tree>,
    pub hash: Arc<[u8]>,
}

#[derive(Clone)]
pub(crate) enum DisplayCommand {
    SetZIndex(i32),
    PushClip {
        clip: Clip,
        hint: Option<ShapeHint>,
    },
    PopClip,
    PushLayer {
        group: Group,
        transform: Affine,
        clip_hint: Option<ShapeHint>,
    },
    PopLayer,
    Draw {
        draw: Draw,
        hint: Option<ShapeHint>,
    },
    DrawImage {
        img: peniko::ImageBrush,
        hash: Arc<[u8]>,
        rect: Rect,
        transform: Affine,
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
}

impl Default for ElementStage {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            property_tree: PaintPropertyTree::default(),
            transform_class: TransformClass::Affine,
        }
    }
}

impl ElementStage {
    pub(crate) fn set_commands(&mut self, commands: Vec<DisplayCommand>) {
        let (chunks, property_tree) = chunk_display_commands(commands);
        self.chunks = chunks;
        self.property_tree = property_tree;
        self.transform_class = if self.chunks.is_empty() {
            TransformClass::Affine
        } else {
            self.chunks
                .iter()
                .map(|chunk| chunk.transform_class)
                .fold(TransformClass::Exact, TransformClass::combine)
        };
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
    pub hint: Option<ShapeHint>,
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
                    fill_rule: FillRule::NonZero,
                },
                hint: Some(ShapeHint::Rect(Rect::ZERO)),
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
    pub commands: Vec<DisplayCommand>,
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
}

impl PaintChunkMetadata {
    fn merge(self, other: Self) -> Self {
        Self {
            has_text: self.has_text || other.has_text,
            has_raster_image: self.has_raster_image || other.has_raster_image,
            has_vector_image: self.has_vector_image || other.has_vector_image,
            has_blur: self.has_blur || other.has_blur,
            requires_layer: self.requires_layer || other.requires_layer,
        }
    }
}

#[derive(Clone, Copy)]
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
        damage.iter().any(|rect| rect.intersect(bounds).area() > 0.0)
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

    pub(crate) fn needs_rerecord(&self, id: ElementId, snapshot: ElementSnapshot) -> bool {
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
}

pub(crate) struct RecordingRenderer<'a> {
    commands: &'a mut Vec<DisplayCommand>,
    transform: Affine,
}

impl<'a> RecordingRenderer<'a> {
    pub fn new(commands: &'a mut Vec<DisplayCommand>) -> Self {
        Self {
            commands,
            transform: Affine::IDENTITY,
        }
    }

    fn record_draw(&mut self, draw: Draw, hint: Option<ShapeHint>) {
        self.commands.push(DisplayCommand::Draw { draw, hint });
    }
}

impl RecordingRenderer<'_> {
    pub fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    pub fn set_z_index(&mut self, z_index: i32) {
        self.commands.push(DisplayCommand::SetZIndex(z_index));
    }

    pub fn clip(&mut self, shape: &impl Shape) {
        let (shape, hint) = shape_to_geometry(shape);
        self.commands.push(DisplayCommand::PushClip {
            clip: Clip::Fill {
                transform: self.transform,
                shape,
                fill_rule: FillRule::NonZero,
            },
            hint,
        });
    }

    pub fn clear_clip(&mut self) {
        self.commands.push(DisplayCommand::PopClip);
    }

    pub fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s Stroke,
    ) {
        let (shape, hint) = shape_to_geometry(shape);
        self.record_draw(
            Draw::Stroke {
                transform: self.transform,
                stroke: StrokeStyle::clone(stroke),
                paint: brush.into().to_owned(),
                paint_transform: None,
                shape,
                composite: Composite::default(),
            },
            hint,
        );
    }

    pub fn fill<'b>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        blur_radius: f64,
    ) {
        let brush = brush.into();
        if blur_radius > 0.0
            && let BrushRef::Solid(color) = brush
            && let Some((rect, radius)) = blurred_rounded_rect(shape)
        {
            self.record_draw(
                Draw::BlurredRoundedRect(imaging::BlurredRoundedRect {
                    transform: self.transform,
                    rect,
                    color,
                    radius,
                    std_dev: blur_radius,
                    composite: Composite::default(),
                }),
                shape_hint(shape),
            );
            return;
        }

        let (shape, hint) = shape_to_geometry(shape);
        self.record_draw(
            Draw::Fill {
                transform: self.transform,
                fill_rule: FillRule::NonZero,
                paint: brush.to_owned(),
                paint_transform: None,
                shape,
                composite: Composite::default(),
            },
            hint,
        );
    }

    pub fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        let (shape, hint) = shape_to_geometry(clip);
        self.commands.push(DisplayCommand::PushLayer {
            group: Group {
                clip: Some(Clip::Fill {
                    transform: self.transform,
                    shape,
                    fill_rule: FillRule::NonZero,
                }),
                filters: Vec::new(),
                composite: Composite::new(blend.into(), alpha),
            },
            transform,
            clip_hint: hint,
        });
    }

    pub fn pop_layer(&mut self) {
        self.commands.push(DisplayCommand::PopLayer);
    }

    pub fn draw_glyphs<'a>(
        &mut self,
        origin: Point,
        props: &GlyphRunProps<'a>,
        glyphs: impl Iterator<Item = Glyph> + 'a,
    ) {
        let glyph_run = GlyphRun {
            font: props.font.clone(),
            transform: self.transform * Affine::translate((origin.x, origin.y)) * props.transform,
            glyph_transform: props.glyph_transform,
            font_size: props.font_size,
            hint: props.hint,
            normalized_coords: props.normalized_coords.to_vec(),
            style: props.style.to_owned(),
            glyphs: glyphs
                .map(|glyph| SceneGlyph {
                    id: glyph.id,
                    x: glyph.x,
                    y: glyph.y,
                })
                .collect(),
            paint: props.brush.to_owned(),
            composite: Composite::new(peniko::BlendMode::default(), props.brush_alpha),
        };
        self.record_draw(Draw::GlyphRun(glyph_run), None);
    }

    pub fn draw_svg<'b>(
        &mut self,
        svg: Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        self.commands.push(DisplayCommand::DrawSvg {
            svg: OwnedSvg {
                tree: Arc::new(svg.tree.clone()),
                hash: Arc::from(svg.hash.to_vec()),
            },
            rect,
            transform: self.transform,
            brush: brush.map(|brush| brush.into().to_owned()),
        });
    }

    pub fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        self.commands.push(DisplayCommand::DrawImage {
            img: img.img,
            hash: Arc::from(img.hash.to_vec()),
            rect,
            transform: self.transform,
        });
    }
}

pub(crate) fn replay_stage(
    stage: &ElementStage,
    renderer: &mut impl FloemRenderer,
    base_transform: Affine,
    render_size: Size,
    local_damage: Option<&[Rect]>,
) {
    let mut current_z_index = None;
    let mut current_transform = None;
    let mut current_clip_stack: Vec<ClipNodeId> = Vec::new();
    // This stays wired through the replay path even though full-scene replay is still active.
    // Once the renderer/compositor can preserve undamaged content across frames, the stage can
    // switch from "replay every chunk" to "replay only intersecting chunks" without changing the
    // artifact format again.
    let chunk_indices = local_damage.map(|damage| stage.chunk_indices_for_damage(damage));

    for (index, chunk) in stage.chunks.iter().enumerate() {
        if let Some(indices) = &chunk_indices && !indices.contains(&index) {
            continue;
        }
        if current_z_index != Some(chunk.properties.z_index) {
            renderer.set_z_index(chunk.properties.z_index);
            current_z_index = Some(chunk.properties.z_index);
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
        for command in &chunk.commands {
            match command {
                DisplayCommand::SetZIndex(_) => {}
                DisplayCommand::PushClip { .. } | DisplayCommand::PopClip => {}
                DisplayCommand::PushLayer {
                    group,
                    transform,
                    clip_hint,
                } => {
                    let Some(Clip::Fill {
                        transform: clip_transform,
                        shape,
                        ..
                    }) = group.clip.as_ref()
                    else {
                        continue;
                    };
                    set_transform_if_needed(
                        renderer,
                        base_transform * *clip_transform,
                        &mut current_transform,
                    );
                    match (clip_hint.clone(), shape) {
                        (Some(ShapeHint::Rect(rect)), _) => renderer.push_layer(
                            group.composite.blend,
                            group.composite.alpha,
                            *transform,
                            &rect,
                        ),
                        (Some(ShapeHint::RoundedRect(rect)), _) => renderer.push_layer(
                            group.composite.blend,
                            group.composite.alpha,
                            *transform,
                            &rect,
                        ),
                        (Some(ShapeHint::Line(line)), _) => renderer.push_layer(
                            group.composite.blend,
                            group.composite.alpha,
                            *transform,
                            &line,
                        ),
                        (Some(ShapeHint::Circle(circle)), _) => renderer.push_layer(
                            group.composite.blend,
                            group.composite.alpha,
                            *transform,
                            &circle,
                        ),
                        (None, Geometry::Rect(rect)) => renderer.push_layer(
                            group.composite.blend,
                            group.composite.alpha,
                            *transform,
                            rect,
                        ),
                        (None, Geometry::RoundedRect(rect)) => renderer.push_layer(
                            group.composite.blend,
                            group.composite.alpha,
                            *transform,
                            rect,
                        ),
                        (None, Geometry::Path(path)) => renderer.push_layer(
                            group.composite.blend,
                            group.composite.alpha,
                            *transform,
                            path,
                        ),
                    }
                }
                DisplayCommand::PopLayer => renderer.pop_layer(),
                DisplayCommand::Draw { draw, hint } => {
                    replay_draw(
                        renderer,
                        draw,
                        hint.clone(),
                        base_transform,
                        render_size,
                        &mut current_transform,
                    )
                }
                DisplayCommand::DrawImage {
                    img,
                    hash,
                    rect,
                    transform,
                } => {
                    set_transform_if_needed(
                        renderer,
                        base_transform * *transform,
                        &mut current_transform,
                    );
                    renderer.draw_img(
                        Img {
                            img: img.clone(),
                            hash,
                        },
                        *rect,
                    );
                }
                DisplayCommand::DrawSvg {
                    svg,
                    rect,
                    transform,
                    brush,
                } => {
                    set_transform_if_needed(
                        renderer,
                        base_transform * *transform,
                        &mut current_transform,
                    );
                    renderer.draw_svg(
                        Svg {
                            tree: svg.tree.as_ref(),
                            hash: svg.hash.as_ref(),
                        },
                        *rect,
                        brush.as_ref(),
                    );
                }
            }
        }
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
            DisplayCommand::SetZIndex(z_index) => {
                properties.z_index = z_index;
            }
            DisplayCommand::PushClip { clip, hint } => {
                let transform_id = intern_transform(clip_transform(&clip), &mut property_tree, &mut transform_intern);
                let clip_id = ClipNodeId(property_tree.clips.len() as u32);
                property_tree.clips.push(ClipNode {
                    parent: clip_stack.last().copied(),
                    transform_id,
                    clip,
                    hint: hint.clone(),
                });
                clip_stack.push(clip_id);
                properties.clip_id = clip_id;
            }
            DisplayCommand::PopClip => {
                clip_stack.pop();
                properties.clip_id = clip_stack.last().copied().unwrap_or_default();
            }
            DisplayCommand::PushLayer {
                group,
                transform,
                clip_hint,
            } => {
                let effect_id = EffectNodeId(property_tree.effects.len() as u32);
                property_tree.effects.push(EffectNode {
                    parent: effect_stack.last().copied(),
                    blend: group.composite.blend,
                    alpha: group.composite.alpha,
                });
                effect_stack.push(effect_id);
                properties.effect_id = effect_id;
                properties.transform_id =
                    intern_transform(transform, &mut property_tree, &mut transform_intern);
                push_boundary_chunk(
                    &mut chunks,
                    properties,
                    command_transform_class(&DisplayCommand::PushLayer {
                        group: group.clone(),
                        transform,
                        clip_hint: clip_hint.clone(),
                    }),
                    DisplayCommand::PushLayer {
                        group,
                        transform,
                        clip_hint,
                    },
                );
            }
            DisplayCommand::PopLayer => {
                effect_stack.pop();
                properties.effect_id = effect_stack.last().copied().unwrap_or_default();
                push_boundary_chunk(
                    &mut chunks,
                    properties,
                    command_transform_class(&DisplayCommand::PopLayer),
                    DisplayCommand::PopLayer,
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
                        *chunk_transform_class =
                            chunk_transform_class.combine(transform_class);
                        *chunk_bounds = union_rects(*chunk_bounds, bounds);
                        *chunk_metadata = chunk_metadata.merge(metadata);
                        chunk_commands.push(command);
                    }
                    _ => chunks.push(PaintChunk {
                        kind: PaintChunkKind::Draw,
                        properties,
                        commands: vec![command],
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
        commands: vec![command],
        bounds: None,
        metadata: PaintChunkMetadata::default(),
        transform_class,
    });
}

fn command_transform_class(command: &DisplayCommand) -> TransformClass {
    match command {
        DisplayCommand::SetZIndex(_)
        | DisplayCommand::PushClip { .. }
        | DisplayCommand::PopClip
        | DisplayCommand::PushLayer { .. }
        | DisplayCommand::PopLayer => TransformClass::Affine,
        DisplayCommand::Draw { draw, .. } => match draw {
            Draw::Fill { .. } | Draw::Stroke { .. } => TransformClass::Affine,
            Draw::GlyphRun(_) | Draw::BlurredRoundedRect(_) => TransformClass::TranslateOnly,
        },
        DisplayCommand::DrawImage { .. } | DisplayCommand::DrawSvg { .. } => {
            TransformClass::TranslateOnly
        }
    }
}

fn command_bounds(command: &DisplayCommand) -> Option<Rect> {
    match command {
        DisplayCommand::SetZIndex(_)
        | DisplayCommand::PushClip { .. }
        | DisplayCommand::PopClip
        | DisplayCommand::PushLayer { .. }
        | DisplayCommand::PopLayer => None,
        DisplayCommand::Draw { draw, .. } => draw_bounds(draw),
        DisplayCommand::DrawImage { rect, .. } | DisplayCommand::DrawSvg { rect, .. } => Some(*rect),
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
        Draw::BlurredRoundedRect(rect) => Some(rect.rect.inflate(rect.std_dev * 3.0, rect.std_dev * 3.0)),
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
        DisplayCommand::SetZIndex(_)
        | DisplayCommand::PushClip { .. }
        | DisplayCommand::PopClip
        | DisplayCommand::PopLayer => PaintChunkMetadata::default(),
        DisplayCommand::PushLayer { .. } => PaintChunkMetadata {
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
        DisplayCommand::DrawImage { .. } => PaintChunkMetadata {
            has_raster_image: true,
            ..PaintChunkMetadata::default()
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
    match (&node.hint, &node.clip) {
        (Some(ShapeHint::Rect(rect)), _) => Some(*rect),
        (Some(ShapeHint::RoundedRect(rect)), _) => Some(rect.rect()),
        (Some(ShapeHint::Circle(circle)), _) => Some(circle.bounding_box()),
        (Some(ShapeHint::Line(line)), _) => Some(line.bounding_box()),
        (None, Clip::Fill { shape, .. }) => Some(geometry_bounds(shape)),
        (None, Clip::Stroke { shape, .. }) => Some(geometry_bounds(shape)),
    }
}

fn command_affine(command: &DisplayCommand) -> Affine {
    match command {
        DisplayCommand::SetZIndex(_) | DisplayCommand::PopClip | DisplayCommand::PopLayer => {
            Affine::IDENTITY
        }
        DisplayCommand::PushClip { clip, .. } => clip_transform(clip),
        DisplayCommand::PushLayer { transform, .. } => *transform,
        DisplayCommand::Draw { draw, .. } => match draw {
            Draw::Fill { transform, .. } | Draw::Stroke { transform, .. } => *transform,
            Draw::GlyphRun(run) => run.transform,
            Draw::BlurredRoundedRect(rect) => rect.transform,
        },
        DisplayCommand::DrawImage { transform, .. } | DisplayCommand::DrawSvg { transform, .. } => {
            *transform
        }
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
    match (
        sanitize_clip_hint(clip_node.hint.clone(), final_transform, render_size),
        shape,
    ) {
        (Some(ShapeHint::Rect(rect)), _) => renderer.clip(&rect),
        (Some(ShapeHint::RoundedRect(rect)), _) => renderer.clip(&rect),
        (Some(ShapeHint::Line(line)), _) => renderer.clip(&line),
        (Some(ShapeHint::Circle(circle)), _) => renderer.clip(&circle),
        (None, Geometry::Rect(rect)) => renderer.clip(rect),
        (None, Geometry::RoundedRect(rect)) => renderer.clip(rect),
        (None, Geometry::Path(path)) => renderer.clip(path),
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

fn replay_draw(
    renderer: &mut impl FloemRenderer,
    draw: &Draw,
    hint: Option<ShapeHint>,
    base_transform: Affine,
    _render_size: Size,
    current_transform: &mut Option<Affine>,
) {
    match draw {
        Draw::Fill {
            transform,
            paint,
            shape,
            ..
        } => {
            set_transform_if_needed(renderer, base_transform * *transform, current_transform);
            match (hint, shape) {
                (Some(ShapeHint::Rect(rect)), _) => renderer.fill(&rect, paint, 0.0),
                (Some(ShapeHint::RoundedRect(rect)), _) => renderer.fill(&rect, paint, 0.0),
                (Some(ShapeHint::Line(line)), _) => renderer.fill(&line, paint, 0.0),
                (Some(ShapeHint::Circle(circle)), _) => renderer.fill(&circle, paint, 0.0),
                (None, Geometry::Rect(rect)) => renderer.fill(rect, paint, 0.0),
                (None, Geometry::RoundedRect(rect)) => renderer.fill(rect, paint, 0.0),
                (None, Geometry::Path(path)) => renderer.fill(path, paint, 0.0),
            }
        }
        Draw::Stroke {
            transform,
            stroke,
            paint,
            shape,
            ..
        } => {
            set_transform_if_needed(renderer, base_transform * *transform, current_transform);
            match (hint, shape) {
                (Some(ShapeHint::Rect(rect)), _) => renderer.stroke(&rect, paint, stroke),
                (Some(ShapeHint::RoundedRect(rect)), _) => renderer.stroke(&rect, paint, stroke),
                (Some(ShapeHint::Line(line)), _) => renderer.stroke(&line, paint, stroke),
                (Some(ShapeHint::Circle(circle)), _) => renderer.stroke(&circle, paint, stroke),
                (None, Geometry::Rect(rect)) => renderer.stroke(rect, paint, stroke),
                (None, Geometry::RoundedRect(rect)) => renderer.stroke(rect, paint, stroke),
                (None, Geometry::Path(path)) => renderer.stroke(path, paint, stroke),
            }
        }
        Draw::GlyphRun(run) => {
            let props = GlyphRunProps {
                font: run.font.clone(),
                font_size: run.font_size,
                hint: run.hint,
                normalized_coords: &run.normalized_coords,
                style: (&run.style).into(),
                brush: (&run.paint).into(),
                brush_alpha: run.composite.alpha,
                transform: run.transform,
                glyph_transform: run.glyph_transform,
            };
            set_transform_if_needed(renderer, base_transform, current_transform);
            renderer.draw_glyphs(
                Point::ZERO,
                &props,
                run.glyphs.iter().map(|glyph| Glyph {
                    id: glyph.id,
                    style_index: 0,
                    x: glyph.x,
                    y: glyph.y,
                    advance: 0.0,
                }),
            );
        }
        Draw::BlurredRoundedRect(rect) => {
            set_transform_if_needed(renderer, base_transform * rect.transform, current_transform);
            let shape = rect.rect.to_rounded_rect(rect.radius);
            renderer.fill(&shape, rect.color, rect.std_dev);
        }
    }
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

fn sanitize_clip_hint(
    hint: Option<ShapeHint>,
    transform: Affine,
    render_size: Size,
) -> Option<ShapeHint> {
    match hint {
        Some(ShapeHint::Rect(rect)) => Some(ShapeHint::Rect(constrain_infinite_rect(
            rect,
            transform,
            render_size,
        ))),
        Some(ShapeHint::RoundedRect(rect)) => Some(ShapeHint::RoundedRect(
            constrain_infinite_rounded_rect(rect, transform, render_size),
        )),
        other => other,
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

fn constrain_infinite_rect(rect: Rect, transform: Affine, render_size: Size) -> Rect {
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

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use peniko::Color;

    #[test]
    fn stage_groups_adjacent_draws_with_matching_properties() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut stage = ElementStage::default();
        stage.set_commands(vec![
            DisplayCommand::SetZIndex(7),
            DisplayCommand::Draw {
                draw: Draw::Fill {
                    transform: Affine::IDENTITY,
                    fill_rule: FillRule::NonZero,
                    paint: Color::BLACK.into(),
                    paint_transform: None,
                    shape: Geometry::Rect(rect),
                    composite: Composite::default(),
                },
                hint: Some(ShapeHint::Rect(rect)),
            },
            DisplayCommand::Draw {
                draw: Draw::Stroke {
                    transform: Affine::IDENTITY,
                    stroke: StrokeStyle::new(1.0),
                    paint: Color::BLACK.into(),
                    paint_transform: None,
                    shape: Geometry::Rect(rect),
                    composite: Composite::default(),
                },
                hint: Some(ShapeHint::Rect(rect)),
            },
        ]);

        assert_eq!(stage.chunks.len(), 1);
        assert_eq!(stage.chunks[0].kind, PaintChunkKind::Draw);
        assert_eq!(stage.chunks[0].properties.z_index, 7);
        assert_eq!(stage.chunks[0].commands.len(), 2);
        assert_eq!(stage.transform_class, TransformClass::Affine);
    }

    #[test]
    fn stage_tracks_clip_state_without_boundary_chunks() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut stage = ElementStage::default();
        stage.set_commands(vec![
            DisplayCommand::PushClip {
                clip: Clip::Fill {
                    transform: Affine::IDENTITY,
                    shape: Geometry::Rect(rect),
                    fill_rule: FillRule::NonZero,
                },
                hint: Some(ShapeHint::Rect(rect)),
            },
            DisplayCommand::Draw {
                draw: Draw::Fill {
                    transform: Affine::IDENTITY,
                    fill_rule: FillRule::NonZero,
                    paint: Color::BLACK.into(),
                    paint_transform: None,
                    shape: Geometry::Rect(rect),
                    composite: Composite::default(),
                },
                hint: Some(ShapeHint::Rect(rect)),
            },
            DisplayCommand::PopClip,
        ]);

        assert_eq!(stage.chunks.len(), 1);
        assert_eq!(stage.chunks[0].kind, PaintChunkKind::Draw);
        assert_ne!(stage.chunks[0].properties.clip_id, ClipNodeId(0));
        assert_eq!(stage.property_tree.clips.len(), 2);
    }

    #[test]
    fn stage_splits_draw_chunks_on_transform_state() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut stage = ElementStage::default();
        stage.set_commands(vec![
            DisplayCommand::Draw {
                draw: Draw::Fill {
                    transform: Affine::IDENTITY,
                    fill_rule: FillRule::NonZero,
                    paint: Color::BLACK.into(),
                    paint_transform: None,
                    shape: Geometry::Rect(rect),
                    composite: Composite::default(),
                },
                hint: Some(ShapeHint::Rect(rect)),
            },
            DisplayCommand::Draw {
                draw: Draw::Fill {
                    transform: Affine::translate((5.0, 0.0)),
                    fill_rule: FillRule::NonZero,
                    paint: Color::BLACK.into(),
                    paint_transform: None,
                    shape: Geometry::Rect(rect),
                    composite: Composite::default(),
                },
                hint: Some(ShapeHint::Rect(rect)),
            },
        ]);

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
        stage.set_commands(vec![DisplayCommand::Draw {
            draw: Draw::BlurredRoundedRect(imaging::BlurredRoundedRect {
                transform: Affine::IDENTITY,
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                color: Color::BLACK,
                radius: 4.0,
                std_dev: 6.0,
                composite: Composite::default(),
            }),
            hint: None,
        }]);

        assert_eq!(stage.transform_class, TransformClass::TranslateOnly);
        assert!(stage.chunks[0].metadata.has_blur);
        assert!(stage.chunks[0].bounds.is_some());
    }

    #[test]
    fn stage_damage_query_filters_chunks_by_bounds() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut stage = ElementStage::default();
        stage.set_commands(vec![
            DisplayCommand::Draw {
                draw: Draw::Fill {
                    transform: Affine::IDENTITY,
                    fill_rule: FillRule::NonZero,
                    paint: Color::BLACK.into(),
                    paint_transform: None,
                    shape: Geometry::Rect(rect),
                    composite: Composite::default(),
                },
                hint: Some(ShapeHint::Rect(rect)),
            },
            DisplayCommand::DrawImage {
                img: peniko::ImageBrush::new(peniko::ImageData {
                    data: peniko::Blob::new(Arc::new(vec![255, 255, 255, 255])),
                    format: peniko::ImageFormat::Rgba8,
                    alpha_type: peniko::ImageAlphaType::Alpha,
                    width: 1,
                    height: 1,
                }),
                hash: Arc::from([1_u8].as_slice()),
                rect: Rect::new(40.0, 40.0, 50.0, 50.0),
                transform: Affine::IDENTITY,
            },
        ]);

        let damage = [Rect::new(1.0, 1.0, 5.0, 5.0)];
        let chunks = stage.chunk_indices_for_damage(&damage);
        assert_eq!(chunks, vec![0]);
        assert_eq!(stage.chunks.len(), 1);
        assert!(stage.chunks[0].metadata.has_raster_image);
        assert_eq!(stage.chunks[0].bounds, Some(Rect::new(0.0, 0.0, 50.0, 50.0)));
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

fn shape_to_geometry(shape: &impl Shape) -> (Geometry, Option<ShapeHint>) {
    if let Some(rect) = shape.as_rect() {
        return (Geometry::Rect(rect), Some(ShapeHint::Rect(rect)));
    }
    if let Some(rect) = shape.as_rounded_rect() {
        return (
            Geometry::RoundedRect(rect),
            Some(ShapeHint::RoundedRect(rect)),
        );
    }
    if let Some(line) = shape.as_line() {
        return (
            Geometry::Path(path_from_shape(shape)),
            Some(ShapeHint::Line(line)),
        );
    }
    if let Some(circle) = shape.as_circle() {
        return (
            Geometry::Path(path_from_shape(shape)),
            Some(ShapeHint::Circle(circle)),
        );
    }
    (Geometry::Path(path_from_shape(shape)), None)
}

fn shape_hint(shape: &impl Shape) -> Option<ShapeHint> {
    shape
        .as_rect()
        .map(ShapeHint::Rect)
        .or_else(|| shape.as_rounded_rect().map(ShapeHint::RoundedRect))
        .or_else(|| shape.as_line().map(ShapeHint::Line))
        .or_else(|| shape.as_circle().map(ShapeHint::Circle))
}

fn path_from_shape(shape: &impl Shape) -> BezPath {
    shape.to_path(0.1)
}

fn blurred_rounded_rect(shape: &impl Shape) -> Option<(Rect, f64)> {
    if let Some(rect) = shape.as_rect() {
        return Some((rect, 0.0));
    }
    let rect = shape.as_rounded_rect()?;
    let radii = rect.radii();
    (radii.top_left == radii.top_right
        && radii.top_left == radii.bottom_left
        && radii.top_left == radii.bottom_right)
        .then_some((rect.rect(), radii.top_left))
}
