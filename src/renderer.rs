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
use std::sync::Arc;

use crate::kurbo::Point;
use floem_renderer::Img;
use floem_renderer::gpu_resources::GpuResources;
use floem_renderer::text::LayoutRun;
#[cfg(feature = "tiny_skia")]
use floem_tiny_skia_renderer::TinySkiaRenderer;
#[cfg(feature = "vello_cpu")]
use floem_vello_cpu_renderer::VelloCpuRenderer;
#[cfg(feature = "vello")]
use floem_vello_renderer::VelloRenderer;
#[cfg(feature = "vger")]
use floem_vger_renderer::VgerRenderer;
use peniko::BrushRef;
use peniko::kurbo::{Affine, Rect, Shape, Size, Stroke};
use winit::window::Window;

/// Enum for selecting a specific renderer at runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RendererKind {
    /// High-performance GPU renderer with full feature support
    Vello,
    /// CPU-based renderer with full feature support, good for compatibility
    VelloCpu,
    /// Simple GPU renderer with limited features but good compatibility
    Vger,
    /// CPU-based renderer with good feature support, performance not as good as `VelloCpu`
    TinySkia,
    /// Auto-select based on availability and performance
    #[default]
    Auto,
}

impl RendererKind {
    /// Get all available renderer kinds based on enabled features
    pub fn available() -> Vec<Self> {
        let mut available = vec![Self::Auto];

        #[cfg(feature = "vello")]
        available.push(Self::Vello);

        #[cfg(feature = "vello_cpu")]
        available.push(Self::VelloCpu);

        #[cfg(feature = "vger")]
        available.push(Self::Vger);

        #[cfg(feature = "tiny_skia")]
        available.push(Self::TinySkia);

        available
    }

    /// Check if this renderer kind is available (feature is enabled)
    pub fn is_available(self) -> bool {
        match self {
            Self::Auto => true,
            #[cfg(feature = "vello")]
            Self::Vello => true,
            #[cfg(not(feature = "vello"))]
            Self::Vello => false,
            #[cfg(feature = "vello_cpu")]
            Self::VelloCpu => true,
            #[cfg(not(feature = "vello_cpu"))]
            Self::VelloCpu => false,
            #[cfg(feature = "vger")]
            Self::Vger => true,
            #[cfg(not(feature = "vger"))]
            Self::Vger => false,
            #[cfg(feature = "tiny_skia")]
            Self::TinySkia => true,
            #[cfg(not(feature = "tiny_skia"))]
            Self::TinySkia => false,
        }
    }

    /// Whether this renderer has full feature support (vs simple renderer)
    pub fn is_full_featured(self) -> bool {
        match self {
            Self::Auto => true, // Auto will prefer full-featured renderers
            Self::Vello | Self::VelloCpu | Self::TinySkia => true,
            Self::Vger => false,
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum Renderer {
    #[cfg(feature = "vello")]
    Vello(VelloRenderer),
    #[cfg(feature = "vello_cpu")]
    VelloCpu(VelloCpuRenderer<Arc<dyn Window>>),
    #[cfg(feature = "vger")]
    Vger(VgerRenderer),
    #[cfg(feature = "tiny_skia")]
    TinySkia(TinySkiaRenderer<Arc<dyn Window>>),
    /// Uninitialized renderer, used to allow the renderer to be created lazily
    /// All operations on this renderer are no-ops
    Uninitialized { scale: f64, size: Size },
}

impl Renderer {
    /// Create a new renderer, trying renderers in preference order
    pub fn new(
        window: Arc<dyn Window>,
        gpu_resources: GpuResources,
        surface: wgpu::Surface<'static>,
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        Self::new_with_kind(
            RendererKind::Auto,
            window,
            gpu_resources,
            surface,
            scale,
            size,
            font_embolden,
        )
    }

    /// Create a new renderer with a specific kind preference
    pub fn new_with_kind(
        kind: RendererKind,
        window: Arc<dyn Window>,
        gpu_resources: GpuResources,
        surface: wgpu::Surface<'static>,
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        let size = Size::new(size.width.max(1.0), size.height.max(1.0));

        // Check for environment variable overrides
        let env_override = if std::env::var("FLOEM_FORCE_VELLO")
            .ok()
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            Some(RendererKind::Vello)
        } else if std::env::var("FLOEM_FORCE_VGER")
            .ok()
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            Some(RendererKind::Vger)
        } else if std::env::var("FLOEM_FORCE_VELLO_CPU")
            .ok()
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            Some(RendererKind::VelloCpu)
        } else if std::env::var("FLOEM_FORCE_TINY_SKIA")
            .ok()
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            Some(RendererKind::TinySkia)
        } else {
            None
        };

