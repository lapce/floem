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
//! it updates the counter and because the label has an effect that is subscribed to those changes (see [floem_reactive::create_effect]), the label will update the text it presents.
//!
//! #### Update
//! The update of states on the Views could cause some of them to need a new layout recalculation, because the size might have changed etc.
//! The reactive system can't directly manipulate the view state of the label because the AppState owns all the views. And instead, it will send the update to a message queue via [ViewId::update_state](crate::ViewId::update_state)
//! After the event propagation is done, Floem will process all the update messages in the queue, and it can manipulate the state of a particular view through the update method.
//!
//!
//! #### Layout
//! The layout method is called from the root view to re-layout the views that have requested a layout call.
//! The layout call is to change the layout properties at Taffy, and after the layout call is done, compute_layout is called to calculate the sizes and positions of each view.
//!
//! #### Paint
//! And in the end, paint is called to render all the views to the screen.
//!
//!
//! ## Terminology
//!
//! Useful definitions for developers of Floem
//!
//! #### Active view
//!
//! Affects pointer events. Pointer events will only be sent to the active View. The View will continue to receive pointer events even if the mouse is outside its bounds.
//! It is useful when you drag things, e.g. the scroll bar, you set the scroll bar active after pointer down, then when you drag, the `PointerMove` will always be sent to the View, even if your mouse is outside of the view.
//!
//! #### Focused view
//! Affects keyboard events. Keyboard events will only be sent to the focused View. The View will continue to receive keyboard events even if it's not the active View.
//!
//! ## Notable invariants and tolerances
//! - There can be only one root `View`
//! - Only one view can be active at a time.
//! - Only one view can be focused at a time.
//!
use crate::text::TextLayout;
use floem_renderer::gpu_resources::GpuResources;
use floem_renderer::Img;
use floem_tiny_skia_renderer::TinySkiaRenderer;
use floem_vger_renderer::VgerRenderer;
use image::DynamicImage;
use peniko::kurbo::{self, Affine, Rect, Shape, Size};
use peniko::BrushRef;

#[allow(clippy::large_enum_variant)]
pub enum Renderer<W> {
    Vger(VgerRenderer),
    TinySkia(TinySkiaRenderer<W>),
}

impl<W: wgpu::WindowHandle> Renderer<W> {
    pub fn new(
        window: W,
        gpu_resources: GpuResources,
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self
    where
        W: Clone + 'static,
    {
        let size = Size::new(size.width.max(1.0), size.height.max(1.0));

        let force_tiny_skia = std::env::var("FLOEM_FORCE_TINY_SKIA")
            .ok()
            .map(|val| val.as_str() == "1")
            .unwrap_or(false);

        let vger_err = if !force_tiny_skia {
            match VgerRenderer::new(
                gpu_resources,
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
            Renderer::Vger(r) => r.resize(size.width as u32, size.height as u32, scale),
            Renderer::TinySkia(r) => r.resize(size.width as u32, size.height as u32, scale),
        }
    }

    pub fn set_scale(&mut self, scale: f64) {
        match self {
            Renderer::Vger(r) => r.set_scale(scale),
            Renderer::TinySkia(r) => r.set_scale(scale),
        }
    }

    pub fn scale(&self) -> f64 {
        match self {
            Renderer::Vger(r) => r.scale(),
            Renderer::TinySkia(r) => r.scale(),
        }
    }
}

impl<W: wgpu::WindowHandle> floem_renderer::Renderer for Renderer<W> {
    fn begin(&mut self, capture: bool) {
        match self {
            Renderer::Vger(r) => {
                r.begin(capture);
            }
            Renderer::TinySkia(r) => {
                r.begin(capture);
            }
        }
    }

    fn clip(&mut self, shape: &impl Shape) {
        match self {
            Renderer::Vger(v) => {
                v.clip(shape);
            }
            Renderer::TinySkia(v) => {
                v.clip(shape);
            }
        }
    }

    fn clear_clip(&mut self) {
        match self {
            Renderer::Vger(v) => {
                v.clear_clip();
            }
            Renderer::TinySkia(v) => {
                v.clear_clip();
            }
        }
    }

    fn stroke<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, width: f64) {
        match self {
            Renderer::Vger(v) => {
                v.stroke(shape, brush, width);
            }
            Renderer::TinySkia(v) => {
                v.stroke(shape, brush, width);
            }
        }
    }

    fn fill<'b>(
        &mut self,
        path: &impl peniko::kurbo::Shape,
        brush: impl Into<peniko::BrushRef<'b>>,
        blur_radius: f64,
    ) {
        match self {
            Renderer::Vger(v) => {
                v.fill(path, brush, blur_radius);
            }
            Renderer::TinySkia(v) => {
                v.fill(path, brush, blur_radius);
            }
        }
    }

    fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<kurbo::Point>) {
        match self {
            Renderer::Vger(v) => {
                v.draw_text(layout, pos);
            }
            Renderer::TinySkia(v) => {
                v.draw_text(layout, pos);
            }
        }
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        match self {
            Renderer::Vger(v) => {
                v.draw_img(img, rect);
            }
            Renderer::TinySkia(v) => {
                v.draw_img(img, rect);
            }
        }
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        match self {
            Renderer::Vger(v) => {
                v.draw_svg(svg, rect, brush);
            }
            Renderer::TinySkia(v) => {
                v.draw_svg(svg, rect, brush);
            }
        }
    }

    fn transform(&mut self, transform: Affine) {
        match self {
            Renderer::Vger(v) => {
                v.transform(transform);
            }
            Renderer::TinySkia(v) => {
                v.transform(transform);
            }
        }
    }

    fn set_z_index(&mut self, z_index: i32) {
        match self {
            Renderer::Vger(v) => {
                v.set_z_index(z_index);
            }
            Renderer::TinySkia(v) => {
                v.set_z_index(z_index);
            }
        }
    }

    fn finish(&mut self) -> Option<DynamicImage> {
        match self {
            Renderer::Vger(r) => r.finish(),
            Renderer::TinySkia(r) => r.finish(),
        }
    }
}
