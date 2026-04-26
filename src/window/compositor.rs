use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, sync_channel},
};

use crate::{
    Application,
    app::UserEvent,
    external_surface::{ExternalSurfaceContent, ExternalSurfaceId},
    gpu_resources::GpuResources,
    paint::composition::{
        CompositionItem, CompositionKey, CompositionPlan, ExternalSurfaceLayer, SceneLayer,
    },
    paint::renderer::{RenderedFrame, WindowRenderer},
};

use super::external_surface::ExternalSurfaceEntry;

#[derive(Default)]
pub(crate) struct WindowCompositor {
    layers_by_key: FxHashMap<CompositionKey, CompositorLayerState>,
    order: Vec<CompositionKey>,

    platform: Option<PlatformCompositor>,
}

impl WindowCompositor {
    pub(crate) fn ensure_platform_presenter(
        &mut self,
        window_id: winit::window::WindowId,
        window: &(impl raw_window_handle::HasWindowHandle + ?Sized),
    ) {
        if self.platform.is_some() {
            return;
        }
        if let Ok(platform) = PlatformCompositor::new(window_id, window) {
            self.platform = Some(platform);
        }
    }

    pub(crate) fn apply_plan(
        &mut self,
        plan: &CompositionPlan,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
        gpu_resources: Option<&GpuResources>,
    ) -> CompositorDiff {
        let mut diff = CompositorDiff::default();
        let mut new_order = Vec::with_capacity(plan.items.len());
        let mut live_keys = FxHashSet::default();

        for item in &plan.items {
            let state = CompositorLayerState::from_item(item, external_surfaces);
            let key = state.key().clone();
            live_keys.insert(key.clone());
            new_order.push(key.clone());

            match self.layers_by_key.get(&key) {
                Some(previous) if previous.equivalent(&state) => {}
                Some(_) => diff.updated.push(key.clone()),
                None => diff.created.push(key.clone()),
            }

            self.layers_by_key.insert(key, state);
        }

        let removed = self
            .order
            .iter()
            .filter(|key| !live_keys.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in &removed {
            self.layers_by_key.remove(key);
        }
        diff.removed = removed;

        if self.order != new_order {
            diff.order_changed = true;
        }
        self.order = new_order;

        if let Some(platform) = &mut self.platform {
            platform.apply_plan(plan, external_surfaces, gpu_resources);
        }

        diff
    }

    pub(crate) fn render_scene_layers(
        &mut self,
        plan: &CompositionPlan,
        gpu_resources: &GpuResources,
        renderer: &mut dyn WindowRenderer,
    ) {
        if let Some(platform) = &mut self.platform {
            platform.render_scene_layers(plan, gpu_resources, renderer);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn display_link_layer(&self) -> Option<subduction_backend_apple::DisplayLinkLayer> {
        self.platform
            .as_ref()
            .map(PlatformCompositor::display_link_layer)
    }
}

struct PlatformCompositor {
    window_id: winit::window::WindowId,
    store: subduction_core::layer::LayerStore,
    presenter: subduction_platform::LayerPresenter,
    root: subduction_core::layer::LayerId,
    layers: FxHashMap<CompositionKey, subduction_core::layer::LayerId>,
    order: Vec<CompositionKey>,
    scene_surfaces: FxHashMap<CompositionKey, SceneSurface>,
    next_scene_surface_id: u32,
}

impl PlatformCompositor {
    fn new(
        window_id: winit::window::WindowId,
        window: &(impl raw_window_handle::HasWindowHandle + ?Sized),
    ) -> Result<Self, subduction_platform::LayerPresenterError> {
        let mut store = subduction_core::layer::LayerStore::new();
        let root = store.create_layer();
        let presenter = subduction_platform::LayerPresenter::from_window(window)?;
        Ok(Self {
            window_id,
            store,
            presenter,
            root,
            layers: FxHashMap::default(),
            order: Vec::new(),
            scene_surfaces: FxHashMap::default(),
            next_scene_surface_id: 0x8000_0000,
        })
    }

    fn apply_plan(
        &mut self,
        plan: &CompositionPlan,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
        gpu_resources: Option<&GpuResources>,
    ) {
        let mut live_keys = FxHashSet::default();
        let mut new_order = Vec::new();
        for item in &plan.items {
            match item {
                CompositionItem::Scene(layer) if layer.promoted => {
                    live_keys.insert(layer.key.clone());
                    new_order.push(layer.key.clone());
                }
                CompositionItem::Scene(_) => {}
                CompositionItem::ExternalSurface(layer) => {
                    live_keys.insert(layer.key.clone());
                    new_order.push(layer.key.clone());
                }
            }
        }

        let removed = self
            .layers
            .keys()
            .filter(|key| !live_keys.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in removed {
            if let Some(layer) = self.layers.remove(&key) {
                if self.store.parent(layer).is_some() {
                    self.store.remove_from_parent(layer);
                }
                self.store.destroy_layer(layer);
            }
            self.scene_surfaces.remove(&key);
        }

        let order_changed = self.order != new_order;
        if order_changed {
            for layer in self.layers.values().copied().collect::<Vec<_>>() {
                if self.store.parent(layer).is_some() {
                    self.store.remove_from_parent(layer);
                }
            }
        }

        for item in &plan.items {
            match item {
                CompositionItem::Scene(layer) if !layer.promoted => continue,
                CompositionItem::Scene(_) | CompositionItem::ExternalSurface(_) => {}
            }
            let key = match item {
                CompositionItem::Scene(layer) => &layer.key,
                CompositionItem::ExternalSurface(layer) => &layer.key,
            };
            let layer_id = *self
                .layers
                .entry(key.clone())
                .or_insert_with(|| self.store.create_layer());
            if order_changed {
                self.store.add_child(self.root, layer_id);
            }

            match item {
                CompositionItem::Scene(layer) => {
                    let surface_id = gpu_resources.and_then(|gpu_resources| {
                        self.ensure_scene_layer_surface(layer, gpu_resources)
                    });
                    let layer_bounds = promoted_scene_layer_bounds(layer);

                    configure_layer_geometry(
                        &mut self.store,
                        layer_id,
                        layer_bounds,
                        layer.transform,
                        layer.clip,
                        layer.opacity,
                    );
                    self.store.set_content(layer_id, surface_id);
                }
                CompositionItem::ExternalSurface(layer) => {
                    configure_layer_geometry(
                        &mut self.store,
                        layer_id,
                        layer.rect,
                        layer.transform,
                        layer.clip,
                        layer.opacity,
                    );
                    self.store.set_content(
                        layer_id,
                        Some(subduction_core::layer::SurfaceId(
                            layer.surface_id.get() as u32
                        )),
                    );
                }
            }
        }
        if order_changed {
            self.order = new_order;
        }

        let changes = self.store.evaluate();
        self.presenter.apply(&self.store, &changes);

        for item in &plan.items {
            match item {
                CompositionItem::Scene(layer) => {
                    if let (Some(layer_id), Some(scene_surface)) = (
                        self.layers.get(&layer.key).copied(),
                        self.scene_surfaces.get(&layer.key),
                    ) {
                        self.presenter
                            .attach_external_wgpu_surface(layer_id.index(), &scene_surface.surface);
                    }
                }
                CompositionItem::ExternalSurface(layer) => {
                    let Some(layer_id) = self.layers.get(&layer.key).copied() else {
                        continue;
                    };
                    let Some(ExternalSurfaceEntry {
                        content: ExternalSurfaceContent::Subduction(surface),
                        ..
                    }) = external_surfaces.get(&layer.surface_id)
                    else {
                        continue;
                    };
                    if let Some(surface) =
                        surface.downcast_ref::<subduction_platform::ExternalWgpuSurface>()
                    {
                        self.presenter
                            .attach_external_wgpu_surface(layer_id.index(), surface);
                    }
                }
            }
        }

        if std::env::var_os("FLOEM_RESIZE_DIAG").is_some() {
            for item in &plan.items {
                match item {
                    CompositionItem::ExternalSurface(layer) => {
                        eprintln!(
                            "floem resize compositor t={:?} external surface={:?} rect=({:.2},{:.2}) {:.2}x{:.2} transform={:?}",
                            std::time::Instant::now(),
                            layer.surface_id,
                            layer.rect.x0,
                            layer.rect.y0,
                            layer.rect.width(),
                            layer.rect.height(),
                            layer.transform,
                        );
                    }
                    CompositionItem::Scene(layer) if layer.promoted => {
                        let bounds = promoted_scene_layer_bounds(layer);
                        eprintln!(
                            "floem resize compositor t={:?} promoted bounds=({:.2},{:.2}) {:.2}x{:.2} content_rev={} transform={:?}",
                            std::time::Instant::now(),
                            bounds.x0,
                            bounds.y0,
                            bounds.width(),
                            bounds.height(),
                            layer.content_revision,
                            layer.transform,
                        );
                    }
                    CompositionItem::Scene(_) => {}
                }
            }
        }

        if std::env::var_os("FLOEM_SUBDUCTION_DEBUG").is_some() {
            let (root_sublayers, presenter_layers, metal_layers) =
                self.presenter.debug_layer_counts();
            eprintln!(
                "floem subduction: plan_items={} platform_layers={} root_sublayers={} presenter_layers={} metal_layers={}",
                plan.items.len(),
                self.layers.len(),
                root_sublayers,
                presenter_layers,
                metal_layers
            );
        }
    }

    #[cfg(target_os = "macos")]
    fn display_link_layer(&self) -> subduction_backend_apple::DisplayLinkLayer {
        self.presenter.display_link_layer()
    }

    pub(crate) fn render_scene_layers(
        &mut self,
        plan: &CompositionPlan,
        gpu_resources: &GpuResources,
        renderer: &mut dyn WindowRenderer,
    ) {
        for item in &plan.items {
            match item {
                CompositionItem::Scene(layer) if !layer.promoted => continue,
                CompositionItem::Scene(layer) => {
                    self.render_scene_layer(layer, gpu_resources, renderer);
                }
                CompositionItem::ExternalSurface(_) => {}
            }
        }
    }

    fn ensure_scene_layer_surface(
        &mut self,
        layer: &SceneLayer,
        gpu_resources: &GpuResources,
    ) -> Option<subduction_core::layer::SurfaceId> {
        let layer_bounds = promoted_scene_layer_bounds(layer);
        let width = layer_bounds.width().ceil().max(1.0) as u32;
        let height = layer_bounds.height().ceil().max(1.0) as u32;
        let key = layer.key.clone();
        if self
            .scene_surfaces
            .get(&key)
            .is_some_and(|surface| surface.width != width || surface.height != height)
        {
            self.scene_surfaces.remove(&key);
        }

        if !self.scene_surfaces.contains_key(&key) {
            let surface_id = subduction_core::layer::SurfaceId(self.next_scene_surface_id);
            self.next_scene_surface_id = self.next_scene_surface_id.wrapping_add(1);
            let surface = subduction_platform::ExternalWgpuSurface::new(
                surface_id,
                f64::from(width),
                f64::from(height),
            );
            let (tx, rx) = sync_channel(1);
            let drawable_queued = Arc::new(AtomicBool::new(false));
            let drawable_requested = Arc::new(AtomicBool::new(true));
            let queued_demand = drawable_queued.clone();
            let requested_demand = drawable_requested.clone();
            let send_queued = drawable_queued.clone();
            let send_requested = drawable_requested.clone();
            let window_id = self.window_id;
            let target = surface
                .start_drawable_target_with_demand(
                    gpu_resources.device.clone(),
                    move || {
                        requested_demand.load(Ordering::Acquire)
                            && !queued_demand.load(Ordering::Acquire)
                    },
                    move |frame| {
                        send_requested.store(false, Ordering::Release);
                        if tx.try_send(frame).is_ok() {
                            send_queued.store(true, Ordering::Release);
                            Application::send_proxy_event(
                                UserEvent::CompositorSceneDrawableReady { window_id },
                            );
                        } else {
                            send_queued.store(true, Ordering::Release);
                        }
                    },
                )
                .ok()?;
            let blitter = wgpu::util::TextureBlitter::new(
                &gpu_resources.device,
                wgpu::TextureFormat::Bgra8Unorm,
            );
            self.scene_surfaces.insert(
                key.clone(),
                SceneSurface {
                    surface_id,
                    surface,
                    _target: target,
                    rx,
                    drawable_queued,
                    drawable_requested,
                    blitter,
                    width,
                    height,
                    content_revision: 0,
                    layer_bounds,
                },
            );
        }

        let scene_surface = self.scene_surfaces.get_mut(&key)?;
        if scene_surface.content_revision == layer.content_revision
            && scene_surface.layer_bounds == layer_bounds
        {
            return Some(scene_surface.surface_id);
        }
        scene_surface
            .drawable_requested
            .store(true, Ordering::Release);
        Some(scene_surface.surface_id)
    }

    fn render_scene_layer(
        &mut self,
        layer: &SceneLayer,
        gpu_resources: &GpuResources,
        renderer: &mut dyn WindowRenderer,
    ) -> Option<subduction_core::layer::SurfaceId> {
        let surface_id = self.ensure_scene_layer_surface(layer, gpu_resources)?;
        let scene_surface = self.scene_surfaces.get_mut(&layer.key)?;
        let layer_bounds = promoted_scene_layer_bounds(layer);
        if scene_surface.content_revision == layer.content_revision
            && scene_surface.layer_bounds == layer_bounds
        {
            return Some(surface_id);
        }

        let drawable_frame = match scene_surface.rx.try_recv() {
            Ok(frame) => {
                scene_surface
                    .drawable_queued
                    .store(false, Ordering::Release);
                frame
            }
            Err(_) => {
                scene_surface
                    .drawable_requested
                    .store(true, Ordering::Release);
                return Some(scene_surface.surface_id);
            }
        };
        if drawable_frame.width != scene_surface.width
            || drawable_frame.height != scene_surface.height
        {
            scene_surface
                .drawable_requested
                .store(true, Ordering::Release);
            return Some(scene_surface.surface_id);
        }
        let Some(view) = drawable_frame.view.as_ref() else {
            return Some(scene_surface.surface_id);
        };
        let scene = translated_promoted_scene(layer, layer_bounds);
        let rendered_frame = renderer.render_offscreen_frame(scene, layer_bounds.size())?;
        let mut encoder =
            gpu_resources
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("floem subduction scene blit"),
                });
        match rendered_frame {
            RenderedFrame::Gpu(texture) => {
                let source_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                scene_surface
                    .blitter
                    .copy(&gpu_resources.device, &mut encoder, &source_view, view);
            }
            RenderedFrame::Cpu(frame) => {
                let texture = create_cpu_scene_texture(
                    &gpu_resources.device,
                    &gpu_resources.queue,
                    frame.image,
                );
                let source_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                scene_surface
                    .blitter
                    .copy(&gpu_resources.device, &mut encoder, &source_view, view);
            }
        }
        gpu_resources.queue.submit([encoder.finish()]);
        drawable_frame.present_after_submit(
            &gpu_resources.queue,
            subduction_core::timing::PresentPacing::AsSoonAsPossible,
        );
        scene_surface.content_revision = layer.content_revision;
        scene_surface.layer_bounds = layer_bounds;

        Some(scene_surface.surface_id)
    }
}

fn promoted_scene_layer_bounds(layer: &SceneLayer) -> peniko::kurbo::Rect {
    layer.content_bounds.unwrap_or(layer.bounds)
}

fn translated_promoted_scene(
    layer: &SceneLayer,
    layer_bounds: peniko::kurbo::Rect,
) -> imaging::record::Scene {
    if layer_bounds.x0 == 0.0 && layer_bounds.y0 == 0.0 {
        return layer.scene.clone();
    }
    let mut scene = imaging::record::Scene::new();
    imaging::record::replay_transformed(
        &layer.scene,
        &mut scene,
        peniko::kurbo::Affine::translate((-layer_bounds.x0, -layer_bounds.y0)),
    );
    scene
}

fn create_cpu_scene_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: imaging::RgbaImage,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("floem subduction cpu scene upload"),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &image.data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * image.width),
            rows_per_image: Some(image.height),
        },
        wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
    );
    texture
}

