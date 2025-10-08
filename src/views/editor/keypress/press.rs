use std::fmt::Display;

use crate::keyboard::{Key, KeyCode, KeyEvent, Modifiers, NamedKey, PhysicalKey};

use super::key::KeyInput;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyPress {
    pub key: KeyInput,
    pub mods: Modifiers,
}

impl KeyPress {
    pub fn new(key: KeyInput, mods: Modifiers) -> Self {
        Self { key, mods }
    }

    pub fn to_lowercase(&self) -> Self {
        let key = match &self.key {
            KeyInput::Keyboard(Key::Character(c), key_code) => {
                KeyInput::Keyboard(Key::Character(c.to_lowercase().into()), *key_code)
            }
            _ => self.key.clone(),
        };
        Self {
            key,
            mods: self.mods,
        }
    }

    pub fn is_char(&self) -> bool {
        let mut mods = self.mods;
        mods.set(Modifiers::SHIFT, false);
        if mods.is_empty() {
            if let KeyInput::Keyboard(Key::Character(_c), _) = &self.key {
                return true;
            }
        }
        false
    }

    pub fn is_modifiers(&self) -> bool {
        if let KeyInput::Keyboard(_, scancode) = &self.key {
            matches!(
                scancode,
                PhysicalKey::Code(KeyCode::MetaLeft)
                    | PhysicalKey::Code(KeyCode::MetaRight)
                    | PhysicalKey::Code(KeyCode::ShiftLeft)
                    | PhysicalKey::Code(KeyCode::ShiftRight)
                    | PhysicalKey::Code(KeyCode::ControlLeft)
                    | PhysicalKey::Code(KeyCode::ControlRight)
                    | PhysicalKey::Code(KeyCode::AltLeft)
                    | PhysicalKey::Code(KeyCode::AltRight)
            )
        } else {
            false
        }
    }

    pub fn label(&self) -> String {
        let mut keys = String::from("");
        if self.mods.control() {
            keys.push_str("Ctrl+");
        }
        if self.mods.alt() {
            keys.push_str("Alt+");
        }
        if self.mods.meta() {
            let keyname = match std::env::consts::OS {
                "macos" => "Cmd+",
                "windows" => "Win+",
                _ => "Meta+",
            };
            keys.push_str(keyname);
        }
        if self.mods.shift() {
            keys.push_str("Shift+");
        }
        keys.push_str(&self.key.to_string());
        keys.trim().to_string()
    }

    pub fn parse(key: &str) -> Vec<Self> {
        key.split(' ')
            .filter_map(|k| {
                let (modifiers, key) = match k.rsplit_once('+') {
                    Some(pair) => pair,
                    None => ("", k),
                };

                let key = match key.parse().ok() {
                    Some(key) => key,
                    None => {
                        // Skip past unrecognized key definitions
                        // warn!("Unrecognized key: {key}");
                        return None;
                    }
                };

                let mut mods = Modifiers::empty();
                for part in modifiers.to_lowercase().split('+') {
                    match part {
                        "ctrl" => mods.set(Modifiers::CONTROL, true),
                        "meta" => mods.set(Modifiers::META, true),
                        "shift" => mods.set(Modifiers::SHIFT, true),
                        "alt" => mods.set(Modifiers::ALT, true),
                        "altgr" => mods.set(Modifiers::ALT, true),
                        "" => (),
                        // other => warn!("Invalid key modifier: {}", other),
                        _ => {}
                    }
                }

                Some(KeyPress { key, mods })
            })
            .collect()
    }
}

impl Display for KeyPress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.mods.contains(Modifiers::CONTROL) {
            let _ = f.write_str("Ctrl+");
        }
        if self.mods.contains(Modifiers::ALT) {
            let _ = f.write_str("Alt+");
        }
        if self.mods.contains(Modifiers::META) {
            let _ = f.write_str("Meta+");
        }
        if self.mods.contains(Modifiers::SHIFT) {
            let _ = f.write_str("Shift+");
        }
        if self.mods.contains(Modifiers::ALTGR) {
            let _ = f.write_str("Altgr+");
        }
        f.write_str(&self.key.to_string())
    }
}
impl TryFrom<&KeyEvent> for KeyPress {
    type Error = ();

    fn try_from(ev: &KeyEvent) -> Result<Self, Self::Error> {
        Ok(KeyPress {
            key: KeyInput::Keyboard(ev.key.logical_key.clone(), ev.key.physical_key),
            mods: get_key_modifiers(ev),
        })
    }
}

pub fn get_key_modifiers(key_event: &KeyEvent) -> Modifiers {
    let mut mods = key_event.modifiers;

    match &key_event.key.logical_key {
        Key::Named(NamedKey::Shift) => mods.set(Modifiers::SHIFT, false),
        Key::Named(NamedKey::Alt) => mods.set(Modifiers::ALT, false),
        Key::Named(NamedKey::Meta) => mods.set(Modifiers::META, false),
        Key::Named(NamedKey::Control) => mods.set(Modifiers::CONTROL, false),
        _ => (),
    }

    mods
}
