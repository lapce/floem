//! Shared renderer-facing types for the Floem UI framework.
//!
//! # Modules
//!
//! - [`text`] — Text layout, shaping, and font management built on [Parley](https://docs.rs/parley).
//! - [`gpu_resources`] — Asynchronous wgpu adapter/device acquisition for GPU backends.
//!
//! # Re-exports
//!
//! [`tiny_skia`] and [`usvg`] are re-exported from [`resvg`](https://docs.rs/resvg) so that
//! renderer backends and downstream crates can use consistent versions of these libraries
//! without adding them as direct dependencies.

pub mod text;
pub use resvg::tiny_skia;
pub use resvg::usvg;

pub mod gpu_resources;
pub use imaging::{
    BeginFrame, CpuBufferFormat, CpuBufferTarget, CpuBufferTargetInfo, GpuTextureTarget,
    RenderCore, Renderer, RenderOutput, TargetRenderer,
};
