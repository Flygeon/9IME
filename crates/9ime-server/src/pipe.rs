//! Named-pipe server: accepts many concurrent clients (one per app
//! process) and funnels every request to the single rime thread through a
//! channel — librime sessions must be driven from one thread only.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe,
    PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
};

use nineime_ipc::{self, Request, Response};
use nineime_librime::{Rime, RimeSession};

use crate::{context_msg, status_msg, UiState};

/// One unit of work for the rime thread.
pub struct Work {
    req: Request,
    reply: Sender<Response>,
}

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

/// Start the pipe listener on a background thread; the returned receiver is
/// drained by the rime thread via [run].
pub fn start_listener() -> Receiver<Work> {
    let (tx, rx) = channel::<Work>();
    std::thread::spawn(move || listener_loop(tx));
    rx
}

fn listener_loop(tx: Sender<Work>) {
    let pipe_name: Vec<u16> = nineime_ipc::PIPE_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    loop {
        let h = unsafe {
            CreateNamedPipeW(
                windows::core::PCWSTR(pipe_name.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                16,
                65536,
                65536,
                0,
                None,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            crate::slog::log(&format!(
                "CreateNamedPipeW failed: {}",
                unsafe { GetLastError() }.0
            ));
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
        let txx = tx.clone();
        let sh = SendHandle(h); // wrap before the spawn so only Send types are captured
        std::thread::spawn(move || client_loop(sh, txx));
    }
}

// The raw pipe handle carries no thread affinity by itself; only its value
// crosses into the spawned reader thread (never dereferenced there).
struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}

fn client_loop(h: SendHandle, tx: Sender<Work>) {
    let h = h.0;
    loop {
        let Some(req) = read_msg(h) else { break };
        let (rtx, rrx) = channel::<Response>();
        if tx.send(Work { req, reply: rtx }).is_err() {
            break;
        }
        let Ok(resp) = rrx.recv() else { break };
        let bytes = nineime_ipc::encode(&resp);
        if !write_all(h, &bytes) {
            break;
        }
    }
    let _ = unsafe { DisconnectNamedPipe(h) };
    let _ = unsafe { CloseHandle(h) };
    crate::slog::log("client disconnected");
}

/// Rime-thread work loop: owns the session and applies every request.
pub fn run(
    rime: &Rime,
    ui: Arc<Mutex<UiState>>,
    changed: Arc<AtomicBool>,
    deploy_done: Arc<AtomicBool>,
    rx: Receiver<Work>,
) {
    let mut sess: Option<RimeSession> = None;
    for work in rx {
        let is_shutdown = matches!(work.req, Request::Shutdown);
        let resp = handle(rime, &mut sess, work.req, &ui, &changed, &deploy_done);
        let _ = work.reply.send(resp);
        if is_shutdown {
            ui.lock().unwrap().quit = true;
            changed.store(true, Ordering::Relaxed);
            break;
        }
    }
    if let Some(mut s) = sess.take() {
        let _ = s.destroy();
    }
}

/// Create the session lazily — only after the startup deploy finished, so
/// librime is never asked for a session while in maintenance mode.
fn ensure_session(
    rime: &Rime,
    sess: &mut Option<RimeSession>,
    deploy_done: &Arc<AtomicBool>,
) -> bool {
    if sess.is_some() {
        return true;
    }
    if !deploy_done.load(Ordering::SeqCst) {
        return false;
    }
    match rime.create_session() {
        Ok(s) => {
            let schema = s.current_schema().unwrap_or_default();
            crate::slog::log(&format!("session {} created, schema {}", s.id, schema));
            *sess = Some(s);
            true
        }
        Err(e) => {
            crate::slog::log(&format!("create_session failed (will retry): {e}"));
            false
        }
    }
}

fn handle(
    rime: &Rime,
    sess: &mut Option<RimeSession>,
    req: Request,
    ui: &Arc<Mutex<UiState>>,
    changed: &Arc<AtomicBool>,
    deploy_done: &Arc<AtomicBool>,
) -> Response {
    match req {
        Request::Hello { pid } => {
            crate::slog::log(&format!("hello from pid {pid}"));
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
            // While the first-run deploy is still building dictionaries we
            // must not touch librime; pass keys through instead of blocking
            // the caller's UI thread.
            if !ensure_session(rime, sess, deploy_done) {
                return Response::KeyResult {
                    handled: false,
                    commit: None,
                    context: Default::default(),
                    status: Default::default(),
                };
            }
            let sess = sess.as_ref().unwrap();
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
            if let Some(sess) = sess.as_ref() {
                let ok = sess.select_candidate(index);
                let context = sess.get_context().map(|c| context_msg(&c)).unwrap_or_default();
                let mut st = ui.lock().unwrap();
                st.context = context.clone();
                st.visible = context.composing;
                changed.store(true, Ordering::Relaxed);
                let _ = ok;
            }
            Response::Ok { ok: true }
        }
        Request::SelectCandidateOnCurrentPage { index } => {
            if let Some(sess) = sess.as_ref() {
                let ok = sess.select_candidate_on_current_page(index);
                let _ = ok;
            }
            Response::Ok { ok: true }
        }
        Request::ChangePage { backward } => {
            if let Some(sess) = sess.as_ref() {
                let ok = sess.change_page(backward);
                let context = sess.get_context().map(|c| context_msg(&c)).unwrap_or_default();
                let mut st = ui.lock().unwrap();
                st.context = context.clone();
                changed.store(true, Ordering::Relaxed);
                let _ = ok;
            }
            Response::Ok { ok: true }
        }
        Request::SetOption { name, value } => {
            let ok = sess.as_ref().map(|s| s.set_option(&name, value)).unwrap_or(false);
            Response::Ok { ok }
        }
        Request::SelectSchema { id } => {
            let ok = sess.as_ref().map(|s| s.select_schema(&id)).unwrap_or(false);
            Response::Ok { ok }
        }
        Request::Deploy => {
            let ok = rime.deploy(false).unwrap_or(false);
            Response::DeployResult { ok }
        }
        Request::Shutdown => Response::Ok { ok: true },
    }
}
