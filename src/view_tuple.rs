use std::any::Any;

use taffy::prelude::Node;

use crate::context::{LayoutCx, PaintCx, UpdateCx};
use crate::event::Event;
use crate::id::Id;
use crate::view::{ChangeFlags, View};

pub trait ViewTuple {
    fn paint(&mut self, cx: &mut PaintCx);

    fn foreach<F: FnMut(&mut dyn View) -> bool>(&mut self, f: &mut F);

    fn foreach_rev<F: FnMut(&mut dyn View) -> bool>(&mut self, f: &mut F);

    fn child(&mut self, id: Id) -> Option<&mut dyn View>;
}

macro_rules! impl_view_tuple {
    ( $n: tt; $( $t:ident),* ; $( $i:tt ),* ; $( $j:tt ),*) => {

        impl< $( $t: View, )* > ViewTuple for ( $( $t, )* ) {
            fn foreach<F: FnMut(&mut dyn View) -> bool>(&mut self, f: &mut F) {
                $( if f(&mut self.$i) { return; } )*
            }

            fn foreach_rev<F: FnMut(&mut dyn View) -> bool>(&mut self, f: &mut F) {
                $( if f(&mut self.$j) { return; } )*
            }

            fn child(&mut self, id: Id) -> Option<&mut dyn View> {
                $( if self.$i.id() == id { return Some(&mut self.$i) } )*
                None
            }

            fn paint(&mut self, cx: &mut PaintCx) {
                $(
                    self.$i.paint(cx);
                )*
            }
        }
    }
}

impl_view_tuple!(1; V0; 0; 0);
impl_view_tuple!(2; V0, V1; 0, 1; 1, 0);
impl_view_tuple!(3; V0, V1, V2; 0, 1, 2; 2, 1, 0);
impl_view_tuple!(4; V0, V1, V2, V3; 0, 1, 2, 3; 3, 2, 1, 0);
impl_view_tuple!(5; V0, V1, V2, V3, V4; 0, 1, 2, 3, 4; 4, 3, 2, 1, 0);
impl_view_tuple!(6; V0, V1, V2, V3, V4, V5; 0, 1, 2, 3, 4, 5; 5, 4, 3, 2, 1, 0);
impl_view_tuple!(7; V0, V1, V2, V3, V4, V5, V6; 0, 1, 2, 3, 4, 5, 6; 6, 5, 4, 3, 2, 1, 0);
impl_view_tuple!(8; V0, V1, V2, V3, V4, V5, V6, V7; 0, 1, 2, 3, 4, 5, 6, 7; 7, 6, 5, 4, 3, 2, 1, 0);
impl_view_tuple!(9; V0, V1, V2, V3, V4, V5, V6, V7, V8; 0, 1, 2, 3, 4, 5, 6, 7, 8; 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_view_tuple!(10; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9; 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
