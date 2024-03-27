pub use floem_winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, NativeKey, PhysicalKey};
pub use floem_winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct KeyEvent {
    pub key: floem_winit::event::KeyEvent,
    pub modifiers: ModifiersState,
}
