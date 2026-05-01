//! # Renderer
//!
//! This section is to help understand how Floem is implemented for developers of Floem.
//!
//! ## Render loop and update lifecycle
//!
//! event -> update -> layout -> paint.
//!
//! #### Event
//! After an event comes in (e.g. the user clicked the mouse, pressed a key etc), the event will be propagated from the root view to the children.
//! If the parent does not handle the event, it will automatically be sent to the child view. If the parent does handle the event the parent can decide whether the event should continue propagating so that the child can also process the event or if the propagation should stop.
//! The event propagation is stopped whenever an event listener returns `true` on the event handling.
//!
//!
//! #### Event handling -> reactive system updates
//! Event handling is a common place for reactive state changes to occur. E.g., on the counter example, when you click increment,
//! it updates the counter and because the label has an effect that is subscribed to those changes (see [`floem_reactive::create_effect`]), the label will update the text it presents.
//!
//! #### Update
//! The update of states on the Views could cause some of them to need a new layout recalculation, because the size might have changed etc.
//! The reactive system can't directly manipulate the view state of the label because the `WindowState` owns all the views. And instead, it will send the update to a message queue via [`ViewId::update_state`](crate::ViewId::update_state)
//! After the event propagation is done, Floem will process all the update messages in the queue, and it can manipulate the state of a particular view through the update method.
//!
//!
//! #### Layout
//! The layout method is called from the root view to re-layout the views that have requested a layout call.
//! The layout call is to change the layout properties at Taffy, and after the layout call is done, `compute_layout` is called to calculate the sizes and positions of each view.
//!
//! #### Paint
//! And in the end, `paint` is called to render all the views to the screen.
//!
//!
//! ## Terminology
//!
//! Useful definitions for developers of Floem
//!
//! #### Active view
//!
//! Affects pointer events. Pointer events will only be sent to the active view. The view will continue to receive pointer events even if the mouse is outside its bounds.
//! It is useful when you drag things, e.g. the scroll bar, you set the scroll bar active after pointer down, then when you drag, the `PointerMove` will always be sent to the view, even if your mouse is outside of the view.
//!
//! #### Focused view
//! Affects keyboard events. Keyboard events will only be sent to the focused view. The view will continue to receive keyboard events even if it's not the active view.
//!
//! ## Notable invariants and tolerances
//! - There can be only one root `View`
//! - Only one view can be active at a time.
//! - Only one view can be focused at a time.
//!
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};

use crate::gpu_resources::GpuResources;
use crate::paint::composition::CompositionKey;
use crate::window::compositor::SceneRenderSignature;
use imaging::{ImageRenderer, PaintSink, RenderSource, RgbaImage, record::Scene};
use imaging_wgpu::{
    ExternalImageResolver, ResolvedExternalImage, TextureRenderer, TextureViewTarget,
};
use peniko::ImageData;
use peniko::kurbo::Size;
use winit::window::{Window, WindowId};

use crate::platform::{Duration, Instant};
#[cfg(not(target_arch = "wasm32"))]
use crate::{Application, app::UserEvent};

#[derive(Clone, Debug, Default)]
pub(crate) struct ExternalImageResources {
    images: Vec<ExternalImageResource>,
}

impl ExternalImageResources {
    pub(crate) fn insert(&mut self, id: imaging::ExternalImageId, image: ResolvedExternalImage) {
        self.images.push(ExternalImageResource { id, image });
    }
}

#[derive(Clone, Debug)]
struct ExternalImageResource {
    id: imaging::ExternalImageId,
    image: ResolvedExternalImage,
}

struct ExternalImageResourceResolver {
    images: ExternalImageResources,
}

impl ExternalImageResourceResolver {
    fn new(images: ExternalImageResources) -> Self {
        Self { images }
    }
}

