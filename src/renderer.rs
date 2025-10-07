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
//! The reactive system can't directly manipulate the view state of the label because the `AppState` owns all the views. And instead, it will send the update to a message queue via [`ViewId::update_state`](crate::ViewId::update_state)
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
use floem_renderer::gpu_resources::GpuResources;
use floem_renderer::text::LayoutRun;
use floem_renderer::Img;
use floem_tiny_skia_renderer::TinySkiaRenderer;
#[cfg(feature = "vello")]
use floem_vello_renderer::VelloRenderer;
#[cfg(not(feature = "vello"))]
use floem_vger_renderer::VgerRenderer;
use peniko::kurbo::{Affine, Rect, Shape, Size, Stroke};
use peniko::BrushRef;
use winit::window::Window;

#[allow(clippy::large_enum_variant)]
pub enum Renderer {
    #[cfg(feature = "vello")]
    Vello(VelloRenderer),
    #[cfg(not(feature = "vello"))]
    Vger(VgerRenderer),
    TinySkia(TinySkiaRenderer<Arc<dyn Window>>),
    /// Uninitialized renderer, used to allow the renderer to be created lazily
    /// All operations on this renderer are no-ops
    Uninitialized {
        scale: f64,
        size: Size,
    },
}

impl Renderer {
    pub fn new(
        window: Arc<dyn Window>,
        gpu_resources: GpuResources,
        surface: wgpu::Surface<'static>,
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        let size = Size::new(size.width.max(1.0), size.height.max(1.0));

        let force_tiny_skia = std::env::var("FLOEM_FORCE_TINY_SKIA")
            .ok()
            .map(|val| val.as_str() == "1")
            .unwrap_or(false);

        #[cfg(feature = "vello")]
        let vger_err = if !force_tiny_skia {
            match VelloRenderer::new(
                gpu_resources,
                surface,
                size.width as u32,
                size.height as u32,
                scale,
                font_embolden,
            ) {
                Ok(vger) => return Self::Vello(vger),
                Err(err) => Some(err),
            }
        } else {
            None
        };

        #[cfg(not(feature = "vello"))]
        let vger_err = if !force_tiny_skia {
            match VgerRenderer::new(
                gpu_resources,
                surface,
                size.width as u32,
                size.height as u32,
                scale,
                font_embolden,
            ) {
                Ok(vger) => return Self::Vger(vger),
                Err(err) => Some(err),
            }
        } else {
            None
        };

        let tiny_skia_err = match TinySkiaRenderer::new(
            window,
            size.width as u32,
            size.height as u32,
            scale,
            font_embolden,
        ) {
            Ok(tiny_skia) => return Self::TinySkia(tiny_skia),
            Err(err) => err,
        };

        if !force_tiny_skia {
            panic!("Failed to create VgerRenderer: {}\nFailed to create TinySkiaRenderer: {tiny_skia_err}", vger_err.unwrap());
        } else {
            panic!("Failed to create TinySkiaRenderer: {tiny_skia_err}");
        }
    }

    pub fn resize(&mut self, scale: f64, size: Size) {
        let size = Size::new(size.width.max(1.0), size.height.max(1.0));
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(r) => r.resize(size.width as u32, size.height as u32, scale),
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(r) => r.resize(size.width as u32, size.height as u32, scale),
            Renderer::TinySkia(r) => r.resize(size.width as u32, size.height as u32, scale),
            Renderer::Uninitialized { .. } => {}
        }
    }

    pub fn set_scale(&mut self, scale: f64) {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(r) => r.set_scale(scale),
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(r) => r.set_scale(scale),
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(r) => r.scale(),
            Renderer::TinySkia(r) => r.scale(),
            Renderer::Uninitialized { scale, .. } => *scale,
        }
    }

    pub fn size(&self) -> Size {
        match self {
            #[cfg(feature = "vello")]
            Renderer::Vello(r) => r.size(),
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(r) => r.size(),
            Renderer::TinySkia(r) => r.size(),
            Renderer::Uninitialized { size, .. } => *size,
        }
    }

    pub(crate) fn debug_info(&self) -> String {
        use crate::Renderer;

        match self {
            #[cfg(feature = "vello")]
            Self::Vello(r) => r.debug_info(),
            #[cfg(not(feature = "vello"))]
            Self::Vger(r) => r.debug_info(),
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(r) => {
                r.begin(capture);
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.clip(shape);
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.clear_clip();
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.stroke(shape, brush, stroke);
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.fill(path, brush, blur_radius);
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.push_layer(blend, alpha, transform, clip);
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.pop_layer();
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.draw_text_with_layout(layout, pos);
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.draw_img(img, rect);
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.draw_svg(svg, rect, brush);
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.set_transform(transform);
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(v) => {
                v.set_z_index(z_index);
            }
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
            #[cfg(not(feature = "vello"))]
            Renderer::Vger(r) => r.finish(),
            Renderer::TinySkia(r) => r.finish(),
            Renderer::Uninitialized { .. } => None,
        }
    }
}