        let preferred_kind = env_override.unwrap_or(kind);
        let mut errors = Vec::new();

        // Determine the order to try renderers
        let try_order = if preferred_kind != RendererKind::Auto {
            // Try preferred first, then fallback order
            let mut order = vec![preferred_kind];
            for &kind in &[
                RendererKind::Vello,
                RendererKind::Vger,
                RendererKind::VelloCpu,
                RendererKind::TinySkia,
            ] {
                if kind != preferred_kind {
                    order.push(kind);
                }
            }
            order
        } else {
            vec![
                RendererKind::Vello,
                RendererKind::Vger,
                RendererKind::VelloCpu,
                RendererKind::TinySkia,
            ]
        };

        let gpu_renderers = [RendererKind::Vello, RendererKind::Vger];
        let cpu_renderers = [RendererKind::VelloCpu, RendererKind::TinySkia];

        let mut surface_option = Some(surface);

        // Try renderers in the preferred order, respecting surface consumption
        for &renderer_kind in &try_order {
            if !renderer_kind.is_available() {
                continue;
            }

            if gpu_renderers.contains(&renderer_kind) {
                // GPU renderer - needs surface
                if let Some(surf) = surface_option.take() {
                    match Self::try_create_renderer_gpu(
                        renderer_kind,
                        window.clone(),
                        gpu_resources.clone(),
                        surf,
                        size,
                        scale,
                        font_embolden,
                    ) {
                        Ok(renderer) => return renderer,
                        Err(err) => {
                            // Surface is consumed on failure, can't try other GPU renderers
                            if env_override == Some(renderer_kind) {
                                panic!(
                                    "Failed to create forced renderer {:?}: {}",
                                    renderer_kind, err
                                );
                            }
                            errors.push(format!("{:?}: {}", renderer_kind, err));
                        }
                    }
                } else {
                    errors.push(format!(
                        "{:?}: surface already consumed by previous GPU renderer",
                        renderer_kind
                    ));
                }
            } else if cpu_renderers.contains(&renderer_kind) {
                // CPU renderer - doesn't need surface
                match Self::try_create_renderer_cpu(
                    renderer_kind,
                    window.clone(),
                    size,
                    scale,
                    font_embolden,
                ) {
                    Ok(renderer) => return renderer,
                    Err(err) => {
                        if env_override == Some(renderer_kind) {
                            panic!(
                                "Failed to create forced renderer {:?}: {}",
                                renderer_kind, err
                            );
                        }
                        errors.push(format!("{:?}: {}", renderer_kind, err));
                    }
                }
            }
        }

