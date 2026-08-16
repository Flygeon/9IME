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

/// Convert a Windows VK code + shift state into a librime keycode
/// (ASCII for printable keys, X11 keysyms for navigation).
fn vk_to_rime(vk: u32, shift: bool) -> u32 {
    match vk {
        0x20 => 0x20, // space
        0x0D => 0x0D, // return
        0x08 => 0x08, // backspace
        0x09 => 0x09, // tab
        0x1B => 0x1B, // escape
        0x21 => 0xFF55, // page up
        0x22 => 0xFF56, // page down
        0x25 => 0xFF51, // left
        0x26 => 0xFF52, // up
        0x27 => 0xFF53, // right
        0x28 => 0xFF54, // down
        0x30..=0x39 => vk, // digits
        0x41..=0x5A => {
            if shift { vk } else { vk + 0x20 }
        }
        _ => vk,
    }
}

pub fn process_key(ke: &KeyEvent) -> EngineOutput {
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
