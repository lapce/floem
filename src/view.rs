//! # Views
//! Views are self-contained components that can be composed together to create complex UIs.
//! Views are the main building blocks of Floem.
//!
//! ## State management
//!
//! You might want some of your view components to have some state. You should place any state that affects
//! the view inside a signal so that it can react to updates and update the `View`. Signals are reactive values that can be read from and written to.
//! See [leptos_reactive](https://docs.rs/leptos_reactive/latest/leptos_reactive/) for more info.
//!
//! ### Use state to update your view
//!
//! To affect the layout and rendering of your component, you will need to send a state update to your component with [Id::update_state](id::Id::update_state)
//! and then call [UpdateCx::request_layout](context::UpdateCx::request_layout) to request a layout which will cause a repaint.
//!
//! ### Local and locally-shared state
//!
//! Some pre-built `Views` can be passed state in their constructor. You can choose to share this state among components.
//!
//! To share state between components child and sibling components, you can simply pass down a signal to your children. Here's are two contrived examples:
//!
//! #### No custom component, simply creating state and sharing among the composed views.
//!
//! ```rust,no_run
//! pub fn label_and_input() -> impl View {
//!     let cx = AppContext::get_current();
//!     let text = create_rw_signal(cx.scope, "Hello world".to_string());
//!     stack(|| (text_input(text), label(|| text.get())))
//!         .style(|| Style::BASE.padding_px(10.0))
//! }
//! ```
//!
//! #### Encapsulating state in a custom component and sharing it with its children.
//!
//! Custom [Views](crate::view::View)s may have encapsulated local state that is stored on the implementing struct.
//!
//!```rust,no_run
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
//!     let text = create_rw_signal(cx.scope, "World!".to_string());
//!     // share the signal between the two children
//!     let (id, child) = AppContext::new_id_with_child(stack(|| (text_input(text)), new_child(text.read_only()));
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
//!     let (id, label) = AppContext::new_id_with_child(|| label(move || format!("Hello, {}", text.get()));
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
//!
//!
//! ### Global state
//!
//! Global state can be implemented using Leptos' [provide_context](leptos_reactive::provide_context) and [use_context](leptos_reactive::use_context).
//!
//!
//!
//! ```

use std::any::Any;

use bitflags::bitflags;
use floem_renderer::Renderer;
use glazier::kurbo::{Affine, Circle, Line, Point, Rect, Size};
use taffy::prelude::Node;

use crate::{
    context::{DragState, EventCx, LayoutCx, PaintCx, UpdateCx},
    event::{Event, EventListener},
    id::Id,
    style::{ComputedStyle, Style},
};

bitflags! {
    #[derive(Default)]
    #[must_use]
    pub struct ChangeFlags: u8 {
        const UPDATE = 1;
        const LAYOUT = 2;
        const ACCESSIBILITY = 4;
        const PAINT = 8;
    }
}

pub trait View {
    fn id(&self) -> Id;

    fn view_style(&self) -> Option<Style> {
        None
    }

    fn child(&mut self, id: Id) -> Option<&mut dyn View>;