        panic!("Failed to create any renderer:\n{}", errors.join("\n"));
    }

    /// Helper method to try creating a GPU renderer (consumes surface)
    fn try_create_renderer_gpu(
        kind: RendererKind,
        _window: Arc<dyn Window>,
        gpu_resources: GpuResources,
        surface: wgpu::Surface<'static>,
        size: Size,
        scale: f64,
        font_embolden: f32,
    ) -> Result<Self, String> {
        match kind {
            #[cfg(feature = "vello")]
            RendererKind::Vello => VelloRenderer::new(
                gpu_resources,
                surface,
                size.width as u32,
                size.height as u32,
                scale,
                font_embolden,
            )
            .map(Self::Vello)
            .map_err(|e| e.to_string()),
            #[cfg(not(feature = "vello"))]
            RendererKind::Vello => {
                Err("Vello renderer not available (feature disabled)".to_string())
            }

            #[cfg(feature = "vger")]
            RendererKind::Vger => VgerRenderer::new(
                gpu_resources,
                surface,
                size.width as u32,
                size.height as u32,
                scale,
                font_embolden,
            )
            .map(Self::Vger)
            .map_err(|e| e.to_string()),
            #[cfg(not(feature = "vger"))]
            RendererKind::Vger => Err("Vger renderer not available (feature disabled)".to_string()),

            _ => Err(format!("Renderer {:?} is not a GPU renderer", kind)),
        }
    }

    /// Helper method to try creating a CPU renderer
    fn try_create_renderer_cpu(
        kind: RendererKind,
        window: Arc<dyn Window>,
        size: Size,
        scale: f64,
        font_embolden: f32,
    ) -> Result<Self, String> {
        match kind {
            #[cfg(feature = "vello_cpu")]
            RendererKind::VelloCpu => VelloCpuRenderer::new(
                window,
                size.width as u32,
                size.height as u32,
                scale,
                font_embolden,
            )
            .map(Self::VelloCpu)
            .map_err(|e| e.to_string()),
            #[cfg(not(feature = "vello_cpu"))]
            RendererKind::VelloCpu => {
                Err("VelloCpu renderer not available (feature disabled)".to_string())
            }

            #[cfg(feature = "tiny_skia")]
            RendererKind::TinySkia => TinySkiaRenderer::new(
                window,
                size.width as u32,
                size.height as u32,
                scale,
                font_embolden,
            )
            .map(Self::TinySkia)
            .map_err(|e| e.to_string()),
            #[cfg(not(feature = "tiny_skia"))]
            RendererKind::TinySkia => {
                Err("TinySkia renderer not available (feature disabled)".to_string())
            }

            _ => Err(format!("Renderer {:?} is not a CPU renderer", kind)),
        }
    }

    /// Get the kind of renderer currently active
    pub fn kind(&self) -> RendererKind {
        match self {
            #[cfg(feature = "vello")]
            Self::Vello(_) => RendererKind::Vello,
            #[cfg(feature = "vello_cpu")]
            Self::VelloCpu(_) => RendererKind::VelloCpu,
            #[cfg(feature = "vger")]
            Self::Vger(_) => RendererKind::Vger,
            #[cfg(feature = "tiny_skia")]
            Self::TinySkia(_) => RendererKind::TinySkia,
            Self::Uninitialized { .. } => RendererKind::Auto, // Not yet initialized
        }
    }

    pub fn resize(&mut self, scale: f64, size: Size) {
        let size = Size::new(size.width.max(1.0), size.height.max(1.0));
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(r) => r.resize(size.width as u32, size.height as u32, scale),
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(r) => r.resize(size.width as u32, size.height as u32, scale),
            #[cfg(feature = "vger")]
            Renderer::Vger(r) => r.resize(size.width as u32, size.height as u32, scale),
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(r) => r.resize(size.width as u32, size.height as u32, scale),
            Renderer::Uninitialized { .. } => {}
        }
    }

    pub fn set_scale(&mut self, scale: f64) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(r) => r.set_scale(scale),
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(r) => r.set_scale(scale),
            #[cfg(feature = "vger")]
            Renderer::Vger(r) => r.set_scale(scale),
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(r) => r.set_scale(scale),
            Renderer::Uninitialized {
                scale: old_scale, ..
            } => {
                *old_scale = scale;
            }
        }
    }

    pub fn scale(&self) -> f64 {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(r) => r.scale(),
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(r) => r.scale(),
            #[cfg(feature = "vger")]
            Renderer::Vger(r) => r.scale(),
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(r) => r.scale(),
            Renderer::Uninitialized { scale, .. } => *scale,
        }
    }

    pub fn size(&self) -> Size {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(r) => r.size(),
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(r) => r.size(),
            #[cfg(feature = "vger")]
            Renderer::Vger(r) => r.size(),
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(r) => r.size(),
            Renderer::Uninitialized { size, .. } => *size,
        }
    }

    pub(crate) fn debug_info(&self) -> String {
        use crate::Renderer;

        match self {
            #[cfg(feature = "vello")]
            Self::Vello(r) => r.debug_info(),
            #[cfg(feature = "vello_cpu")]
            Self::VelloCpu(r) => r.debug_info(),
            #[cfg(feature = "vger")]
            Self::Vger(r) => r.debug_info(),
            #[cfg(feature = "tiny_skia")]
            Self::TinySkia(r) => r.debug_info(),
            Self::Uninitialized { .. } => "Uninitialized".to_string(),
        }
    }
}

