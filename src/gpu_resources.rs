//! Asynchronous GPU resource acquisition for rendering with wgpu.
//!
//! To support WebGPU on WASM, the GPU resources need to be acquired asynchronously because
//! the wgpu library provides only asynchronous methods for requesting adapters and devices.
//! In WASM, blocking the main thread is not an option, as the JavaScript
//! execution model does not support thread blocking. Consequently, we must use asynchronous
//! execution (via `wasm_bindgen_futures`) to handle these operations.
//!
//! Based on a [code snippet by Luke Petherbridge](https://github.com/rust-windowing/winit/issues/3560#issuecomment-2085754164).

use std::{future::Future, sync::Arc};

#[cfg(feature = "crossbeam")]
use crossbeam::channel::{Receiver, bounded as sync_channel};
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::{Receiver, sync_channel};
use wgpu::{Backends, InstanceFlags};

use winit::window::{Window, WindowId};

/// The acquired GPU resources needed for rendering with wgpu.
#[derive(Debug, Clone)]
pub struct GpuResources {
    /// The wgpu instance
    pub instance: wgpu::Instance,

    /// The adapter that represents the GPU or a rendering backend.
    pub adapter: wgpu::Adapter,

    /// The logical device that serves as an interface to the GPU.
    pub device: wgpu::Device,

    /// The command queue that manages the submission of command buffers to the GPU.
    pub queue: wgpu::Queue,
}

impl GpuResources {
    /// Request GPU resources.
    pub fn request<F: Fn(WindowId) + 'static>(
        on_result: F,
        required_features: wgpu::Features,
        backends: Option<Backends>,
        window: Arc<dyn Window>,
    ) -> Receiver<Result<(Self, subduction::wgpu::ExternalSurfaceCapabilities), GpuResourceError>>
    {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: Backends::from_env().or(backends).unwrap_or(Backends::all()),
            flags: InstanceFlags::from_env_or_default(),
            ..Default::default()
        });
        let (tx, rx) = sync_channel(1);

        spawn({
            async move {
                let Ok(adapter) = instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::default(),
                        compatible_surface: None,
                        force_fallback_adapter: false,
                    })
                    .await
                else {
                    tx.send(Err(GpuResourceError::AdapterNotFoundError))
                        .unwrap();
                    on_result(window.id());
                    return;
                };

                let surface_caps = subduction::wgpu::ExternalSurfaceCapabilities {
                    formats: vec![
                        wgpu::TextureFormat::Rgba8Unorm,
                        wgpu::TextureFormat::Bgra8Unorm,
                    ],
                    color_spaces: vec![subduction::wgpu::SurfaceColorSpace::Srgb],
                    dynamic_ranges: vec![subduction::wgpu::SurfaceDynamicRange::Standard],
                    alpha_modes: vec![
                        wgpu::CompositeAlphaMode::PreMultiplied,
                        wgpu::CompositeAlphaMode::Opaque,
                    ],
                    usages: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    max_size: None,
                    supports_frame_opportunities: true,
                    supports_render_targets: true,
                    supports_submitted_textures: true,
                };
                let request_result = adapter
                    .request_device(&wgpu::DeviceDescriptor {
                        label: Some("floem shared wgpu device"),
                        required_features,
                        ..Default::default()
                    })
                    .await;
                tx.send(
                    request_result
                        .map_err(GpuResourceError::DeviceRequestError)
                        .map(|(device, queue)| {
                            (
                                Self {
                                    adapter,
                                    device,
                                    queue,
                                    instance,
                                },
                                surface_caps,
                            )
                        }),
                )
                .unwrap();
                on_result(window.id());
            }
        });
        rx
    }
}

/// Possible errors during GPU resource setup.
#[derive(Debug)]
pub enum GpuResourceError {
    AdapterNotFoundError,
    DeviceRequestError(wgpu::RequestDeviceError),
}

impl std::fmt::Display for GpuResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuResourceError::AdapterNotFoundError => {
                write!(f, "Failed to find a suitable GPU adapter")
            }
            GpuResourceError::DeviceRequestError(err) => write!(f, "Device request error: {err}"),
        }
    }
}

/// Spawns a future for execution, adapting to the target environment.
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(future);
    #[cfg(not(target_arch = "wasm32"))]
    futures::executor::block_on(future)
}
