use crate::{Memo, ReadSignal, RwSignal, SignalUpdate, SignalWith, WriteSignal};

// Unary operation macro
macro_rules! impl_unary_op {
    ($signal_type:ident, $trait:ident, $method:ident, $op:tt, [$($extra:tt)*]) => {
        impl<T: 'static $(+ $extra)*> std::ops::$trait for $signal_type<T>
        where
            for<'a> &'a T: std::ops::$trait<Output = T>,
        {
            type Output = T;

            fn $method(self) -> Self::Output {
                self.with(|val| $op val)
            }
        }
    };
}

macro_rules! impl_bin_op {
    ($signal_type:ident, $trait:ident, $method:ident, $op:tt, [$($extra:tt)*]) => {
        impl<T: std::ops::$trait<Output = T> + 'static $(+ $extra)*> std::ops::$trait<T>
            for $signal_type<T>
        where
            for<'a> &'a T: std::ops::$trait<T, Output = T>,
        {
            type Output = T;
            fn $method(self, rhs: T) -> Self::Output {
                self.with(|val| val $op rhs)
            }
        }
    };
}

macro_rules! impl_assign_op {
    ($signal_type:ident, $trait:ident, $method:ident, $op:tt, [$($extra:tt)*]) => {
        impl<T: std::ops::$trait + 'static $(+ $extra)*> std::ops::$trait<T> for $signal_type<T> {
            fn $method(&mut self, rhs: T) {
                self.update(|val| *val $op rhs);
            }
        }
    };
}

macro_rules! impl_partial_eq {
    ($signal_type:ident, [$($extra:tt)*]) => {
        impl<T: PartialEq + 'static $(+ $extra)*> PartialEq<T> for $signal_type<T> {
            fn eq(&self, other: &T) -> bool {
                self.with(|val| *val == *other)
            }
        }
    };
}

macro_rules! impl_display {
    ($signal_type:ident, [$($extra:tt)*]) => {
        impl<T: std::fmt::Display + 'static $(+ $extra)*> std::fmt::Display for $signal_type<T> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.with(|val| std::fmt::Display::fmt(val, f))
            }
        }
    };
}

macro_rules! impl_with_ops {
    ($signal_type:ident, [$($extra:tt)*]) => {
        impl_bin_op!($signal_type, Add, add, +, [$($extra)*]);
        impl_bin_op!($signal_type, Sub, sub, -, [$($extra)*]);
        impl_bin_op!($signal_type, Mul, mul, *, [$($extra)*]);
        impl_bin_op!($signal_type, Div, div, /, [$($extra)*]);
        impl_bin_op!($signal_type, Rem, rem, %, [$($extra)*]);
        impl_bin_op!($signal_type, BitAnd, bitand, &, [$($extra)*]);
        impl_bin_op!($signal_type, BitOr, bitor, |, [$($extra)*]);
        impl_bin_op!($signal_type, BitXor, bitxor, ^, [$($extra)*]);
        impl_bin_op!($signal_type, Shl, shl, <<, [$($extra)*]);
        impl_bin_op!($signal_type, Shr, shr, >>, [$($extra)*]);
        impl_unary_op!($signal_type, Not, not, !, [$($extra)*]);
        impl_unary_op!($signal_type, Neg, neg, -, [$($extra)*]);
        impl_partial_eq!($signal_type, [$($extra)*]);
        impl_display!($signal_type, [$($extra)*]);
    };
}

macro_rules! impl_assign_ops {
    ($signal_type:ident, [$($extra:tt)*]) => {
        impl_assign_op!($signal_type, AddAssign, add_assign, +=, [$($extra)*]);
        impl_assign_op!($signal_type, SubAssign, sub_assign, -=, [$($extra)*]);
        impl_assign_op!($signal_type, MulAssign, mul_assign, *=, [$($extra)*]);
        impl_assign_op!($signal_type, DivAssign, div_assign, /=, [$($extra)*]);
        impl_assign_op!($signal_type, RemAssign, rem_assign, %=, [$($extra)*]);
        impl_assign_op!($signal_type, BitAndAssign, bitand_assign, &=, [$($extra)*]);
        impl_assign_op!($signal_type, BitOrAssign, bitor_assign, |=, [$($extra)*]);
        impl_assign_op!($signal_type, BitXorAssign, bitxor_assign, ^=, [$($extra)*]);
        impl_assign_op!($signal_type, ShlAssign, shl_assign, <<=, [$($extra)*]);
        impl_assign_op!($signal_type, ShrAssign, shr_assign, >>=, [$($extra)*]);
    };
}

macro_rules! impl_all_ops {
    ($signal_type:ident, [$($extra:tt)*]) => {
        impl_assign_ops!($signal_type, [$($extra)*]);
        impl_with_ops!($signal_type, [$($extra)*]);
    };
}

impl_all_ops!(RwSignal, []);
impl_assign_ops!(WriteSignal, []);
impl_with_ops!(ReadSignal, []);
impl_with_ops!(Memo, [PartialEq]);
