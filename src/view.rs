//! # View and Widget Traits
//! Views are self-contained components that can be composed together to create complex UIs.
//! Views are the main building blocks of Floem.
//!
//! Views are structs that implement the [`View`] and [`Widget`] traits. Many of these structs will also contain a child field that also implements [`View`]. In this way, views can be composed together easily to create complex UIs. This is the most common way to build UIs in Floem. For more information on how to compose views check out the [views](crate::views) module.
//!
//! Creating a struct and manually implementing the [`View`] and [`Widget`] traits is typically only needed for building new widgets and for special cases. The rest of this module documentation is for help when manually implementing [`View`] and [`Widget`] on your own types.
//!
//!
//! ## The View and Widget Traits
//! The [`View`] trait is the trait that Floem uses to build  and display elements, and it builds on the [`Widget`] trait. The [`Widget`] trait contains the methods for implementing updates, styling, layout, events, and painting.
//! Eventually, the goal is for Floem to integrate the [`Widget`] trait with other rust UI libraries so that the widget layer can be shared among all compatible UI libraries.
//!
//! ## State management
//!
//! For all reactive state that your type contains, either in the form of signals or derived signals, you need to process the changes within an effect.
//! The most common pattern is to [`get`](floem_reactive::ReadSignal::get) the data in an effect and pass it in to `id.update_state()` and then handle that data in the `update` method of the View trait.
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
use peniko::kurbo::*;
use std::any::Any;
use understory_box_tree::{NodeId, QueryFilter};
use understory_responder::{
    adapters::box_tree::navigation::{next_depth_first_filtered, prev_depth_first_filtered},
    types::{Outcome, Phase},
};

use crate::{
    Renderer,
    context::{EventCx, PaintCx, StyleCx, UpdateCx},
    event::{Event, EventPropagation},
    id::ViewId,
    style::{LayoutProps, Style, StyleClassRef},
    unit::PxPct,
    view_state::ViewStyleProps,
    view_storage::VIEW_STORAGE,
    views::{DynamicView, dyn_view},
    window_state::WindowState,
};

/// type erased [`View`]
///
/// Views in Floem are strongly typed. [`AnyView`] allows you to escape the strong typing by converting any type implementing [`View`] into the [`AnyView`] type.
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
/// the else must return the same type. However the branches return different types. The solution to this is to use the [`IntoView::into_any`] method
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

