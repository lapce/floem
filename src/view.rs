//! # View and Widget Traits
//! Views are self-contained components that can be composed together to create complex UIs.
//! Views are the main building blocks of Floem.
//!
//! Views are structs that implement the View and widget traits. Many of these structs will also contain a child field that also implements View. In this way, views can be composed together easily to create complex UIs. This is the most common way to build UIs in Floem. For more information on how to compose views check out the [Views](crate::views) module.
//!
//! Creating a struct and manually implementing the View and Widget traits is typically only needed for building new widgets and for special cases. The rest of this module documentation is for help when manually implementing View and Widget on your own types.
//!
//!
//! ## The View and Widget Traits
//! The [`View`] trait is the trait that Floem uses to build  and display elements, and it builds on the [`Widget`] trait. The [`Widget`] trait contains the methods for implementing updates, styling, layout, events, and painting.
//! Eventually, the goal is for Floem to integrate the Widget trait with other rust UI libraries so that the widget layer can be shared among all compatible UI libraries.
//!
//! ## State management
//!
//! For all reactive state that your type contains, either in the form of signals or derived signals, you need to process the changes within an effect.
//! The most common pattern is to [get](floem_reactive::ReadSignal::get) the data in an effect and pass it in to `id.update_state()` and then handle that data in the `update` method of the View trait.
//!
//! For example a minimal slider might look like the following. First, we define the struct with the [`ViewData`] that contains the [`Id`].
//! Then, we use a function to construct the slider. As part of this function we create an effect that will be re-run every time the signals in the  `percent` closure change.
//! In the effect we send the change to the associated [`Id`]. This change can then be handled in the [`Widget::update`] method.
//! ```rust
//! use floem::ViewId;
//! use floem::reactive::*;
//!
//! struct Slider {
//!     id: ViewId,
//! }
//! pub fn slider(percent: impl Fn() -> f32 + 'static) -> Slider {
//!    let id = ViewId::new();
//!
//!    // If the following effect is not created, and `percent` is accessed directly,
//!    // `percent` will only be accessed a single time and will not be reactive.
//!    // Therefore the following `create_effect` is necessary for reactivity.
//!    create_effect(move |_| {
//!        let percent = percent();
//!        id.update_state(percent);
//!    });
//!    Slider {
//!        id,
//!    }
//! }
//! ```
//!

use floem_reactive::{ReadSignal, RwSignal, SignalGet};
use peniko::kurbo::{
    Arc, BezPath, Circle, Insets, Line, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point, Rect,
    RoundedRect, RoundedRectRadii, Shape, Size, Stroke, Vec2,
};
use std::{any::Any, f64::consts::FRAC_PI_2, iter::Peekable, ops::Range};
use taffy::tree::NodeId;

use crate::{
    app_state::AppState,
    context::{ComputeLayoutCx, EventCx, LayoutCx, PaintCx, StyleCx, UpdateCx},
    event::{Event, EventPropagation},
    id::ViewId,
    style::{LayoutProps, Style, StyleClassRef},
    unit::Pct,
    view_state::ViewStyleProps,
    views::{dyn_view, DynamicView},
    Renderer,
};

/// type erased [`View`]
///
/// Views in Floem are strongly typed. [`AnyView`] allows you to escape the strong typing by converting any type implementing [View] into the [AnyView] type.
///
/// ## Bad Example
///```compile_fail
/// use floem::views::*;
/// use floem::widgets::*;
/// use floem::reactive::{RwSignal, SignalGet};
///
/// let check = true;
///
/// container(if check == true {
///     checkbox(|| true)
/// } else {
///     label(|| "no check".to_string())
/// });
/// ```
/// The above example will fail to compile because `container` is expecting a single type implementing `View` so the if and
/// the else must return the same type. However the branches return different types. The solution to this is to use the [IntoView::into_any] method
/// to escape the strongly typed requirement.
///
/// ```
/// use floem::reactive::{RwSignal, SignalGet};
/// use floem::views::*;
/// use floem::{IntoView, View};
///
/// let check = true;
///
/// container(if check == true {
///     checkbox(|| true).into_any()
/// } else {
///     label(|| "no check".to_string()).into_any()
/// });
/// ```
pub type AnyView = Box<dyn View>;

/// Converts a value into a [`View`].
///
/// This trait can be implemented on types which can be built into another type that implements the `View` trait.
///
/// For example, `&str` implements `IntoView` by building a `text` view and can therefore be used directly in a View tuple.
/// ```rust
/// # use floem::reactive::*;
/// # use floem::views::*;
/// # use floem::IntoView;
/// fn app_view() -> impl IntoView {
///     v_stack(("Item One", "Item Two"))
/// }
/// ```
/// Check out the [other types](#foreign-impls) that `IntoView` is implemented for.
pub trait IntoView: Sized {
    type V: View + 'static;

