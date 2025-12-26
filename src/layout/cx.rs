//! Layout context types for view layout computation.
//!
//! This module contains the context types used during the layout phase:
//! - [`LayoutCx`] - Context for computing Taffy layout nodes
//! - [`ComputeLayoutCx`] - Context for computing view positions after Taffy layout

use peniko::kurbo::{Affine, Point, Rect, Size, Vec2};
use taffy::prelude::NodeId;

use crate::view::ViewId;
use crate::view::{ChangeFlags, IsHiddenState, View};
use crate::window::state::WindowState;

// =============================================================================
// Transform computation
// =============================================================================

/// CSS transform components, computed once and used throughout layout.
#[derive(Clone, Copy)]
pub struct TransformComponents {
    /// Complete transform (translate + scale + rotation) for painting
    pub full: Affine,
    /// Just the translation component
    pub translate: Vec2,
    /// Scale and rotation only (for rects that already have translate in origin)
    pub scale_rotation: Affine,
}

impl TransformComponents {
    /// Compute CSS transform from layout properties.
    pub fn from_layout_props(layout_props: &crate::style::LayoutProps, size: Size) -> Self {
        // Compute translate
        let translate_x = match layout_props.translate_x() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => size.width * pct / 100.,
        };
        let translate_y = match layout_props.translate_y() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => size.height * pct / 100.,
        };
        let translate = Vec2::new(translate_x, translate_y);

        // Compute scale and rotation around center
        let scale_x = layout_props.scale_x().0 / 100.;
        let scale_y = layout_props.scale_y().0 / 100.;
        let rotation = layout_props.rotation().0;
        let center = Vec2::new(size.width / 2., size.height / 2.);

        let scale_rotation = Affine::translate(center)
            * Affine::scale_non_uniform(scale_x, scale_y)
            * Affine::rotate(rotation)
            * Affine::translate(-center);

        // Full transform = translate + scale/rotation
        let full = Affine::translate(translate) * scale_rotation;

        Self {
            full,
            translate,
            scale_rotation,
        }
    }
}

// =============================================================================
// Window origin computation
// =============================================================================

/// Computed window origins for a view.
#[derive(Clone, Copy)]
struct WindowOrigins {
    /// Position before transform (used for move listeners)
    base: Point,
    /// Position after translate (where children are positioned)
    visual: Point,
}

fn compute_window_origins(
    origin: Point,
    parent_window_origin: Point,
    viewport_origin: Vec2,
    translate: Vec2,
    is_fixed: bool,
) -> WindowOrigins {
    let base = if is_fixed {
        // Fixed positioning: relative to viewport, not parent
        origin
    } else {
        // Normal positioning: relative to parent
        origin + parent_window_origin.to_vec2() - viewport_origin
    };

    let visual = Point::new(base.x + translate.x, base.y + translate.y);

    WindowOrigins { base, visual }
}

// =============================================================================
// Clip rect computation
// =============================================================================

fn compute_clip_rect(
    size: Size,
    visual_origin: Point,
    parent_clip_rect: Rect,
    is_absolute: bool,
    is_fixed: bool,
) -> Rect {
    let view_rect = size.to_rect().with_origin(visual_origin);

    if is_absolute || is_fixed {
        // Absolute/fixed elements can receive events anywhere they're rendered
        view_rect
    } else {
        parent_clip_rect.intersect(view_rect)
    }
}

fn apply_viewport_clipping(
    clip_rect: Rect,
    viewport: Option<Rect>,
    base_window_origin: Point,
    viewport_origin: Vec2,
) -> Rect {
    if let Some(vp) = viewport {
        let scroll_origin = base_window_origin + viewport_origin;
        let viewport_rect = Rect::new(
            scroll_origin.x,
            scroll_origin.y,
            scroll_origin.x + vp.width(),
            scroll_origin.y + vp.height(),
        );
        clip_rect.intersect(viewport_rect)
    } else {
        clip_rect
    }
}

// =============================================================================
// Listener notification
// =============================================================================

fn notify_resize_listeners(id: ViewId, size: Size, origin: Point) {
    let view_state = id.state();
    let vs = view_state.borrow();
    let mut resize_listeners = vs.resize_listeners.borrow_mut();

    let new_rect = size.to_rect().with_origin(origin);
    if new_rect != resize_listeners.rect {
        resize_listeners.rect = new_rect;
        let callbacks = resize_listeners.callbacks.clone();
        std::mem::drop(resize_listeners);
        std::mem::drop(vs);
        for callback in callbacks {
            (*callback)(new_rect);
        }
    }
}

fn notify_move_listeners(id: ViewId, base_window_origin: Point) {
    let view_state = id.state();
    let vs = view_state.borrow();
    let mut move_listeners = vs.move_listeners.borrow_mut();

    if base_window_origin != move_listeners.window_origin {
        move_listeners.window_origin = base_window_origin;
        let callbacks = move_listeners.callbacks.clone();
        std::mem::drop(move_listeners);
        std::mem::drop(vs);
        for callback in callbacks {
            (*callback)(base_window_origin);
        }
    }
}

