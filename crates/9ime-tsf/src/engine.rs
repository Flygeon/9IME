//! Engine client (M3): forwards keys to nineime-server over the named pipe.
//! The server owns librime and renders the candidate window.

use std::sync::{Mutex, OnceLock};

use nineime_ipc::{Request, Response};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

use crate::ipc::Client;

pub struct KeyEvent {
    pub keycode: u32,
    pub mask: u32,
}

// Modifier masks follow the X11 convention librime expects:
// ShiftMask=1, LockMask=2, ControlMask=4, Mod1Mask(Alt)=8.
pub const MASK_SHIFT: u32 = 1;
pub const MASK_LOCK: u32 = 1 << 1;
pub const MASK_CTRL: u32 = 1 << 2;
pub const MASK_ALT: u32 = 1 << 3;

pub enum EngineOutput {
    Passthrough,
    Handled { commit: Option<String> },
}

const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_MENU: i32 = 0x12;
const VK_CAPITAL: i32 = 0x14;

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
        if GetKeyState(VK_CAPITAL) & 1 != 0 {
            m |= MASK_LOCK;
        }
    }
    m
}

static CLIENT: OnceLock<Mutex<Option<Client>>> = OnceLock::new();

fn log_line(msg: &str) {
    // diagnostic: %APPDATA%\9IME\tsf.log (appends one line per call)
    if let Ok(dir) = std::env::var("APPDATA") {
        let path = std::path::Path::new(&dir).join("9IME").join("tsf.log");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            use std::io::Write;
            let _ = writeln!(f, "{msg}");
        }
    }
}

fn with_client<T>(f: impl FnOnce(&mut Option<Client>) -> T) -> T {
    let c = CLIENT.get_or_init(|| Mutex::new(None));
    let mut guard = c.lock().unwrap();
    f(&mut guard)
}

pub fn on_focus(focused: bool) {
    with_client(|c| {
        if let Some(cl) = c {
            let _ = cl.request(&Request::Focus { focused });
        }
    });
}

/// Convert a Windows VK code + shift state into a librime keycode.
/// Printable keys map to their ASCII keysym (shift selects the shifted
/// glyph); control/navigation keys use X11 keysyms (0xFFxx), which is
/// what librime matches against XK_* constants.
fn vk_to_rime(vk: u32, shift: bool) -> u32 {
    match vk {
        0x08 => 0xFF08, // BackSpace
        0x09 => 0xFF09, // Tab
        0x0D => 0xFF0D, // Return
        0x13 => 0xFF13, // Pause
        0x14 => 0xFFE5, // CapsLock
        0x1B => 0xFF1B, // Escape
        0x20 => 0x20,   // space
        0x21 => 0xFF55, // Prior (PageUp)
        0x22 => 0xFF56, // Next (PageDown)
        0x23 => 0xFF57, // End
        0x24 => 0xFF50, // Home
        0x25 => 0xFF51, // Left
        0x26 => 0xFF52, // Up
        0x27 => 0xFF53, // Right
        0x28 => 0xFF54, // Down
        0x2D => 0xFF63, // Insert
        0x2E => 0xFFFF, // Delete
        0x30..=0x39 => {
            if shift {
                match vk {
                    0x30 => b')' as u32,
                    0x31 => b'!' as u32,
                    0x32 => b'@' as u32,
                    0x33 => b'#' as u32,
                    0x34 => b'$' as u32,
                    0x35 => b'%' as u32,
                    0x36 => b'^' as u32,
                    0x37 => b'&' as u32,
                    0x38 => b'*' as u32,
                    0x39 => b'(' as u32,
                    _ => vk,
                }
            } else {
                vk
            }
        }
        0x41..=0x5A => {
            if shift { vk } else { vk + 0x20 }
        }
        0x60..=0x69 => 0xFFB0 + (vk - 0x60), // KP_0..9
        0x6A => 0xFFAA, // KP_Multiply
        0x6B => 0xFFAB, // KP_Add
        0x6C => 0xFFAC, // KP_Separator
        0x6D => 0xFFAD, // KP_Subtract
        0x6E => 0xFFAE, // KP_Decimal
        0x6F => 0xFFAF, // KP_Divide
        0x70..=0x7B => 0xFFBE + (vk - 0x70), // F1..F12
        0xBA => if shift { b':' as u32 } else { b';' as u32 }, // OEM_1
        0xBB => if shift { b'+' as u32 } else { b'=' as u32 }, // OEM_PLUS
        0xBC => if shift { b'<' as u32 } else { b',' as u32 }, // OEM_COMMA
        0xBD => if shift { b'_' as u32 } else { b'-' as u32 }, // OEM_MINUS
        0xBE => if shift { b'>' as u32 } else { b'.' as u32 }, // OEM_PERIOD
        0xBF => if shift { b'?' as u32 } else { b'/' as u32 }, // OEM_2
        0xC0 => if shift { b'~' as u32 } else { b'`' as u32 }, // OEM_3 backtick
        0xDB => if shift { b'{' as u32 } else { b'[' as u32 }, // OEM_4
        0xDC => if shift { b'|' as u32 } else { b'\\' as u32 }, // OEM_5 backslash
        0xDD => if shift { b'}' as u32 } else { b']' as u32 }, // OEM_6
        0xDE => if shift { b'"' as u32 } else { b'\'' as u32 }, // OEM_7 quote
        _ => vk,
    }
}

/// Keys we never forward to the engine: bare modifier presses and anything
/// with Alt held (system shortcuts / menu accelerators).
fn should_skip(ke: &KeyEvent) -> bool {
    matches!(ke.keycode, 0x10..=0x12 | 0x5B | 0x5C) // shift/ctrl/menu, L/R Win
        || ke.mask & MASK_ALT != 0
}

pub fn process_key(ke: &KeyEvent) -> EngineOutput {
    if should_skip(ke) {
        return EngineOutput::Passthrough;
    }
    let (ax, ay) = crate::ipc::current_anchor();
    let shifted = ke.mask & MASK_SHIFT != 0;
    let keycode = vk_to_rime(ke.keycode, shifted);
    let req = Request::ProcessKey {
        keycode,
        mask: ke.mask,
        anchor_x: ax,
        anchor_y: ay,
    };
    with_client(|c| {
        if c.is_none() {
            *c = Client::connect();
            if c.is_none() {
                log_line("connect to server failed - keys will pass through");
            }
        }
        match c.as_ref().and_then(|cl| cl.request(&req)) {
            Some(Response::KeyResult { handled, commit, .. }) => {
                if handled {
                    EngineOutput::Handled { commit }
                } else {
                    EngineOutput::Passthrough
                }
            }
            _ => {
                log_line("server request failed - reconnecting next key");
                *c = None;
                EngineOutput::Passthrough
            }
        }
    })
}
