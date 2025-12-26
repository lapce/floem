//! Layout context types for view layout computation.
//!
//! This module contains the context types used during the layout phase:
//! - [`LayoutCx`] - Context for computing Taffy layout nodes
//! - [`ComputeLayoutCx`] - Context for computing view positions after Taffy layout

use peniko::kurbo::{Affine, Point, Rect, Size};
use taffy::prelude::NodeId;

use crate::style::Style;
use crate::view::ViewId;
use crate::view::{ChangeFlags, IsHiddenState, View};
use crate::window::state::WindowState;

/// Context for computing view layout after Taffy has calculated sizes.
///
/// This context is used in the second phase of layout, where we traverse the view tree
/// and compute the actual positions of views based on Taffy's layout calculations.
pub struct ComputeLayoutCx<'a> {
    pub window_state: &'a mut WindowState,
    pub(crate) viewport: Rect,
    pub(crate) window_origin: Point,
    /// The accumulated clip rect in window coordinates. Views outside this rect
    /// are clipped by ancestor overflow:hidden/scroll containers.
    pub(crate) clip_rect: Rect,
    pub(crate) saved_viewports: Vec<Rect>,
    pub(crate) saved_window_origins: Vec<Point>,
    pub(crate) saved_clip_rects: Vec<Rect>,
}

impl<'a> ComputeLayoutCx<'a> {
    pub(crate) fn new(window_state: &'a mut WindowState, viewport: Rect) -> Self {
        Self {
            window_state,
            viewport,
            window_origin: Point::ZERO,
            // Start with a large clip rect that effectively means "no clipping"
            clip_rect: Rect::new(-1e9, -1e9, 1e9, 1e9),
            saved_viewports: Vec::new(),
            saved_window_origins: Vec::new(),
            saved_clip_rects: Vec::new(),
        }
    }

    pub fn window_origin(&self) -> Point {
        self.window_origin
    }

    pub fn save(&mut self) {
        self.saved_viewports.push(self.viewport);
        self.saved_window_origins.push(self.window_origin);
        self.saved_clip_rects.push(self.clip_rect);
    }

    pub fn restore(&mut self) {
        self.viewport = self.saved_viewports.pop().unwrap_or_default();
        self.window_origin = self.saved_window_origins.pop().unwrap_or_default();
        self.clip_rect = self
            .saved_clip_rects
            .pop()
            .unwrap_or(Rect::new(-1e9, -1e9, 1e9, 1e9));
    }

    pub fn current_viewport(&self) -> Rect {
        self.viewport
    }

    /// Internal method used by Floem. This method derives its calculations based on the [Taffy Node](taffy::tree::NodeId) returned by the `View::layout` method.
    ///
    /// It's responsible for:
    /// - calculating and setting the view's origin (local coordinates and window coordinates)
    /// - calculating and setting the view's viewport
    /// - invoking any attached `context::ResizeListener`s
    ///
    /// Returns the bounding rect that encompasses this view and its children
    pub fn compute_view_layout(&mut self, id: ViewId) -> Option<Rect> {
        let view_state = id.state();

        if view_state.borrow().is_hidden_state == IsHiddenState::Hidden {
            view_state.borrow_mut().layout_rect = Rect::ZERO;
            return None;
        }

        self.save();

        let layout = id.get_layout().unwrap_or_default();
        let origin = Point::new(layout.location.x as f64, layout.location.y as f64);
        let this_viewport = view_state.borrow().viewport;
        let this_viewport_origin = this_viewport.unwrap_or_default().origin().to_vec2();
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);
        let parent_viewport = self.viewport.with_origin(
            Point::new(
                self.viewport.x0 - layout.location.x as f64,
                self.viewport.y0 - layout.location.y as f64,
            ) + this_viewport_origin,
        );
        self.viewport = parent_viewport.intersect(size.to_rect());
        if let Some(this_viewport) = this_viewport {
            self.viewport = self.viewport.intersect(this_viewport);
        }

        // Check if this is a fixed-positioned element
        let is_fixed = view_state
            .borrow()
            .combined_style
            .get(crate::style::IsFixed);

        // For fixed positioning, the element is positioned relative to the viewport (window)
        // rather than relative to its parent. So we set window_origin to (0, 0).
        let window_origin = if is_fixed {
            Point::ZERO
        } else {
            origin + self.window_origin.to_vec2() - this_viewport_origin
        };
        self.window_origin = window_origin;
        {
            view_state.borrow_mut().window_origin = window_origin;
        }

        // Compute this view's clip_rect in window coordinates.
        // It's the intersection of the parent's clip_rect with this view's visible area.
        let view_rect_in_window = size.to_rect().with_origin(window_origin);

        // For absolute and fixed positioned elements, don't constrain clip_rect to parent's clip.
        // This allows dropdowns, modals, tooltips, etc. to receive events even when
        // they extend beyond their parent container (like a scroll view).
        let is_absolute = view_state.borrow().taffy_style.position == taffy::Position::Absolute;
        let mut view_clip_rect = if is_absolute || is_fixed {
            // Absolute/fixed elements can receive events anywhere they're rendered
            view_rect_in_window
        } else {
            self.clip_rect.intersect(view_rect_in_window)
        };

