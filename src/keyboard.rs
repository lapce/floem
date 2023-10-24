pub use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, NativeKey};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct KeyEvent {
    pub key: winit::event::KeyEvent,
    pub modifiers: ModifiersState,
}
