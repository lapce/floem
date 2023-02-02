use std::any::Any;

use bitflags::bitflags;
use taffy::prelude::Node;

use crate::{
    context::{EventCx, LayoutCx, PaintCx, UpdateCx},
    event::Event,
    id::Id,
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

    fn update(&mut self, cx: &mut UpdateCx, id_path: &[Id], state: Box<dyn Any>) -> ChangeFlags;

    fn layout(&mut self, cx: &mut LayoutCx) -> Node;

    fn event(&mut self, cx: &mut EventCx, event: Event);

    fn paint(&mut self, cx: &mut PaintCx);
}
