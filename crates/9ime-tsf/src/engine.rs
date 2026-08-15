//! M2 placeholder engine: passthrough that commits printable ASCII.
//! Replaced by the librime engine + named-pipe IPC in M3.

use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

pub struct KeyEvent {
    pub keycode: u32,
    pub mask: u32,
}

pub const MASK_SHIFT: u32 = 1 << 0;
pub const MASK_CTRL: u32 = 1 << 1;
pub const MASK_ALT: u32 = 1 << 2;

pub enum EngineOutput {
    Passthrough,
    Handled { commit: Option<String> },
}

const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_MENU: i32 = 0x12;

pub fn current_mask() -> u32 {
    let mut m = 0;
    unsafe {
        if GetKeyState(VK_SHIFT) < 0 {
            m |= MASK_SHIFT;
        }
        if GetKeyState(VK_CONTROL) < 0 {
            m |= MASK_CTRL;
        }
        if GetKeyState(VK_MENU) < 0 {
            m |= MASK_ALT;
        }
    }
    m
}

pub fn process(ke: &KeyEvent) -> EngineOutput {
    if ke.mask & (MASK_CTRL | MASK_ALT) != 0 {
        return EngineOutput::Passthrough;
    }
    let kc = ke.keycode;
    if (0x30..=0x39).contains(&kc) {
        let ch = (kc - 0x30) as u8 as char;
        return EngineOutput::Handled { commit: Some(ch.to_string()) };
    }
    if (0x41..=0x5a).contains(&kc) {
        let ch = if ke.mask & MASK_SHIFT != 0 {
            kc as u8 as char
        } else {
            (kc + 0x20) as u8 as char
        };
        return EngineOutput::Handled { commit: Some(ch.to_string()) };
    }
    match kc {
        0x20 => EngineOutput::Handled { commit: Some(" ".to_string()) },
        _ => EngineOutput::Passthrough,
    }
}
