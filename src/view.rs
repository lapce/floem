use std::any::Any;

use bitflags::bitflags;
use taffy::prelude::Node;

use crate::{
    context::{LayoutCx, PaintCx},
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
    type State;

    fn id(&self) -> Id;

    fn update(&mut self, id_path: &[Id], state: Box<dyn Any>) -> ChangeFlags;

    fn build_layout(&mut self, cx: &mut LayoutCx) -> Node;

    fn layout(&mut self, cx: &mut LayoutCx);

    fn event(&mut self, event: Event);

    fn paint(&mut self, cx: &mut PaintCx);
}
