//! # Views
//! Views are self-contained components that can be composed together to create complex UIs.
//! Views are the main building blocks of Floem.
//!
//! Views are structs that implement the View trait. Many of these structs will also contain a child field that is a generic type V where V also implements View. In this way views can be composed together easily to create complex views without the need for creating new structs and manually implementing View. This is the most common way to build UIs in Floem. Creating a struct and manually implementing View is typically only needed for special cases. The rest of this module documentation is for help when manually implementing View on your own types.
//!
//! ## State management
//!
//! For all reactive state that your type contains either in the form of signals or derived signals you need to process the changes within an effect.
//! Often times the pattern is to [get](floem_reactive::ReadSignal::get) the data in an effect and pass it in to `id.update_state()` and then handle that data in the `update` method of the View trait.
//!
//! ### Use state to update your view
//!
//! To affect the layout and rendering of your component, you will need to send a state update to your component with [Id::update_state](crate::id::Id::update_state)
//! and then call [UpdateCx::request_layout](crate::context::UpdateCx::request_layout) to request a layout which will cause a repaint.
//!
//! ### Local and locally-shared state
//!
//! Some pre-built `Views` can be passed state in their constructor. You can choose to share this state among components.
//!
//! To share state between components child and sibling components, you can simply pass down a signal to your children. Here's are two contrived examples:
//!
//! #### No custom component, simply creating state and sharing among the composed views.
//!
//! ```ignore
//! pub fn label_and_input() -> impl View {
//!     let text = create_rw_signal("Hello world".to_string());
//!     stack(|| (text_input(text), label(|| text.get())))
//!         .style(|| Style::new().padding(10.0))
//! }
//! ```
//!
//! #### Encapsulating state in a custom component and sharing it with its children.
//!
//! Custom [Views](crate::view::View)s may have encapsulated local state that is stored on the implementing struct.
//!
//!```ignore
//!
//! struct Parent<V> {
//!     id: Id,
//!     text: ReadSignal<String>,
//!     child: V,
//! }
//!
//! // Creates a new parent view with the given child.
//! fn parent<V>(new_child: impl FnOnce(ReadSignal<String>) -> V) -> Parent<impl View>
//! where
//!     V: View + 'static,
//! {
//!     let text = create_rw_signal("World!".to_string());
//!     // share the signal between the two children
//!     let (id, child) = ViewContext::new_id_with_child(stack(|| (text_input(text)), new_child(text.read_only())));
//!     Parent { id, text, child }
//! }
//!
//! impl<V> View for Parent<V>
//! where
//!     V: View,
//! {
//! // implementation omitted for brevity
//! }
//!
//! struct Child {
//!     id: Id,
//!     label: Label,
//! }
//!
//! // Creates a new child view with the given state (a read only signal)
//! fn child(text: ReadSignal<String>) -> Child {
//!     let (id, label) = ViewContext::new_id_with_child(|| label(move || format!("Hello, {}", text.get())));
//!     Child { id, label }
//! }
//!
//! impl View for Child {
//!   // implementation omitted for brevity
//! }
//!
//! // Usage
//! fn main() {
//!     floem::launch(parent(child));
//! }
//! ```
//!
//!

use floem_renderer::Renderer;
use kurbo::{Circle, Insets, Line, Point, Rect, RoundedRect, Size};
use std::any::Any;
use taffy::prelude::Node;

use crate::{
    context::{AppState, ComputeLayoutCx, EventCx, LayoutCx, PaintCx, StyleCx, UpdateCx},
    event::Event,
    id::Id,
    style::{BoxShadowProp, Style, StyleClassRef},
    view_data::ViewStyleProps,
    EventPropagation,
};

pub use crate::view_data::ViewData;

pub trait View {
    fn view_data(&self) -> &ViewData;
    fn view_data_mut(&mut self) -> &mut ViewData;

