use std::hash::{Hash, Hasher};

use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Size};

use crate::{ElementId, compositor_surface::CompositorSurfaceId, effects::CompositorShaderPass};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayerSourceId(u64);

impl LayerSourceId {
    pub(crate) fn from_element_id(id: ElementId) -> Self {
        let mut hasher = rustc_hash::FxHasher::default();
        id.hash(&mut hasher);
        Self(hasher.finish())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PaintStage {
    Paint,
    Post,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CompositionKey {
    SceneRun {
        run_index: u32,
    },
    CompositorSurface {
        surface_id: CompositorSurfaceId,
        occurrence: u32,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct CompositionPlan {
    pub items: Vec<CompositionItem>,
}

impl CompositionPlan {
    pub(crate) fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub(crate) fn has_compositor_surfaces(&self) -> bool {
        self.items.iter().any(|item| match item {
            CompositionItem::CompositorSurface(_) => true,
            CompositionItem::Scene(layer) => !layer.external_images.is_empty(),
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CompositionItem {
    Scene(SceneLayer),
    CompositorSurface(CompositorSurfaceLayer),
}

#[derive(Clone, Debug)]
pub(crate) struct SceneLayer {
    pub key: CompositionKey,
    pub source_element_id: Option<LayerSourceId>,
    pub debug_name: Option<String>,
    pub scene: imaging::record::Scene,
    pub external_images: Vec<SceneExternalImage>,
    pub color_filters: Vec<CompositorShaderPass>,
    pub content_revision: u64,
    pub transform: Affine,
    pub clip: Option<RoundedRect>,
    pub bounds: Rect,
    pub content_bounds: Option<Rect>,
    pub opacity: f32,
    pub promoted: bool,
    pub target_fps: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SceneExternalImage {
    pub image_id: imaging::ExternalImageId,
    pub surface_id: CompositorSurfaceId,
    pub rect: Rect,
    pub source_size: peniko::kurbo::Size,
}

#[derive(Clone, Debug)]
pub(crate) struct CompositorSurfaceLayer {
    pub key: CompositionKey,
    pub surface_id: CompositorSurfaceId,
    pub rect: Rect,
    pub source_size: peniko::kurbo::Size,
    pub transform: Affine,
    pub clip: Option<RoundedRect>,
    pub opacity: f32,
}

pub(crate) fn clip_scene_layers_to_viewport(plan: &mut CompositionPlan, viewport_size: Size) {
    let viewport = Rect::from_origin_size(Point::ZERO, viewport_size);
    if !is_non_empty(viewport) {
        return;
    }

    for item in &mut plan.items {
        let CompositionItem::Scene(layer) = item else {
            continue;
        };

        let world_bounds = transform_rect_bbox(layer.transform, layer.bounds);
        let visible_world = intersect_rects(world_bounds, viewport);
        if !is_non_empty(visible_world) {
            layer.bounds = Rect::ZERO;
            layer.content_bounds = None;
            continue;
        }

        let visible_local = intersect_rects(
            layer.bounds,
            transform_rect_bbox(layer.transform.inverse(), visible_world),
        );
        if !is_non_empty(visible_local) {
            layer.bounds = Rect::ZERO;
            layer.content_bounds = None;
            continue;
        }

        layer.bounds = visible_local;
        layer.content_bounds = layer
            .content_bounds
            .map(|content_bounds| intersect_rects(content_bounds, visible_local))
            .filter(|bounds| is_non_empty(*bounds));
    }
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

fn is_non_empty(rect: Rect) -> bool {
    rect.x0 < rect.x1 && rect.y0 < rect.y1
}
