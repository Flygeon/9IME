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

pub fn process_key(ke: &KeyEvent) -> EngineOutput {
    let (ax, ay) = crate::ipc::current_anchor();
    let req = Request::ProcessKey {
        keycode: ke.keycode,
        mask: ke.mask,
        anchor_x: ax,
        anchor_y: ay,
    };
    with_client(|c| {
        if c.is_none() {
            *c = Client::connect();
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
                *c = None;
                EngineOutput::Passthrough
            }
        }
    })
}