        // If this view has a viewport (scroll view child), clip to the viewport bounds.
        // The viewport defines the visible area of content, so events outside it should
        // not reach this view or its children.
        if let Some(vp) = this_viewport {
            // Convert viewport to window coordinates.
            // window_origin has been adjusted by -viewport_origin (for scroll offset),
            // so to get the scroll container's window position, we add back viewport_origin.
            // The clip rect is at the scroll container's position with viewport size.
            let scroll_window_origin = window_origin + this_viewport_origin;
            let viewport_in_window = Rect::new(
                scroll_window_origin.x,
                scroll_window_origin.y,
                scroll_window_origin.x + vp.width(),
                scroll_window_origin.y + vp.height(),
            );
            view_clip_rect = view_clip_rect.intersect(viewport_in_window);
            // Also update clip_rect for children to be clipped to viewport
            self.clip_rect = view_clip_rect;
        }

        // For absolute and fixed positioned elements, also update clip_rect for children.
        // This ensures that children of absolute/fixed elements (like dropdown items)
        // can receive events even when the element extends beyond
        // its parent's clip area.
        if is_absolute || is_fixed {
            self.clip_rect = view_clip_rect;
        }

        {
            let view_state = view_state.borrow();
            let mut resize_listeners = view_state.resize_listeners.borrow_mut();

            let new_rect = size.to_rect().with_origin(origin);
            if new_rect != resize_listeners.rect {
                resize_listeners.rect = new_rect;

                let callbacks = resize_listeners.callbacks.clone();

                // explicitly dropping borrows before using callbacks
                std::mem::drop(resize_listeners);
                std::mem::drop(view_state);

                for callback in callbacks {
                    (*callback)(new_rect);
                }
            }
        }

        {
            let view_state = view_state.borrow();
            let mut move_listeners = view_state.move_listeners.borrow_mut();

            if window_origin != move_listeners.window_origin {
                move_listeners.window_origin = window_origin;

                let callbacks = move_listeners.callbacks.clone();

                // explicitly dropping borrows before using callbacks
                std::mem::drop(move_listeners);
                std::mem::drop(view_state);

                for callback in callbacks {
                    (*callback)(window_origin);
                }
            }
        }

        let view = id.view();
        let child_layout_rect = view.borrow_mut().compute_layout(self);

        let layout_rect = size.to_rect().with_origin(self.window_origin);
        let layout_rect = if let Some(child_layout_rect) = child_layout_rect {
            layout_rect.union(child_layout_rect)
        } else {
            layout_rect
        };

        let transform = view_state.borrow().transform;
        let layout_rect = transform.transform_rect_bbox(layout_rect);

        // Compute the cumulative transform from local coordinates to root (window) coordinates.
        // This combines translation to window_origin with the view's CSS transform.
        // To convert from root coords to local: local = local_to_root.inverse() * root
        let local_to_root_transform =
            Affine::translate((self.window_origin.x, self.window_origin.y)) * transform;

        {
            let mut vs = view_state.borrow_mut();
            vs.layout_rect = layout_rect;
            vs.clip_rect = view_clip_rect;
            vs.local_to_root_transform = local_to_root_transform;
        }

        self.restore();

        Some(layout_rect)
    }
}

/// Holds current layout state for given position in the tree.
/// You'll use this in the `View::layout` implementation to call `layout_node` on children and to access any font
pub struct LayoutCx<'a> {
    pub window_state: &'a mut WindowState,
}

impl<'a> LayoutCx<'a> {
    pub(crate) fn new(window_state: &'a mut WindowState) -> Self {
        Self { window_state }
    }

    /// Responsible for invoking the recalculation of style and thus the layout and
    /// creating or updating the layout of child nodes within the closure.
    ///
    /// You should ensure that all children are laid out within the closure and/or whatever
    /// other work you need to do to ensure that the layout for the returned nodes is correct.
    pub fn layout_node(
        &mut self,
        id: ViewId,
        has_children: bool,
        mut children: impl FnMut(&mut LayoutCx) -> Vec<NodeId>,
    ) -> NodeId {
        let view_state = id.state();
        let node = view_state.borrow().node;
        if !view_state
            .borrow()
            .requested_changes
            .contains(ChangeFlags::LAYOUT)
        {
            return node;
        }
        view_state
            .borrow_mut()
            .requested_changes
            .remove(ChangeFlags::LAYOUT);
        let layout_style = view_state.borrow().layout_props.to_style();
        let animate_out_display = view_state.borrow().is_hidden_state.get_display();
        let combined_style = view_state.borrow().combined_style.clone();
        let is_fixed = combined_style.get(crate::style::IsFixed);
        let mut style = combined_style
            .apply(layout_style)
            .apply_opt(animate_out_display, crate::style::Style::display)
            .to_taffy_style();

        // For fixed positioning, set explicit dimensions to window size.
        // This ensures percentage-based children are relative to the viewport.
        if is_fixed {
            let root_size = self.window_state.root_size / self.window_state.scale;
            style.size = taffy::prelude::Size {
                width: taffy::style::Dimension::length(root_size.width as f32),
                height: taffy::style::Dimension::length(root_size.height as f32),
            };
            // Fixed elements should be positioned at the origin relative to viewport
            style.inset = taffy::prelude::Rect {
                left: taffy::style::LengthPercentageAuto::length(0.0),
                right: taffy::style::LengthPercentageAuto::auto(),
                top: taffy::style::LengthPercentageAuto::length(0.0),
                bottom: taffy::style::LengthPercentageAuto::auto(),
            };
        }

        let _ = id.taffy().borrow_mut().set_style(node, style);

        if has_children {
            let nodes = children(self);
            let _ = id.taffy().borrow_mut().set_children(node, &nodes);
        }

        node
    }

    /// Internal method used by Floem to invoke the user-defined `View::layout` method.
    pub fn layout_view(&mut self, view: &mut dyn View) -> NodeId {
        view.layout(self)
    }
}
