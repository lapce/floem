use peniko::kurbo::{Affine, Rect, RoundedRect};

use crate::{ElementId, external_surface::ExternalSurfaceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PaintStage {
    Paint,
    Post,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CompositionKey {
    SceneChunk {
        owner: ElementId,
        stage: PaintStage,
        chunk_index: u32,
    },
    ExternalSurface {
        owner: ElementId,
        stage: PaintStage,
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
        self.items
            .iter()
            .any(|item| matches!(item, CompositionItem::ExternalSurface(_)))
    }

    pub(crate) fn window_prefix_fingerprint(&self) -> WindowPrefixFingerprint {
        let mut scenes = Vec::new();
        for item in &self.items {
            match item {
                CompositionItem::Scene(layer) => scenes.push(SceneFingerprint {
                    key: layer.key.clone(),
                    content_revision: layer.content_revision,
                    transform: layer.transform,
                    clip: layer.clip,
                    bounds: layer.bounds,
                    opacity: layer.opacity,
                    command_count: layer.scene.commands().len(),
                }),
                CompositionItem::ExternalSurface(_) => break,
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
    opacity: f32,
    command_count: usize,
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
    pub content_revision: u64,
    pub transform: Affine,
    pub clip: Option<RoundedRect>,
    pub bounds: Rect,
    pub opacity: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalSurfaceLayer {
    pub key: CompositionKey,
    pub surface_id: ExternalSurfaceId,
    pub rect: Rect,
    pub transform: Affine,
    pub clip: Option<RoundedRect>,
    pub opacity: f32,
}