    /// Converts the value into a [`View`].
    fn into_view(self) -> Self::V;

    /// Converts the value into a [`AnyView`].
    fn into_any(self) -> AnyView {
        Box::new(self.into_view())
    }
}

impl<IV: IntoView + 'static> IntoView for Box<dyn Fn() -> IV> {
    type V = DynamicView;

    fn into_view(self) -> Self::V {
        dyn_view(self)
    }
}

impl<T: IntoView + Clone + 'static> IntoView for RwSignal<T> {
    type V = DynamicView;

    fn into_view(self) -> Self::V {
        dyn_view(move || self.get())
    }
}

impl<T: IntoView + Clone + 'static> IntoView for ReadSignal<T> {
    type V = DynamicView;

    fn into_view(self) -> Self::V {
        dyn_view(move || self.get())
    }
}

impl<VW: View + 'static> IntoView for VW {
    type V = VW;

    fn into_view(self) -> Self::V {
        self
    }
}

impl IntoView for i32 {
    type V = crate::views::Label;

    fn into_view(self) -> Self::V {
        crate::views::text(self)
    }
}

impl IntoView for usize {
    type V = crate::views::Label;

    fn into_view(self) -> Self::V {
        crate::views::text(self)
    }
}

impl IntoView for &str {
    type V = crate::views::Label;

    fn into_view(self) -> Self::V {
        crate::views::text(self)
    }
}

impl IntoView for String {
    type V = crate::views::Label;

    fn into_view(self) -> Self::V {
        crate::views::text(self)
    }
}

impl<IV: IntoView + 'static> IntoView for Vec<IV> {
    type V = crate::views::Stack;

    fn into_view(self) -> Self::V {
        crate::views::stack_from_iter(self)
    }
}

/// Default implementation of `View::layout()` which can be used by
/// view implementations that need the default behavior and also need
/// to implement that method to do additional work.
pub fn recursively_layout_view(id: ViewId, cx: &mut LayoutCx) -> NodeId {
    cx.layout_node(id, true, |cx| {
        let mut nodes = Vec::new();
        for child in id.children() {
            let view = child.view();
            let mut view = view.borrow_mut();
            nodes.push(view.layout(cx));
        }
        nodes
    })
}

/// The View trait contains the methods for implementing updates, styling, layout, events, and painting.
///
/// The [id](View::id) method must be implemented.
/// The other methods may be implemented as necessary to implement the functionality of the View.
/// ## State Management in a Custom View
///
/// For all reactive state that your type contains, either in the form of signals or derived signals, you need to process the changes within an effect.
/// The most common pattern is to [get](floem_reactive::SignalGet::get) the data in an effect and pass it in to `id.update_state()` and then handle that data in the `update` method of the View trait.
///
/// For example a minimal slider might look like the following. First, we define the struct that contains the [ViewId](crate::ViewId).
/// Then, we use a function to construct the slider. As part of this function we create an effect that will be re-run every time the signals in the  `percent` closure change.
/// In the effect we send the change to the associated [ViewId](crate::ViewId). This change can then be handled in the [View::update](crate::View::update) method.
/// ```rust
/// # use floem::{*, views::*, reactive::*};
///
/// struct Slider {
///     id: ViewId,
///     percent: f32,
/// }
/// pub fn slider(percent: impl Fn() -> f32 + 'static) -> Slider {
///     let id = ViewId::new();
///
///     // If the following effect is not created, and `percent` is accessed directly,
///     // `percent` will only be accessed a single time and will not be reactive.
///     // Therefore the following `create_effect` is necessary for reactivity.
///     create_effect(move |_| {
///         let percent = percent();
///         id.update_state(percent);
///     });
///     Slider { id, percent: 0.0 }
/// }
/// impl View for Slider {
///     fn id(&self) -> ViewId {
///         self.id
///     }
///
///     fn update(&mut self, cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
///         if let Ok(percent) = state.downcast::<f32>() {
///             self.percent = *percent;
///             self.id.request_layout();
///         }
///     }
/// }
/// ```
pub trait View {
    fn id(&self) -> ViewId;

    fn view_style(&self) -> Option<Style> {
        None
    }

    fn view_class(&self) -> Option<StyleClassRef> {
        None
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        core::any::type_name::<Self>().into()
    }

