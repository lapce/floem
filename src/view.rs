use std::any::Any;

use bitflags::bitflags;
use floem_renderer::Renderer;
use glazier::kurbo::{Line, Point, Size};
use taffy::prelude::Node;

use crate::{
    app_handle::AppContext,
    context::{EventCx, LayoutCx, PaintCx, UpdateCx},
    event::{Event, EventListner},
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

    fn children(&mut self) -> Vec<&mut dyn View>;

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        core::any::type_name::<Self>().into()
    }

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

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) -> ChangeFlags;

    fn layout_main(&mut self, cx: &mut LayoutCx) -> Node {
        cx.save();

        let view_style = self.view_style();
        cx.app_state.compute_style(self.id(), view_style);
        let style = cx.app_state.get_computed_style(self.id()).clone();

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

    fn layout(&mut self, cx: &mut LayoutCx) -> Node;

    fn compute_layout_main(&mut self, cx: &mut LayoutCx) {
        if cx.app_state.is_hidden(self.id()) {
            return;
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

        self.compute_layout(cx);

        cx.restore();
    }

    fn compute_layout(&mut self, cx: &mut LayoutCx);

    fn event_main(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool {
        let id = self.id();
        if cx.app_state.is_hidden(id) {
            return false;
        }
        if cx.app_state.is_disabled(&id) && !event.allow_disabled() {
            return false;
        }

        let event = cx.offset_event(self.id(), event);

        if let Some(id_path) = id_path {
            let id = id_path[0];
            let id_path = &id_path[1..];
            if id == self.id() && !id_path.is_empty() {
                if let Some(child) = self.child(id_path[0]) {
                    return child.event_main(cx, Some(id_path), event);
                }
            }
        }

        match &event {
            Event::PointerDown(event) => {
                let rect = cx.get_size(self.id()).unwrap_or_default().to_rect();
                let was_focused = cx.app_state.is_focused(&self.id());
                let now_focused = rect.contains(event.pos);

                if now_focused && !was_focused {
                    cx.app_state.update_focus(self.id(), false);
                } else if !now_focused && was_focused {
                    cx.app_state.clear_focus();
                }

                if now_focused {
                    cx.app_state.keyboard_navigation = false;
                    if event.count == 2 && cx.has_event_listener(id, EventListner::DoubleClick) {
                        let view_state = cx.app_state.view_state(id);
                        view_state.last_pointer_down = Some(event.clone());
                        cx.update_active(id);
                        return true;
                    }
                    if cx.has_event_listener(id, EventListner::Click) {
                        cx.update_active(id);
                        return true;
                    }
                }
            }
            Event::PointerUp(pointer_event) => {
                let last_pointer_down = cx.app_state.view_state(id).last_pointer_down.take();
                let rect = cx.get_size(self.id()).unwrap_or_default().to_rect();
                if let Some(action) = cx.get_event_listener(id, &EventListner::DoubleClick) {
                    if rect.contains(pointer_event.pos)
                        && last_pointer_down
                            .as_ref()
                            .map(|e| e.count == 2)
                            .unwrap_or(false)
                    {
                        (*action)(&event);
                        return true;
                    }
                }
                if let Some(action) = cx.get_event_listener(id, &EventListner::Click) {
                    if rect.contains(pointer_event.pos) {
                        (*action)(&event);
                        return true;
                    }
                }
            }
            Event::KeyDown(_) => {
                if event.is_keyboard_trigger() {
                    let mut ancestor = Some(id);
                    let mut action = None;
                    // Bubble the trigger to parent views
                    while let Some(current_ancestor) = ancestor.filter(|_| action.is_none()) {
                        action = cx.get_event_listener(current_ancestor, &EventListner::Click);
                        ancestor = current_ancestor.parent();
                    }
                    if let Some(action) = action {
                        (*action)(&event);
                        cx.update_active(id);
                        return true;
                    }
                }
            }
            Event::PointerMove(event) => {
                let rect = cx.get_size(id).unwrap_or_default().to_rect();
                if rect.contains(event.pos) {
                    cx.app_state.hovered.insert(id);
                    let style = cx.app_state.get_computed_style(id);
                    if let Some(cursor) = style.cursor {
                        AppContext::update_cursor_style(cursor);
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
                if (*action)(&event) {
                    return true;
                }
            }
        }

        if self.event(cx, id_path, event) {
            return true;
        }

        false
    }

    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool;

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
        cx.restore();
    }

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
            && (!app_state.keyboard_navigatable.contains(&new_focus)
                || app_state.is_disabled(&new_focus)
                || app_state.is_hidden_recursive(new_focus))
        {
            new_focus = tree_iter(new_focus);
        }

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
        let rect = size.to_rect().to_rounded_rect(radius as f64);
        cx.fill(&rect, bg);
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