/// The View trait contains the methods for implementing updates, styling, layout, events, and painting.
///
/// The [`id`](View::id) method must be implemented.
/// The other methods may be implemented as necessary to implement the functionality of the View.
/// ## State Management in a Custom View
///
/// For all reactive state that your type contains, either in the form of signals or derived signals, you need to process the changes within an effect.
/// The most common pattern is to [`get`](floem_reactive::SignalGet::get) the data in an effect and pass it in to `id.update_state()` and then handle that data in the `update` method of the `View` trait.
///
/// For example a minimal slider might look like the following. First, we define the struct that contains the [`ViewId`](crate::ViewId).
/// Then, we use a function to construct the slider. As part of this function we create an effect that will be re-run every time the signals in the  `percent` closure change.
/// In the effect we send the change to the associated [`ViewId`](crate::ViewId). This change can then be handled in the [`View::update`](crate::View::update) method.
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
    /// `_cx.window_state_mut().request_changes`.
    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        // these are here to just ignore these arguments in the default case
        let _ = cx;
        let _ = state;
    }

    /// Use this method to style the view's children.
    ///
    /// If the style changes needs other passes to run you're expected to call
    /// `cx.window_state_mut().request_changes`.
    fn style_pass(&mut self, cx: &mut StyleCx<'_>) {
        for child in self.id().children() {
            cx.style_view(child);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event, phase: Phase) -> EventPropagation {
        let _ = (cx, event, phase);
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
    fn scroll_to(&mut self, cx: &mut WindowState, target: ViewId, rect: Option<Rect>) -> bool {
        if self.id() == target {
            return true;
        }
        let mut found = false;

        for child in self.id().children() {
            found |= child.view().borrow_mut().scroll_to(cx, target, rect);
        }
        found
    }

    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;
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

    fn event(&mut self, cx: &mut EventCx, event: &Event, phase: Phase) -> EventPropagation {
        (**self).event(cx, event, phase)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        (**self).paint(cx)
    }

    fn scroll_to(&mut self, cx: &mut WindowState, target: ViewId, rect: Option<Rect>) -> bool {
        (**self).scroll_to(cx, target, rect)
    }

    fn as_any(&self) -> &dyn Any {
        (**self).as_any()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        (**self).as_any_mut()
    }
}

pub(crate) fn border_radius(radius: crate::unit::PxPct, size: f64) -> f64 {
    match radius {
        crate::unit::PxPct::Px(px) => px,
        crate::unit::PxPct::Pct(pct) => size * (pct / 100.),
    }
}

fn border_to_radii_view(style: &ViewStyleProps, size: Size) -> RoundedRectRadii {
    let border_radii = style.border_radius();
    RoundedRectRadii {
        top_left: border_radius(
            border_radii.top_left.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        top_right: border_radius(
            border_radii.top_right.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        bottom_left: border_radius(
            border_radii.bottom_left.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        bottom_right: border_radius(
            border_radii.bottom_right.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
    }
}

pub(crate) fn border_to_radii(style: &Style, size: Size) -> RoundedRectRadii {
    let border_radii = style.get(crate::style::BorderRadiusProp);
    RoundedRectRadii {
        top_left: border_radius(
            border_radii.top_left.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        top_right: border_radius(
            border_radii.top_right.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        bottom_left: border_radius(
            border_radii.bottom_left.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        bottom_right: border_radius(
            border_radii.bottom_right.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
    }
}

pub(crate) fn paint_bg(cx: &mut PaintCx, style: &ViewStyleProps, size: Size) {
    let radii = border_to_radii_view(style, size);
    if radii_max(radii) > 0.0 {
        let rect = size.to_rect();
        let width = rect.width();
        let height = rect.height();
        if width > 0.0 && height > 0.0 && radii_min(radii) > width.max(height) / 2.0 {
            let radius = width.max(height) / 2.0;
            let circle = Circle::new(rect.center(), radius);
            let bg = match style.background() {
                Some(color) => color,
                None => return,
            };
            cx.fill(&circle, &bg, 0.0);
        } else {
            paint_box_shadow(cx, style, rect, Some(radii));
            let bg = match style.background() {
                Some(color) => color,
                None => return,
            };
            let rounded_rect = rect.to_rounded_rect(radii);
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
    rect_radius: Option<RoundedRectRadii>,
) {
    for shadow in &style.shadow() {
        let min = rect.size().min_side();
        let left_offset = match shadow.left_offset {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let right_offset = match shadow.right_offset {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let top_offset = match shadow.top_offset {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let bottom_offset = match shadow.bottom_offset {
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
            left_offset / 2.0,
            top_offset / 2.0,
            right_offset / 2.0,
            bottom_offset / 2.0,
        );
        let rect = rect.inflate(spread, spread).inset(inset);
        if let Some(radii) = rect_radius {
            let rounded_rect = RoundedRect::from_rect(rect, radii_add(radii, spread));
            cx.fill(&rounded_rect, shadow.color, blur_radius);
        } else {
            cx.fill(&rect, shadow.color, blur_radius);
        }
    }
}
#[cfg(feature = "vello")]
pub(crate) fn paint_outline(cx: &mut PaintCx, style: &ViewStyleProps, size: Size) {
    use crate::{
        border_path_iter::{BorderPath, BorderPathEvent},
        unit::Pct,
    };

    let outlines = [
        (style.outline().0, style.outline_color()),
        (style.outline().0, style.outline_color()),
        (style.outline().0, style.outline_color()),
        (style.outline().0, style.outline_color()),
    ];

    // Early return if no outlines
    if outlines.iter().any(|o| o.0.width == 0.0) {
        return;
    }

    let outline_color = style.outline_color();
    let Pct(outline_progress) = style.outline_progress();

    let half_width = outlines[0].0.width / 2.0;
    let rect = size.to_rect().inflate(half_width, half_width);

    let radii = radii_map(border_to_radii_view(style, size), |r| {
        (r + half_width).max(0.0)
    });

    let mut outline_path = BorderPath::new(rect, radii);

    // Only create subsegment if needed
    if outline_progress < 100. {
        outline_path.subsegment(0.0..(outline_progress.clamp(0.0, 100.) / 100.));
    }

    let mut current_path = Vec::new();
    for event in outline_path.path_elements(&outlines, 0.1) {
        match event {
            BorderPathEvent::PathElement(el) => current_path.push(el),
            BorderPathEvent::NewStroke(stroke) => {
                // Render current path with previous stroke if any
                if !current_path.is_empty() {
                    cx.stroke(&current_path.as_slice(), &outline_color, &stroke.0);
                    current_path.clear();
                }
            }
        }
    }
    assert!(current_path.is_empty());
}

#[cfg(not(feature = "vello"))]
pub(crate) fn paint_outline(cx: &mut PaintCx, style: &ViewStyleProps, size: Size) {
    let outline = &style.outline().0;
    if outline.width == 0. {
        // TODO: we should warn! when outline is < 0
        return;
    }
    let half = outline.width / 2.0;
    let rect = size.to_rect().inflate(half, half);
    let border_radii = border_to_radii_view(style, size);
    cx.stroke(
        &rect.to_rounded_rect(radii_add(border_radii, half)),
        &style.outline_color(),
        outline,
    );
}

#[cfg(not(feature = "vello"))]
pub(crate) fn paint_border(
    cx: &mut PaintCx,
    layout_style: &LayoutProps,
    style: &ViewStyleProps,
    size: Size,
) {
    let border = layout_style.border();

    let left = border.left.map(|v| v.0).unwrap_or(Stroke::new(0.));
    let top = border.top.map(|v| v.0).unwrap_or(Stroke::new(0.));
    let right = border.right.map(|v| v.0).unwrap_or(Stroke::new(0.));
    let bottom = border.bottom.map(|v| v.0).unwrap_or(Stroke::new(0.));

    if left.width == top.width
        && top.width == right.width
        && right.width == bottom.width
        && bottom.width == left.width
        && left.width > 0.0
        && style.border_color().left.is_some()
        && style.border_color().top.is_some()
        && style.border_color().right.is_some()
        && style.border_color().bottom.is_some()
        && style.border_color().left == style.border_color().top
        && style.border_color().top == style.border_color().right
        && style.border_color().right == style.border_color().bottom
    {
        let half = left.width / 2.0;
        let rect = size.to_rect().inflate(-half, -half);
        let radii = border_to_radii_view(style, size);
        if let Some(color) = style.border_color().left {
            if radii_max(radii) > 0.0 {
                let radii = radii_map(radii, |r| (r - half).max(0.0));
                cx.stroke(&rect.to_rounded_rect(radii), &color, &left);
            } else {
                cx.stroke(&rect, &color, &left);
            }
        }
    } else {
        // TODO: now with vello should we do this left.width > 0. check?
        if left.width > 0.0
            && let Some(color) = style.border_color().left
        {
            let half = left.width / 2.0;
            cx.stroke(
                &Line::new(Point::new(half, 0.0), Point::new(half, size.height)),
                &color,
                &left,
            );
        }
        if right.width > 0.0
            && let Some(color) = style.border_color().right
        {
            let half = right.width / 2.0;
            cx.stroke(
                &Line::new(
                    Point::new(size.width - half, 0.0),
                    Point::new(size.width - half, size.height),
                ),
                &color,
                &right,
            );
        }
        if top.width > 0.0
            && let Some(color) = style.border_color().top
        {
            let half = top.width / 2.0;
            cx.stroke(
                &Line::new(Point::new(0.0, half), Point::new(size.width, half)),
                &color,
                &top,
            );
        }
        if bottom.width > 0.0
            && let Some(color) = style.border_color().bottom
        {
            let half = bottom.width / 2.0;
            cx.stroke(
                &Line::new(
                    Point::new(0.0, size.height - half),
                    Point::new(size.width, size.height - half),
                ),
                &color,
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
    use crate::{
        border_path_iter::{BorderPath, BorderPathEvent},
        unit::Pct,
    };

    let border = layout_style.border();
    let borders = [
        (
            border.top.map(|v| v.0).unwrap_or(Stroke::new(0.)),
            style.border_color().top.unwrap_or_default(),
        ),
        (
            border.right.map(|v| v.0).unwrap_or(Stroke::new(0.)),
            style.border_color().right.unwrap_or_default(),
        ),
        (
            border.bottom.map(|v| v.0).unwrap_or(Stroke::new(0.)),
            style.border_color().bottom.unwrap_or_default(),
        ),
        (
            border.left.map(|v| v.0).unwrap_or(Stroke::new(0.)),
            style.border_color().left.unwrap_or_default(),
        ),
    ];

    // Early return if no borders
    if borders.iter().all(|b| b.0.width == 0.0) {
        return;
    }

    let Pct(border_progress) = style.border_progress();

    let half_width = borders[0].0.width / 2.0;
    let rect = size.to_rect().inflate(-half_width, -half_width);

    let radii = radii_map(border_to_radii_view(style, size), |r| {
        (r - half_width).max(0.0)
    });

    let mut border_path = BorderPath::new(rect, radii);

    // Only create subsegment if needed
    if border_progress < 100. {
        border_path.subsegment(0.0..(border_progress.clamp(0.0, 100.) / 100.));
    }

    // optimize for maximum which is 12 paths and a single move to
    let mut current_path = smallvec::SmallVec::<[_; 13]>::new();
    for event in border_path.path_elements(&borders, 0.1) {
        match event {
            BorderPathEvent::PathElement(el) => {
                if !current_path.is_empty() && matches!(el, PathEl::MoveTo(_)) {
                    // extra move to's will mess up dashed patterns
                    continue;
                }
                current_path.push(el)
            }
            BorderPathEvent::NewStroke(stroke) => {
                // Render current path with previous stroke if any
                if !current_path.is_empty() && stroke.0.width > 0. {
                    cx.stroke(&current_path.as_slice(), &stroke.1, &stroke.0);
                    current_path.clear();
                } else if stroke.0.width == 0. {
                    current_path.clear();
                }
            }
        }
    }
    assert!(current_path.is_empty());
}

fn view_nested_last_child(view: ViewId) -> ViewId {
    let mut last_child = view;
    while let Some(new_last_child) = last_child.children().pop() {
        last_child = new_last_child;
    }
    last_child
}

// Helper functions for futzing with RoundedRectRadii. These should probably be in kurbo.

fn radii_map(radii: RoundedRectRadii, f: impl Fn(f64) -> f64) -> RoundedRectRadii {
    RoundedRectRadii {
        top_left: f(radii.top_left),
        top_right: f(radii.top_right),
        bottom_left: f(radii.bottom_left),
        bottom_right: f(radii.bottom_right),
    }
}

pub(crate) const fn radii_min(radii: RoundedRectRadii) -> f64 {
    radii
        .top_left
        .min(radii.top_right)
        .min(radii.bottom_left)
        .min(radii.bottom_right)
}

pub(crate) const fn radii_max(radii: RoundedRectRadii) -> f64 {
    radii
        .top_left
        .max(radii.top_right)
        .max(radii.bottom_left)
        .max(radii.bottom_right)
}

fn radii_add(radii: RoundedRectRadii, offset: f64) -> RoundedRectRadii {
    radii_map(radii, |r| r + offset)
}