    /// Use this method to react to changes in view-related state.
    /// You will usually send state to this hook manually using the `View`'s `Id` handle
    ///
    /// ```ignore
    /// self.id.update_state(SomeState)
    /// ```
    ///
    /// You are in charge of downcasting the state to the expected type.
    ///
    /// If the update needs other passes to run you're expected to call
    /// `_cx.app_state_mut().request_changes`.
    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        // these are here to just ignore these arguments in the default case
        let _ = cx;
        let _ = state;
    }

    /// Use this method to style the view's children.
    ///
    /// If the style changes needs other passes to run you're expected to call
    /// `cx.app_state_mut().request_changes`.
    fn style_pass(&mut self, cx: &mut StyleCx<'_>) {
        for child in self.id().children() {
            cx.style_view(child);
        }
    }

    /// Use this method to layout the view's children.
    /// Usually you'll do this by calling `LayoutCx::layout_node`.
    ///
    /// If the layout changes needs other passes to run you're expected to call
    /// `cx.app_state_mut().request_changes`.
    fn layout(&mut self, cx: &mut LayoutCx) -> NodeId {
        recursively_layout_view(self.id(), cx)
    }

    /// Responsible for computing the layout of the view's children.
    ///
    /// If the layout changes needs other passes to run you're expected to call
    /// `cx.app_state_mut().request_changes`.
    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        default_compute_layout(self.id(), cx)
    }

    fn event_before_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        // these are here to just ignore these arguments in the default case
        let _ = cx;
        let _ = event;

        EventPropagation::Continue
    }

    fn event_after_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        // these are here to just ignore these arguments in the default case
        let _ = cx;
        let _ = event;

        EventPropagation::Continue
    }

    /// `View`-specific implementation. Will be called in [`PaintCx::paint_view`](crate::context::PaintCx::paint_view).
    /// Usually you'll call `paint_view` for every child view. But you might also draw text, adjust the offset, clip
    /// or draw text.
    fn paint(&mut self, cx: &mut PaintCx) {
        cx.paint_children(self.id());
    }

    /// Scrolls the view and all direct and indirect children to bring the `target` view to be
    /// visible. Returns true if this view contains or is the target.
    fn scroll_to(&mut self, cx: &mut AppState, target: ViewId, rect: Option<Rect>) -> bool {
        if self.id() == target {
            return true;
        }
        let mut found = false;

        for child in self.id().children() {
            found |= child.view().borrow_mut().scroll_to(cx, target, rect);
        }
        found
    }
}

impl View for Box<dyn View> {
    fn id(&self) -> ViewId {
        (**self).id()
    }

    fn view_style(&self) -> Option<Style> {
        (**self).view_style()
    }

    fn view_class(&self) -> Option<StyleClassRef> {
        (**self).view_class()
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        (**self).debug_name()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        (**self).update(cx, state)
    }

    fn style_pass(&mut self, cx: &mut StyleCx) {
        (**self).style_pass(cx)
    }

    fn layout(&mut self, cx: &mut LayoutCx) -> NodeId {
        (**self).layout(cx)
    }

    fn event_before_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        (**self).event_before_children(cx, event)
    }

    fn event_after_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        (**self).event_after_children(cx, event)
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        (**self).compute_layout(cx)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        (**self).paint(cx)
    }

    fn scroll_to(&mut self, cx: &mut AppState, target: ViewId, rect: Option<Rect>) -> bool {
        (**self).scroll_to(cx, target, rect)
    }
}

/// Computes the layout of the view's children, if any.
pub fn default_compute_layout(id: ViewId, cx: &mut ComputeLayoutCx) -> Option<Rect> {
    let mut layout_rect: Option<Rect> = None;
    for child in id.children() {
        if !child.style_has_hidden() {
            let child_layout = cx.compute_view_layout(child);
            if let Some(child_layout) = child_layout {
                if let Some(rect) = layout_rect {
                    layout_rect = Some(rect.union(child_layout));
                } else {
                    layout_rect = Some(child_layout);
                }
            }
        }
    }
    layout_rect
}

pub(crate) fn paint_bg(cx: &mut PaintCx, style: &ViewStyleProps, size: Size) {
    let radius = match style.border_radius() {
        crate::unit::PxPct::Px(px) => px,
        crate::unit::PxPct::Pct(pct) => size.min_side() * (pct / 100.),
    };
    if radius > 0.0 {
        let rect = size.to_rect();
        let width = rect.width();
        let height = rect.height();
        if width > 0.0 && height > 0.0 && radius > width.max(height) / 2.0 {
            let radius = width.max(height) / 2.0;
            let circle = Circle::new(rect.center(), radius);
            let bg = match style.background() {
                Some(color) => color,
                None => return,
            };
            cx.fill(&circle, &bg, 0.0);
        } else {
            paint_box_shadow(cx, style, rect, Some(radius));
            let bg = match style.background() {
                Some(color) => color,
                None => return,
            };
            let rounded_rect = rect.to_rounded_rect(radius);
            cx.fill(&rounded_rect, &bg, 0.0);
        }
    } else {
        paint_box_shadow(cx, style, size.to_rect(), None);
        let bg = match style.background() {
            Some(color) => color,
            None => return,
        };
        cx.fill(&size.to_rect(), &bg, 0.0);
    }
}

