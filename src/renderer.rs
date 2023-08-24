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
//! The reactive system can't directly manipulate the view state of the label because the AppState owns all the views. And instead, it will send the update to a message queue via [Id::update_state](crate::id::Id::update_state)
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
use crate::cosmic_text::TextLayout;
use floem_vger::VgerRenderer;
use kurbo::{Affine, Rect, Shape, Size};
use peniko::BrushRef;

pub enum Renderer {
    Vger(VgerRenderer),
}

impl Renderer {
    pub fn new<W>(window: &W, scale: f64, size: Size) -> Self
    where
        W: raw_window_handle::HasRawDisplayHandle + raw_window_handle::HasRawWindowHandle,
    {
        let size = Size::new(
            (size.width * scale).max(1.0),
            (size.height * scale).max(1.0),
        );
        Self::Vger(VgerRenderer::new(window, size.width as u32, size.height as u32, scale).unwrap())
    }

    pub fn resize(&mut self, scale: f64, size: Size) {
        let size = Size::new(size.width * scale, size.height * scale);
        match self {
            Renderer::Vger(r) => r.resize(size.width as u32, size.height as u32, scale),
        }
    }

    pub fn set_scale(&mut self, scale: f64) {
        match self {
            Renderer::Vger(r) => r.set_scale(scale),
        }
    }
}

impl floem_renderer::Renderer for Renderer {
    fn begin(&mut self) {
        match self {
            Renderer::Vger(r) => {
                r.begin();
            }
        }
    }

    fn clip(&mut self, shape: &impl Shape) {
        match self {
            Renderer::Vger(v) => {
                v.clip(shape);
            }
        }
    }

    fn clear_clip(&mut self) {
        match self {
            Renderer::Vger(v) => {
                v.clear_clip();
            }
        }
    }

    fn stroke<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, width: f64) {
        match self {
            Renderer::Vger(v) => {
                v.stroke(shape, brush, width);
            }
        }
    }

    fn fill<'b>(
        &mut self,
        path: &impl kurbo::Shape,
        brush: impl Into<peniko::BrushRef<'b>>,
        blur_radius: f64,
    ) {
        match self {
            Renderer::Vger(v) => {
                v.fill(path, brush, blur_radius);
            }
        }
    }

    fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<kurbo::Point>) {
        match self {
            Renderer::Vger(v) => {
                v.draw_text(layout, pos);
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
        }
    }

    fn transform(&mut self, transform: Affine) {
        match self {
            Renderer::Vger(v) => {
                v.transform(transform);
            }
        }
    }

    fn set_z_index(&mut self, z_index: i32) {
        match self {
            Renderer::Vger(v) => {
                v.set_z_index(z_index);
            }
        }
    }

    fn finish(&mut self) {
        match self {
            Renderer::Vger(r) => {
                r.finish();
            }
        }
    }
}
