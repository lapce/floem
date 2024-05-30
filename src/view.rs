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
//! For exmaple a minimal slider might look like the following. First, we define the struct with the [`ViewData`] that contains the [`Id`].
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
//!    // If the following effect is not created, and `percent` is accesed directly,
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

use floem_reactive::{ReadSignal, RwSignal};
use floem_renderer::Renderer;
use peniko::kurbo::{Circle, Insets, Line, Point, Rect, RoundedRect, Size};
use std::any::Any;
use taffy::tree::NodeId;

use crate::{
    app_state::AppState,
    context::{ComputeLayoutCx, EventCx, LayoutCx, PaintCx, StyleCx, UpdateCx},
    event::{Event, EventPropagation},
    id::ViewId,
    style::{BoxShadowProp, Style, StyleClassRef},
    view_state::ViewStyleProps,
    views::{dyn_view, DynamicView},
};

/// type erased [`View`]
///
/// Views in Floem are strongly typed. [`AnyView`] allows you to escape the strong typing by converting any type implementing [View] into the [AnyView] type.
///
/// ## Bad Example
///```compile_fail
/// use floem::views::*;
/// use floem::widgets::*;
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
/// the else must return the same type. However the branches return different types. The solution to this is to use the [View::any] method
/// to escape the strongly typed requirement.
///
/// ```
/// use floem::views::*;
/// use floem::{View, IntoView};
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

/// Converts the value into a [`View`].
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

/// The Widget trait contains the methods for implementing updates, styling, layout, events, and painting.
///
/// The [view_data](Widget::view_data) and [view_data_mut](Widget::view_data_mut) methods must be implemented. If the widget contains a child then the [for_each_child](Widget::for_each_child), [for_each_child_mut](Widget::for_each_child_mut), and [for_each_child_rev_mut](Widget::for_each_child_rev_mut) methods must also be implemented.
/// The other methods may be implemented as necessary to implement the Widget.
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
    fn update(&mut self, _cx: &mut UpdateCx, _state: Box<dyn Any>) {}

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

    /// Implement this to handle events and to pass them down to children
    ///
    /// Return true to stop the event from propagating to other views
    ///
    /// If the event needs other passes to run you're expected to call
    /// `cx.app_state_mut().request_changes`.
    // fn event(
    //     &mut self,
    //     cx: &mut EventCx,
    //     id_path: Option<&[Id]>,
    //     event: Event,
    // ) -> EventPropagation {
    //     default_event(self, cx, id_path, event)
    // }

    fn event_before_children(&mut self, _cx: &mut EventCx, _event: &Event) -> EventPropagation {
        EventPropagation::Continue
    }

    fn event_after_children(&mut self, _cx: &mut EventCx, _event: &Event) -> EventPropagation {
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
        let child_layout = cx.compute_view_layout(child);
        if let Some(child_layout) = child_layout {
            if let Some(rect) = layout_rect {
                layout_rect = Some(rect.union(child_layout));
            } else {
                layout_rect = Some(child_layout);
            }
        }
    }
    layout_rect
}

pub(crate) fn paint_bg(
    cx: &mut PaintCx,
    computed_style: &Style,
    style: &ViewStyleProps,
    size: Size,
) {
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
            cx.fill(&circle, bg, 0.0);
        } else {
            paint_box_shadow(cx, computed_style, rect, Some(radius));
            let bg = match style.background() {
                Some(color) => color,
                None => return,
            };
            let rounded_rect = rect.to_rounded_rect(radius);
            cx.fill(&rounded_rect, bg, 0.0);
        }
    } else {
        paint_box_shadow(cx, computed_style, size.to_rect(), None);
        let bg = match style.background() {
            Some(color) => color,
            None => return,
        };
        cx.fill(&size.to_rect(), bg, 0.0);
    }
}

fn paint_box_shadow(cx: &mut PaintCx, style: &Style, rect: Rect, rect_radius: Option<f64>) {
    if let Some(shadow) = style.get(BoxShadowProp).as_ref() {
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
    let outline = style.outline().0;
    if outline == 0. {
        // TODO: we should warn! when outline is < 0
        return;
    }
    let half = outline / 2.0;
    let rect = size.to_rect().inflate(half, half);
    let border_radius = match style.border_radius() {
        crate::unit::PxPct::Px(px) => px,
        crate::unit::PxPct::Pct(pct) => size.min_side() * (pct / 100.),
    };
    cx.stroke(
        &rect.to_rounded_rect(border_radius + half),
        style.outline_color(),
        outline,
    );
}

pub(crate) fn paint_border(cx: &mut PaintCx, style: &ViewStyleProps, size: Size) {
    let left = style.border_left().0;
    let top = style.border_top().0;
    let right = style.border_right().0;
    let bottom = style.border_bottom().0;

    let border_color = style.border_color();
    if left == top && top == right && right == bottom && bottom == left && left > 0.0 {
        let half = left / 2.0;
        let rect = size.to_rect().inflate(-half, -half);
        let radius = match style.border_radius() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => size.min_side() * (pct / 100.),
        };
        if radius > 0.0 {
            let radius = (radius - half).max(0.0);
            cx.stroke(&rect.to_rounded_rect(radius), border_color, left);
        } else {
            cx.stroke(&rect, border_color, left);
        }
    } else {
        if left > 0.0 {
            let half = left / 2.0;
            cx.stroke(
                &Line::new(Point::new(half, 0.0), Point::new(half, size.height)),
                border_color,
                left,
            );
        }
        if right > 0.0 {
            let half = right / 2.0;
            cx.stroke(
                &Line::new(
                    Point::new(size.width - half, 0.0),
                    Point::new(size.width - half, size.height),
                ),
                border_color,
                right,
            );
        }
        if top > 0.0 {
            let half = top / 2.0;
            cx.stroke(
                &Line::new(Point::new(0.0, half), Point::new(size.width, half)),
                border_color,
                top,
            );
        }
        if bottom > 0.0 {
            let half = bottom / 2.0;
            cx.stroke(
                &Line::new(
                    Point::new(0.0, size.height - half),
                    Point::new(size.width, size.height - half),
                ),
                border_color,
                bottom,
            );
        }
    }
}

/// Tab navigation finds the next or previous view with the `keyboard_navigatable` status in the tree.
#[allow(dead_code)]
pub(crate) fn view_tab_navigation(root_view: ViewId, app_state: &mut AppState, backwards: bool) {
    let start = app_state.focus.unwrap_or(root_view);

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
    let pos = children.iter().position(|v| v == &id).unwrap();

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
