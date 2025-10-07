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
use crossbeam::channel::{bounded as sync_channel, Receiver};
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::{sync_channel, Receiver};
use wgpu::Backends;

use winit::window::{Window, WindowId};

/// The acquired GPU resources needed for rendering with wgpu.
#[derive(Debug, Clone)]
pub struct GpuResources {
    /// The wgpu instance
    pub instance: wgpu::Instance,

    /// The adapter that represents the GPU or a rendering backend. It provides information about
    /// the capabilities of the hardware and is used to request a logical device (`wgpu::Device`).
    pub adapter: wgpu::Adapter,

    /// The logical device that serves as an interface to the GPU. It is responsible for creating
    /// resources such as buffers, textures, and pipelines, and manages the execution of commands.
    /// The `device` provides a connection to the physical hardware represented by the `adapter`.
    pub device: wgpu::Device,

    /// The command queue that manages the submission of command buffers to the GPU for execution.
    /// It is used to send rendering and computation commands to the device. The `queue` ensures
    /// that commands are executed in the correct order and manages synchronization.
    pub queue: wgpu::Queue,
}

impl GpuResources {
    /// Request GPU resources
    ///
    /// # Parameters
    /// - `on_result`: Function to notify upon completion or error.
    /// - `window`: The window to associate with the created surface.
    pub fn request<F: Fn(WindowId) + 'static>(
        on_result: F,
        required_features: wgpu::Features,
        window: Arc<dyn Window>,
    ) -> Receiver<Result<(Self, wgpu::Surface<'static>), GpuResourceError>> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: Backends::from_env().unwrap_or(Backends::all()),
            ..Default::default()
        });
        // Channel passing to do async out-of-band within the winit event_loop since wasm can't
        // execute futures with a return value
        let (tx, rx) = sync_channel(1);

        spawn({
            async move {
                let surface = match instance.create_surface(Arc::clone(&window)) {
                    Ok(surface) => surface,
                    Err(err) => {
                        tx.send(Err(GpuResourceError::SurfaceCreationError(err)))
                            .unwrap();
                        on_result(window.id());
                        return;
                    }
                };

                let Ok(adapter) = instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::default(),
                        compatible_surface: Some(&surface),
                        force_fallback_adapter: false,
                    })
                    .await
                else {
                    tx.send(Err(GpuResourceError::AdapterNotFoundError))
                        .unwrap();
                    on_result(window.id());
                    return;
                };

                tx.send(
                    adapter
                        .request_device(&wgpu::DeviceDescriptor {
                            label: None,
                            required_features,
                            ..Default::default()
                        })
                        .await
                        .map_err(GpuResourceError::DeviceRequestError)
                        .map(|(device, queue)| Self {
                            adapter,
                            device,
                            queue,
                            instance,
                        })
                        .map(|res| (res, surface)),
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
    SurfaceCreationError(wgpu::CreateSurfaceError),
    AdapterNotFoundError,
    DeviceRequestError(wgpu::RequestDeviceError),
}

impl std::fmt::Display for GpuResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuResourceError::SurfaceCreationError(err) => {
                write!(f, "Surface creation error: {err}")
            }
            GpuResourceError::AdapterNotFoundError => {
                write!(f, "Failed to find a suitable GPU adapter")
            }
            GpuResourceError::DeviceRequestError(err) => write!(f, "Device request error: {err}"),
        }
    }
}

/// Spawns a future for execution, adapting to the target environment.
///
/// On WASM (`wasm32`), it uses `wasm_bindgen_futures::spawn_local` to avoid blocking
/// the main thread. On other targets, it uses `pollster::block_on` to synchronously
/// wait for the future to complete.
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(future);
    #[cfg(not(target_arch = "wasm32"))]
    futures::executor::block_on(future)
}