struct SceneSurface {
    surface_id: subduction_core::layer::SurfaceId,
    surface: subduction_platform::ExternalWgpuSurface,
    _target: subduction_platform::WgpuDrawableTargetThread,
    rx: Receiver<subduction_platform::WgpuDrawableFrame>,
    drawable_queued: Arc<AtomicBool>,
    drawable_requested: Arc<AtomicBool>,
    blitter: wgpu::util::TextureBlitter,
    width: u32,
    height: u32,
    content_revision: u64,
    layer_bounds: peniko::kurbo::Rect,
}

fn configure_layer_geometry(
    store: &mut subduction_core::layer::LayerStore,
    layer_id: subduction_core::layer::LayerId,
    bounds: peniko::kurbo::Rect,
    transform: peniko::kurbo::Affine,
    clip: Option<peniko::kurbo::RoundedRect>,
    opacity: f32,
) {
    let size = peniko::kurbo::Size::new(bounds.width().max(0.0), bounds.height().max(0.0));
    let center =
        peniko::kurbo::Vec2::new(bounds.x0 + size.width * 0.5, bounds.y0 + size.height * 0.5);
    store.set_bounds(layer_id, size);
    store.set_transform(
        layer_id,
        subduction_core::transform::Transform3d::from(
            transform * peniko::kurbo::Affine::translate(center),
        ),
    );
    store.set_clip(layer_id, layer_clip_shape(bounds, clip));
    store.set_opacity(layer_id, opacity);
}

