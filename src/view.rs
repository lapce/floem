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

use bitflags::bitflags;
use floem_renderer::Renderer;
use kurbo::{Circle, Insets, Line, Point, Rect, RoundedRect, Size};
use std::any::Any;
use taffy::prelude::Node;

use crate::{
    context::{AppState, EventCx, LayoutCx, PaintCx, StyleCx, UpdateCx, ViewStyleProps},
    event::Event,
    id::Id,
    style::{BoxShadowProp, Outline, OutlineColor, Style, StyleClassRef},
};

bitflags! {
    #[derive(Default, Copy, Clone)]
    #[must_use]
    pub struct ChangeFlags: u8 {
        const UPDATE = 1;
        const STYLE = 1 << 1;
        const LAYOUT = 1 << 2;
        const ACCESSIBILITY = 1 << 3;
        const PAINT = 1 << 4;
    }
}

pub trait View {
    fn id(&self) -> Id;

    fn view_style(&self) -> Option<Style> {
        None
    }

    fn view_class(&self) -> Option<StyleClassRef> {
        None
    }

    fn child(&self, id: Id) -> Option<&dyn View>;

    fn child_mut(&mut self, id: Id) -> Option<&mut dyn View>;

    fn children(&self) -> Vec<&dyn View>;

    /// At the moment, this is used only to build the debug tree.
    fn children_mut(&mut self) -> Vec<&mut dyn View>;

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
    /// You are in charge of downcasting the state to the expected type and you're required to return
    /// indicating if you'd like a layout or paint pass to be scheduled.
    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) -> ChangeFlags;

    /// Use this method to style the view's children.
    fn style(&mut self, cx: &mut StyleCx<'_>) {
        for child in self.children_mut() {
            cx.style_view(child)
        }
    }

    /// Use this method to layout the view's children.
    /// Usually you'll do this by calling `LayoutCx::layout_node`
    fn layout(&mut self, cx: &mut LayoutCx) -> Node;

    /// You must implement this if your view has children.
    ///
    /// Responsible for computing the layout of the view's children.
    fn compute_layout(&mut self, _cx: &mut LayoutCx) -> Option<Rect> {
        None
    }

    /// Implement this to handle events and to pass them down to children
    ///
    /// Return true to stop the event from propagating to other views
    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool;

    /// `View`-specific implementation. Will be called in the [`View::paint_main`] entry point method.
    /// Usually you'll call the child `View::paint_main` method. But you might also draw text, adjust the offset, clip or draw text.
    fn paint(&mut self, cx: &mut PaintCx);
}

pub(crate) fn paint_bg(
    cx: &mut PaintCx,
    computed_style: &Style,
    style: &ViewStyleProps,
    size: Size,
) {
    let radius = style.border_radius().0;
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
        let inset = Insets::new(
            -shadow.h_offset / 2.0,
            -shadow.v_offset / 2.0,
            shadow.h_offset / 2.0,
            shadow.v_offset / 2.0,
        );
        let rect = rect.inflate(shadow.spread, shadow.spread).inset(inset);
        if let Some(radii) = rect_radius {
            let rounded_rect = RoundedRect::from_rect(rect, radii + shadow.spread);
            cx.fill(&rounded_rect, shadow.color, shadow.blur_radius);
        } else {
            cx.fill(&rect, shadow.color, shadow.blur_radius);
        }
    }
}

pub(crate) fn paint_outline(cx: &mut PaintCx, style: &Style, size: Size) {
    let outline = style.get(Outline).0;
    if outline == 0. {
        // TODO: we should warn! when outline is < 0
        return;
    }
    let half = outline / 2.0;
    let rect = size.to_rect().inflate(half, half);
    cx.stroke(&rect, style.get(OutlineColor), outline);
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
        let radius = style.border_radius().0;
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

/// Tab navigation finds the next or previous view with the `keyboard_navigatable` status in the tree.
#[allow(dead_code)]
pub(crate) fn view_tab_navigation(root_view: &dyn View, app_state: &mut AppState, backwards: bool) {
    let start = app_state.focus.unwrap_or(root_view.id());
    println!("start id is {start:?}");
    let tree_iter = |id: Id| {
        if backwards {
            view_tree_previous(root_view, &id, app_state)
                .unwrap_or(view_nested_last_child(root_view).id())
        } else {
            view_tree_next(root_view, &id, app_state).unwrap_or(root_view.id())
        }
    };

    let mut new_focus = tree_iter(start);
    println!("new focus is {new_focus:?}");
    while new_focus != start
        && (!app_state.keyboard_navigable.contains(&new_focus) || app_state.is_disabled(&new_focus))
    {
        new_focus = tree_iter(new_focus);
        println!("new focus is {new_focus:?}");
    }

    app_state.clear_focus();
    app_state.update_focus(new_focus, true);
    println!("Tab to {new_focus:?}");
}

fn view_children<'a>(view: &'a dyn View, id_path: &[Id]) -> Vec<&'a dyn View> {
    let id = id_path[0];
    let id_path = &id_path[1..];

    if id == view.id() {
        if id_path.is_empty() {
            view.children()
        } else if let Some(child) = view.child(id_path[0]) {
            view_children(child, id_path)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    }
}

