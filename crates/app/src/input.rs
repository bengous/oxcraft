//! Logical key input translation, independent of window-system event types.

use winit::event::{ElementState, MouseButton};
use winit::keyboard::{Key, NamedKey};

use crate::game::Action;
use ox_core::player::Input;

/// One logical key press or release.
///
/// Wraps winit types because `winit::event::KeyEvent` cannot be constructed
/// outside winit's own event loop.
#[derive(Clone, Debug)]
pub struct KeyInput {
    /// Logical key identity.
    pub key: Key,
    /// Pressed or released.
    pub state: ElementState,
}

/// Builds a [`KeyInput`] from a driver key name (`"W"`, `"Space"`, `"3"`).
/// Returns `None` for names with no mapping.
pub fn synthetic_key(name: &str, pressed: bool) -> Option<KeyInput> {
    let state = if pressed {
        ElementState::Pressed
    } else {
        ElementState::Released
    };
    let logical = match name {
        "W" => Key::Character("w".into()),
        "A" => Key::Character("a".into()),
        "S" => Key::Character("s".into()),
        "D" => Key::Character("d".into()),
        "F" => Key::Character("f".into()),
        "Space" => Key::Named(NamedKey::Space),
        "Shift" => Key::Named(NamedKey::Shift),
        "Escape" => Key::Named(NamedKey::Escape),
        d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
            Key::Character(d.to_string().into())
        }
        _ => return None,
    };
    Some(KeyInput {
        key: logical,
        state,
    })
}

/// Aggregated input state: movement snapshot, queued actions, grab flag.
pub struct InputState {
    /// Movement flags consumed by [`Game::update`](crate::game::Game::update).
    pub input: Input,
    /// Actions queued since the last drain.
    pub actions: Vec<Action>,
    /// Whether the game view currently grabs keyboard and mouse.
    pub grabbed: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    /// An idle state: nothing held, nothing queued, ungrabbed.
    pub fn new() -> Self {
        Self {
            input: Input::default(),
            actions: Vec::new(),
            grabbed: false,
        }
    }

    /// Translates one key event into movement flags or queued actions.
    /// Ignored while the view is ungrabbed.
    pub fn key(&mut self, event: &KeyInput) {
        if !self.grabbed {
            return;
        }
        let pressed = event.state == ElementState::Pressed;
        match &event.key {
            Key::Character(c) => match c.to_uppercase().as_ref() {
                "F" => {
                    if pressed {
                        self.actions.push(Action::ToggleFly);
                    }
                }
                "W" => self.input.forward = pressed,
                "S" => self.input.back = pressed,
                "A" => self.input.left = pressed,
                "D" => self.input.right = pressed,
                digit if pressed && digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() => {
                    let n = (digit.as_bytes()[0] - b'0') as usize;
                    if n >= 1 {
                        self.actions.push(Action::Select(n - 1));
                    }
                }
                _ => {}
            },
            Key::Named(NamedKey::Space) => self.input.jump = pressed,
            Key::Named(NamedKey::Shift) => {
                self.input.sprint = pressed;
                self.input.down = pressed;
            }
            _ => {}
        }
    }

    /// Left press and release hold and drop the attack flag; right and
    /// middle presses queue place and pick. Ignored while the view is
    /// ungrabbed, except for the left release, which always drops the
    /// attack so a button let go off-view cannot leave it held.
    pub fn mouse(&mut self, button: MouseButton, pressed: bool) {
        if button == MouseButton::Left && !pressed {
            self.input.attack = false;
            return;
        }
        if !self.grabbed {
            return;
        }
        match (button, pressed) {
            (MouseButton::Left, true) => self.input.attack = true,
            (MouseButton::Right, true) => self.actions.push(Action::Place),
            (MouseButton::Middle, true) => self.actions.push(Action::Pick),
            _ => {}
        }
    }
}
