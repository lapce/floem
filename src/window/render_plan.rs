use peniko::kurbo::{Affine, Point, Rect};

use crate::paint::composition::{CompositionItem, CompositionPlan, SceneLayer};

/// Explicit render DAG for compositor scene fragments.
///
/// The render pool does not preserve submission or completion order across jobs.
/// This plan therefore treats every scene fragment as a node whose content/mask
/// jobs may run independently, and whose effect/publish phases are ordered only
/// by explicit per-node dependencies enforced by `WindowCompositor`.
///
/// Batches are scheduling groups, not visual ordering. The compositor layer tree
/// still owns final presentation order.
pub(crate) struct SceneRenderPlan<'a> {
    nodes: Vec<SceneRenderNode<'a>>,
    batches: Vec<SceneRenderBatch>,
}

impl<'a> SceneRenderPlan<'a> {
    pub(crate) fn from_composition_plan(plan: &'a CompositionPlan) -> Self {
        let mut nodes = Vec::new();
        let mut batcher = OrderedSceneBatcher::new();

        for (plan_index, item) in plan.items.iter().enumerate() {
            let CompositionItem::Scene(layer) = item else {
                batcher.clear_lookback();
                continue;
            };

            let id = SceneRenderNodeId(nodes.len());
            let key = SceneBatchKey::from_layer(layer);
            let bounds = world_bounds(layer);
            let batch_index = batcher.find_or_add_batch(key, bounds);
            nodes.push(SceneRenderNode {
                id,
                plan_index,
                layer,
                phases: SceneRenderPhases::from_layer(layer),
            });
            batcher.add_node_to_batch(batch_index, id, bounds);
        }

        Self {
            nodes,
            batches: batcher.finish(),
        }
    }

    pub(crate) fn batches(&self) -> &[SceneRenderBatch] {
        &self.batches
    }

    pub(crate) fn node(&self, id: SceneRenderNodeId) -> &SceneRenderNode<'a> {
        &self.nodes[id.0]
    }

    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn effect_node_count(&self) -> usize {
        self.nodes.iter().filter(|node| node.phases.effect).count()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SceneRenderNodeId(usize);

pub(crate) struct SceneRenderNode<'a> {
    id: SceneRenderNodeId,
    plan_index: usize,
    layer: &'a SceneLayer,
    phases: SceneRenderPhases,
}

impl<'a> SceneRenderNode<'a> {
    pub(crate) fn id(&self) -> SceneRenderNodeId {
        self.id
    }

    pub(crate) fn plan_index(&self) -> usize {
        self.plan_index
    }

    pub(crate) fn layer(&self) -> &'a SceneLayer {
        self.layer
    }

    pub(crate) fn phases(&self) -> SceneRenderPhases {
        self.phases
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SceneRenderPhases {
    pub(crate) content: bool,
    pub(crate) clip_mask: ClipMaskPhase,
    pub(crate) effect: bool,
    pub(crate) publish: bool,
}

impl SceneRenderPhases {
    fn from_layer(layer: &SceneLayer) -> Self {
        Self {
            content: true,
            clip_mask: if layer
                .color_filters
                .iter()
                .any(|effect| effect.clip.is_some())
            {
                ClipMaskPhase::ClassifyAtEmit
            } else {
                ClipMaskPhase::None
            },
            effect: !layer.color_filters.is_empty(),
            publish: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipMaskPhase {
    None,
    ClassifyAtEmit,
}

pub(crate) struct SceneRenderBatch {
    nodes: Vec<SceneRenderNodeId>,
    key: SceneBatchKey,
    bounds: Rect,
}

impl SceneRenderBatch {
    pub(crate) fn nodes(&self) -> &[SceneRenderNodeId] {
        &self.nodes
    }

    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn key(&self) -> SceneBatchKey {
        self.key
    }

    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SceneBatchKey {
    has_external_images: bool,
    has_effects: bool,
    has_effect_clip: bool,
    has_layer_clip: bool,
}

impl SceneBatchKey {
    fn from_layer(layer: &SceneLayer) -> Self {
        Self {
            has_external_images: !layer.external_images.is_empty(),
            has_effects: !layer.color_filters.is_empty(),
            has_effect_clip: layer
                .color_filters
                .iter()
                .any(|effect| effect.clip.is_some()),
            has_layer_clip: layer.clip.is_some(),
        }
    }
}

struct OrderedSceneBatcher {
    batches: Vec<SceneRenderBatch>,
    lookback: Vec<OpenBatch>,
}

impl OrderedSceneBatcher {
    fn new() -> Self {
        Self {
            batches: Vec::new(),
            lookback: Vec::new(),
        }
    }

    fn clear_lookback(&mut self) {
        self.lookback.clear();
    }

    fn find_or_add_batch(&mut self, key: SceneBatchKey, bounds: Rect) -> usize {
        for open in self.lookback.iter().rev() {
            if open.bounds.intersect(bounds).area() > 0.0 {
                break;
            }
            if open.key == key {
                return open.batch_index;
            }
        }

        let batch_index = self.batches.len();
        self.batches.push(SceneRenderBatch {
            nodes: Vec::new(),
            key,
            bounds,
        });
        self.lookback.push(OpenBatch {
            batch_index,
            key,
            bounds,
        });
        batch_index
    }

    fn add_node_to_batch(&mut self, batch_index: usize, node: SceneRenderNodeId, bounds: Rect) {
        let batch = &mut self.batches[batch_index];
        batch.nodes.push(node);
        batch.bounds = batch.bounds.union(bounds);
        if let Some(open) = self
            .lookback
            .iter_mut()
            .find(|open| open.batch_index == batch_index)
        {
            open.bounds = batch.bounds;
        }
    }

    fn finish(self) -> Vec<SceneRenderBatch> {
        self.batches
    }
}

struct OpenBatch {
    batch_index: usize,
    key: SceneBatchKey,
    bounds: Rect,
}

fn world_bounds(layer: &SceneLayer) -> Rect {
    transform_rect_bbox(layer.transform, layer.bounds)
}

fn transform_rect_bbox(transform: Affine, rect: Rect) -> Rect {
    let p0 = transform * rect.origin();
    let p1 = transform * Point::new(rect.x1, rect.y0);
    let p2 = transform * Point::new(rect.x0, rect.y1);
    let p3 = transform * Point::new(rect.x1, rect.y1);
    let x0 = p0.x.min(p1.x).min(p2.x).min(p3.x);
    let y0 = p0.y.min(p1.y).min(p2.y).min(p3.y);
    let x1 = p0.x.max(p1.x).max(p2.x).max(p3.x);
    let y1 = p0.y.max(p1.y).max(p2.y).max(p3.y);
    Rect::new(x0, y0, x1, y1)
}