fn paint_box_shadow(
    cx: &mut PaintCx,
    style: &ViewStyleProps,
    rect: Rect,
    rect_radius: Option<f64>,
) {
    if let Some(shadow) = &style.shadow() {
        let min = rect.size().min_side();
        let h_offset = match shadow.h_offset {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let v_offset = match shadow.v_offset {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let spread = match shadow.spread {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let blur_radius = match shadow.blur_radius {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let inset = Insets::new(
            -h_offset / 2.0,
            -v_offset / 2.0,
            h_offset / 2.0,
            v_offset / 2.0,
        );
        let rect = rect.inflate(spread, spread).inset(inset);
        if let Some(radii) = rect_radius {
            let rounded_rect = RoundedRect::from_rect(rect, radii + spread);
            cx.fill(&rounded_rect, shadow.color, blur_radius);
        } else {
            cx.fill(&rect, shadow.color, blur_radius);
        }
    }
}

pub(crate) fn paint_outline(cx: &mut PaintCx, style: &ViewStyleProps, size: Size) {
    let outline = &style.outline().0;
    if outline.width == 0. {
        return;
    }

    let half = outline.width / 2.0;
    let rect = size.to_rect().inflate(half, half);
    let border_radius = match style.border_radius() {
        crate::unit::PxPct::Px(px) => px,
        crate::unit::PxPct::Pct(pct) => size.min_side() * (pct / 100.),
    };
    let Pct(outline_progress) = style.outline_progress();

    // Fast path for complete outline
    if outline_progress >= 100.0 {
        cx.stroke(
            &rect.to_rounded_rect(border_radius + half),
            &style.outline_color(),
            outline,
        );
        return;
    }

    // Create the path with explicit move_to
    let mut path = BezPath::new();
    let rounded_rect = rect.to_rounded_rect(border_radius + half);
    path.move_to(rounded_rect.origin());
    for seg in rounded_rect.path_segments(0.1) {
        path.push(seg.as_path_el());
    }

    let segments: Vec<_> = path.path_segments(0.1).collect();
    let segment_lengths: Vec<_> = segments.iter().map(|seg| seg.perimeter(0.1)).collect();
    let total_length: f64 = segment_lengths.iter().sum();
    let target_length = total_length * (outline_progress / 100.0);

    let mut result_path = BezPath::new();
    let mut current_length = 0.0;

    // Handle first segment to ensure proper move_to
    if let Some((first_seg, &first_len)) = segments.iter().zip(&segment_lengths).next() {
        if first_len <= target_length {
            result_path.move_to(first_seg.start());
            result_path.push(first_seg.as_path_el());
            current_length = first_len;
        } else {
            let t = target_length / first_len;
            let partial = first_seg.subsegment(0.0..t);
            result_path.move_to(first_seg.start());
            result_path.push(partial.as_path_el());
            return cx.stroke(&result_path, &style.outline_color(), outline);
        }
    }

    // Handle remaining segments
    for (seg, &seg_length) in segments.iter().zip(&segment_lengths).skip(1) {
        if current_length + seg_length <= target_length {
            result_path.push(seg.as_path_el());
            current_length += seg_length;
        } else {
            let t = (target_length - current_length) / seg_length;
            let partial = seg.subsegment(0.0..t);
            result_path.push(partial.as_path_el());
            break;
        }
    }

    cx.stroke(&result_path, &style.outline_color(), outline);
}

#[cfg(not(feature = "vello"))]
pub(crate) fn paint_border(
    cx: &mut PaintCx,
    layout_style: &LayoutProps,
    style: &ViewStyleProps,
    size: Size,
) {
    let left = layout_style.border_left().0;
    let top = layout_style.border_top().0;
    let right = layout_style.border_right().0;
    let bottom = layout_style.border_bottom().0;

    let border_color = style.border_color();
    if left.width == top.width
        && top.width == right.width
        && right.width == bottom.width
        && bottom.width == left.width
        && left.width > 0.0
    {
        let half = left.width / 2.0;
        let rect = size.to_rect().inflate(-half, -half);
        let radius = match style.border_radius() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => size.min_side() * (pct / 100.),
        };
        if radius > 0.0 {
            let radius = (radius - half).max(0.0);
            cx.stroke(&rect.to_rounded_rect(radius), &border_color, &left);
        } else {
            cx.stroke(&rect, &border_color, &left);
        }
    } else {
        // TODO: now with vello should we do this left.width > 0. check?
        if left.width > 0.0 {
            let half = left.width / 2.0;
            cx.stroke(
                &Line::new(Point::new(half, 0.0), Point::new(half, size.height)),
                &border_color,
                &left,
            );
        }
        if right.width > 0.0 {
            let half = right.width / 2.0;
            cx.stroke(
                &Line::new(
                    Point::new(size.width - half, 0.0),
                    Point::new(size.width - half, size.height),
                ),
                &border_color,
                &right,
            );
        }
        if top.width > 0.0 {
            let half = top.width / 2.0;
            cx.stroke(
                &Line::new(Point::new(0.0, half), Point::new(size.width, half)),
                &border_color,
                &top,
            );
        }
        if bottom.width > 0.0 {
            let half = bottom.width / 2.0;
            cx.stroke(
                &Line::new(
                    Point::new(0.0, size.height - half),
                    Point::new(size.width, size.height - half),
                ),
                &border_color,
                &bottom,
            );
        }
    }
}

#[cfg(feature = "vello")]
pub(crate) fn paint_border(
    cx: &mut PaintCx,
    layout_style: &LayoutProps,
    style: &ViewStyleProps,
    size: Size,
) {
    let borders = [
        layout_style.border_left().0,
        layout_style.border_top().0,
        layout_style.border_right().0,
        layout_style.border_bottom().0,
    ];

    // Early return if no borders
    if borders.iter().all(|b| b.width == 0.0) {
        return;
    }

    let border_color = style.border_color();
    let Pct(border_progress) = style.border_progress();

    // Check if borders match before doing any other work
    let borders_match = borders
        .windows(2)
        .all(|s| s[0].width == s[1].width && s[0].dash_pattern == s[1].dash_pattern);

    // For the simple case, we don't need to calculate radii unless we have to
    if borders_match && border_progress >= 100. {
        let half_width = borders[0].width / 2.0;
        let rect = size.to_rect().inflate(-half_width, -half_width);

        let radius = match style.border_radius() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => size.min_side() * (pct / 100.),
        };
        let adjusted_radius = (radius - half_width).max(0.0);

        return cx.stroke(
            &rect.to_rounded_rect(adjusted_radius),
            &border_color,
            &borders[0],
        );
    }

    // For complex cases, now we need to set up the border path
    let half_width = borders[0].width / 2.0;
    let rect = size.to_rect().inflate(-half_width, -half_width);

    let radius = match style.border_radius() {
        crate::unit::PxPct::Px(px) => px,
        crate::unit::PxPct::Pct(pct) => size.min_side() * (pct / 100.),
    };
    let adjusted_radius = (radius - half_width).max(0.0);
    let radii = RoundedRectRadii::from_single_radius(adjusted_radius);

    let mut border_path = BorderPath::new(rect, radii);

    // Only create subsegment if needed
    if border_progress < 100. {
        border_path.subsegment(0.0..(border_progress / 100.));
    }

    let mut current_path = Vec::new();
    for event in border_path.path_elements(&borders, 0.1) {
        match event {
            BorderPathEvent::PathElement(el) => current_path.push(el),
            BorderPathEvent::NewStroke(stroke) => {
                // Render current path with previous stroke if any
                if !current_path.is_empty() {
                    cx.stroke(&current_path.as_slice(), &border_color, &stroke);
                    current_path.clear();
                } else {
                }
            }
        }
    }
}

/// Tab navigation finds the next or previous view with the `keyboard_navigatable` status in the tree.
#[allow(dead_code)]
pub(crate) fn view_tab_navigation(root_view: ViewId, app_state: &mut AppState, backwards: bool) {
    let start = app_state
        .focus
        .unwrap_or(app_state.prev_focus.unwrap_or(root_view));

    let tree_iter = |id: ViewId| {
        if backwards {
            view_tree_previous(root_view, id).unwrap_or_else(|| view_nested_last_child(root_view))
        } else {
            view_tree_next(id).unwrap_or(root_view)
        }
    };

    let mut new_focus = tree_iter(start);
    while new_focus != start && !app_state.can_focus(new_focus) {
        new_focus = tree_iter(new_focus);
    }

    app_state.clear_focus();
    app_state.update_focus(new_focus, true);
}

/// Get the next item in the tree, either the first child or the next sibling of this view or of the first parent view
fn view_tree_next(id: ViewId) -> Option<ViewId> {
    if let Some(child) = id.children().into_iter().next() {
        return Some(child);
    }

    let mut ancestor = id;
    loop {
        if let Some(next_sibling) = view_next_sibling(ancestor) {
            return Some(next_sibling);
        }
        ancestor = ancestor.parent()?;
    }
}

/// Get the id of the view after this one (but with the same parent and level of nesting)
fn view_next_sibling(id: ViewId) -> Option<ViewId> {
    let parent = id.parent();

    let Some(parent) = parent else {
        // We're the root, which has no sibling
        return None;
    };

    let children = parent.children();
    //TODO: Log a warning if the child isn't found. This shouldn't happen (error in floem if it does), but this shouldn't panic if that does happen
    let pos = children.iter().position(|v| v == &id)?;

    if pos + 1 < children.len() {
        Some(children[pos + 1])
    } else {
        None
    }
}

/// Get the next item in the tree, the deepest last child of the previous sibling of this view or the parent
fn view_tree_previous(root_view: ViewId, id: ViewId) -> Option<ViewId> {
    view_previous_sibling(id)
        .map(view_nested_last_child)
        .or_else(|| {
            (root_view != id).then_some(
                id.parent()
                    .unwrap_or_else(|| view_nested_last_child(root_view)),
            )
        })
}

/// Get the id of the view before this one (but with the same parent and level of nesting)
fn view_previous_sibling(id: ViewId) -> Option<ViewId> {
    let parent = id.parent();

    let Some(parent) = parent else {
        // We're the root, which has no sibling
        return None;
    };

    let children = parent.children();
    let pos = children.iter().position(|v| v == &id).unwrap();

    if pos > 0 {
        Some(children[pos - 1])
    } else {
        None
    }
}

fn view_nested_last_child(view: ViewId) -> ViewId {
    let mut last_child = view;
    while let Some(new_last_child) = last_child.children().pop() {
        last_child = new_last_child;
    }
    last_child
}

/// Produces an ascii art debug display of all of the views.
#[allow(dead_code)]
pub(crate) fn view_debug_tree(root_view: ViewId) {
    let mut views = vec![(root_view, Vec::new())];
    while let Some((current_view, active_lines)) = views.pop() {
        // Ascii art for the tree view
        if let Some((leaf, root)) = active_lines.split_last() {
            for line in root {
                print!("{}", if *line { "│   " } else { "    " });
            }
            print!("{}", if *leaf { "├── " } else { "└── " });
        }
        println!(
            "{:?} {}",
            current_view,
            current_view.view().borrow().debug_name()
        );

        let mut children = current_view.children();
        if let Some(last_child) = children.pop() {
            views.push((last_child, [active_lines.as_slice(), &[false]].concat()));
        }

        views.extend(
            children
                .into_iter()
                .rev()
                .map(|child| (child, [active_lines.as_slice(), &[true]].concat())),
        );
    }
}

pub struct BorderPath {
    path_iter: RoundedRectPathIter,
    range: Range<f64>,
    // segment_lengths: [f64; 8],
    // total_length: f64,
}

impl BorderPath {
    pub fn new(rect: Rect, radii: RoundedRectRadii) -> Self {
        let rounded_path = RectPathIter {
            idx: 0,
            rect,
            radii,
        }
        .rounded_rect_path();

        Self {
            path_iter: rounded_path,
            range: 0.0..1.0,
        }
    }

    pub fn subsegment(&mut self, range: Range<f64>) {
        self.range = range;
    }

    pub fn path_elements<'a>(
        &'a mut self,
        strokes: &'a [Stroke; 4],
        tolerance: f64,
    ) -> BorderPathIter<'a> {
        let total_len = self.path_iter.rect.total_len(tolerance);
        BorderPathIter {
            border_path: self,
            tolerance,
            current_len: 0.,
            current_iter: None,
            stroke_iter: strokes.iter().peekable(),
            current_stroke: strokes.get(0).unwrap(),
            emitted_finished: false,
            already_did_normalized: false,
            total_len,
        }
    }
}

