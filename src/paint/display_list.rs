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
    pub commands: Vec<DisplayCommand>,
    pub transform_class: TransformClass,
}

impl Default for ElementStage {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            transform_class: TransformClass::Affine,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ElementSnapshot {
    pub local_bounds: Rect,
    pub clip: Option<RoundedRect>,
    pub world_transform: Affine,
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

        if previous.local_bounds != snapshot.local_bounds || previous.clip != snapshot.clip {
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
    for command in &stage.commands {
        match command {
            DisplayCommand::SetZIndex(z_index) => renderer.set_z_index(*z_index),
            DisplayCommand::PushClip { clip, hint } => {
                replay_clip(renderer, clip, hint.clone(), base_transform)
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
                renderer.set_transform(base_transform * *clip_transform);
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
                replay_draw(renderer, draw, hint.clone(), base_transform)
            }
            DisplayCommand::DrawImage {
                img,
                hash,
                rect,
                transform,
            } => {
                renderer.set_transform(base_transform * *transform);
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
                renderer.set_transform(base_transform * *transform);
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

fn replay_clip(
    renderer: &mut impl FloemRenderer,
    clip: &Clip,
    hint: Option<ShapeHint>,
    base_transform: Affine,
) {
    let Clip::Fill {
        transform, shape, ..
    } = clip
    else {
        return;
    };
    renderer.set_transform(base_transform * *transform);
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
) {
    match draw {
        Draw::Fill {
            transform,
            paint,
            shape,
            ..
        } => {
            renderer.set_transform(base_transform * *transform);
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
            renderer.set_transform(base_transform * *transform);
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
            renderer.set_transform(base_transform);
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
            renderer.set_transform(base_transform * rect.transform);
            let shape = rect.rect.to_rounded_rect(rect.radius);
            renderer.fill(&shape, rect.color, rect.std_dev);
        }
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