    /// This method walks over children and must be implemented if the view has any children.
    /// It should return children back to front and should stop if `_for_each` returns `true`.
    fn for_each_child<'a>(&'a self, _for_each: &mut dyn FnMut(&'a dyn View) -> bool) {}

    /// This method walks over children and must be implemented if the view has any children.
    /// It should return children back to front and should stop if `_for_each` returns `true`.
    fn for_each_child_mut<'a>(&'a mut self, _for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {}

    /// This method walks over children and must be implemented if the view has any children.
    /// It should return children front to back and should stop if `_for_each` returns `true`.
    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        _for_each: &mut dyn FnMut(&'a mut dyn View) -> bool,
    ) {
    }

    fn id(&self) -> Id {
        self.view_data().id()
    }

    fn view_style(&self) -> Option<Style> {
        None
    }

    fn view_class(&self) -> Option<StyleClassRef> {
        None
    }

    fn child(&self, id: Id) -> Option<&dyn View> {
        let mut result = None;
        self.for_each_child(&mut |view| {
            if view.id() == id {
                result = Some(view);
                true
            } else {
                false
            }
        });
        result
    }

    fn child_mut(&mut self, id: Id) -> Option<&mut dyn View> {
        let mut result = None;
        self.for_each_child_mut(&mut |view| {
            if view.id() == id {
                result = Some(view);
                true
            } else {
                false
            }
        });
        result
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
    fn style(&mut self, cx: &mut StyleCx<'_>) {
        self.for_each_child_mut(&mut |child| {
            cx.style_view(child);
            false
        });
    }

    /// Use this method to layout the view's children.
    /// Usually you'll do this by calling `LayoutCx::layout_node`.
    ///
    /// If the layout changes needs other passes to run you're expected to call
    /// `cx.app_state_mut().request_changes`.
    fn layout(&mut self, cx: &mut LayoutCx) -> Node {
        cx.layout_node(self.id(), true, |cx| {
            let mut nodes = Vec::new();
            self.for_each_child_mut(&mut |child| {
                nodes.push(cx.layout_view(child));
                false
            });
            nodes
        })
    }

    /// Responsible for computing the layout of the view's children.
    ///
    /// If the layout changes needs other passes to run you're expected to call
    /// `cx.app_state_mut().request_changes`.
    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        default_compute_layout(self, cx)
    }

    /// Implement this to handle events and to pass them down to children
    ///
    /// Return true to stop the event from propagating to other views
    ///
    /// If the event needs other passes to run you're expected to call
    /// `cx.app_state_mut().request_changes`.
    fn event(
        &mut self,
        cx: &mut EventCx,
        id_path: Option<&[Id]>,
        event: Event,
    ) -> EventPropagation {
        default_event(self, cx, id_path, event)
    }

    /// `View`-specific implementation. Will be called in [`PaintCx::paint_view`](crate::context::PaintCx::paint_view).
    /// Usually you'll call `paint_view` for every child view. But you might also draw text, adjust the offset, clip
    /// or draw text.
    fn paint(&mut self, cx: &mut PaintCx) {
        self.for_each_child_mut(&mut |child| {
            cx.paint_view(child);
            false
        });
    }

    /// Scrolls the view and all direct and indirect children to bring the `target` view to be
    /// visible. Returns true if this view contains or is the target.
    fn scroll_to(&mut self, cx: &mut AppState, target: Id, rect: Option<Rect>) -> bool {
        if self.id() == target {
            return true;
        }
        let mut found = false;
        self.for_each_child_mut(&mut |child| {
            found |= child.scroll_to(cx, target, rect);
            found
        });
        found
    }
}

/// Computes the layout of the view's children, if any.
pub fn default_compute_layout<V: View + ?Sized>(
    view: &mut V,
    cx: &mut ComputeLayoutCx,
) -> Option<Rect> {
    let mut layout_rect: Option<Rect> = None;
    view.for_each_child_mut(&mut |child| {
        let child_layout = cx.compute_view_layout(child);
        if let Some(child_layout) = child_layout {
            if let Some(rect) = layout_rect {
                layout_rect = Some(rect.union(child_layout));
            } else {
                layout_rect = Some(child_layout);
            }
        }
        false
    });
    layout_rect
}

pub fn default_event<V: View + ?Sized>(
    view: &mut V,
    cx: &mut EventCx,
    id_path: Option<&[Id]>,
    event: Event,
) -> EventPropagation {
    let mut handled = false;
    view.for_each_child_rev_mut(&mut |child| {
        handled |= cx.view_event(child, id_path, event.clone()).is_processed();
        handled
    });
    if handled {
        EventPropagation::Stop
    } else {
        EventPropagation::Continue
    }
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

pub(crate) fn view_children(view: &dyn View) -> Vec<&dyn View> {
    let mut result = Vec::new();
    view.for_each_child(&mut |view| {
        result.push(view);
        false
    });
    result
}

/// Tab navigation finds the next or previous view with the `keyboard_navigatable` status in the tree.
#[allow(dead_code)]
pub(crate) fn view_tab_navigation(root_view: &dyn View, app_state: &mut AppState, backwards: bool) {
    let start = app_state
        .focus
        .filter(|id| id.id_path().is_some())
        .unwrap_or(root_view.id());

    assert!(
        view_filtered_children(root_view, start.id_path().unwrap().dispatch()).is_some(),
        "The focused view is missing from the tree"
    );

    let tree_iter = |id: Id| {
        if backwards {
            view_tree_previous(root_view, &id)
                .unwrap_or_else(|| view_nested_last_child(root_view).id())
        } else {
            view_tree_next(root_view, &id).unwrap_or(root_view.id())
        }
    };

    let mut new_focus = tree_iter(start);
    while new_focus != start && !app_state.can_focus(new_focus) {
        new_focus = tree_iter(new_focus);
    }

    app_state.clear_focus();
    app_state.update_focus(new_focus, true);
}

fn view_filtered_children<'a>(view: &'a dyn View, id_path: &[Id]) -> Option<Vec<&'a dyn View>> {
    let id = id_path[0];
    let id_path = &id_path[1..];

    if id == view.id() {
        if id_path.is_empty() {
            Some(view_children(view))
        } else if let Some(child) = view.child(id_path[0]) {
            view_filtered_children(child, id_path)
        } else {
            None
        }
    } else {
        None
    }
}