/// Returns a new Arc that represents a subsegment of this arc.
/// The range should be between 0.0 and 1.0, where:
/// - 0.0 represents the start of the original arc
/// - 1.0 represents the end of the original arc
pub fn arc_subsegment(arc: &Arc, range: Range<f64>) -> Arc {
    // Clamp the range to ensure it's within [0.0, 1.0]
    let start = range.start.clamp(0.0, 1.0);
    let end = range.end.clamp(0.0, 1.0);

    // Calculate the new start and sweep angles
    let total_sweep = arc.sweep_angle;
    let new_start_angle = arc.start_angle + total_sweep * start;
    let new_sweep_angle = total_sweep * (end - start);

    Arc {
        // These properties remain unchanged
        center: arc.center,
        radii: arc.radii,
        x_rotation: arc.x_rotation,
        // These are adjusted for the subsegment
        start_angle: new_start_angle,
        sweep_angle: new_sweep_angle,
    }
}

// First define the enum for our iterator output
pub enum BorderPathEvent<'a> {
    PathElement(PathEl),
    NewStroke(&'a Stroke),
}

pub struct BorderPathIter<'a> {
    border_path: &'a mut BorderPath,
    tolerance: f64,
    current_len: f64,
    current_iter: Option<Box<dyn Iterator<Item = PathEl> + 'a>>,
    stroke_iter: Peekable<std::slice::Iter<'a, Stroke>>,
    current_stroke: &'a Stroke,
    emitted_finished: bool,
    already_did_normalized: bool,
    total_len: f64,
}