impl ExternalImageResolver for ExternalImageResourceResolver {
    fn resolve_external_image(
        &mut self,
        image: imaging::ExternalImage,
    ) -> Option<ResolvedExternalImage> {
        self.images
            .images
            .iter()
            .find(|resource| resource.id == image.id)
            .map(|resource| resource.image.clone())
    }
}

pub(crate) type RendererChooser = Arc<dyn Fn(NewRendererCx) -> RendererSpec + Send + Sync>;

#[derive(Clone, Default)]
pub(crate) struct SharedSceneFragmentRendererPool {
    inner: Arc<Mutex<Option<Arc<SceneFragmentRendererPool>>>>,
}

impl SharedSceneFragmentRendererPool {
    pub(crate) fn get(&self) -> Option<Arc<SceneFragmentRendererPool>> {
        self.inner.lock().ok()?.clone()
    }

    pub(crate) fn init_if_needed(
        &self,
        chooser: &RendererChooser,
        cx: NewRendererCx,
    ) -> Arc<SceneFragmentRendererPool> {
        let mut inner = self.inner.lock().expect("renderer pool mutex poisoned");
        if let Some(pool) = inner.as_ref() {
            return Arc::clone(pool);
        }
        let pool = Arc::new(
            SceneFragmentRendererPool::new(Arc::clone(chooser), cx)
                .expect("create scene fragment renderer pool"),
        );
        *inner = Some(Arc::clone(&pool));
        pool
    }
}

pub(crate) struct SceneFragmentRenderJob {
    pub(crate) scene: Scene,
    pub(crate) base_transform: peniko::kurbo::Affine,
    pub(crate) clip: Option<peniko::kurbo::RoundedRect>,
    pub(crate) render_size: Size,
    pub(crate) texture: wgpu::Texture,
    pub(crate) external_images: ExternalImageResources,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct SceneFragmentRenderCompletion {
    pub(crate) window_id: WindowId,
    pub(crate) key: CompositionKey,
    pub(crate) signature: SceneRenderSignature,
}

pub(crate) struct SceneFragmentRendererPool {
    name: &'static str,
    compositor_texture_format: Option<wgpu::TextureFormat>,
    workers: Vec<SceneFragmentRenderWorker>,
}

#[derive(Clone)]
pub struct NewRendererCx {
    pub window: Arc<dyn Window>,
    pub gpu_resources: Option<GpuResources>,
    pub surface_caps: Option<subduction::wgpu::CompositorSurfaceCapabilities>,
    pub transparent: bool,
    pub size: Size,
    pub scale: f64,
    pub maximum_drawable_count: u32,
}

impl NewRendererCx {
    #[cfg(target_arch = "wasm32")]
    fn normalized_size(&self) -> Size {
        Size::new(self.size.width.max(1.0), self.size.height.max(1.0))
    }

    pub fn gpu(&self) -> Option<GpuRendererChooserCx<'_>> {
        if force_cpu_requested() {
            return None;
        }

        match (&self.surface_caps, &self.gpu_resources) {
            (Some(surface_caps), Some(gpu_resources)) => Some(GpuRendererChooserCx {
                gpu_resources,
                surface_caps,
            }),
            _ => None,
        }
    }

    pub fn image_renderer(
        self,
        backend: impl ImageRenderer + 'static,
        name: &'static str,
    ) -> RendererSpec {
        RendererSpec(RendererSpecInner::Cpu(CpuRenderer::new(backend, name)))
    }

    pub fn provided_texture_renderer(
        self,
        backend: impl TextureRenderer<TextureTarget = wgpu::Texture, Texture = wgpu::Texture> + 'static,
        surface_format: wgpu::TextureFormat,
        name: &'static str,
    ) -> RendererSpec {
        let device = self
            .gpu_resources
            .as_ref()
            .expect("renderer requires GPU device")
            .device
            .clone();
        RendererSpec(RendererSpecInner::Gpu {
            backend: GpuRenderer::provided_texture(backend, device, name),
            surface_format,
        })
    }

