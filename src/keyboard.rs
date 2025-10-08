use bitflags::bitflags;
pub use winit::keyboard::{
    Key, KeyCode, KeyLocation, ModifiersState, NamedKey, NativeKey, PhysicalKey,
};

/// Represents a single keyboard input with any active modifier keys.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct KeyEvent {
    pub key: winit::event::KeyEvent,
    pub modifiers: Modifiers,
}

bitflags! {
    /// Represents the current state of the keyboard modifiers
    ///
    /// Each flag represents a modifier and is set if this modifier is active.
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Modifiers: u32 {
        /// The "shift" key.
        const SHIFT = 0b100;
        /// The "control" key.
        const CONTROL = 0b100 << 3;
        /// The "alt" key.
        const ALT = 0b100 << 6;
        /// This is the "windows" key on PC and "command" key on Mac.
        const META = 0b100 << 9;
        /// The "altgr" key.
        const ALTGR = 0b100 << 12;
    }
}

impl Modifiers {
    /// Returns `true` if the shift key is pressed.
    pub fn shift(&self) -> bool {
        self.intersects(Self::SHIFT)
    }
    /// Returns `true` if the control key is pressed.
    pub fn control(&self) -> bool {
        self.intersects(Self::CONTROL)
    }
    /// Returns `true` if the alt key is pressed.
    pub fn alt(&self) -> bool {
        self.intersects(Self::ALT)
    }
    /// Returns `true` if the meta key is pressed.
    pub fn meta(&self) -> bool {
        self.intersects(Self::META)
    }
    /// Returns `true` if the altgr key is pressed.
    pub fn altgr(&self) -> bool {
        self.intersects(Self::ALTGR)
    }
}

impl From<ModifiersState> for Modifiers {
    fn from(value: ModifiersState) -> Self {
        let mut modifiers = Modifiers::empty();
        if value.shift_key() {
            modifiers.set(Modifiers::SHIFT, true);
        }
        if value.alt_key() {
            modifiers.set(Modifiers::ALT, true);
        }
        if value.control_key() {
            modifiers.set(Modifiers::CONTROL, true);
        }
        if value.meta_key() {
            modifiers.set(Modifiers::META, true);
        }
        modifiers
    }
}
