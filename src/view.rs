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

use std::any::Any;

use bitflags::bitflags;
use floem_renderer::Renderer;
use kurbo::{Affine, Circle, Insets, Line, Point, Rect, RoundedRect, Size};
use taffy::prelude::Node;

use crate::{
    action::{exec_after, show_context_menu},
    context::{AppState, DragState, EventCx, LayoutCx, PaintCx, StyleCx, UpdateCx},
    event::{Event, EventListener},
    id::Id,
    inspector::CaptureState,
    style::{BoxShadowProp, LayoutProps, Outline, OutlineColor, Style, StyleClassRef, ZIndex},
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

    fn cleanup(&mut self, app_state: &mut crate::context::AppState) {
        for child in self.children_mut() {
            child.cleanup(app_state);
        }
        let id = self.id();
        let view_state = app_state.view_state(id);
        if let Some(action) = view_state.cleanup_listener.as_ref() {
            action();
        }
        let node = view_state.node;
        if let Ok(children) = app_state.taffy.children(node) {
            for child in children {
                let _ = app_state.taffy.remove(child);
            }
        }
        let _ = app_state.taffy.remove(node);
        id.remove_id_path();
        app_state.view_states.remove(&id);
        if app_state.focus == Some(id) {
            app_state.focus = None;
        }
    }

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
            } else if let Some(child) = self.child_mut(id_path[0]) {
                return child.update_main(cx, id_path, state);
            }
        }
        ChangeFlags::empty()
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

    /// Internal method used by Floem to compute the styles for the view.
    ///
    /// You shouldn't need to implement this.
    fn style_main(&mut self, cx: &mut StyleCx<'_>) {
        cx.save();

        let view_state = cx.app_state_mut().view_state(self.id());
        if !view_state.request_style {
            return;
        }
        view_state.request_style = false;

        let view_style = self.view_style();
        let view_class = self.view_class();
        let class = view_state.class;
        let class_array;
        let classes = if let Some(class) = class {
            class_array = [class];
            &class_array[..]
        } else {
            &[]
        };

        // Propagate style requests to children if needed.
        if view_state.request_style_recursive {
            view_state.request_style_recursive = false;
            for child in self.children() {
                let state = cx.app_state_mut().view_state(child.id());
                state.request_style_recursive = true;
                state.request_style = true;
            }
        }

        cx.app_state
            .compute_style(self.id(), view_style, view_class, classes, &cx.current);
        let style = cx.app_state_mut().get_computed_style(self.id()).clone();
        cx.direct = style;
        Style::apply_only_inherited(&mut cx.current, &cx.direct);
        CaptureState::capture_style(self.id(), cx);

        // If there's any changes to the Taffy style, request layout.
        let taffy_style = cx.direct.to_taffy_style();
        let view_state = cx.app_state_mut().view_state(self.id());
        if taffy_style != view_state.taffy_style {
            view_state.taffy_style = taffy_style;
            cx.app_state_mut().request_layout(self.id());
        }

        // Extract the relevant layout properties so the content rect can be calculated
        // when painting.
        let mut props = LayoutProps::default();
        props.read(cx);
        cx.app_state_mut().view_state(self.id()).layout_props = props;

        self.style(cx);

        cx.restore();
    }

    /// Use this method to style the view's children.
    fn style(&mut self, cx: &mut StyleCx<'_>) {
        for child in self.children_mut() {
            child.style_main(cx)
        }
    }

    /// Internal method used by Floem to invoke the user-defined `View::layout` method.
    ///
    /// You shouldn't need to implement this.
    fn layout_main(&mut self, cx: &mut LayoutCx) -> Node {
        cx.save();

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
    /// - invoking any attached context::ResizeListeners
    ///
    /// Returns the bounding rect that encompasses this view and its children
    ///
    /// You shouldn't need to implement this.
    fn compute_layout_main(&mut self, cx: &mut LayoutCx) -> Rect {
        if cx.app_state().is_hidden(self.id()) {
            return Rect::ZERO;
        }

        cx.save();

        let layout = cx
            .app_state()
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
            .app_state()
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
        let window_origin = origin + cx.window_origin.to_vec2() - viewport.origin().to_vec2();
        cx.window_origin = window_origin;

        if let Some(resize) = cx.get_resize_listener(self.id()) {
            let new_rect = size.to_rect().with_origin(origin);
            if new_rect != resize.rect {
                resize.rect = new_rect;
                (*resize.callback)(new_rect);
            }
        }

        if let Some(listener) = cx.get_move_listener(self.id()) {
            if window_origin != listener.window_origin {
                listener.window_origin = window_origin;
                (*listener.callback)(window_origin);
            }
        }

        let child_layout_rect = self.compute_layout(cx);

        let layout_rect = size.to_rect().with_origin(cx.window_origin);
        let layout_rect = if let Some(child_layout_rect) = child_layout_rect {
            layout_rect.union(child_layout_rect)
        } else {
            layout_rect
        };
        cx.app_state_mut().view_state(self.id()).layout_rect = layout_rect;

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
                if let Some(child) = self.child_mut(id_path[0]) {
                    return child.event_main(cx, Some(id_path), event.clone());
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
                cx.app_state.clicking.insert(self.id());
                if event.button.is_primary() {
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
                        }
                        if cx.has_event_listener(id, EventListener::Click) {
                            let view_state = cx.app_state.view_state(id);
                            view_state.last_pointer_down = Some(event.clone());
                        }

                        let bottom_left = {
                            let layout = cx.app_state.view_state(id).layout_rect;
                            Point::new(layout.x0, layout.y1)
                        };
                        if let Some(menu) = &cx.app_state.view_state(id).popout_menu {
                            show_context_menu(menu(), Some(bottom_left))
                        }
                        if cx.app_state.draggable.contains(&id) && cx.app_state.drag_start.is_none()
                        {
                            cx.app_state.drag_start = Some((id, event.pos));
                        }
                    }
                } else if event.button.is_secondary() {
                    let rect = cx.get_size(self.id()).unwrap_or_default().to_rect();
                    let now_focused = rect.contains(event.pos);

                    if now_focused {
                        if cx.app_state.keyboard_navigable.contains(&id) {
                            // if the view can be focused, we update the focus
                            cx.app_state.update_focus(id, false);
                        }
                        if cx.has_event_listener(id, EventListener::SecondaryClick) {
                            let view_state = cx.app_state.view_state(id);
                            view_state.last_pointer_down = Some(event.clone());
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
                        let style = cx.app_state.get_builtin_style(id);
                        if let Some(cursor) = style.cursor() {
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
                if let Some(action) = cx.get_event_listener(id, &EventListener::PointerMove) {
                    if (*action)(&event) {
                        return true;
                    }
                }
            }
            Event::PointerUp(pointer_event) => {
                if pointer_event.button.is_primary() {
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
                    } else if let Some(dragging) =
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

                    let last_pointer_down = cx.app_state.view_state(id).last_pointer_down.take();
                    if let Some(action) = cx.get_event_listener(id, &EventListener::DoubleClick) {
                        if on_view
                            && cx.app_state.is_clicking(&id)
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
                        if on_view
                            && cx.app_state.is_clicking(&id)
                            && last_pointer_down.is_some()
                            && (*action)(&event)
                        {
                            return true;
                        }
                    }

                    if let Some(action) = cx.get_event_listener(id, &EventListener::PointerUp) {
                        if (*action)(&event) {
                            return true;
                        }
                    }
                } else if pointer_event.button.is_secondary() {
                    let rect = cx.get_size(self.id()).unwrap_or_default().to_rect();
                    let on_view = rect.contains(pointer_event.pos);

                    let last_pointer_down = cx.app_state.view_state(id).last_pointer_down.take();
                    if let Some(action) = cx.get_event_listener(id, &EventListener::SecondaryClick)
                    {
                        if on_view && last_pointer_down.is_some() && (*action)(&event) {
                            return true;
                        }
                    }

                    let viewport_event_position = {
                        let layout = cx.app_state.view_state(id).layout_rect;
                        Point::new(
                            layout.x0 + pointer_event.pos.x,
                            layout.y0 + pointer_event.pos.y,
                        )
                    };
                    if let Some(menu) = &cx.app_state.view_state(id).context_menu {
                        show_context_menu(menu(), Some(viewport_event_position))
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
                    if view_state.has_style_selectors.has_responsive() {
                        cx.app_state.request_style(self.id());
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
            .map(|rect| rect.rect().intersect(size.to_rect()).is_empty())
            .unwrap_or(false);
        if !is_empty {
            let style = cx.app_state.get_computed_style(id).clone();

            if let Some(z_index) = style.get(ZIndex) {
                cx.set_z_index(z_index);
            }

            paint_bg(cx, &style, size);

            self.paint(cx);
            paint_border(cx, &style, size);
            paint_outline(cx, &style, size)
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
                        exec_after(std::time::Duration::from_millis(8), move |_| {
                            id.request_paint();
                        });
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
                    cx.paint_state.renderer.transform(cx.transform);
                    cx.set_z_index(1000);
                    cx.clear_clip();

                    let style = cx.app_state.get_computed_style(id).clone();
                    let view_state = cx.app_state.view_state(id);
                    let style = if let Some(dragging_style) = view_state.dragging_style.clone() {
                        view_state.combined_style.clone().apply(dragging_style)
                    } else {
                        style
                    };
                    paint_bg(cx, &style, size);

                    self.paint(cx);
                    paint_border(cx, &style, size);
                    paint_outline(cx, &style, size);

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
}

fn paint_bg(cx: &mut PaintCx, computed_style: &Style, size: Size) {
    let style = computed_style.builtin();
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

fn paint_outline(cx: &mut PaintCx, style: &Style, size: Size) {
    let outline = style.get(Outline).0;
    if outline == 0. {
        // TODO: we should warn! when outline is < 0
        return;
    }
    let half = outline / 2.0;
    let rect = size.to_rect().inflate(half, half);
    cx.stroke(&rect, style.get(OutlineColor), outline);
}

fn paint_border(cx: &mut PaintCx, style: &Style, size: Size) {
    let style = style.builtin();
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