    /// At the moment, this is used only to build the debug tree.
    fn children(&mut self) -> Vec<&mut dyn View>;

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        core::any::type_name::<Self>().into()
    }

    /// Used internally by Floem to send an update to the correct view based on the `Id` path.
    /// It will invoke only once `update` when the correct view is located.
    ///
    /// You shouldn't need to implement this.
    fn update_main(
        &mut self,
        cx: &mut UpdateCx,
        id_path: &[Id],
        state: Box<dyn Any>,
    ) -> ChangeFlags {
        let id = id_path[0];
        let id_path = &id_path[1..];
        if id == self.id() {
            if id_path.is_empty() {
                return self.update(cx, state);
            } else if let Some(child) = self.child(id_path[0]) {
                return child.update_main(cx, id_path, state);
            }
        }
        ChangeFlags::empty()
    }

    /// Use this method to react to changes in view-related state.
    /// You will usually send state to this hook manually using the `View`'s `Id` handle
    ///
    /// ```rust
    /// self.id.update_state(SomeState)
    /// ```
    ///
    /// You are in charge of downcasting the state to the expected type and you're required to return
    /// indicating if you'd like a layout or paint pass to be scheduled.
    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) -> ChangeFlags;

    /// Internal method used by Floem to compute the styles for the view and to invoke the
    /// user-defined `View::layout` method.
    ///
    /// You shouldn't need to implement this.
    fn layout_main(&mut self, cx: &mut LayoutCx) -> Node {
        cx.save();

        let view_style = self.view_style();
        cx.app_state.compute_style(self.id(), view_style);
        let style = cx.app_state.get_computed_style(self.id()).clone();

        if style.color.is_some() {
            cx.color = style.color;
        }
        if style.font_size.is_some() {
            cx.font_size = style.font_size;
        }
        if style.font_family.is_some() {
            cx.font_family = style.font_family;
        }
        if style.font_weight.is_some() {
            cx.font_weight = style.font_weight;
        }
        if style.font_style.is_some() {
            cx.font_style = style.font_style;
        }
        if style.line_height.is_some() {
            cx.line_height = style.line_height;
        }

        let node = self.layout(cx);

        cx.restore();
        node
    }

    /// Use this method to layout the view's children.
    /// Usually you'll do this by calling `LayoutCx::layout_node`
    fn layout(&mut self, cx: &mut LayoutCx) -> Node;

    /// Internal method used by Floem. This method derives its calculations based on the [Taffy Node](taffy::prelude::Node) returned by the `View::layout` method.
    ///
    /// It's responsible for:
    /// - calculating and setting the view's origin (local coordinates and window coordinates)
    /// - calculating and setting the view's viewport
    /// - invoking any attached [ResizeListeners](crate::context::ResizeListener)
    ///
    /// Returns the bounding rect that encompasses this view and its children
    ///
    /// You shouldn't need to implement this.
    fn compute_layout_main(&mut self, cx: &mut LayoutCx) -> Rect {
        if cx.app_state.is_hidden(self.id()) {
            return Rect::ZERO;
        }

        cx.save();

        let layout = cx
            .app_state
            .get_layout(self.id())
            .unwrap_or(taffy::layout::Layout::new());
        let origin = Point::new(layout.location.x as f64, layout.location.y as f64);
        let parent_viewport = cx.viewport.map(|rect| {
            rect.with_origin(Point::new(
                rect.x0 - layout.location.x as f64,
                rect.y0 - layout.location.y as f64,
            ))
        });
        let viewport = cx
            .app_state
            .view_states
            .get(&self.id())
            .and_then(|view| view.viewport);
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);
        match (parent_viewport, viewport) {
            (Some(parent_viewport), Some(viewport)) => {
                cx.viewport = Some(
                    parent_viewport
                        .intersect(viewport)
                        .intersect(size.to_rect()),
                );
            }
            (Some(parent_viewport), None) => {
                cx.viewport = Some(parent_viewport.intersect(size.to_rect()));
            }
            (None, Some(viewport)) => {
                cx.viewport = Some(viewport.intersect(size.to_rect()));
            }
            (None, None) => {
                cx.viewport = None;
            }
        }

        let viewport = cx.viewport.unwrap_or_default();
        let window_origin = origin + cx.window_origin.to_vec2() + viewport.origin().to_vec2();
        cx.window_origin = window_origin;

        if let Some(resize) = cx.get_resize_listener(self.id()) {
            let new_rect = size.to_rect().with_origin(origin);
            if new_rect != resize.rect || window_origin != resize.window_origin {
                resize.rect = new_rect;
                resize.window_origin = window_origin;
                (*resize.callback)(window_origin, new_rect);
            }
        }

        let child_layout_rect = self.compute_layout(cx);

        let layout_rect = size.to_rect().with_origin(cx.window_origin);
        let layout_rect = if let Some(child_layout_rect) = child_layout_rect {
            layout_rect.union(child_layout_rect)
        } else {
            layout_rect
        };
        cx.app_state.view_state(self.id()).layout_rect = layout_rect;

        cx.restore();

        layout_rect
    }

    /// You must implement this if your view has children.
    ///
    /// Responsible for computing the layout of the view's children.
    fn compute_layout(&mut self, _cx: &mut LayoutCx) -> Option<Rect> {
        None
    }

    /// Internal method used by Floem. This can be called from parent `View`s to propagate an event to the child `View`.
    ///
    /// You shouldn't need to implement this.
    fn event_main(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool {
        let id = self.id();
        if cx.app_state.is_hidden(id) {
            // we don't process events for hidden view
            return false;
        }
        if cx.app_state.is_disabled(&id) && !event.allow_disabled() {
            // if the view is disabled and the event is not processed
            // for disabled views
            return false;
        }

        // offset the event positions if the event has positions
        // e.g. pointer events, so that the position is relative
        // to the view, taking into account of the layout location
        // of the view and the viewport of the view if it's in a scroll.
        let event = cx.offset_event(self.id(), event);

        // if there's id_path, it's an event only for a view.
        if let Some(id_path) = id_path {
            if id_path.is_empty() {
                // this happens when the parent is the destination,
                // but the parent just passed the event on,
                // so it's not really for this view and we stop
                // the event propagation.
                return false;
            }

            let id = id_path[0];
            let id_path = &id_path[1..];

            if id != self.id() {
                // This shouldn't happen
                return false;
            }

            // we're the parent of the event destination, so pass it on to the child
            if !id_path.is_empty() {
                if let Some(child) = self.child(id_path[0]) {
                    if child.event_main(cx, Some(id_path), event.clone()) {
                        return true;
                    }
                } else {
                    // we don't have the child, stop the event propagation
                    return false;
                }
            }
        }

        // if the event was dispatched to an id_path, the event is supposed to be only
        // handled by this view only, so we pass an empty id_path
        // and the event propagation would be stopped at this view
        if self.event(
            cx,
            if id_path.is_some() { Some(&[]) } else { None },
            event.clone(),
        ) {
            return true;
        }

        match &event {
            Event::PointerDown(event) => {
                if event.button.is_left() {
                    let rect = cx.get_size(self.id()).unwrap_or_default().to_rect();
                    let now_focused = rect.contains(event.pos);

                    if now_focused {
                        if cx.app_state.keyboard_navigable.contains(&id) {
                            // if the view can be focused, we update the focus
                            cx.app_state.update_focus(id, false);
                        }
                        if event.count == 2 && cx.has_event_listener(id, EventListener::DoubleClick)
                        {
                            let view_state = cx.app_state.view_state(id);
                            view_state.last_pointer_down = Some(event.clone());
                            cx.update_active(id);
                        }
                        if cx.has_event_listener(id, EventListener::Click) {
                            let view_state = cx.app_state.view_state(id);
                            view_state.last_pointer_down = Some(event.clone());
                            cx.update_active(id);
                        }
                        if cx.app_state.draggable.contains(&id) && cx.app_state.drag_start.is_none()
                        {
                            cx.app_state.drag_start = Some((id, event.pos));
                        }
                    }
                }
            }
            Event::PointerMove(pointer_event) => {
                let rect = cx.get_size(id).unwrap_or_default().to_rect();
                if rect.contains(pointer_event.pos) {
                    if cx.app_state.is_dragging() {
                        cx.app_state.dragging_over.insert(id);
                        if let Some(action) = cx.get_event_listener(id, &EventListener::DragOver) {
                            (*action)(&event);
                        }
                    } else {
                        cx.app_state.hovered.insert(id);
                        let style = cx.app_state.get_computed_style(id);
                        if let Some(cursor) = style.cursor {
                            if cx.app_state.cursor.is_none() {
                                cx.app_state.cursor = Some(cursor);
                            }
                        }
                    }
                }
                if cx.app_state.draggable.contains(&id) {
                    if let Some((_, drag_start)) = cx
                        .app_state
                        .drag_start
                        .as_ref()
                        .filter(|(drag_id, _)| drag_id == &id)
                    {
                        let vec2 = pointer_event.pos - *drag_start;

                        if let Some(dragging) = cx
                            .app_state
                            .dragging
                            .as_mut()
                            .filter(|d| d.id == id && d.released_at.is_none())
                        {
                            // update the dragging offset if the view is dragging and not released
                            dragging.offset = vec2;
                            id.request_paint();
                        } else if vec2.x.abs() + vec2.y.abs() > 1.0 {
                            // start dragging when moved 1 px
                            cx.app_state.active = None;
                            cx.update_active(id);
                            cx.app_state.dragging = Some(DragState {
                                id,
                                offset: vec2,
                                released_at: None,
                            });
                            id.request_paint();
                            if let Some(action) =
                                cx.get_event_listener(id, &EventListener::DragStart)
                            {
                                (*action)(&event);
                            }
                        }
                    }
                }
            }
            Event::PointerUp(pointer_event) => {
                if pointer_event.button.is_left() {
                    let rect = cx.get_size(self.id()).unwrap_or_default().to_rect();
                    let on_view = rect.contains(pointer_event.pos);

                    if id_path.is_none() {
                        if on_view {
                            if let Some(dragging) = cx.app_state.dragging.as_mut() {
                                let dragging_id = dragging.id;
                                if let Some(action) =
                                    cx.get_event_listener(id, &EventListener::Drop)
                                {
                                    if (*action)(&event) {
                                        // if the drop is processed, we set dragging to none so that the animation
                                        // for the dragged view back to its original position isn't played.
                                        cx.app_state.dragging = None;
                                        id.request_paint();
                                        if let Some(action) = cx.get_event_listener(
                                            dragging_id,
                                            &EventListener::DragEnd,
                                        ) {
                                            (*action)(&event);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        if let Some(dragging) =
                            cx.app_state.dragging.as_mut().filter(|d| d.id == id)
                        {
                            let dragging_id = dragging.id;
                            dragging.released_at = Some(std::time::Instant::now());
                            id.request_paint();
                            if let Some(action) =
                                cx.get_event_listener(dragging_id, &EventListener::DragEnd)
                            {
                                (*action)(&event);
                            }
                        }
                        let last_pointer_down =
                            cx.app_state.view_state(id).last_pointer_down.take();
                        if let Some(action) = cx.get_event_listener(id, &EventListener::DoubleClick)
                        {
                            if on_view
                                && last_pointer_down
                                    .as_ref()
                                    .map(|e| e.count == 2)
                                    .unwrap_or(false)
                                && (*action)(&event)
                            {
                                return true;
                            }
                        }
                        if let Some(action) = cx.get_event_listener(id, &EventListener::Click) {
                            if on_view && last_pointer_down.is_some() && (*action)(&event) {
                                return true;
                            }
                        }
                    }
                }
            }
            Event::KeyDown(_) => {
                if cx.app_state.is_focused(&id) && event.is_keyboard_trigger() {
                    if let Some(action) = cx.get_event_listener(id, &EventListener::Click) {
                        (*action)(&event);
                    }
                }
            }
            Event::WindowResized(_) => {
                if let Some(view_state) = cx.app_state.view_states.get(&self.id()) {
                    if !view_state.responsive_styles.is_empty() {
                        cx.app_state.request_layout(self.id());
                    }
                }
            }
            _ => (),
        }

        if let Some(listener) = event.listener() {
            if let Some(action) = cx.get_event_listener(self.id(), &listener) {
                let should_run = if let Some(pos) = event.point() {
                    let rect = cx.get_size(self.id()).unwrap_or_default().to_rect();
                    rect.contains(pos)
                } else {
                    true
                };
                if should_run && (*action)(&event) {
                    return true;
                }
            }
        }

        false
    }

    /// Implement this to handle events and to pass them down to children
    ///
    /// Return true to stop the event from propagating to other views
    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool;

    /// The entry point for painting a view. You shouldn't need to implement this yourself. Instead, implement [`View::paint`].
    /// It handles the internal work before and after painting [`View::paint`] implementations.
    /// It is responsible for
    /// - managing hidden status
    /// - clipping
    /// - painting computed styles like background color, border, font-styles, and z-index and handling painting requirements of drag and drop
    fn paint_main(&mut self, cx: &mut PaintCx) {
        let id = self.id();
        if cx.app_state.is_hidden(id) {
            return;
        }

        cx.save();
        let size = cx.transform(id);
        let is_empty = cx
            .clip
            .map(|rect| rect.intersect(size.to_rect()).is_empty())
            .unwrap_or(false);
        if !is_empty {
            let style = cx.app_state.get_computed_style(id).clone();

            if let Some(z_index) = style.z_index {
                cx.set_z_index(z_index);
            }

            paint_bg(cx, &style, size);

            if style.color.is_some() {
                cx.color = style.color;
            }
            if style.font_size.is_some() {
                cx.font_size = style.font_size;
            }
            if style.font_family.is_some() {
                cx.font_family = style.font_family.clone();
            }
            if style.font_weight.is_some() {
                cx.font_weight = style.font_weight;
            }
            if style.font_style.is_some() {
                cx.font_style = style.font_style;
            }
            if style.line_height.is_some() {
                cx.line_height = style.line_height;
            }
            self.paint(cx);
            paint_border(cx, &style, size);
        }

        let mut drag_set_to_none = false;
        if let Some(dragging) = cx.app_state.dragging.as_ref() {
            if dragging.id == id {
                let dragging_offset = dragging.offset;
                let mut offset_scale = None;
                if let Some(released_at) = dragging.released_at {
                    const LIMIT: f64 = 300.0;
                    let elapsed = released_at.elapsed().as_millis() as f64;
                    if elapsed < LIMIT {
                        offset_scale = Some(1.0 - elapsed / LIMIT);
                        cx.app_state.request_timer(
                            std::time::Duration::from_millis(8),
                            Box::new(move || {
                                id.request_paint();
                            }),
                        );
                    } else {
                        drag_set_to_none = true;
                    }
                } else {
                    offset_scale = Some(1.0);
                }

                if let Some(offset_scale) = offset_scale {
                    let offset = dragging_offset * offset_scale;
                    cx.save();

                    let mut new = cx.transform.as_coeffs();
                    new[4] += offset.x;
                    new[5] += offset.y;
                    cx.transform = Affine::new(new);
                    cx.paint_state
                        .renderer
                        .as_mut()
                        .unwrap()
                        .transform(cx.transform);
                    cx.set_z_index(1000);
                    cx.clear_clip();

                    let style = cx.app_state.get_computed_style(id).clone();
                    let view_state = cx.app_state.view_state(id);
                    let style = if let Some(dragging_style) = view_state.dragging_style.clone() {
                        view_state
                            .combined_style
                            .clone()
                            .apply(dragging_style)
                            .compute(&ComputedStyle::default())
                    } else {
                        style
                    };
                    paint_bg(cx, &style, size);
                    self.paint(cx);
                    paint_border(cx, &style, size);

                    cx.restore();
                }
            }
        }
        if drag_set_to_none {
            cx.app_state.dragging = None;
        }

        cx.restore();
    }

    /// `View`-specific implementation. Will be called in the [`View::paint_main`] entry point method.
    /// Usually you'll call the child `View::paint_main` method. But you might also draw text, adjust the offset, clip or draw text.
    fn paint(&mut self, cx: &mut PaintCx);

    /// Produces an ascii art debug display of all of the views.
    fn debug_tree(&mut self)
    where
        Self: Sized,
    {
        let mut views = vec![(self as &mut dyn View, Vec::new())];
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

    /// Tab navigation finds the next or previous view with the `keyboard_navigatable` status in the tree.
    fn tab_navigation(&mut self, app_state: &mut crate::context::AppState, backwards: bool)
    where
        Self: Sized,
    {
        let start = app_state.focus.unwrap_or(self.id());
        let tree_iter = |id: Id| {
            if backwards {
                id.tree_previous().unwrap_or(self.id().nested_last_child())
            } else {
                id.tree_next().unwrap_or(self.id())
            }
        };

        let mut new_focus = tree_iter(start);
        while new_focus != start
            && (!app_state.keyboard_navigable.contains(&new_focus)
                || app_state.is_disabled(&new_focus)
                || app_state.is_hidden_recursive(new_focus))
        {
            new_focus = tree_iter(new_focus);
        }

        app_state.clear_focus();
        app_state.update_focus(new_focus, true);
        self.debug_tree();
        println!("Tab to {new_focus:?}");
    }
}

fn paint_bg(cx: &mut PaintCx, style: &ComputedStyle, size: Size) {
    let bg = match style.background {
        Some(color) => color,
        None => return,
    };

    let radius = style.border_radius;
    if radius > 0.0 {
        let rect = size.to_rect();
        let width = rect.width();
        let height = rect.height();
        if width > 0.0 && height > 0.0 && radius as f64 > width.max(height) / 2.0 {
            let radius = width.max(height) / 2.0;
            let circle = Circle::new(rect.center(), radius);
            cx.fill(&circle, bg);
        } else {
            let rect = rect.to_rounded_rect(radius as f64);
            cx.fill(&rect, bg);
        }
    } else {
        cx.fill(&size.to_rect(), bg);
    }
}

fn paint_border(cx: &mut PaintCx, style: &ComputedStyle, size: Size) {
    let left = style.border_left;
    let top = style.border_top;
    let right = style.border_right;
    let bottom = style.border_bottom;

    let border_color = style.border_color;
    if left == top && top == right && right == bottom && bottom == left && left > 0.0 {
        let half = left as f64 / 2.0;
        let rect = size.to_rect().inflate(-half, -half);
        let radius = style.border_radius;
        if radius > 0.0 {
            cx.stroke(
                &rect.to_rounded_rect(radius as f64),
                border_color,
                left as f64,
            );
        } else {
            cx.stroke(&rect, border_color, left as f64);
        }
    } else {
        if left > 0.0 {
            let half = left as f64 / 2.0;
            cx.stroke(
                &Line::new(Point::new(half, 0.0), Point::new(half, size.height)),
                border_color,
                left as f64,
            );
        }
        if right > 0.0 {
            let half = right as f64 / 2.0;
            cx.stroke(
                &Line::new(
                    Point::new(size.width - half, 0.0),
                    Point::new(size.width - half, size.height),
                ),
                border_color,
                right as f64,
            );
        }
        if top > 0.0 {
            let half = top as f64 / 2.0;
            cx.stroke(
                &Line::new(Point::new(0.0, half), Point::new(size.width, half)),
                border_color,
                top as f64,
            );
        }
        if bottom > 0.0 {
            let half = bottom as f64 / 2.0;
            cx.stroke(
                &Line::new(
                    Point::new(0.0, size.height - half),
                    Point::new(size.width, size.height - half),
                ),
                border_color,
                bottom as f64,
            );
        }
    }
}