impl<'a> Iterator for BorderPathIter<'a> {
    type Item = BorderPathEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let end = self.border_path.range.end; // This is now 0.0..1.0

        if (self.current_len / self.total_len) >= end {
            if self.emitted_finished {
                return None;
            } else {
                self.emitted_finished = true;
                return self.stroke_iter.next().map(BorderPathEvent::NewStroke);
            }
        }

        // Handle current iterator if it exists
        if let Some(iter) = &mut self.current_iter {
            if let Some(element) = iter.next() {
                return Some(BorderPathEvent::PathElement(element));
            }
            self.current_iter = None;
        }

        if self.already_did_normalized {
            if self.emitted_finished {
                return None;
            } else {
                self.emitted_finished = true;
                return self.stroke_iter.next().map(BorderPathEvent::NewStroke);
            }
        }

        let next_seg = self.border_path.path_iter.next();
        match next_seg {
            Some(ArcOrPath::Arc(arc)) => {
                let arc_len = arc.perimeter(self.tolerance);
                let normalized_current = self.current_len / self.total_len;
                let normalized_seg_len = arc_len / self.total_len;

                if normalized_current + normalized_seg_len > end {
                    self.already_did_normalized = true;
                    // Need to subsegment
                    let remaining_percentage = end - normalized_current;
                    let t = remaining_percentage / normalized_seg_len;
                    let subseg = arc_subsegment(&arc, 0.0..t);
                    self.current_len += subseg.perimeter(self.tolerance);
                    self.current_iter = Some(Box::new(subseg.path_elements(self.tolerance)));
                } else {
                    self.current_len += arc_len;
                    self.current_iter = Some(Box::new(arc.path_elements(self.tolerance)))
                }
            }
            Some(ArcOrPath::Path(path_seg)) => {
                let seg_len = path_seg.arclen(self.tolerance);
                let normalized_current = self.current_len / self.total_len;
                let normalized_seg_len = seg_len / self.total_len;

                if normalized_current + normalized_seg_len > end {
                    self.already_did_normalized = true;
                    // Need to subsegment
                    let remaining_percentage = end - normalized_current;
                    let t = remaining_percentage / normalized_seg_len;
                    let subseg = path_seg.subsegment(0.0..t);
                    self.current_len += subseg.arclen(self.tolerance);
                    self.current_iter = Some(Box::new(std::iter::once(subseg.as_path_el())));
                } else {
                    self.current_len += seg_len;
                    self.current_iter = Some(Box::new(std::iter::once(path_seg.as_path_el())));
                }
            }
            None => {}
        }

