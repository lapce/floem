use kurbo::{Point, Rect, Size, Vec2};
use taffy::style::FlexDirection;
use floem_reactive::{create_rw_signal, RwSignal};
use crate::{EventPropagation, style_class, view::View};
use crate::context::{EventCx, UpdateCx};
use crate::event::Event;
use crate::id::Id;
use crate::pointer::PointerButton;
use crate::style::{Style};
use crate::unit::{PxPctAuto};
use crate::view::ViewData;
use crate::views::{clip, Decorators, empty};

style_class!(pub SplitterClass);
style_class!(pub SplitterSectionClass);
style_class!(pub SplitterHorizontalHandleClass);
style_class!(pub SplitterVerticalHandleClass);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitterDirection {
    Vertical,
    Horizontal
}

pub struct Splitter {
    data: ViewData,
    down_position: Option<Point>,
    down_size: Option<Size>,
    direction: SplitterDirection,
    position: RwSignal<PxPctAuto>,
    child_1: Box<dyn View>,
    handle: Box<dyn View>,
    child_2: Box<dyn View>,
}

pub fn splitter(direction: SplitterDirection, position: impl Into<PxPctAuto>, child_1: Box<dyn View>, child_2 : Box<dyn View>) -> impl View {
    let position = create_rw_signal(position.into());

    let handle = match direction {
        SplitterDirection::Vertical => empty().class(SplitterVerticalHandleClass),
        SplitterDirection::Horizontal => empty().class(SplitterHorizontalHandleClass)
    };

    Splitter {
        data: ViewData::new(Id::next()),
        down_position: None,
        down_size: None,
        direction,
        position,
        child_1: Box::new(clip(child_1).class(SplitterSectionClass).style(move |s| {
            match direction {
                SplitterDirection::Vertical => s.height(position.get()),
                SplitterDirection::Horizontal => s.width(position.get())
            }
        })),
        handle: Box::new(handle),
        child_2: Box::new(clip(child_2).class(SplitterSectionClass)),
    }
}

pub fn h_splitter(position: impl Into<PxPctAuto>, child_1: impl View + 'static, child_2: impl View + 'static) -> impl View {
    splitter(SplitterDirection::Horizontal, position, Box::new(child_1), Box::new(child_2))
}

pub fn v_splitter(position: impl Into<PxPctAuto>, child_1: impl View + 'static, child_2: impl View + 'static) -> impl View {
    splitter(SplitterDirection::Vertical, position, Box::new(child_1), Box::new(child_2))
}

impl View for Splitter {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        if for_each(&self.child_1) {
        } else if for_each(&self.handle) {

        } else {
            for_each(&self.child_2);
        }
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        if for_each(&mut self.child_1) {
        } else if for_each(&mut self.handle) {

        } else {
            for_each(&mut self.child_2);
        }
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn View) -> bool,
    ) {
        if for_each(&mut self.child_2) {
        } else if for_each(&mut self.handle) {

        } else {
            for_each(&mut self.child_1);
        }
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        Some(Style::new().flex_direction(match self.direction {
            SplitterDirection::Vertical => FlexDirection::Column,
            SplitterDirection::Horizontal => FlexDirection::Row
        }))
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        match self.direction {
            SplitterDirection::Vertical => "Vertical Splitter".into(),
            SplitterDirection::Horizontal => "Horizontal Splitter".into(),
        }
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast() {
            self.position.set(*state);
            cx.request_all(self.id());
        }
    }

    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> EventPropagation {
        match &event {
            Event::PointerDown(evt) => {
                if evt.button == PointerButton::Primary {
                    if let Some(handle_layout) = cx.get_layout(self.handle.id()) {
                        let rect = Rect::new(handle_layout.location.x as f64, handle_layout.location.y as f64, (handle_layout.location.x + handle_layout.size.width) as f64, (handle_layout.location.y + handle_layout.size.height) as f64);
                        if rect.contains(evt.pos) {
                            if let Some(child_1_layout) = cx.get_layout(self.child_1.id()) {
                                self.down_position = Some(evt.pos);
                                self.down_size = Some(Size::new(child_1_layout.size.width as f64, child_1_layout.size.height as f64));

                                return EventPropagation::Stop;
                            }
                        }
                    }
                }
            },
            Event::PointerUp(evt) => {
                if evt.button == PointerButton::Primary && self.down_position.is_some() {
                    self.down_position = None;
                    return EventPropagation::Stop;
                }
            },
            Event::PointerMove(evt) => {
                if let (Some(down_pos), Some(down_size)) = (self.down_position, self.down_size) {
                    if let Some(size) = cx.get_size(self.id()) {
                        let delta = Vec2::new(evt.pos.x - down_pos.x, evt.pos.y - down_pos.y);

                        let new_split = if let PxPctAuto::Pct(_) = self.position.get() {
                            match self.direction {
                                SplitterDirection::Horizontal => PxPctAuto::Pct(100.0 * (down_size.width + delta.x) / size.width),
                                SplitterDirection::Vertical => PxPctAuto::Pct(100.0 * (down_size.height + delta.y) / size.height)
                            }
                        } else {
                            match self.direction {
                                SplitterDirection::Horizontal => PxPctAuto::Px(down_size.width + delta.x),
                                SplitterDirection::Vertical => PxPctAuto::Px(down_size.height + delta.y)
                            }
                        };

                        self.position.set(new_split);
                        cx.request_layout(self.id());

                        return EventPropagation::Stop;
                    }
                }
            },
            _ => ()
        }

        if cx.view_event(&mut self.child_1, id_path, event.clone()).is_processed() {
            return EventPropagation::Stop;
        }
        if cx.view_event(&mut self.handle, id_path, event.clone()).is_processed() {
            return EventPropagation::Stop;
        }
        if cx.view_event(&mut self.child_2, id_path, event.clone()).is_processed() {
            return EventPropagation::Stop;
        }
        EventPropagation::Continue
    }
}