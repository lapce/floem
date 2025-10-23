use taffy::FlexDirection;

use crate::{
    view::{IntoView, View},
    views::{Stack, create_stack},
};

/// Adds the `flatten` function to turn a tuple of [View]'s into a [Vec] of View trait objects.
pub trait ViewTupleFlat {
    fn flatten(self) -> Vec<Box<dyn View>>;
}

// Macro to implement ViewTupleFlat for tuples of Vec<Box<dyn View>>
macro_rules! impl_view_tuple_flat {
    ($capacity:expr, $($t:ident),+) => {
        impl<$($t: IntoIterator<Item = Box<dyn View>>),+> ViewTupleFlat for ($($t,)+) {
            fn flatten(self) -> Vec<Box<dyn View>> {
                #[allow(non_snake_case)]
                let ($($t,)+) = self;
                let mut views = Vec::new();
                $(
                    views.extend($t);
                )+
                views
            }
        }
    };
}
impl_view_tuple_flat!(1, A);
impl_view_tuple_flat!(2, A, B);
impl_view_tuple_flat!(3, A, B, C);
impl_view_tuple_flat!(4, A, B, C, D);
impl_view_tuple_flat!(5, A, B, C, D, E);
impl_view_tuple_flat!(6, A, B, C, D, E, F);
impl_view_tuple_flat!(7, A, B, C, D, E, F, G);
impl_view_tuple_flat!(8, A, B, C, D, E, F, G, H);
impl_view_tuple_flat!(9, A, B, C, D, E, F, G, H, I);
impl_view_tuple_flat!(10, A, B, C, D, E, F, G, H, I, J);
impl_view_tuple_flat!(11, A, B, C, D, E, F, G, H, I, J, K);
impl_view_tuple_flat!(12, A, B, C, D, E, F, G, H, I, J, K, L);
impl_view_tuple_flat!(13, A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_view_tuple_flat!(14, A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_view_tuple_flat!(15, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_view_tuple_flat!(16, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

pub trait ViewTuple {
    fn into_views(self) -> Vec<Box<dyn View>>;
    fn stack(self, direction: FlexDirection) -> Stack;
    fn v_stack(self) -> Stack
    where
        Self: Sized,
    {
        ViewTuple::stack(self, FlexDirection::Column)
    }
    fn h_stack(self) -> Stack
    where
        Self: Sized,
    {
        ViewTuple::stack(self, FlexDirection::Row)
    }
}

// Macro to implement ViewTuple for tuples of Views and Vec<Box<dyn View>>
macro_rules! impl_view_tuple {
    ($capacity:expr, $($t:ident),+) => {
        impl<$($t: IntoView + 'static),+> ViewTuple for ($($t,)+) {
            fn into_views(self) -> Vec<Box<dyn View>> {
                #[allow(non_snake_case)]
                let ($($t,)+) = self;
                vec![
                    $($t.into_any(),)+
                ]
            }
            fn stack(self, direction: FlexDirection) -> Stack {
                create_stack(self.into_views(), Some(direction))
            }
        }

        impl<$($t: IntoView + 'static),+> IntoView for ($($t,)+) {
            type V = crate::views::Stack;

            fn into_view(self) -> Self::V {
                #[allow(non_snake_case)]
                let ($($t,)+) = self;
                let views = vec![ $($t.into_any(),)+ ];
                crate::views::create_stack(views, None)
            }
        }
    };
}

impl_view_tuple!(1, A);
impl_view_tuple!(2, A, B);
impl_view_tuple!(3, A, B, C);
impl_view_tuple!(4, A, B, C, D);
impl_view_tuple!(5, A, B, C, D, E);
impl_view_tuple!(6, A, B, C, D, E, F);
impl_view_tuple!(7, A, B, C, D, E, F, G);
impl_view_tuple!(8, A, B, C, D, E, F, G, H);
impl_view_tuple!(9, A, B, C, D, E, F, G, H, I);
impl_view_tuple!(10, A, B, C, D, E, F, G, H, I, J);
impl_view_tuple!(11, A, B, C, D, E, F, G, H, I, J, K);
impl_view_tuple!(12, A, B, C, D, E, F, G, H, I, J, K, L);
impl_view_tuple!(13, A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_view_tuple!(14, A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_view_tuple!(15, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_view_tuple!(16, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
