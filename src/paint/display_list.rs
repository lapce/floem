use std::sync::Arc;

use floem_renderer::text::{Glyph, GlyphRunProps};
use floem_renderer::{Img, Renderer as FloemRenderer, Svg, usvg};
use imaging::{
    Clip, Composite, Draw, FillRule, Geometry, Glyph as SceneGlyph, GlyphRun, Group, StrokeStyle,
};
use peniko::BrushRef;
use peniko::kurbo::{Affine, BezPath, Circle, Line, Point, Rect, RoundedRect, Shape, Stroke};
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
    pub transform_class: TransformClass,
}

#[derive(Clone, Copy)]
pub(crate) struct ElementSnapshot {
    pub local_bounds: Rect,
    pub clip: Option<RoundedRect>,
    pub effective_clip: Option<RoundedRect>,
    pub world_transform: Affine,
}

impl ElementSnapshot {
    pub(crate) fn from_box_tree(box_tree: &crate::BoxTree, element_id: ElementId) -> Self {
        Self {
            local_bounds: box_tree.local_bounds(element_id.0).unwrap_or_default(),
            clip: box_tree.local_clip(element_id.0).flatten(),
            effective_clip: box_tree.clipped_local_clip(element_id.0),
            world_transform: box_tree.world_transform(element_id.0).unwrap_or_default(),
        }
    }

    pub(crate) fn supports_reuse(self, current: Self) -> bool {
        self.local_bounds == current.local_bounds
            && self.clip == current.clip
            && effective_clip_not_loosened(self.effective_clip, current.effective_clip)
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
) {
    let mut current_z_index = None;
    let mut current_transform = None;

    for chunk in &stage.chunks {
        if current_z_index != Some(chunk.properties.z_index) {
            renderer.set_z_index(chunk.properties.z_index);
            current_z_index = Some(chunk.properties.z_index);
        }
        for command in &chunk.commands {
            match command {
                DisplayCommand::SetZIndex(_) => {}
                DisplayCommand::PushClip { clip, hint } => {
                    replay_clip(renderer, clip, hint.clone(), base_transform, &mut current_transform)
                }
                DisplayCommand::PopClip => renderer.clear_clip(),
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
                push_boundary_chunk(
                    &mut chunks,
                    properties,
                    command_transform_class(&DisplayCommand::PushClip {
                        clip: property_tree.clips[clip_id.0 as usize].clip.clone(),
                        hint: property_tree.clips[clip_id.0 as usize].hint.clone(),
                    }),
                    DisplayCommand::PushClip {
                        clip: property_tree.clips[clip_id.0 as usize].clip.clone(),
                        hint: property_tree.clips[clip_id.0 as usize].hint.clone(),
                    },
                );
            }
            DisplayCommand::PopClip => {
                clip_stack.pop();
                properties.clip_id = clip_stack.last().copied().unwrap_or_default();
                push_boundary_chunk(
                    &mut chunks,
                    properties,
                    command_transform_class(&DisplayCommand::PopClip),
                    DisplayCommand::PopClip,
                );
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
                match chunks.last_mut() {
                    Some(PaintChunk {
                        kind: PaintChunkKind::Draw,
                        properties: chunk_properties,
                        commands: chunk_commands,
                        transform_class: chunk_transform_class,
                    }) if *chunk_properties == properties => {
                        *chunk_transform_class =
                            chunk_transform_class.combine(transform_class);
                        chunk_commands.push(command);
                    }
                    _ => chunks.push(PaintChunk {
                        kind: PaintChunkKind::Draw,
                        properties,
                        commands: vec![command],
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

fn replay_clip(
    renderer: &mut impl FloemRenderer,
    clip: &Clip,
    hint: Option<ShapeHint>,
    base_transform: Affine,
    current_transform: &mut Option<Affine>,
) {
    let Clip::Fill {
        transform, shape, ..
    } = clip
    else {
        return;
    };
    set_transform_if_needed(renderer, base_transform * *transform, current_transform);
    match (hint, shape) {
        (Some(ShapeHint::Rect(rect)), _) => renderer.clip(&rect),
        (Some(ShapeHint::RoundedRect(rect)), _) => renderer.clip(&rect),
        (Some(ShapeHint::Line(line)), _) => renderer.clip(&line),
        (Some(ShapeHint::Circle(circle)), _) => renderer.clip(&circle),
        (None, Geometry::Rect(rect)) => renderer.clip(rect),
        (None, Geometry::RoundedRect(rect)) => renderer.clip(rect),
        (None, Geometry::Path(path)) => renderer.clip(path),
    }
}

fn replay_draw(
    renderer: &mut impl FloemRenderer,
    draw: &Draw,
    hint: Option<ShapeHint>,
    base_transform: Affine,
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

fn effective_clip_not_loosened(
    recorded: Option<RoundedRect>,
    current: Option<RoundedRect>,
) -> bool {
    match (recorded, current) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(recorded), Some(current)) => {
            recorded == current
                || (recorded.radii() == current.radii()
                    && recorded.rect().contains_rect(current.rect()))
        }
    }
}

#[cfg(test)]
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
    fn stage_tracks_clip_depth_across_boundary_chunks() {
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

        assert_eq!(stage.chunks.len(), 3);
        assert_eq!(stage.chunks[0].kind, PaintChunkKind::Boundary);
        assert_eq!(stage.chunks[1].kind, PaintChunkKind::Draw);
        assert_ne!(stage.chunks[1].properties.clip_id, ClipNodeId(0));
        assert_eq!(stage.chunks[2].kind, PaintChunkKind::Boundary);
        assert_eq!(stage.chunks[2].properties.clip_id, ClipNodeId(0));
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
