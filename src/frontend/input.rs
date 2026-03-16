use winit::keyboard::KeyCode;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PlayerKeys {
    pub up: KeyCode, pub down: KeyCode, pub left: KeyCode, pub right: KeyCode,
    pub b1: KeyCode, pub b2: KeyCode, pub start: KeyCode,
}

impl PlayerKeys {
    pub fn p1() -> Self {
        Self {
            up: KeyCode::ArrowUp, down: KeyCode::ArrowDown,
            left: KeyCode::ArrowLeft, right: KeyCode::ArrowRight,
            b1: KeyCode::KeyZ, b2: KeyCode::KeyX, start: KeyCode::Enter,
        }
    }
    pub fn p2() -> Self {
        Self {
            up: KeyCode::KeyW, down: KeyCode::KeyS,
            left: KeyCode::KeyA, right: KeyCode::KeyD,
            b1: KeyCode::Digit1, b2: KeyCode::Digit2, start: KeyCode::Digit3,
        }
    }
    pub fn get(&self, action: usize) -> KeyCode {
        [self.up, self.down, self.left, self.right, self.b1, self.b2, self.start][action]
    }
    pub fn set(&mut self, action: usize, key: KeyCode) {
        match action {
            0 => self.up = key, 1 => self.down = key, 2 => self.left = key,
            3 => self.right = key, 4 => self.b1 = key, 5 => self.b2 = key,
            6 => self.start = key, _ => {}
        }
    }
}

#[derive(Clone)]
pub struct KeyConfig {
    pub p1: PlayerKeys,
    pub p2: PlayerKeys,
}

impl Default for KeyConfig {
    fn default() -> Self { Self { p1: PlayerKeys::p1(), p2: PlayerKeys::p2() } }
}

pub fn key_label(k: KeyCode) -> &'static str {
    match k {
        KeyCode::ArrowUp    => "↑",
        KeyCode::ArrowDown  => "↓",
        KeyCode::ArrowLeft  => "←",
        KeyCode::ArrowRight => "→",
        KeyCode::Enter      => "Enter",
        KeyCode::Space      => "Space",
        KeyCode::Escape     => "Esc",
        KeyCode::Tab        => "Tab",
        KeyCode::Backspace  => "Backspace",
        KeyCode::KeyA => "A", KeyCode::KeyB => "B", KeyCode::KeyC => "C",
        KeyCode::KeyD => "D", KeyCode::KeyE => "E", KeyCode::KeyF => "F",
        KeyCode::KeyG => "G", KeyCode::KeyH => "H", KeyCode::KeyI => "I",
        KeyCode::KeyJ => "J", KeyCode::KeyK => "K", KeyCode::KeyL => "L",
        KeyCode::KeyM => "M", KeyCode::KeyN => "N", KeyCode::KeyO => "O",
        KeyCode::KeyP => "P", KeyCode::KeyQ => "Q", KeyCode::KeyR => "R",
        KeyCode::KeyS => "S", KeyCode::KeyT => "T", KeyCode::KeyU => "U",
        KeyCode::KeyV => "V", KeyCode::KeyW => "W", KeyCode::KeyX => "X",
        KeyCode::KeyY => "Y", KeyCode::KeyZ => "Z",
        KeyCode::Digit0 => "0", KeyCode::Digit1 => "1", KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3", KeyCode::Digit4 => "4", KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6", KeyCode::Digit7 => "7", KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        KeyCode::F1  => "F1",  KeyCode::F2  => "F2",  KeyCode::F3  => "F3",
        KeyCode::F4  => "F4",  KeyCode::F5  => "F5",  KeyCode::F6  => "F6",
        KeyCode::F7  => "F7",  KeyCode::F8  => "F8",  KeyCode::F9  => "F9",
        KeyCode::F10 => "F10", KeyCode::F11 => "F11", KeyCode::F12 => "F12",
        _ => "?",
    }
}

#[derive(Default)]
pub struct PadState {
    pub up: bool, pub down: bool, pub left: bool, pub right: bool,
    pub b1: bool, pub b2: bool, pub start: bool,
}