    pub fn owned_texture_renderer(
        self,
        backend: impl TextureRenderer<TextureTarget = wgpu::Texture, Texture = wgpu::Texture> + 'static,
        surface_format: wgpu::TextureFormat,
        name: &'static str,
    ) -> RendererSpec {
        let device = self
            .gpu_resources
            .as_ref()
            .expect("renderer requires GPU device")
            .device
            .clone();
        RendererSpec(RendererSpecInner::Gpu {
            backend: GpuRenderer::owned_texture(backend, device, name),
            surface_format,
        })
    }

    pub fn provided_texture_view_renderer(
        self,
        backend: impl TextureRenderer<TextureTarget = TextureViewTarget, Texture = wgpu::Texture>
        + 'static,
        surface_format: wgpu::TextureFormat,
        name: &'static str,
    ) -> RendererSpec {
        let device = self
            .gpu_resources
            .as_ref()
            .expect("renderer requires GPU device")
            .device
            .clone();
        RendererSpec(RendererSpecInner::Gpu {
            backend: GpuRenderer::provided_texture_view(backend, device, name),
            surface_format,
        })
    }

    pub fn owned_texture_view_renderer(
        self,
        backend: impl TextureRenderer<TextureTarget = TextureViewTarget, Texture = wgpu::Texture>
        + 'static,
        surface_format: wgpu::TextureFormat,
        name: &'static str,
    ) -> RendererSpec {
        let device = self
            .gpu_resources
            .as_ref()
            .expect("renderer requires GPU device")
            .device
            .clone();
        RendererSpec(RendererSpecInner::Gpu {
            backend: GpuRenderer::owned_texture_view(backend, device, name),
            surface_format,
        })
    }
}

pub struct GpuRendererChooserCx<'a> {
    pub gpu_resources: &'a GpuResources,
    pub surface_caps: &'a subduction::wgpu::CompositorSurfaceCapabilities,
}

impl GpuRendererChooserCx<'_> {
    pub fn surface_formats(&self) -> &[wgpu::TextureFormat] {
        &self.surface_caps.formats
    }
}

fn rgba_image_into_image_data(image: RgbaImage) -> ImageData {
    ImageData {
        data: peniko::Blob::new(Arc::new(image.data)),
        format: peniko::ImageFormat::Rgba8,
        width: image.width,
        height: image.height,
        alpha_type: peniko::ImageAlphaType::Alpha,
    }
}

