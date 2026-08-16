//! Named-pipe server loop: one client at a time, JSON messages.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT};

use nineime_ipc::{self, Request, Response};
use nineime_librime::{Rime, RimeSession};

use crate::{UiState, context_msg, status_msg};

fn write_all(h: HANDLE, mut data: &[u8]) -> bool {
    while !data.is_empty() {
        let mut written = 0;
        if unsafe { WriteFile(h, Some(data), Some(&mut written), None) }.is_err() {
            return false;
        }
        if written == 0 {
            return false;
        }
        data = &data[written as usize..];
    }
    true
}

fn read_exact(h: HANDLE, buf: &mut [u8]) -> bool {
    let mut off = 0;
    while off < buf.len() {
        let mut got = 0;
        if unsafe { ReadFile(h, Some(&mut buf[off..]), Some(&mut got), None) }.is_err() {
            return false;
        }
        if got == 0 {
            return false;
        }
        off += got as usize;
    }
    true
}

fn read_msg(h: HANDLE) -> Option<Request> {
    let mut len_buf = [0u8; 4];
    if !read_exact(h, &mut len_buf) {
        return None;
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 1 << 20 {
        return None;
    }
    let mut body = vec![0u8; len];
    if !read_exact(h, &mut body) {
        return None;
    }
    nineime_ipc::decode::<Request>(&body).ok()
}

pub fn serve(
    rime: &Rime,
    sess: &mut RimeSession,
    ui: Arc<Mutex<UiState>>,
    changed: Arc<AtomicBool>,
) {
    let pipe_name: Vec<u16> = nineime_ipc::PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    let mut shutdown = false;

    while !shutdown {
        let h = unsafe {
            CreateNamedPipeW(
                windows::core::PCWSTR(pipe_name.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                65536,
                65536,
                0,
                None,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            eprintln!("9IME server: CreateNamedPipeW failed: {}", unsafe { GetLastError() }.0);
            std::thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }
        let ok = unsafe { ConnectNamedPipe(h, None) }.is_ok()
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if !ok {
            let _ = unsafe { CloseHandle(h) };
            continue;
        }
        crate::slog::log("client connected");

        loop {
            let Some(req) = read_msg(h) else { break };
            let is_shutdown = matches!(&req, Request::Shutdown);
            let resp = handle(rime, sess, req, &ui, &changed);
            let bytes = nineime_ipc::encode(&resp);
            if !write_all(h, &bytes) {
                break;
            }
            // Shutdown is answered as Ok; detect via the request itself.
            if is_shutdown {
                shutdown = true;
                ui.lock().unwrap().quit = true;
                break;
            }
        }
        let _ = unsafe { CloseHandle(h) };
        crate::slog::log("client disconnected");
    }
}

fn handle(
    rime: &Rime,
    sess: &mut RimeSession,
    req: Request,
    ui: &Arc<Mutex<UiState>>,
    changed: &Arc<AtomicBool>,
) -> Response {
    match req {
        Request::Hello { pid } => {
            println!("9IME server: hello from pid {pid}");
            Response::Hello {
                ok: true,
                version: rime.version().unwrap_or_default(),
            }
        }
        Request::Focus { focused } => {
            let mut s = ui.lock().unwrap();
            if !focused {
                s.visible = false;
                changed.store(true, Ordering::Relaxed);
            }
            Response::Ok { ok: true }
        }
        Request::ProcessKey {
            keycode,
            mask,
            anchor_x,
            anchor_y,
        } => {
            let handled = sess.process_key(keycode, mask);
            let commit = sess.get_commit();
            let context = sess.get_context().map(|c| context_msg(&c)).unwrap_or_default();
            let status = sess.get_status().map(|s| status_msg(&s)).unwrap_or_default();
            let mut st = ui.lock().unwrap();
            // skin hot-reload: re-read the user config on every key event
            let want = nineime_core::config::load().skin;
            if st.loaded_skin != want {
                st.loaded_skin = want.clone();
                st.skin = crate::skin::load_skin(&want);
            }
            st.context = context.clone();
            st.status = status.clone();
            st.anchor_x = anchor_x;
            st.anchor_y = anchor_y;
            st.visible = context.composing || !context.menu.candidates.is_empty();
            changed.store(true, Ordering::Relaxed);
            Response::KeyResult {
                handled,
                commit,
                context,
                status,
            }
        }
        Request::SelectCandidate { index } => {
            let ok = sess.select_candidate(index);
            let context = sess.get_context().map(|c| context_msg(&c)).unwrap_or_default();
            let mut st = ui.lock().unwrap();
            st.context = context.clone();
            st.visible = context.composing;
            changed.store(true, Ordering::Relaxed);
            let _ = ok;
            Response::Ok { ok: true }
        }
        Request::SelectCandidateOnCurrentPage { index } => {
            let ok = sess.select_candidate_on_current_page(index);
            let _ = ok;
            Response::Ok { ok: true }
        }
        Request::ChangePage { backward } => {
            let ok = sess.change_page(backward);
            let context = sess.get_context().map(|c| context_msg(&c)).unwrap_or_default();
            let mut st = ui.lock().unwrap();
            st.context = context.clone();
            changed.store(true, Ordering::Relaxed);
            let _ = ok;
            Response::Ok { ok: true }
        }
        Request::SetOption { name, value } => {
            let ok = sess.set_option(&name, value);
            Response::Ok { ok }
        }
        Request::SelectSchema { id } => {
            let ok = sess.select_schema(&id);
            Response::Ok { ok }
        }
        Request::Deploy => {
            let ok = rime.deploy(false).unwrap_or(false);
            Response::DeployResult { ok }
        }
        Request::Shutdown => Response::Ok { ok: true },
    }
}
