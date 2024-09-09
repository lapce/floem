use floem_winit::event::MouseButton;
use peniko::kurbo::{Point, Vec2};

use crate::keyboard::Modifiers;

#[derive(Debug, Clone)]
pub struct PointerWheelEvent {
    pub pos: Point,
    pub delta: Vec2,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash, Ord, PartialOrd)]
pub enum PointerButton {
    Primary,
    Secondary,
    Auxiliary,
    X1,
    X2,
    None,
}

impl From<MouseButton> for PointerButton {
    fn from(value: MouseButton) -> Self {
        match value {
            MouseButton::Left => Self::Primary,
            MouseButton::Right => Self::Secondary,
            MouseButton::Middle => Self::Auxiliary,
            MouseButton::Back => Self::X1,
            MouseButton::Forward => Self::X2,
            MouseButton::Other(_) => Self::None,
        }
    }
}

impl PointerButton {
    pub fn is_primary(self) -> bool {
        self == PointerButton::Primary
    }

    pub fn is_secondary(self) -> bool {
        self == PointerButton::Secondary
    }

    pub fn is_auxiliary(self) -> bool {
        self == PointerButton::Auxiliary
    }

    pub fn is_x1(self) -> bool {
        self == PointerButton::X1
    }

    pub fn is_x2(self) -> bool {
        self == PointerButton::X2
    }
}

#[derive(Debug, Clone)]
pub struct PointerInputEvent {
    pub pos: Point,
    pub button: PointerButton,
    pub modifiers: Modifiers,
    pub count: u8,
}

#[derive(Debug, Clone)]
pub struct PointerMoveEvent {
    pub pos: Point,
    pub modifiers: Modifiers,
}