fn layer_clip_shape(
    bounds: peniko::kurbo::Rect,
    clip: Option<peniko::kurbo::RoundedRect>,
) -> Option<subduction_core::layer::ClipShape> {
    let clip = clip?;
    let rect = clip.rect();
    let intersection = peniko::kurbo::Rect::new(
        rect.x0.max(bounds.x0),
        rect.y0.max(bounds.y0),
        rect.x1.min(bounds.x1),
        rect.y1.min(bounds.y1),
    );
    if intersection.x0 >= intersection.x1 || intersection.y0 >= intersection.y1 {
        return Some(subduction_core::layer::ClipShape::Rect(
            peniko::kurbo::Rect::ZERO,
        ));
    }
    let layer_rect = peniko::kurbo::Rect::new(
        intersection.x0 - bounds.x0,
        intersection.y0 - bounds.y0,
        intersection.x1 - bounds.x0,
        intersection.y1 - bounds.y0,
    );
    Some(subduction_core::layer::ClipShape::RoundedRect(
        peniko::kurbo::RoundedRect::from_rect(layer_rect, clip.radii()),
    ))
}

#[derive(Clone, Debug)]
pub(crate) enum CompositorLayerState {
    Scene(SceneCompositorLayer),
    ExternalSurface(ExternalSurfaceCompositorLayer),
}

