//! Layout context types for view layout computation.
//!
//! This module contains the context types used during the layout phase:
//! - [`LayoutCx`] - Context for computing Taffy layout nodes
//! - [`ComputeLayoutCx`] - Context for computing view positions after Taffy layout

use peniko::kurbo::{Point, Rect, Size, Vec2};

use crate::view::ViewId;
use crate::window::state::WindowState;

// =============================================================================
// Window origin computation
// =============================================================================

/// Computed window origins for a view.
///
/// We track two origins because CSS transforms and move listeners need different values:
/// - `base`: The logical position in window coords, ignoring CSS translate. This is what
///   move listeners report since they care about where the element "should" be.
/// - `translated`: The position after CSS translate. This becomes `window_origin` and is
///   used for child positioning. Note: this does NOT include scale/rotate effects.
#[derive(Clone, Copy)]
struct WindowOrigins {
    /// Position before CSS translate (used for move listeners)
    base: Point,
    /// Position after CSS translate (stored as window_origin, used for child positioning)
    translated: Point,
}

/// Compute window origins for a view based on its layout position.
///
/// The window origin is used for child positioning and is different from
/// `visual_transform.translation()` which includes scale/rotate effects.
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

    let translated = base + translate;

    WindowOrigins { base, translated }
}

// =============================================================================
// Clip rect computation
// =============================================================================

/// Compute the clip rect for a view.
///
/// For normal flow elements, the clip rect is the intersection of the parent's
/// accumulated clip rect and this view's bounds - ensuring the view can only
/// receive events within its parent's visible area.
///
/// For absolute/fixed elements, the clip rect equals their own bounds since
/// they escape the normal document flow and aren't clipped by ancestors.
fn compute_clip_rect(
    size: Size,
    visual_origin: Point,
    parent_clip_rect: Rect,
    is_absolute: bool,
    is_fixed: bool,
) -> Rect {
    let view_rect = size.to_rect().with_origin(visual_origin);

    if is_absolute || is_fixed {
        view_rect
    } else {
        parent_clip_rect.intersect(view_rect)
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

    // /// Responsible for invoking the recalculation of style and thus the layout and
    // /// creating or updating the layout of child nodes within the closure.
    // ///
    // /// You should ensure that all children are laid out within the closure and/or whatever
    // /// other work you need to do to ensure that the layout for the returned nodes is correct.
    // pub fn layout_node(
    //     &mut self,
    //     id: ViewId,
    //     has_children: bool,
    //     mut children: impl FnMut(&mut LayoutCx) -> Vec<NodeId>,
    // ) -> NodeId {
    //     let view_state = id.state();
    //     let node = view_state.borrow().layout_id;

    //     if !view_state
    //         .borrow()
    //         .requested_changes
    //         .contains(ChangeFlags::LAYOUT)
    //     {
    //         return node;
    //     }
    //     view_state
    //         .borrow_mut()
    //         .requested_changes
    //         .remove(ChangeFlags::LAYOUT);

    //     let combined_style = view_state.borrow().combined_style.clone();
    //     let is_fixed = combined_style.get(crate::style::IsFixed);
    //     let layout_style = view_state.borrow().layout_props.to_style();
    //     let visibility = view_state.borrow().visibility;
    //     // For Animating, preserve the original display; for Hidden/force_hidden, force None
    //     let display_override = if visibility.force_hidden {
    //         Some(taffy::Display::None)
    //     } else {
    //         match visibility.phase {
    //             VisibilityPhase::Animating(dis) => Some(dis),
    //             VisibilityPhase::Hidden => Some(taffy::Display::None),
    //             _ => None,
    //         }
    //     };

    //     let mut style = combined_style
    //         .apply(layout_style)
    //         .apply_opt(display_override, crate::style::Style::display)
    //         .to_taffy_style();

    //     if is_fixed {
    //         self.apply_fixed_positioning(&mut style);
    //     }

    //     let _ = id.taffy().borrow_mut().set_style(node, style);

    //     if has_children {
    //         let nodes = children(self);
    //         let _ = id.taffy().borrow_mut().set_children(node, &nodes);
    //     }

    //     node
    // }

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
}

pub struct PostLayoutCx<'a> {
    pub window_state: &'a mut WindowState,
    pub layout: &'a taffy::Layout,
}
impl<'a> PostLayoutCx<'a> {
    pub fn new(window_state: &'a mut WindowState, layout: &'a taffy::Layout) -> Self {
        Self {
            window_state,
            layout,
        }
    }
}
