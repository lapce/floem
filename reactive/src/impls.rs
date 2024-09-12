use crate::{Memo, ReadSignal, RwSignal, SignalUpdate, SignalWith, WriteSignal};

// Macro for implementing binary operations (both int and float)
macro_rules! impl_bin_op {
    ($signal_type:ident, $trait:ident, $method:ident, $op:tt, $($type:ty),+) => {
        $(
            impl std::ops::$trait<$type> for $signal_type<$type> {
                type Output = $type;
                fn $method(self, rhs: $type) -> Self::Output {
                    self.with(|val| *val $op rhs)
                }
            }
        )+
    };
}

// Macro for implementing integer-only binary operations
macro_rules! impl_int_bin_op {
    ($signal_type:ident, $trait:ident, $method:ident, $op:tt, $($type:ty),+) => {
        $(
            impl std::ops::$trait<$type> for $signal_type<$type> {
                type Output = $type;
                fn $method(self, rhs: $type) -> Self::Output {
                    self.with(|val| *val $op rhs)
                }
            }
        )+
    };
}

// Macro for implementing assignment operations (both int and float)
macro_rules! impl_assign_op {
    ($signal_type:ident, $trait:ident, $method:ident, $op:tt, $($type:ty),+) => {
        $(
            impl std::ops::$trait<$type> for $signal_type<$type> {
                fn $method(&mut self, rhs: $type) {
                    self.update(|val| *val $op rhs);
                }
            }
        )+
    };
}

// Macro for implementing integer-only assignment operations
macro_rules! impl_int_assign_op {
    ($signal_type:ident, $trait:ident, $method:ident, $op:tt, $($type:ty),+) => {
        $(
            impl std::ops::$trait<$type> for $signal_type<$type> {
                fn $method(&mut self, rhs: $type) {
                    self.update(|val| *val $op rhs);
                }
            }
        )+
    };
}

// Macro for implementing PartialEq
macro_rules! impl_partial_eq {
    ($signal_type:ident, $($type:ty),+) => {
        $(
            impl PartialEq<$type> for $signal_type<$type> {
                fn eq(&self, other: &$type) -> bool {
                    self.with(|val| *val == *other)
                }
            }
        )+
    };
}

// Macro for operations that use 'with' (binary ops and PartialEq)
macro_rules! impl_with_ops {
    ($signal_type:ident, $($int_type:ty),+; $($float_type:ty),+) => {
        // Operations for both integer and float types
        impl_bin_op!($signal_type, Add, add, +, $($int_type,)+ $($float_type),+);
        impl_bin_op!($signal_type, Sub, sub, -, $($int_type,)+ $($float_type),+);
        impl_bin_op!($signal_type, Mul, mul, *, $($int_type,)+ $($float_type),+);
        impl_bin_op!($signal_type, Div, div, /, $($int_type,)+ $($float_type),+);

        // Operations only for integer types
        impl_int_bin_op!($signal_type, Rem, rem, %, $($int_type),+);
        impl_int_bin_op!($signal_type, BitAnd, bitand, &, $($int_type),+);
        impl_int_bin_op!($signal_type, BitOr, bitor, |, $($int_type),+);
        impl_int_bin_op!($signal_type, BitXor, bitxor, ^, $($int_type),+);
        impl_int_bin_op!($signal_type, Shl, shl, <<, $($int_type),+);
        impl_int_bin_op!($signal_type, Shr, shr, >>, $($int_type),+);

        // PartialEq for both integer and float types
        impl_partial_eq!($signal_type, $($int_type,)+ $($float_type),+);
    };
}

// Macro for assignment operations
macro_rules! impl_assign_ops {
    ($signal_type:ident, $($int_type:ty),+; $($float_type:ty),+) => {
        // Operations for both integer and float types
        impl_assign_op!($signal_type, AddAssign, add_assign, +=, $($int_type,)+ $($float_type),+);
        impl_assign_op!($signal_type, SubAssign, sub_assign, -=, $($int_type,)+ $($float_type),+);
        impl_assign_op!($signal_type, MulAssign, mul_assign, *=, $($int_type,)+ $($float_type),+);
        impl_assign_op!($signal_type, DivAssign, div_assign, /=, $($int_type,)+ $($float_type),+);

        // Operations only for integer types
        impl_int_assign_op!($signal_type, RemAssign, rem_assign, %=, $($int_type),+);
        impl_int_assign_op!($signal_type, BitAndAssign, bitand_assign, &=, $($int_type),+);
        impl_int_assign_op!($signal_type, BitOrAssign, bitor_assign, |=, $($int_type),+);
        impl_int_assign_op!($signal_type, BitXorAssign, bitxor_assign, ^=, $($int_type),+);
        impl_int_assign_op!($signal_type, ShlAssign, shl_assign, <<=, $($int_type),+);
        impl_int_assign_op!($signal_type, ShrAssign, shr_assign, >>=, $($int_type),+);

    };
}

// Combined macro for all operations
macro_rules! impl_all_ops {
    ($signal_type:ident, $($int_type:ty),+; $($float_type:ty),+) => {
        impl_with_ops!($signal_type, $($int_type),+; $($float_type),+);
        impl_assign_ops!($signal_type, $($int_type),+; $($float_type),+);
    };
}

impl_all_ops!(RwSignal, i32, i16, i8, u64, u32, u16, u8; f32, f64);
impl_assign_ops!(WriteSignal, i32, i16, i8, u64, u32, u16, u8; f32, f64);
impl_with_ops!(ReadSignal, i32, i16, i8, u64, u32, u16, u8; f32, f64);
impl_with_ops!(Memo, i32, i16, i8, u64, u32, u16, u8; f32, f64);
