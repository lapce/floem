use glazier::{
    kurbo::{Point, Vec2},
    Modifiers, MouseButton, MouseButtons,
};

#[derive(Debug, Clone)]
pub enum Event {
    MouseDown(MouseEvent),
    MouseUp(MouseEvent),
    MouseMove(MouseEvent),
    MouseWheel(MouseEvent),
    MouseLeft(),
}

#[derive(Debug, Clone)]
pub struct MouseEvent {
    /// The position of the mouse in the coordinate space of the receiver.
    pub pos: Point,
    /// The position of the mose in the window coordinate space.
    pub window_pos: Point,
    pub buttons: MouseButtons,
    pub mods: Modifiers,
    pub count: u8,
    pub focus: bool,
    pub button: MouseButton,
    pub wheel_delta: Vec2,
}

impl<'a> From<&'a glazier::MouseEvent> for MouseEvent {
    fn from(src: &glazier::MouseEvent) -> MouseEvent {
        let glazier::MouseEvent {
            pos,
            buttons,
            mods,
            count,
            focus,
            button,
            wheel_delta,
        } = src;
        MouseEvent {
            pos: *pos,
            window_pos: *pos,
            buttons: *buttons,
            mods: *mods,
            count: *count,
            focus: *focus,
            button: *button,
            wheel_delta: *wheel_delta,
        }
    }
}
