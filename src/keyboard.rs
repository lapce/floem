pub use floem_winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, NativeKey, PhysicalKey};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct KeyEvent {
    pub key: floem_winit::event::KeyEvent,
    pub modifiers: ModifiersState,
}
