use peniko::kurbo::{Affine, Rect, RoundedRect};

use crate::{effects::ColorEffect, external_surface::ExternalSurfaceId};

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
    ExternalSurface {
        surface_id: ExternalSurfaceId,
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

    pub(crate) fn has_external_surfaces(&self) -> bool {
        self.items.iter().any(|item| match item {
            CompositionItem::ExternalSurface(_) => true,
            CompositionItem::Scene(layer) => !layer.external_images.is_empty(),
        })
    }

    pub(crate) fn window_prefix_fingerprint(&self) -> WindowPrefixFingerprint {
        let mut scenes = Vec::new();
        for item in &self.items {
            match item {
                CompositionItem::Scene(layer) if !layer.promoted => {
                    scenes.push(SceneFingerprint {
                        key: layer.key.clone(),
                        content_revision: layer.content_revision,
                        transform: layer.transform,
                        clip: layer.clip,
                        bounds: layer.bounds,
                        content_bounds: layer.content_bounds,
                        opacity: layer.opacity,
                        command_count: layer.scene.commands().len(),
                        external_image_count: layer.external_images.len(),
                        color_effect_count: layer.color_effects.len(),
                    });
                }
                CompositionItem::Scene(_) | CompositionItem::ExternalSurface(_) => {}
            }
        }
        WindowPrefixFingerprint { scenes }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WindowPrefixFingerprint {
    scenes: Vec<SceneFingerprint>,
}

#[derive(Clone, Debug, PartialEq)]
struct SceneFingerprint {
    key: CompositionKey,
    content_revision: u64,
    transform: Affine,
    clip: Option<RoundedRect>,
    bounds: Rect,
    content_bounds: Option<Rect>,
    opacity: f32,
    command_count: usize,
    external_image_count: usize,
    color_effect_count: usize,
}

#[derive(Clone, Debug)]
pub(crate) enum CompositionItem {
    Scene(SceneLayer),
    ExternalSurface(ExternalSurfaceLayer),
}

#[derive(Clone, Debug)]
pub(crate) struct SceneLayer {
    pub key: CompositionKey,
    pub scene: imaging::record::Scene,
    pub external_images: Vec<SceneExternalImage>,
    pub color_effects: Vec<ColorEffect>,
    pub content_revision: u64,
    pub transform: Affine,
    pub clip: Option<RoundedRect>,
    pub bounds: Rect,
    pub content_bounds: Option<Rect>,
    pub opacity: f32,
    pub promoted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SceneExternalImage {
    pub image_id: imaging::ExternalImageId,
    pub surface_id: ExternalSurfaceId,
    pub rect: Rect,
    pub source_size: peniko::kurbo::Size,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalSurfaceLayer {
    pub key: CompositionKey,
    pub surface_id: ExternalSurfaceId,
    pub rect: Rect,
    pub source_size: peniko::kurbo::Size,
    pub transform: Affine,
    pub clip: Option<RoundedRect>,
    pub opacity: f32,
}
