use rustc_hash::FxHashMap;

use crate::{
    external_surface::{
        ExternalSurfaceContent, ExternalSurfaceId, ExternalSurfaceOutcome,
        ExternalSurfaceProviderHandle,
    },
    frame::FrameTime,
    gpu_resources::GpuResources,
    paint::composition::{CompositionItem, CompositionPlan, WindowPrefixFingerprint},
};

use super::compositor::WindowCompositor;

#[derive(Default)]
pub(crate) struct WindowExternalSurfaces {
    entries: FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
    frame_time: Option<FrameTime>,
    needs_frame_pull: bool,
    pending_outcomes: Vec<ExternalSurfaceOutcome>,
}

impl WindowExternalSurfaces {
    pub(crate) fn entries(&self) -> &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry> {
        &self.entries
    }

    pub(crate) fn set_content(
        &mut self,
        surface_id: ExternalSurfaceId,
        content: ExternalSurfaceContent,
    ) {
        let entry = self.entries.entry(surface_id).or_default();
        entry.content = content;
        entry.content_dirty = true;
        entry.version = entry.version.wrapping_add(1);
    }

    pub(crate) fn set_provider(
        &mut self,
        surface_id: ExternalSurfaceId,
        provider: ExternalSurfaceProviderHandle,
    ) {
        let entry = self.entries.entry(surface_id).or_default();
        entry.provider = Some(provider);
        entry.content_dirty = true;
        entry.version = entry.version.wrapping_add(1);
    }

    pub(crate) fn request_frame(&mut self, surface_id: ExternalSurfaceId) {
        self.entries.entry(surface_id).or_default();
        self.needs_frame_pull = true;
    }

    pub(crate) fn update_providers(&mut self, plan: &CompositionPlan, effective_scale: f64) {
        let Some(frame_time) = self.frame_time else {
            return;
        };
        let mut request_next_frame = false;
        self.pending_outcomes.clear();
        for item in &plan.items {
            let CompositionItem::ExternalSurface(layer) = item else {
                continue;
            };
            let Some(entry) = self.entries.get_mut(&layer.surface_id) else {
                continue;
            };
            let Some(provider) = &entry.provider else {
                continue;
            };
            let size_px = layer.rect.size() * effective_scale;
            let args = crate::external_surface::ExternalSurfaceFrameArgs {
                surface_id: layer.surface_id,
                interval: frame_time.interval,
                visible: true,
                rect: layer.rect,
                size_px,
                previous_outcome: entry.previous_outcome,
            };
            let Ok(mut provider) = provider.lock() else {
                continue;
            };
            let update = provider.update_current_content(args);
            request_next_frame |= update.request_next_frame;
            if update.content_changed {
                entry.content = provider
                    .current_content()
                    .unwrap_or(ExternalSurfaceContent::Empty);
                entry.content_dirty = true;
                entry.version = entry.version.wrapping_add(1);
            }
            self.pending_outcomes.push(ExternalSurfaceOutcome {
                surface_id: layer.surface_id,
                frame_index: frame_time.frame_index,
                visible: true,
                outcome: crate::frame::FrameOutcome {
                    draw_attempted: true,
                    draw_completed: false,
                    missed_deadline: None,
                },
            });
        }
        if request_next_frame {
            self.needs_frame_pull = true;
        }
    }

    pub(crate) fn has_frame_pull(&self) -> bool {
        self.needs_frame_pull
    }

    pub(crate) fn take_frame_pull(&mut self) -> bool {
        std::mem::take(&mut self.needs_frame_pull)
    }

    pub(crate) fn pull_frame(
        &mut self,
        frame_time: FrameTime,
        plan: &CompositionPlan,
        compositor: &mut WindowCompositor,
        effective_scale: f64,
        gpu_resources: Option<&GpuResources>,
    ) {
        self.take_frame_pull();
        self.frame_time = Some(frame_time);
        self.update_providers(plan, effective_scale);
        let _composition_diff = compositor.apply_plan(plan, &self.entries, gpu_resources);
    }

    pub(crate) fn begin_composition_update(
        &mut self,
        frame_time: FrameTime,
        plan: &CompositionPlan,
    ) -> WindowPrefixFingerprint {
        let old_prefix = plan.window_prefix_fingerprint();
        self.frame_time = Some(frame_time);
        old_prefix
    }

    pub(crate) fn release_outcomes(&mut self, mut update: impl FnMut(&mut ExternalSurfaceOutcome)) {
        for mut outcome in std::mem::take(&mut self.pending_outcomes) {
            update(&mut outcome);
            if let Some(entry) = self.entries.get_mut(&outcome.surface_id) {
                entry.previous_outcome = Some(outcome);
                if let Some(provider) = &entry.provider
                    && let Ok(mut provider) = provider.lock()
                {
                    provider.release_current_content(outcome);
                }
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct ExternalSurfaceEntry {
    pub content: ExternalSurfaceContent,
    pub provider: Option<ExternalSurfaceProviderHandle>,
    pub content_dirty: bool,
    pub version: u64,
    pub previous_outcome: Option<ExternalSurfaceOutcome>,
}

impl Default for ExternalSurfaceEntry {
    fn default() -> Self {
        Self {
            content: ExternalSurfaceContent::Empty,
            provider: None,
            content_dirty: false,
            version: 0,
            previous_outcome: None,
        }
    }
}
