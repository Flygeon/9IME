//! Named-pipe client: talks to nineime-server from the TSF DLL.

use std::path::{Path, PathBuf};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, RECT};
use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL,
};
use windows::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
};
use windows::Win32::System::Pipes::WaitNamedPipeW;
use windows::Win32::System::Threading::{
    CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW, CREATE_NO_WINDOW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ClientToScreen, GetForegroundWindow, GetGUIThreadInfo, GetWindowRect, GUITHREADINFO, POINT,
};

use nineime_ipc::{PIPE_NAME, Request, Response};

pub struct Client {
    handle: HANDLE,
}

// The handle is only ever used from the thread that created it; moving the
// Client between threads (e.g. inside the OnceLock mutex) is safe because the
// raw handle value carries no thread affinity by itself.
unsafe impl Send for Client {}

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

fn try_open() -> Option<HANDLE> {
    let name: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    // The server may be busy serving another app; wait briefly for a free
    // pipe instance instead of failing right away.
    let h = open_once(&name);
    if h.is_some() {
        return h;
    }
    let _ = unsafe { WaitNamedPipeW(PCWSTR(name.as_ptr()), 500) };
    open_once(&name)
}

fn open_once(name: &[u16]) -> Option<HANDLE> {
    let h = unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .ok()?;
    if h == INVALID_HANDLE_VALUE {
        None
    } else {
        Some(h)
    }
}

fn module_dir() -> Option<PathBuf> {
    unsafe {
        // Address of a function in this DLL -> its module handle.
        let mut hmod = windows::Win32::Foundation::HMODULE::default();
        let flag = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS;
        let addr = try_open as *const () as *const u16;
        if GetModuleHandleExW(flag, PCWSTR(addr), &mut hmod).is_err() {
            return None;
        }
        let mut buf = vec![0u16; 2048];
        let n = GetModuleFileNameW(Some(hmod), &mut buf);
        buf.truncate(n as usize);
        let p = String::from_utf16_lossy(&buf);
        Some(Path::new(&p).parent().map(|d| d.to_path_buf()).unwrap_or_default())
    }
}

fn launch_server() {
    let Some(dir) = module_dir() else { return };
    let exe = dir.join("nineime-server.exe");
    if !exe.exists() {
        return;
    }
    unsafe {
        let mut cmd: Vec<u16> = format!("\"{}\"", exe.display())
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut si = STARTUPINFOW::default();
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut pi = PROCESS_INFORMATION::default();
        let ok = CreateProcessW(
            None,
            Some(PWSTR(cmd.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NO_WINDOW,
            None,
            None,
            &si,
            &mut pi,
        );
        if ok.is_ok() {
            let _ = CloseHandle(pi.hThread);
            let _ = CloseHandle(pi.hProcess);
        }
    }
}

fn kill_stale_servers() {
    // A previous 9IME installation may have left a broken nineime-server.exe
    // holding the pipe; it would answer keys with passthrough forever.
    let _ = std::process::Command::new("taskkill")
        .args(["/f", "/im", "nineime-server.exe"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

impl Client {
    /// Connect to the server, launching it if needed.
    pub fn connect() -> Option<Client> {
        if let Some(h) = try_open() {
            return Some(Client { handle: h });
        }
        // launch the server (do not kill anything yet: a first-run server
        // may legitimately be deploying and has not created the pipe yet)
        launch_server();
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if let Some(h) = try_open() {
                return Some(Client { handle: h });
            }
        }
        // pipe still absent after 6s: likely a stale server holds it
        // (or the launch failed); clear and retry once
        kill_stale_servers();
        std::thread::sleep(std::time::Duration::from_millis(300));
        launch_server();
        for _ in 0..15 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if let Some(h) = try_open() {
                return Some(Client { handle: h });
            }
        }
        None
    }

    pub fn request(&self, req: &Request) -> Option<Response> {
        let bytes = nineime_ipc::encode(req);
        if !write_all(self.handle, &bytes) {
            return None;
        }
        let mut len_buf = [0u8; 4];
        if !read_exact(self.handle, &mut len_buf) {
            return None;
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len == 0 || len > 1 << 20 {
            return None;
        }
        let mut body = vec![0u8; len];
        if !read_exact(self.handle, &mut body) {
            return None;
        }
        nineime_ipc::decode::<Response>(&body).ok()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

/// Caret-based anchor (screen coords) for the candidate window.
pub fn current_anchor() -> (i32, i32) {
    unsafe {
        let mut gti = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(0, &mut gti).is_ok() {
            // rcCaret is in client coordinates of hwndCaret; convert to screen.
            if !gti.hwndCaret.0.is_null()
                && (gti.rcCaret.right - gti.rcCaret.left > 0
                    || gti.rcCaret.bottom - gti.rcCaret.top > 0)
            {
                let mut pt = POINT {
                    x: gti.rcCaret.left,
                    y: gti.rcCaret.bottom,
                };
                let _ = ClientToScreen(gti.hwndCaret, &mut pt);
                return (pt.x, pt.y);
            }
            if !gti.hwndFocus.0.is_null() {
                let mut r = RECT::default();
                if GetWindowRect(gti.hwndFocus, &mut r).is_ok() {
                    return ((r.left + r.right) / 2, r.bottom);
                }
            }
        }
        // Fallback to the foreground window so the candidate window still
        // shows up in applications that don't expose a TSF caret.
        let hwnd = GetForegroundWindow();
        if !hwnd.0.is_null() {
            let mut r = RECT::default();
            if GetWindowRect(hwnd, &mut r).is_ok() {
                return ((r.left + r.right) / 2, r.bottom);
            }
        }
    }
    (0, 0)
}