impl CompositorLayerState {
    fn key(&self) -> &CompositionKey {
        match self {
            Self::Scene(layer) => &layer.key,
            Self::ExternalSurface(layer) => &layer.key,
        }
    }

    fn equivalent(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Scene(a), Self::Scene(b)) => a == b,
            (Self::ExternalSurface(a), Self::ExternalSurface(b)) => a.equivalent(b),
            _ => false,
        }
    }

    fn from_item(
        item: &CompositionItem,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
    ) -> Self {
        match item {
            CompositionItem::Scene(layer) => Self::Scene(SceneCompositorLayer::from_layer(layer)),
            CompositionItem::ExternalSurface(layer) => Self::ExternalSurface(
                ExternalSurfaceCompositorLayer::from_layer(layer, external_surfaces),
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SceneCompositorLayer {
    pub key: CompositionKey,
    pub transform: peniko::kurbo::Affine,
    pub clip: Option<peniko::kurbo::RoundedRect>,
    pub bounds: peniko::kurbo::Rect,
    pub content_bounds: Option<peniko::kurbo::Rect>,
    pub opacity: f32,
    pub content_revision: u64,
    pub command_count: usize,
    pub promoted: bool,
}

impl SceneCompositorLayer {
    fn from_layer(layer: &SceneLayer) -> Self {
        Self {
            key: layer.key.clone(),
            transform: layer.transform,
            clip: layer.clip,
            bounds: layer.bounds,
            content_bounds: layer.content_bounds,
            opacity: layer.opacity,
            content_revision: layer.content_revision,
            command_count: layer.scene.commands().len(),
            promoted: layer.promoted,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalSurfaceCompositorLayer {
    pub key: CompositionKey,
    pub surface_id: ExternalSurfaceId,
    pub rect: peniko::kurbo::Rect,
    pub transform: peniko::kurbo::Affine,
    pub clip: Option<peniko::kurbo::RoundedRect>,
    pub opacity: f32,
    pub content: ExternalSurfaceContent,
    pub content_version: u64,
}

impl ExternalSurfaceCompositorLayer {
    fn from_layer(
        layer: &ExternalSurfaceLayer,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
    ) -> Self {
        Self {
            key: layer.key.clone(),
            surface_id: layer.surface_id,
            rect: layer.rect,
            transform: layer.transform,
            clip: layer.clip,
            opacity: layer.opacity,
            content: external_surfaces
                .get(&layer.surface_id)
                .map(|entry| entry.content.clone())
                .unwrap_or(ExternalSurfaceContent::Empty),
            content_version: external_surfaces
                .get(&layer.surface_id)
                .map(|entry| entry.version)
                .unwrap_or(0),
        }
    }

    fn equivalent(&self, other: &Self) -> bool {
        self.key == other.key
            && self.surface_id == other.surface_id
            && self.rect == other.rect
            && self.transform == other.transform
            && self.clip == other.clip
            && self.opacity == other.opacity
            && self.content_version == other.content_version
            && external_content_key(&self.content) == external_content_key(&other.content)
    }
}

fn external_content_key(content: &ExternalSurfaceContent) -> ExternalContentKey {
    match content {
        ExternalSurfaceContent::Empty => ExternalContentKey::Empty,
        ExternalSurfaceContent::Texture(texture) => {
            ExternalContentKey::Texture { size: texture.size }
        }
        ExternalSurfaceContent::Image(image) => ExternalContentKey::Image {
            size: peniko::kurbo::Size::new(image.width as f64, image.height as f64),
        },
        ExternalSurfaceContent::Subduction(surface) => ExternalContentKey::Subduction {
            ptr: std::sync::Arc::as_ptr(surface).cast::<()>() as usize,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ExternalContentKey {
    Empty,
    Texture { size: peniko::kurbo::Size },
    Image { size: peniko::kurbo::Size },
    Subduction { ptr: usize },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CompositorDiff {
    pub created: Vec<CompositionKey>,
    pub updated: Vec<CompositionKey>,
    pub removed: Vec<CompositionKey>,
    pub order_changed: bool,
}
