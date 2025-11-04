// Copyright 2025 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Support routines for converting pointer data from [`winit`].

use ui_events::pointer::PointerButton;
use winit::event::MouseButton;

/// Try to make a [`PointerButton`] from a [`MouseButton`].
///
/// Because values of [`MouseButton::Other`] can start at 0, they are mapped
/// to the arbitrary buttons B7..B32.
/// Values greater than 25 will not be mapped.
pub fn try_from_winit_button(b: MouseButton) -> Option<PointerButton> {
    Some(match b {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Auxiliary,
        MouseButton::Back => PointerButton::X1,
        MouseButton::Forward => PointerButton::X2,
        MouseButton::Other(u) => match u {
            6 => PointerButton::B7,
            7 => PointerButton::B8,
            8 => PointerButton::B9,
            9 => PointerButton::B10,
            10 => PointerButton::B11,
            11 => PointerButton::B12,
            12 => PointerButton::B13,
            13 => PointerButton::B14,
            14 => PointerButton::B15,
            15 => PointerButton::B16,
            16 => PointerButton::B17,
            17 => PointerButton::B18,
            18 => PointerButton::B19,
            19 => PointerButton::B20,
            20 => PointerButton::B21,
            21 => PointerButton::B22,
            22 => PointerButton::B23,
            23 => PointerButton::B24,
            24 => PointerButton::B25,
            25 => PointerButton::B26,
            26 => PointerButton::B27,
            27 => PointerButton::B28,
            28 => PointerButton::B29,
            29 => PointerButton::B30,
            30 => PointerButton::B31,
            31 => PointerButton::B32,
            _ => {
                return None;
            }
        },
    })
}