/// Get the next item in the tree, either the first child or the next sibling of this view or of the first parent view
fn view_tree_next(root_view: &dyn View, id: &Id) -> Option<Id> {
    let id_path = id.id_path().unwrap();

    if let Some(child) = view_filtered_children(root_view, id_path.dispatch())
        .unwrap()
        .into_iter()
        .next()
    {
        return Some(child.id());
    }

    let mut ancestor = *id;
    loop {
        let id_path = ancestor.id_path().unwrap();
        if id_path.dispatch().is_empty() {
            return None;
        }
        if let Some(next_sibling) = view_next_sibling(root_view, id_path.dispatch()) {
            return Some(next_sibling.id());
        }
        ancestor = ancestor.parent()?;
    }
}

/// Get the id of the view after this one (but with the same parent and level of nesting)
fn view_next_sibling<'a>(root_view: &'a dyn View, id_path: &[Id]) -> Option<&'a dyn View> {
    let id = *id_path.last().unwrap();
    let parent = &id_path[0..(id_path.len() - 1)];

    if parent.is_empty() {
        // We're the root, which has no sibling
        return None;
    }

    let children = view_filtered_children(root_view, parent).unwrap();
    let pos = children.iter().position(|v| v.id() == id).unwrap();

    if pos + 1 < children.len() {
        Some(children[pos + 1])
    } else {
        None
    }
}

/// Get the next item in the tree, the deepest last child of the previous sibling of this view or the parent
fn view_tree_previous(root_view: &dyn View, id: &Id) -> Option<Id> {
    let id_path = id.id_path().unwrap();

    view_previous_sibling(root_view, id_path.dispatch())
        .map(|view| view_nested_last_child(view).id())
        .or_else(|| {
            (root_view.id() != *id).then_some(
                id.parent()
                    .unwrap_or_else(|| view_nested_last_child(root_view).id()),
            )
        })
}

/// Get the id of the view before this one (but with the same parent and level of nesting)
fn view_previous_sibling<'a>(root_view: &'a dyn View, id_path: &[Id]) -> Option<&'a dyn View> {
    let id = *id_path.last().unwrap();
    let parent = &id_path[0..(id_path.len() - 1)];

    if parent.is_empty() {
        // We're the root, which has no sibling
        return None;
    }

    let children = view_filtered_children(root_view, parent).unwrap();
    let pos = children.iter().position(|v| v.id() == id).unwrap();

    if pos > 0 {
        Some(children[pos - 1])
    } else {
        None
    }
}

pub(crate) fn view_children_set_parent_id(view: &dyn View) {
    let parent_id = view.id();
    view.for_each_child(&mut |child| {
        child.id().set_parent(parent_id);
        view_children_set_parent_id(child);
        false
    });
}

fn view_nested_last_child(view: &dyn View) -> &dyn View {
    let mut last_child = view;
    while let Some(new_last_child) = view_children(last_child).pop() {
        last_child = new_last_child;
    }
    last_child
}

/// Produces an ascii art debug display of all of the views.
#[allow(dead_code)]
pub(crate) fn view_debug_tree(root_view: &dyn View) {
    let mut views = vec![(root_view, Vec::new())];
    while let Some((current_view, active_lines)) = views.pop() {
        // Ascii art for the tree view
        if let Some((leaf, root)) = active_lines.split_last() {
            for line in root {
                print!("{}", if *line { "│   " } else { "    " });
            }
            print!("{}", if *leaf { "├── " } else { "└── " });
        }
        println!("{:?} {}", current_view.id(), &current_view.debug_name());

        let mut children = view_children(current_view);
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

impl View for Box<dyn View> {
    fn view_data(&self) -> &ViewData {
        (**self).view_data()
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        (**self).view_data_mut()
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        (**self).for_each_child(for_each)
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        (**self).for_each_child_mut(for_each)
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn View) -> bool,
    ) {
        (**self).for_each_child_rev_mut(for_each)
    }

    fn id(&self) -> Id {
        (**self).id()
    }

    fn view_style(&self) -> Option<Style> {
        (**self).view_style()
    }

    fn view_class(&self) -> Option<StyleClassRef> {
        (**self).view_class()
    }

    fn child(&self, id: Id) -> Option<&dyn View> {
        (**self).child(id)
    }

    fn child_mut(&mut self, id: Id) -> Option<&mut dyn View> {
        (**self).child_mut(id)
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        (**self).debug_name()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        (**self).update(cx, state)
    }

    fn style(&mut self, cx: &mut StyleCx) {
        (**self).style(cx)
    }

    fn layout(&mut self, cx: &mut LayoutCx) -> Node {
        (**self).layout(cx)
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        (**self).compute_layout(cx)
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        id_path: Option<&[Id]>,
        event: Event,
    ) -> EventPropagation {
        (**self).event(cx, id_path, event)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        (**self).paint(cx)
    }

    fn scroll_to(&mut self, cx: &mut AppState, target: Id, rect: Option<Rect>) -> bool {
        (**self).scroll_to(cx, target, rect)
    }
}