        let next_stroke = self.stroke_iter.peek().unwrap();
        let current_stroke = self.current_stroke;
        if current_stroke.width != next_stroke.width
            || current_stroke.dash_pattern != next_stroke.dash_pattern
        {
            self.current_stroke = self.stroke_iter.next().unwrap();
            return Some(BorderPathEvent::NewStroke(&current_stroke));
        }

        // Get first element from new iterator
        if let Some(iter) = &mut self.current_iter {
            let el = iter.next().unwrap();
            Some(BorderPathEvent::PathElement(el))
        } else {
            None
        }
    }
}

// Taken from kurbo
struct RectPathIter {
    rect: Rect,
    radii: RoundedRectRadii,
    idx: usize,
}

// This is clockwise in a y-down coordinate system for positive area.
impl Iterator for RectPathIter {
    type Item = PathSeg;
    fn next(&mut self) -> Option<PathSeg> {
        self.idx += 1;
        match self.idx {
            1 => Some(PathSeg::Line(Line::new(
                // Top edge - horizontal line
                Point::new(self.rect.x0 + self.radii.top_left, self.rect.y0), // Start after top-left corner
                Point::new(self.rect.x1 - self.radii.top_right, self.rect.y0), // End before top-right corner
            ))),
            2 => Some(PathSeg::Line(Line::new(
                // Right edge - vertical line
                Point::new(self.rect.x1, self.rect.y0 + self.radii.top_right), // Start after top-right corner
                Point::new(self.rect.x1, self.rect.y1 - self.radii.bottom_right), // End before bottom-right corner
            ))),
            3 => Some(PathSeg::Line(Line::new(
                // Bottom edge - horizontal line
                Point::new(self.rect.x1 - self.radii.bottom_right, self.rect.y1), // Start after bottom-right corner
                Point::new(self.rect.x0 + self.radii.bottom_left, self.rect.y1), // End before bottom-left corner
            ))),
            4 => Some(PathSeg::Line(Line::new(
                // Left edge - vertical line
                Point::new(self.rect.x0, self.rect.y1 - self.radii.bottom_left), // Start after bottom-left corner
                Point::new(self.rect.x0, self.rect.y0 + self.radii.top_left), // End before top-left corner
            ))),
            _ => None,
        }
    }
}