fn read_texture_into_rgba_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
    image: &mut RgbaImage,
) -> Result<(), String> {
    let width = image.width;
    let height = image.height;
    let width_bytes = width.saturating_mul(4);
    let padded_row_bytes = width_bytes.div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("floem inspector compositor capture readback"),
        size: u64::from(padded_row_bytes) * u64::from(height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("floem inspector compositor capture readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|_| "wgpu device poll failed during compositor capture".to_owned())?;
    rx.recv()
        .map_err(|_| "wgpu readback callback dropped during compositor capture".to_owned())?
        .map_err(|_| "wgpu readback buffer map failed during compositor capture".to_owned())?;

    let mapped = slice.get_mapped_range();
    let row_bytes = width_bytes as usize;
    for (source, dest) in mapped
        .chunks_exact(padded_row_bytes as usize)
        .zip(image.data.chunks_exact_mut(row_bytes))
    {
        dest.copy_from_slice(&source[..row_bytes]);
        if matches!(
            format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for pixel in dest.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
    }
    drop(mapped);
    readback.unmap();
    Ok(())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CaptureTiming {
    pub total: Duration,
    pub resize: Duration,
    pub pre_present_notify: Duration,
    pub prepare: Duration,
    pub scene: Duration,
    pub finalize: Duration,
    pub readback: Duration,
    pub convert: Duration,
}

#[derive(Clone, Debug, Default)]
pub struct CaptureOutput {
    pub image: Option<ImageData>,
    pub error: Option<String>,
    pub timing: CaptureTiming,
}

pub(crate) fn capture_source_with_external_images(
    renderer_pool: &SceneFragmentRendererPool,
    gpu_resources: &GpuResources,
    size: Size,
    scene: Scene,
    external_images: ExternalImageResources,
) -> CaptureOutput {
    let total_start = Instant::now();
    let width = size.width.ceil().max(1.0) as u32;
    let height = size.height.ceil().max(1.0) as u32;
    let Some(format) = renderer_pool.compositor_texture_format() else {
        return CaptureOutput {
            error: Some("renderer has no compositor texture format".to_owned()),
            timing: CaptureTiming {
                total: total_start.elapsed(),
                ..Default::default()
            },
            ..Default::default()
        };
    };

    let texture = gpu_resources
        .device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("floem inspector compositor capture target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

    let rendered = renderer_pool.render_for_capture(SceneFragmentRenderJob {
        scene,
        base_transform: peniko::kurbo::Affine::IDENTITY,
        clip: None,
        render_size: size,
        texture: texture.clone(),
        external_images,
    });
    if !rendered {
        return CaptureOutput {
            error: Some("renderer could not render compositor capture texture".to_owned()),
            timing: CaptureTiming {
                total: total_start.elapsed(),
                ..Default::default()
            },
            ..Default::default()
        };
    }

    let readback_start = Instant::now();
    let mut image = RgbaImage::new(width, height);
    let error = read_texture_into_rgba_image(
        &gpu_resources.device,
        &gpu_resources.queue,
        &texture,
        format,
        &mut image,
    )
    .err();
    let readback = readback_start.elapsed();
    CaptureOutput {
        image: error.is_none().then(|| rgba_image_into_image_data(image)),
        error,
        timing: CaptureTiming {
            total: total_start.elapsed(),
            readback,
            ..Default::default()
        },
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct SceneFragmentRenderWorker {
    sender: mpsc::Sender<SceneFragmentRenderCommand>,
    in_flight: Arc<AtomicUsize>,
    join_handle: Option<thread::JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
enum SceneFragmentRenderCommand {
    Render {
        job: SceneFragmentRenderJob,
        completion: SceneFragmentRenderCompletion,
    },
    RenderForCapture {
        job: SceneFragmentRenderJob,
        response: mpsc::Sender<bool>,
    },
    Shutdown,
}

#[cfg(not(target_arch = "wasm32"))]
struct SceneFragmentSource {
    scene: Scene,
    base_transform: peniko::kurbo::Affine,
    clip: Option<peniko::kurbo::RoundedRect>,
    render_size: Size,
}

#[cfg(not(target_arch = "wasm32"))]
impl RenderSource for SceneFragmentSource {
    fn paint_into(&mut self, sink: &mut dyn PaintSink) {
        if let Some(clip) = self.clip {
            crate::paint::display_list::replay_view_clip(
                sink,
                clip,
                self.base_transform,
                self.render_size,
            );
        }
        crate::paint::display_list::replay_scene(
            &self.scene,
            sink,
            self.base_transform,
            self.render_size,
        );
        if self.clip.is_some() {
            sink.pop_clip();
        }
    }
}

pub struct RendererSpec(RendererSpecInner);

enum RendererSpecInner {
    Cpu(CpuRenderer),
    Gpu {
        backend: GpuRenderer,
        surface_format: wgpu::TextureFormat,
    },
}

#[cfg(not(target_arch = "wasm32"))]
enum RendererInit {
    Cpu {
        name: &'static str,
    },
    Gpu {
        name: &'static str,
        surface_format: wgpu::TextureFormat,
    },
}

#[cfg(not(target_arch = "wasm32"))]
impl RendererInit {
    fn from_spec(spec: &RendererSpec) -> Self {
        match &spec.0 {
            RendererSpecInner::Cpu(backend) => Self::Cpu { name: backend.name },
            RendererSpecInner::Gpu {
                backend,
                surface_format,
            } => Self::Gpu {
                name: backend.name,
                surface_format: *surface_format,
            },
        }
    }

    fn name(&self) -> &'static str {
        match self {
            RendererInit::Cpu { name } | RendererInit::Gpu { name, .. } => name,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SceneFragmentRendererPool {
    fn new(chooser: RendererChooser, cx: NewRendererCx) -> Result<Self, String> {
        let worker_count = std::env::var("FLOEM_RENDER_THREADS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(1)
                    .clamp(1, 4)
            })
            .max(1);
        let mut workers = Vec::with_capacity(worker_count);
        let mut init = None;
        for index in 0..worker_count {
            let (worker, worker_init) =
                SceneFragmentRenderWorker::spawn(index, Arc::clone(&chooser), cx.clone())?;
            init.get_or_insert(worker_init);
            workers.push(worker);
        }
        let init = init.expect("at least one renderer worker");
        let compositor_texture_format = match &init {
            RendererInit::Gpu { surface_format, .. } => Some(*surface_format),
            RendererInit::Cpu { .. } => None,
        };
        Ok(Self {
            name: init.name(),
            compositor_texture_format,
            workers,
        })
    }

    pub(crate) fn compositor_texture_format(&self) -> Option<wgpu::TextureFormat> {
        self.compositor_texture_format
    }

    pub(crate) fn debug_info(&self) -> String {
        format!(
            "Renderer: {} (scene fragment pool, workers={})",
            self.name,
            self.workers.len()
        )
    }

    pub(crate) fn submit(
        &self,
        job: SceneFragmentRenderJob,
        completion: SceneFragmentRenderCompletion,
    ) -> bool {
        let Some(worker) = self.least_loaded_worker() else {
            return false;
        };
        worker.submit(job, completion)
    }

    fn render_for_capture(&self, job: SceneFragmentRenderJob) -> bool {
        let Some(worker) = self.least_loaded_worker() else {
            return false;
        };
        worker.render_for_capture(job)
    }

    fn least_loaded_worker(&self) -> Option<&SceneFragmentRenderWorker> {
        self.workers
            .iter()
            .min_by_key(|worker| worker.in_flight.load(Ordering::Relaxed))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for SceneFragmentRendererPool {
    fn drop(&mut self) {
        for worker in &mut self.workers {
            worker.shutdown();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SceneFragmentRenderWorker {
    fn spawn(
        index: usize,
        chooser: RendererChooser,
        cx: NewRendererCx,
    ) -> Result<(Self, RendererInit), String> {
        let (command_tx, command_rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::channel();
        let in_flight = Arc::new(AtomicUsize::new(0));
        let worker_in_flight = Arc::clone(&in_flight);
        let join_handle = thread::Builder::new()
            .name(format!("floem-render-pool-{index}"))
            .spawn(move || {
                let mut backend = chooser(cx);
                let init = RendererInit::from_spec(&backend);
                if init_tx.send(init).is_err() {
                    return;
                }
                while let Ok(command) = command_rx.recv() {
                    match command {
                        SceneFragmentRenderCommand::Render { job, completion } => {
                            let render_start = Instant::now();
                            let mut source = SceneFragmentSource {
                                scene: job.scene,
                                base_transform: job.base_transform,
                                clip: job.clip,
                                render_size: job.render_size,
                            };
                            let rendered = render_into_existing_texture_with_external_images(
                                &mut backend,
                                &mut source,
                                job.render_size,
                                &job.texture,
                                job.external_images,
                            );
                            let render_end = Instant::now();
                            Application::send_proxy_event(UserEvent::SceneFragmentReady {
                                window_id: completion.window_id,
                                key: completion.key,
                                signature: completion.signature,
                                rendered,
                                worker_index: index,
                                render_start,
                                render_end,
                            });
                            worker_in_flight.fetch_sub(1, Ordering::Relaxed);
                        }
                        SceneFragmentRenderCommand::RenderForCapture { job, response } => {
                            let mut source = SceneFragmentSource {
                                scene: job.scene,
                                base_transform: job.base_transform,
                                clip: job.clip,
                                render_size: job.render_size,
                            };
                            let rendered = render_into_existing_texture_with_external_images(
                                &mut backend,
                                &mut source,
                                job.render_size,
                                &job.texture,
                                job.external_images,
                            );
                            let _ = response.send(rendered);
                            worker_in_flight.fetch_sub(1, Ordering::Relaxed);
                        }
                        SceneFragmentRenderCommand::Shutdown => break,
                    }
                }
            })
            .expect("failed to spawn render worker");
        let init = init_rx
            .recv()
            .map_err(|_| "render worker thread stopped during initialization".to_string())?;
        Ok((
            Self {
                sender: command_tx,
                in_flight,
                join_handle: Some(join_handle),
            },
            init,
        ))
    }

    fn submit(
        &self,
        job: SceneFragmentRenderJob,
        completion: SceneFragmentRenderCompletion,
    ) -> bool {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        let sent = self
            .sender
            .send(SceneFragmentRenderCommand::Render { job, completion })
            .is_ok();
        if !sent {
            self.in_flight.fetch_sub(1, Ordering::Relaxed);
            return false;
        }
        true
    }

    fn render_for_capture(&self, job: SceneFragmentRenderJob) -> bool {
        let (response_tx, response_rx) = mpsc::channel();
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        let sent = self
            .sender
            .send(SceneFragmentRenderCommand::RenderForCapture {
                job,
                response: response_tx,
            })
            .is_ok();
        if !sent {
            self.in_flight.fetch_sub(1, Ordering::Relaxed);
            return false;
        }
        response_rx.recv().unwrap_or(false)
    }

    fn shutdown(&mut self) {
        let _ = self.sender.send(SceneFragmentRenderCommand::Shutdown);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

struct CpuRenderer {
    name: &'static str,
}

impl CpuRenderer {
    fn new(_backend: impl ImageRenderer + 'static, name: &'static str) -> Self {
        Self { name }
    }
}

struct GpuRenderer {
    backend: GpuRendererBackend,
    name: &'static str,
}

#[allow(
    dead_code,
    reason = "Some GPU backend variants are only constructed when optional renderers are enabled."
)]
enum GpuRendererBackend {
    Texture(Box<dyn TextureRenderer<TextureTarget = wgpu::Texture, Texture = wgpu::Texture>>),
    TextureView(
        Box<dyn TextureRenderer<TextureTarget = TextureViewTarget, Texture = wgpu::Texture>>,
    ),
}

impl GpuRenderer {
    #[allow(
        dead_code,
        reason = "Texture-target GPU constructors are only used when optional renderers are enabled."
    )]
    fn provided_texture(
        backend: impl TextureRenderer<TextureTarget = wgpu::Texture, Texture = wgpu::Texture> + 'static,
        _device: wgpu::Device,
        name: &'static str,
    ) -> Self {
        Self {
            backend: GpuRendererBackend::Texture(Box::new(backend)),
            name,
        }
    }

    #[allow(
        dead_code,
        reason = "Texture-target GPU constructors are only used when optional renderers are enabled."
    )]
    fn owned_texture(
        backend: impl TextureRenderer<TextureTarget = wgpu::Texture, Texture = wgpu::Texture> + 'static,
        _device: wgpu::Device,
        name: &'static str,
    ) -> Self {
        Self {
            backend: GpuRendererBackend::Texture(Box::new(backend)),
            name,
        }
    }

    fn provided_texture_view(
        backend: impl TextureRenderer<TextureTarget = TextureViewTarget, Texture = wgpu::Texture>
        + 'static,
        _device: wgpu::Device,
        name: &'static str,
    ) -> Self {
        Self {
            backend: GpuRendererBackend::TextureView(Box::new(backend)),
            name,
        }
    }

    fn owned_texture_view(
        backend: impl TextureRenderer<TextureTarget = TextureViewTarget, Texture = wgpu::Texture>
        + 'static,
        _device: wgpu::Device,
        name: &'static str,
    ) -> Self {
        Self {
            backend: GpuRendererBackend::TextureView(Box::new(backend)),
            name,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn render_into_existing_texture_with_external_images(
    backend: &mut RendererSpec,
    source: &mut dyn RenderSource,
    size: Size,
    texture: &wgpu::Texture,
    external_images: ExternalImageResources,
) -> bool {
    let width = size.width.max(1.0) as u32;
    let height = size.height.max(1.0) as u32;
    let mut resolver = ExternalImageResourceResolver::new(external_images);
    match &mut backend.0 {
        RendererSpecInner::Cpu(_) => false,
        RendererSpecInner::Gpu { backend, .. } => match &mut backend.backend {
            GpuRendererBackend::Texture(renderer) => renderer
                .render_source_into_texture_with_external_images(
                    source,
                    texture.clone(),
                    &mut resolver,
                )
                .is_ok(),
            GpuRendererBackend::TextureView(renderer) => {
                let view = texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("floem render existing texture external-image target view"),
                    ..Default::default()
                });
                renderer
                    .render_source_into_texture_with_external_images(
                        source,
                        TextureViewTarget::new(&view, width, height),
                        &mut resolver,
                    )
                    .is_ok()
            }
        },
    }
}

fn env_flag_requested(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| value.as_str() == "1")
}

pub(crate) fn force_cpu_requested() -> bool {
    env_flag_requested("FLOEM_FORCE_CPU") || env_flag_requested("FLOEM_FORCE_TINY_SKIA")
}

#[allow(
    dead_code,
    reason = "Used only when a GPU renderer feature with provided texture targets is enabled."
)]
fn pick_supported_texture_format(
    surface_formats: &[wgpu::TextureFormat],
    renderer_formats: &[wgpu::TextureFormat],
) -> Option<wgpu::TextureFormat> {
    surface_formats
        .iter()
        .copied()
        .find(|format| renderer_formats.contains(format))
}

fn is_srgb_texture_format(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
    )
}

fn choose_default_renderer(cx: NewRendererCx) -> Result<RendererSpec, String> {
    #[allow(
        unreachable_code,
        reason = "Some feature combinations end the chooser earlier with a concrete fallback renderer."
    )]
    {
        #[cfg(feature = "vello")]
        if let Some(gpu) = cx.gpu() {
            let fallback_surface_format = gpu
                .surface_formats()
                .iter()
                .copied()
                .find(|format| !is_srgb_texture_format(*format));
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            let backend =
                imaging_vello::VelloRenderer::new(device, queue).map_err(|err| err.to_string())?;
            if let Some(surface_format) = pick_supported_texture_format(
                gpu.surface_formats(),
                &backend.supported_texture_formats(),
            ) {
                return Ok(cx.provided_texture_view_renderer(backend, surface_format, "Vello GPU"));
            }
            if let Some(surface_format) = fallback_surface_format {
                return Ok(cx.owned_texture_view_renderer(backend, surface_format, "Vello GPU"));
            }
        }

        #[cfg(feature = "vger")]
        if let Some(gpu) = cx.gpu() {
            let fallback_surface_format = gpu
                .surface_formats()
                .iter()
                .copied()
                .find(|format| !is_srgb_texture_format(*format));
            let adapter = gpu.gpu_resources.adapter.clone();
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            let width = cx.size.width.max(1.0) as u32;
            let height = cx.size.height.max(1.0) as u32;
            let backend =
                floem_vger_renderer::VgerRenderer::new(adapter, device, queue, width, height)
                    .map_err(|err| err.to_string())?;
            if let Some(surface_format) = pick_supported_texture_format(
                gpu.surface_formats(),
                &backend.supported_texture_formats(),
            ) {
                return Ok(cx.provided_texture_view_renderer(backend, surface_format, "Vger GPU"));
            }
            if let Some(surface_format) = fallback_surface_format {
                return Ok(cx.owned_texture_view_renderer(backend, surface_format, "Vger GPU"));
            }
        }

        #[cfg(feature = "skia")]
        if let Some(gpu) = cx.gpu() {
            let fallback_surface_format = gpu
                .surface_formats()
                .iter()
                .copied()
                .find(|format| !(is_srgb_texture_format(*format)));
            let adapter = gpu.gpu_resources.adapter.clone();
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            let backend = imaging_skia::SkiaRenderer::new(adapter, device, queue)
                .map_err(|err| err.to_string())?;
            if let Some(surface_format) = pick_supported_texture_format(
                gpu.surface_formats(),
                &backend.supported_texture_formats(),
            ) {
                return Ok(cx.provided_texture_renderer(backend, surface_format, "Skia GPU"));
            }
            if let Some(surface_format) = fallback_surface_format {
                return Ok(cx.owned_texture_renderer(backend, surface_format, "Skia GPU"));
            }
        }

        #[cfg(feature = "vello-hybrid")]
        if let Some(gpu) = cx.gpu() {
            let fallback_surface_format = gpu
                .surface_formats()
                .iter()
                .copied()
                .find(|format| !is_srgb_texture_format(*format));
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            let backend = imaging_vello_hybrid::VelloHybridRenderer::new(device, queue);
            if let Some(surface_format) = pick_supported_texture_format(
                gpu.surface_formats(),
                &backend.supported_texture_formats(),
            ) {
                return Ok(cx.provided_texture_view_renderer(
                    backend,
                    surface_format,
                    "Vello Hybrid GPU",
                ));
            }
            if let Some(surface_format) = fallback_surface_format {
                return Ok(cx.owned_texture_view_renderer(
                    backend,
                    surface_format,
                    "Vello Hybrid GPU",
                ));
            }
        }

        #[cfg(feature = "vello-cpu")]
        {
            let width = u16::try_from(cx.size.width.max(1.0) as u32)
                .map_err(|_| "width exceeds vello cpu limit".to_string())?;
            let height = u16::try_from(cx.size.height.max(1.0) as u32)
                .map_err(|_| "height exceeds vello cpu limit".to_string())?;
            let backend = imaging_vello_cpu::VelloCpuRenderer::new(width, height);
            return Ok(cx.image_renderer(backend, "Vello CPU"));
        }

        #[cfg(feature = "skia-cpu")]
        {
            let backend = imaging_skia::SkiaCpuRenderer::new();
            return Ok(cx.image_renderer(backend, "Skia CPU"));
        }

        #[cfg(feature = "tiny-skia")]
        {
            let width = cx.size.width.max(1.0) as u32;
            let height = cx.size.height.max(1.0) as u32;
            let backend = imaging_tiny_skia::TinySkiaRenderer::new_with_size(width, height)
                .map_err(|err| err.to_string())?;
            return Ok(cx.image_renderer(backend, "Tiny Skia CPU"));
        }

        #[cfg(feature = "vello-cpu")]
        {
            let width = u16::try_from(cx.size.width.max(1.0) as u32)
                .map_err(|_| "width exceeds vello_cpu limit".to_string())?;
            let height = u16::try_from(cx.size.height.max(1.0) as u32)
                .map_err(|_| "height exceeds vello_cpu limit".to_string())?;
            let backend = imaging_vello_cpu::VelloCpuRenderer::new(width, height);
            return Ok(cx.image_renderer(backend, "Vello CPU"));
        }

        Err("no renderer available for this window target".to_string())
    }
}

pub(crate) fn default_renderer() -> RendererChooser {
    Arc::new(|cx| choose_default_renderer(cx).expect("create renderer"))
}
