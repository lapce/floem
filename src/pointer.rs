use std::hash::{Hash, Hasher};

use winit::event::ButtonSource;
pub use winit::event::{FingerId, Force};

use peniko::kurbo::{Point, Vec2};

use crate::keyboard::Modifiers;

#[derive(Debug, Clone)]
pub struct PointerWheelEvent {
    pub pos: Point,
    pub delta: Vec2,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum PointerButton {
    Mouse(MouseButton),
    Touch {
        finger_id: FingerId,
        force: Option<Force>,
    },
    Unknown(u16),
}

impl Eq for PointerButton {}

impl Hash for PointerButton {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            PointerButton::Mouse(mouse_button) => mouse_button.hash(state),
            PointerButton::Touch { finger_id, .. } => finger_id.hash(state),
            PointerButton::Unknown(n) => n.hash(state),
        }
    }
}

impl From<ButtonSource> for PointerButton {
    fn from(value: ButtonSource) -> Self {
        match value {
            ButtonSource::Mouse(mouse_button) => PointerButton::Mouse(mouse_button.into()),
            ButtonSource::Touch { finger_id, force } => PointerButton::Touch { finger_id, force },
            ButtonSource::Unknown(n) => PointerButton::Unknown(n),
            ButtonSource::TabletTool { .. } => {
                // todo! fixme
                PointerButton::Unknown(0)
            }
        }
    }
}

impl PointerButton {
    pub fn is_primary(&self) -> bool {
        self.mouse_button() == MouseButton::Primary
    }

    pub fn is_secondary(&self) -> bool {
        self.mouse_button() == MouseButton::Secondary
    }

    pub fn is_auxiliary(&self) -> bool {
        self.mouse_button() == MouseButton::Auxiliary
    }

    pub fn mouse_button(self) -> MouseButton {
        match self {
            PointerButton::Mouse(mouse) => mouse,
            PointerButton::Touch { .. } => MouseButton::Primary,
            PointerButton::Unknown(button) => match button {
                0 => MouseButton::Primary,
                1 => MouseButton::Auxiliary,
                2 => MouseButton::Secondary,
                3 => MouseButton::X1,
                4 => MouseButton::X2,
                _ => MouseButton::None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash, Ord, PartialOrd)]
pub enum MouseButton {
    Primary,
    Secondary,
    Auxiliary,
    X1,
    X2,
    None,
}

impl From<winit::event::MouseButton> for MouseButton {
    fn from(value: winit::event::MouseButton) -> Self {
        match value {
            winit::event::MouseButton::Left => Self::Primary,
            winit::event::MouseButton::Right => Self::Secondary,
            winit::event::MouseButton::Middle => Self::Auxiliary,
            winit::event::MouseButton::Back => Self::X1,
            winit::event::MouseButton::Forward => Self::X2,
            winit::event::MouseButton::Other(_) => Self::None,
        }
    }
}

impl MouseButton {
    pub fn is_primary(&self) -> bool {
        self == &MouseButton::Primary
    }

    pub fn is_secondary(&self) -> bool {
        self == &MouseButton::Secondary
    }

    pub fn is_auxiliary(&self) -> bool {
        self == &MouseButton::Auxiliary
    }

    pub fn is_x1(&self) -> bool {
        self == &MouseButton::X1
    }

    pub fn is_x2(&self) -> bool {
        self == &MouseButton::X2
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
