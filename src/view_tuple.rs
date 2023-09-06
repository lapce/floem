use crate::context::PaintCx;
use crate::id::Id;
use crate::view::View;

pub trait ViewTuple {
    fn paint(&mut self, cx: &mut PaintCx);

    fn foreach<F: Fn(&dyn View)>(&self, f: F);

    fn foreach_mut<F: FnMut(&mut dyn View) -> bool>(&mut self, f: &mut F);

    fn foreach_rev<F: FnMut(&mut dyn View) -> bool>(&mut self, f: &mut F);

    fn child(&self, id: Id) -> Option<&dyn View>;

    fn child_mut(&mut self, id: Id) -> Option<&mut dyn View>;

    fn children(&self) -> Vec<&dyn View>;

    fn children_mut(&mut self) -> Vec<&mut dyn View>;
}

macro_rules! impl_view_tuple {
    ( $n: tt; $( $t:ident),* ; $( $i:tt ),* ; $( $j:tt ),*) => {

        impl< $( $t: View, )* > ViewTuple for ( $( $t, )* ) {
            fn foreach<F: Fn(&dyn View)>(&self, f: F) {
                $( f(&self.$i); )*
            }

            fn foreach_mut<F: FnMut(&mut dyn View) -> bool>(&mut self, f: &mut F) {
                $( if f(&mut self.$i) { return; } )*
            }

            fn foreach_rev<F: FnMut(&mut dyn View) -> bool>(&mut self, f: &mut F) {
                $( if f(&mut self.$j) { return; } )*
            }

            fn child(&self, id: Id) -> Option<&dyn View> {
                $( if self.$i.id() == id { return Some(&self.$i) } )*
                None
            }

            fn child_mut(&mut self, id: Id) -> Option<&mut dyn View> {
                $( if self.$i.id() == id { return Some(&mut self.$i) } )*
                None
            }

            fn children(&self) -> Vec<&dyn View> {
                vec![ $( &self.$i ),* ]
            }

            fn children_mut(&mut self) -> Vec<&mut dyn View> {
                vec![ $( &mut self.$i ),* ]
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
impl_view_tuple!(11; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10; 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_view_tuple!(12; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11; 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_view_tuple!(13; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12; 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_view_tuple!(14; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13; 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_view_tuple!(15; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14; 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_view_tuple!(16; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14, V15; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15; 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