// =============================================================================
// ComputeLayoutCx
// =============================================================================

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

    /// Compute layout for a view and its children.
    ///
    /// Returns the bounding rect that encompasses this view and its children.
    pub fn compute_view_layout(&mut self, id: ViewId) -> Option<Rect> {
        let view_state = id.state();

        // Early return for hidden views
        if view_state.borrow().is_hidden_state == IsHiddenState::Hidden {
            view_state.borrow_mut().layout_rect = Rect::ZERO;
            return None;
        }

        self.save();

        // Get basic layout info from Taffy
        let layout = id.get_layout().unwrap_or_default();
        let origin = Point::new(layout.location.x as f64, layout.location.y as f64);
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);

        // Compute transform components once
        let transform = {
            let vs = view_state.borrow();
            TransformComponents::from_layout_props(&vs.layout_props, size)
        };

        // Get positioning info
        let this_viewport = view_state.borrow().viewport;
        let viewport_origin = this_viewport.unwrap_or_default().origin().to_vec2();
        let is_fixed = view_state
            .borrow()
            .combined_style
            .get(crate::style::IsFixed);
        let is_absolute = view_state.borrow().taffy_style.position == taffy::Position::Absolute;

        // Compute window origins
        let origins = compute_window_origins(
            origin,
            self.window_origin,
            viewport_origin,
            transform.translate,
            is_fixed,
        );

        // Update context and view state with visual origin
        self.window_origin = origins.visual;
        view_state.borrow_mut().window_origin = origins.visual;

        // Update viewport
        self.update_viewport(layout.location, viewport_origin, size, this_viewport);

        // Compute and update clip rect
        let mut view_clip_rect =
            compute_clip_rect(size, origins.visual, self.clip_rect, is_absolute, is_fixed);
        view_clip_rect =
            apply_viewport_clipping(view_clip_rect, this_viewport, origins.base, viewport_origin);

        if this_viewport.is_some() || is_absolute || is_fixed {
            self.clip_rect = view_clip_rect;
        }

        // Notify listeners
        notify_resize_listeners(id, size, origin);
        notify_move_listeners(id, origins.base);

        // Recursively compute children layouts
        let view = id.view();
        let child_layout_rect = view.borrow_mut().compute_layout(self);

        // Compute final layout rect
        let layout_rect = self.compute_final_layout_rect(size, child_layout_rect, &transform);
        let view_clip_rect = transform.scale_rotation.transform_rect_bbox(view_clip_rect);

        // Compute cumulative transform
        let local_to_root = Affine::translate((self.window_origin.x, self.window_origin.y))
            * transform.scale_rotation;

        // Store results
        {
            let mut vs = view_state.borrow_mut();
            vs.transform = transform.full;
            vs.layout_rect = layout_rect;
            vs.clip_rect = view_clip_rect;
            vs.local_to_root_transform = local_to_root;
        }

        self.restore();
        Some(layout_rect)
    }

    fn update_viewport(
        &mut self,
        location: taffy::Point<f32>,
        viewport_origin: Vec2,
        size: Size,
        this_viewport: Option<Rect>,
    ) {
        let parent_viewport = self.viewport.with_origin(
            Point::new(
                self.viewport.x0 - location.x as f64,
                self.viewport.y0 - location.y as f64,
            ) + viewport_origin,
        );
        self.viewport = parent_viewport.intersect(size.to_rect());
        if let Some(vp) = this_viewport {
            self.viewport = self.viewport.intersect(vp);
        }
    }

    fn compute_final_layout_rect(
        &self,
        size: Size,
        child_layout_rect: Option<Rect>,
        transform: &TransformComponents,
    ) -> Rect {
        let layout_rect = size.to_rect().with_origin(self.window_origin);
        let layout_rect = if let Some(child_rect) = child_layout_rect {
            layout_rect.union(child_rect)
        } else {
            layout_rect
        };
        transform.scale_rotation.transform_rect_bbox(layout_rect)
    }
}

// =============================================================================
// LayoutCx - Taffy layout computation
// =============================================================================

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

        let combined_style = view_state.borrow().combined_style.clone();
        let is_fixed = combined_style.get(crate::style::IsFixed);
        let layout_style = view_state.borrow().layout_props.to_style();
        let animate_out_display = view_state.borrow().is_hidden_state.get_display();

        let mut style = combined_style
            .apply(layout_style)
            .apply_opt(animate_out_display, crate::style::Style::display)
            .to_taffy_style();

        if is_fixed {
            self.apply_fixed_positioning(&mut style);
        }

        let _ = id.taffy().borrow_mut().set_style(node, style);

        if has_children {
            let nodes = children(self);
            let _ = id.taffy().borrow_mut().set_children(node, &nodes);
        }

        node
    }

    /// Apply fixed positioning adjustments to the style.
    fn apply_fixed_positioning(&self, style: &mut taffy::Style) {
        let root_size = self.window_state.root_size / self.window_state.scale;

        fn is_definite_length(val: &taffy::style::LengthPercentageAuto) -> Option<f32> {
            let raw = val.into_raw();
            if raw.tag() == taffy::CompactLength::LENGTH_TAG {
                Some(raw.value())
            } else {
                None
            }
        }

        let left_len = is_definite_length(&style.inset.left);
        let right_len = is_definite_length(&style.inset.right);
        let top_len = is_definite_length(&style.inset.top);
        let bottom_len = is_definite_length(&style.inset.bottom);

        // Width from inset or percentage
        if let (Some(left), Some(right)) = (left_len, right_len) {
            let computed = (root_size.width as f32 - left - right).max(0.0);
            style.size.width = taffy::style::Dimension::length(computed);
        } else if style.size.width == taffy::style::Dimension::percent(1.0) {
            style.size.width = taffy::style::Dimension::length(root_size.width as f32);
        }

        // Height from inset or percentage
        if let (Some(top), Some(bottom)) = (top_len, bottom_len) {
            let computed = (root_size.height as f32 - top - bottom).max(0.0);
            style.size.height = taffy::style::Dimension::length(computed);
        } else if style.size.height == taffy::style::Dimension::percent(1.0) {
            style.size.height = taffy::style::Dimension::length(root_size.height as f32);
        }
    }

    /// Internal method used by Floem to invoke the user-defined `View::layout` method.
    pub fn layout_view(&mut self, view: &mut dyn View) -> NodeId {
        view.layout(self)
    }
}
