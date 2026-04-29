use std::sync::Arc;

use subduction::wgpu::SurfaceColorSpace;

/// Stable identifier for a reusable compositor effect program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColorEffectId(pub u64);

/// A SwiftUI-style color/layer effect applied to an isolated compositor subtree.
///
/// The effect is evaluated over an input texture containing the already-rendered subtree. Backends
/// expose the input texture and sampler to the shader, so the shader may either use the pre-sampled
/// `color` value or sample the input texture at another `uv`.
///
/// Effect shaders are written in logical window coordinates by default: the `position` argument
/// passed to `color_effect` is measured in logical pixels, not framebuffer pixels. Use
/// `frame.effective_scale` to convert between logical and physical pixels when needed. `uv` is
/// normalized texture space with `(0, 0)` at the top-left and `(1, 1)` at the bottom-right.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorEffect {
    pub id: ColorEffectId,
    pub shader: ColorEffectShader,
    pub args: ColorEffectArgs,
    pub color_space: SurfaceColorSpace,
}

impl ColorEffect {
    #[must_use]
    pub fn wgsl(id: ColorEffectId, fragment_body: impl Into<Arc<str>>) -> Self {
        Self {
            id,
            shader: ColorEffectShader::Wgsl {
                label: None,
                fragment_body: fragment_body.into(),
            },
            args: ColorEffectArgs::default(),
            color_space: SurfaceColorSpace::ExtendedLinearSrgb,
        }
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<Arc<str>>) -> Self {
        match &mut self.shader {
            ColorEffectShader::Wgsl { label: slot, .. } => *slot = Some(label.into()),
        }
        self
    }

    #[must_use]
    pub fn with_args(mut self, args: impl Into<Vec<u8>>) -> Self {
        self.args = ColorEffectArgs { bytes: args.into() };
        self
    }

    #[must_use]
    pub fn with_color_space(mut self, color_space: SurfaceColorSpace) -> Self {
        self.color_space = color_space;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ColorEffectShader {
    /// WGSL function body for the generated `color_effect` function.
    ///
    /// The generated wrapper provides:
    ///
    /// ```wgsl
    /// @group(0) @binding(0) var input_texture: texture_2d<f32>;
    /// @group(0) @binding(1) var input_sampler: sampler;
    /// @group(0) @binding(2) var<uniform> args: ColorEffectArgs;
    /// @group(0) @binding(3) var<uniform> frame: ColorEffectFrame;
    ///
    /// `frame.effective_scale` converts between logical and framebuffer pixels.
    /// `frame.target_width` and `frame.target_height` are the render target size in logical
    /// pixels.
    ///
    /// fn color_effect(
    ///     position: vec2<f32>, // logical pixels, top-left origin
    ///     uv: vec2<f32>,
    ///     color: vec4<f32>,
    ///     args: ColorEffectArgs,
    ///     frame: ColorEffectFrame,
    /// ) -> vec4<f32> {
    ///     // fragment_body
    /// }
    /// ```
    Wgsl {
        label: Option<Arc<str>>,
        fragment_body: Arc<str>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColorEffectArgs {
    pub bytes: Vec<u8>,
}

/// Uniform values made available to effect shaders from frame timing.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColorEffectFrameUniform {
    pub time_seconds: f32,
    pub delta_seconds: f32,
    pub frame_index: u32,
    pub _pad0: u32,
    pub effective_scale: f32,
    pub target_width: f32,
    pub target_height: f32,
    pub _pad1: f32,
}
