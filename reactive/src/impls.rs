use crate::{Memo, ReadSignal, RwSignal, SignalUpdate, SignalWith, WriteSignal};

// Binary operation macro
macro_rules! impl_bin_op {
    ($signal_type:ident, $trait:ident, $method:ident, $op:tt) => {
        impl<T: std::ops::$trait<Output = T> + 'static> std::ops::$trait<T> for $signal_type<T>
        where
            for<'a> &'a T: std::ops::$trait<T, Output = T>
        {
            type Output = T;
            fn $method(self, rhs: T) -> Self::Output {
                self.with(|val| val $op rhs)
            }
        }
    };
}

// Assignment operation macro
macro_rules! impl_assign_op {
    ($signal_type:ident, $trait:ident, $method:ident, $op:tt) => {
        impl<T: std::ops::$trait + 'static> std::ops::$trait<T> for $signal_type<T> {
            fn $method(&mut self, rhs: T) {
                self.update(|val| *val $op rhs);
            }
        }
    };
}

// PartialEq implementation macro
macro_rules! impl_partial_eq {
    ($signal_type:ident) => {
        impl<T: PartialEq + 'static> PartialEq<T> for $signal_type<T> {
            fn eq(&self, other: &T) -> bool {
                self.with(|val| *val == *other)
            }
        }
    };
}

// Macro for implementing all binary operations and PartialEq
macro_rules! impl_with_ops {
    ($signal_type:ident) => {
        impl_bin_op!($signal_type, Add, add, +);
        impl_bin_op!($signal_type, Sub, sub, -);
        impl_bin_op!($signal_type, Mul, mul, *);
        impl_bin_op!($signal_type, Div, div, /);
        impl_bin_op!($signal_type, Rem, rem, %);
        impl_bin_op!($signal_type, BitAnd, bitand, &);
        impl_bin_op!($signal_type, BitOr, bitor, |);
        impl_bin_op!($signal_type, BitXor, bitxor, ^);
        impl_bin_op!($signal_type, Shl, shl, <<);
        impl_bin_op!($signal_type, Shr, shr, >>);
        impl_partial_eq!($signal_type);
    };
}

// Macro for implementing all assignment operations
macro_rules! impl_assign_ops {
    ($signal_type:ident) => {
        impl_assign_op!($signal_type, AddAssign, add_assign, +=);
        impl_assign_op!($signal_type, SubAssign, sub_assign, -=);
        impl_assign_op!($signal_type, MulAssign, mul_assign, *=);
        impl_assign_op!($signal_type, DivAssign, div_assign, /=);
        impl_assign_op!($signal_type, RemAssign, rem_assign, %=);
        impl_assign_op!($signal_type, BitAndAssign, bitand_assign, &=);
        impl_assign_op!($signal_type, BitOrAssign, bitor_assign, |=);
        impl_assign_op!($signal_type, BitXorAssign, bitxor_assign, ^=);
        impl_assign_op!($signal_type, ShlAssign, shl_assign, <<=);
        impl_assign_op!($signal_type, ShrAssign, shr_assign, >>=);
    };
}

macro_rules! impl_all_ops {
    ($signal_type:ident) => {
        impl_assign_ops!($signal_type);
        impl_with_ops!($signal_type);
    };
}

impl_all_ops!(RwSignal);
impl_assign_ops!(WriteSignal);
impl_with_ops!(ReadSignal);
impl_with_ops!(Memo);
