use taffy::style::Dimension;

use crate::{app::AppContext, height, view::View, Height};

pub trait Decorators: View + Sized {
    // fn height(self, cx: AppContext, dimension: Dimension) -> Height<Self> {
    //     height(cx, dimension, self)
    // }
}

impl<V: View> Decorators for V {}