impl RectPathIter {
    fn build_corner_arc(&self, corner_idx: usize) -> Arc {
        let (center, radius) = match corner_idx {
            0 => (
                // top-left
                Point {
                    x: self.rect.x0 + self.radii.top_left,
                    y: self.rect.y0 + self.radii.top_left,
                },
                self.radii.top_left,
            ),
            1 => (
                // top-right
                Point {
                    x: self.rect.x1 - self.radii.top_right,
                    y: self.rect.y0 + self.radii.top_right,
                },
                self.radii.top_right,
            ),
            2 => (
                // bottom-right
                Point {
                    x: self.rect.x1 - self.radii.bottom_right,
                    y: self.rect.y1 - self.radii.bottom_right,
                },
                self.radii.bottom_right,
            ),
            3 => (
                // bottom-left
                Point {
                    x: self.rect.x0 + self.radii.bottom_left,
                    y: self.rect.y1 - self.radii.bottom_left,
                },
                self.radii.bottom_left,
            ),
            _ => unreachable!(),
        };

        Arc {
            center,
            radii: Vec2 {
                x: radius,
                y: radius,
            },
            start_angle: FRAC_PI_2 * ((corner_idx + 2) % 4) as f64,
            sweep_angle: FRAC_PI_2,
            x_rotation: 0.0,
        }
    }

    fn total_len(&self, tolerance: f64) -> f64 {
        // Calculate arc lengths - one for each corner
        let arc_lengths: f64 = (0..4)
            .map(|i| self.build_corner_arc(i).perimeter(tolerance))
            .sum();

        // Calculate straight segment lengths
        let straight_lengths = {
            let rect = self.rect;
            let radii = self.radii;
            // Top edge (minus the arc segments)
            let top = rect.x1 - rect.x0 - radii.top_left - radii.top_right;
            // Right edge
            let right = rect.y1 - rect.y0 - radii.top_right - radii.bottom_right;
            // Bottom edge
            let bottom = rect.x1 - rect.x0 - radii.bottom_left - radii.bottom_right;
            // Left edge
            let left = rect.y1 - rect.y0 - radii.top_left - radii.bottom_left;

            top + right + bottom + left
        };

        arc_lengths + straight_lengths
    }

    fn rounded_rect_path(&self) -> RoundedRectPathIter {
        // Note: order follows the rectangle path iterator.
        let arcs = [
            self.build_corner_arc(0),
            self.build_corner_arc(1),
            self.build_corner_arc(2),
            self.build_corner_arc(3),
        ];

        let rect = RectPathIter {
            rect: self.rect,
            idx: 0,
            radii: self.radii,
        };

        RoundedRectPathIter { idx: 0, rect, arcs }
    }
}

pub struct RoundedRectPathIter {
    idx: usize,
    rect: RectPathIter,
    arcs: [Arc; 4],
}
#[derive(Debug)]
pub enum ArcOrPath {
    Arc(Arc),
    Path(PathSeg),
}
// This is clockwise in a y-down coordinate system for positive area.
impl Iterator for RoundedRectPathIter {
    type Item = ArcOrPath;

    fn next(&mut self) -> Option<Self::Item> {
        // The total sequence is:
        // 0. First arc (idx = 0)
        // 1. LineTo from rect
        // 2. Second arc (idx = 1)
        // 3. LineTo from rect
        // 4. Third arc (idx = 2)
        // 5. LineTo from rect
        // 6. Fourth arc (idx = 3)
        // 7. Final LineTo from rect

        if self.idx >= 9 {
            return None;
        }

        // Odd indices (1, 3, 5, 7) are from rect iterator
        if self.idx % 2 != 0 {
            let path_el = self.rect.next().map(ArcOrPath::Path);
            self.idx += 1;
            return path_el;
        }

        // Even indices (0, 2, 4, 6) are from arc iterators
        let arc_idx = self.idx / 2;
        if arc_idx < self.arcs.len() {
            self.idx += 1;
            Some(ArcOrPath::Arc(self.arcs[arc_idx].clone()))
        } else {
            None
        }
    }
}