/// Get the next item in the tree, either the first child or the next sibling of this view or of the first parent view
fn view_tree_next(root_view: &dyn View, id: &Id, app_state: &AppState) -> Option<Id> {
    let id_path = id.id_path()?;

    println!("id is {id:?}");
    println!("id path is {:?}", id_path.0);

    let children = view_children(root_view, &id_path.0);

    println!(
        "children is {:?}",
        children.iter().map(|v| v.id()).collect::<Vec<_>>()
    );
    for child in children {
        if app_state.is_hidden(child.id()) {
            continue;
        }
        return Some(child.id());
    }

    let mut ancestor = *id;
    loop {
        let id_path = ancestor.id_path()?;
        println!("try to find sibling for {:?}", id_path.0);
        if let Some(next_sibling) = view_next_sibling(root_view, &id_path.0, app_state) {
            println!("next sibling is {:?}", next_sibling.id());
            return Some(next_sibling.id());
        }
        ancestor = ancestor.parent()?;
        println!("go to ancestor {ancestor:?}");
    }
}

/// Get the id of the view after this one (but with the same parent and level of nesting)
fn view_next_sibling<'a>(
    view: &'a dyn View,
    id_path: &[Id],
    app_state: &AppState,
) -> Option<&'a dyn View> {
    let id = id_path[0];
    let id_path = &id_path[1..];
    if id == view.id() {
        if app_state.is_hidden(id) {
            return None;
        }

        if id_path.is_empty() {
            return None;
        }

        if id_path.len() == 1 {
            println!("id is {id:?} id_path is {:?}", id_path);
            let child_id = id_path[0];
            let children = view.children();
            let pos = children.iter().position(|v| v.id() == child_id);
            if let Some(pos) = pos {
                if children.len() > 1 && pos < children.len() - 1 {
                    return Some(children[pos + 1]);
                }
            }
            return None;
        }

        if let Some(child) = view.child(id_path[0]) {
            return view_next_sibling(child, id_path, app_state);
        }
    }
    None
}

/// Get the next item in the tree, the deepest last child of the previous sibling of this view or the parent
fn view_tree_previous(root_view: &dyn View, id: &Id, app_state: &AppState) -> Option<Id> {
    let id_path = id.id_path()?;

    view_previous_sibling(root_view, &id_path.0, app_state)
        .map(|view| view_nested_last_child(view).id())
        .or_else(|| id.parent())
}

/// Get the id of the view before this one (but with the same parent and level of nesting)
fn view_previous_sibling<'a>(
    view: &'a dyn View,
    id_path: &[Id],
    app_state: &AppState,
) -> Option<&'a dyn View> {
    let id = id_path[0];
    let id_path = &id_path[1..];
    if id == view.id() {
        if app_state.is_hidden(id) {
            return None;
        }

        if id_path.is_empty() {
            return None;
        }

        if id_path.len() == 1 {
            let child_id = id_path[0];
            let children = view.children();
            let pos = children.iter().position(|v| v.id() == child_id);
            if let Some(pos) = pos {
                if pos > 0 {
                    return Some(children[pos - 1]);
                }
            }
            return None;
        }

        if let Some(child) = view.child(id_path[0]) {
            return view_previous_sibling(child, id_path, app_state);
        }
    }
    None
}

pub(crate) fn view_children_set_parent_id(view: &dyn View) {
    let parent_id = view.id();
    for child in view.children() {
        child.id().set_parent(parent_id);
        view_children_set_parent_id(child);
    }
}

fn view_nested_last_child(view: &dyn View) -> &dyn View {
    let mut last_child = view;
    while let Some(new_last_child) = last_child.children().pop() {
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

impl View for Box<dyn View> {
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

    fn children(&self) -> Vec<&dyn View> {
        (**self).children()
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        (**self).children_mut()
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        (**self).debug_name()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) -> ChangeFlags {
        (**self).update(cx, state)
    }

    fn style(&mut self, cx: &mut StyleCx) {
        (**self).style(cx)
    }

    fn layout(&mut self, cx: &mut LayoutCx) -> Node {
        (**self).layout(cx)
    }

    fn compute_layout(&mut self, cx: &mut LayoutCx) -> Option<Rect> {
        (**self).compute_layout(cx)
    }

    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool {
        (**self).event(cx, id_path, event)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        (**self).paint(cx)
    }
}