impl floem_renderer::Renderer for Renderer {
    fn begin(&mut self, capture: bool) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(r) => {
                r.begin(capture);
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(r) => {
                r.begin(capture);
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(r) => {
                r.begin(capture);
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(r) => {
                r.begin(capture);
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn clip(&mut self, shape: &impl Shape) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.clip(shape);
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.clip(shape);
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.clip(shape);
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => {
                v.clip(shape);
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn clear_clip(&mut self) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.clear_clip();
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.clear_clip();
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.clear_clip();
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => {
                v.clear_clip();
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s Stroke,
    ) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.stroke(shape, brush, stroke);
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.stroke(shape, brush, stroke);
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.stroke(shape, brush, stroke);
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => {
                v.stroke(shape, brush, stroke);
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn fill<'b>(
        &mut self,
        path: &impl peniko::kurbo::Shape,
        brush: impl Into<peniko::BrushRef<'b>>,
        blur_radius: f64,
    ) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.fill(path, brush, blur_radius);
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.fill(path, brush, blur_radius);
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.fill(path, brush, blur_radius);
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => {
                v.fill(path, brush, blur_radius);
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.push_layer(blend, alpha, transform, clip);
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.push_layer(blend, alpha, transform, clip);
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.push_layer(blend, alpha, transform, clip);
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => v.push_layer(blend, alpha, transform, clip),
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn pop_layer(&mut self) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.pop_layer();
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.pop_layer();
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.pop_layer();
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => v.pop_layer(),
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn draw_text_with_layout<'b>(
        &mut self,
        layout: impl Iterator<Item = LayoutRun<'b>>,
        pos: impl Into<Point>,
    ) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.draw_text_with_layout(layout, pos);
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.draw_text_with_layout(layout, pos);
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.draw_text_with_layout(layout, pos);
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => {
                v.draw_text_with_layout(layout, pos);
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.draw_img(img, rect);
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.draw_img(img, rect);
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.draw_img(img, rect);
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => {
                v.draw_img(img, rect);
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.draw_svg(svg, rect, brush);
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.draw_svg(svg, rect, brush);
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.draw_svg(svg, rect, brush);
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => {
                v.draw_svg(svg, rect, brush);
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn set_transform(&mut self, transform: Affine) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.set_transform(transform);
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.set_transform(transform);
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.set_transform(transform);
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => {
                v.set_transform(transform);
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn set_z_index(&mut self, z_index: i32) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(v) => {
                v.set_z_index(z_index);
            }
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(v) => {
                v.set_z_index(z_index);
            }
            #[cfg(feature = "vger")]
            Renderer::Vger(v) => {
                v.set_z_index(z_index);
            }
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(v) => {
                v.set_z_index(z_index);
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn finish(&mut self) -> Option<peniko::ImageBrush> {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(r) => r.finish(),
            #[cfg(feature = "vello_cpu")]
            Renderer::VelloCpu(r) => r.finish(),
            #[cfg(feature = "vger")]
            Renderer::Vger(r) => r.finish(),
            #[cfg(feature = "tiny_skia")]
            Renderer::TinySkia(r) => r.finish(),
            Renderer::Uninitialized { .. } => None,
        }
    }
}
