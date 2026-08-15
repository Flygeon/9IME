//! Candidate window: topmost GDI popup. Skin rendering replaces this in M4.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint,
    FillRect, InvalidateRect, SelectObject, SetBkMode, SetTextColor, TextOutW,
    TRANSPARENT, FONT_CHARSET, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION,
    FONT_QUALITY, HDC, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect,
    GetMessageW, IsWindowVisible, LoadCursorW, MoveWindow,
    PostQuitMessage, RegisterClassW, SetWindowPos, ShowWindow, WNDCLASSW,
    CS_HREDRAW, CS_VREDRAW, HTCAPTION, HWND_TOPMOST, IDC_ARROW, MSG,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, SW_HIDE,
    WM_DESTROY, WM_ERASEBKGND, WM_NCHITTEST, WM_PAINT, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::UiState;

const CLASS_NAME: PCWSTR = PCWSTR::from_raw("NineImeCandWnd\0".as_ptr() as _);

// Colors stored as 0x00BBGGRR (COLORREF layout).
const BG: u32 = 0x00F5F5F5;
const SEL_BG: u32 = 0x00D8E8FF;
const TEXT: u32 = 0x00111111;
const PREEDIT: u32 = 0x00004488;

fn cref(v: u32) -> COLORREF {
    COLORREF(v)
}

/// Last rendered snapshot, written by the UI thread, read by WM_PAINT.
static SNAPSHOT: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static HIGHLIGHT: OnceLock<Mutex<i32>> = OnceLock::new();

pub struct CandidateWindow {
    handle: Option<JoinHandle<()>>,
}

impl CandidateWindow {
    pub fn spawn(ui: Arc<Mutex<UiState>>, changed: Arc<AtomicBool>) -> Self {
        let _ = SNAPSHOT.set(Mutex::new(Vec::new()));
        let _ = HIGHLIGHT.set(Mutex::new(-1));
        let handle = std::thread::spawn(move || ui_thread(ui, changed));
        CandidateWindow { handle: Some(handle) }
    }

    pub fn join(mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn ui_thread(ui: Arc<Mutex<UiState>>, changed: Arc<AtomicBool>) {
    unsafe {
        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);

        let hwnd = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            CLASS_NAME,
            PCWSTR::from_raw("9IME\0".as_ptr() as _),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(windows::Win32::Foundation::HINSTANCE(hinstance.0)),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("9IME window: CreateWindowExW failed: {e}");
                return;
            }
        };

        let mut msg = MSG::default();
        loop {
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                DispatchMessageW(&msg);
                if msg.message == WM_DESTROY {
                    return;
                }
            }
            if changed.swap(false, Ordering::Relaxed) {
                let (visible, ax, ay, ctx) = {
                    let s = ui.lock().unwrap();
                    (s.visible, s.anchor_x, s.anchor_y, s.context.clone())
                };
                if visible {
                    let dpi = GetDpiForWindow(hwnd);
                    let scale = dpi as f32 / 96.0;
                    let line_h = (22.0 * scale) as i32;
                    let pad = (6.0 * scale) as i32;
                    let n_cand = ctx.menu.candidates.len() as i32;
                    let width = (340.0 * scale) as i32;
                    let height = line_h * (1 + n_cand.max(0)) + pad * 2;
                    let _ = MoveWindow(hwnd, ax, ay + (4.0 * scale) as i32, width, height, true);
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                    // snapshot for WM_PAINT
                    let mut lines = Vec::new();
                    if ctx.composing {
                        lines.push(ctx.preedit.clone());
                    }
                    for (i, c) in ctx.menu.candidates.iter().enumerate() {
                        let key = ctx
                            .menu
                            .select_keys
                            .chars()
                            .nth(i)
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| (i + 1).to_string());
                        if c.comment.is_empty() {
                            lines.push(format!("{key}. {}", c.text));
                        } else {
                            lines.push(format!("{key}. {}  {}", c.text, c.comment));
                        }
                    }
                    if let (Some(s), Some(h)) = (SNAPSHOT.get(), HIGHLIGHT.get()) {
                        *s.lock().unwrap() = lines;
                        *h.lock().unwrap() = ctx.menu.highlighted;
                    }
                    let _ = InvalidateRect(Some(hwnd), None, true);
                } else if IsWindowVisible(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                paint(hwnd, hdc);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_NCHITTEST => LRESULT(HTCAPTION as isize),
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn paint(hwnd: HWND, hdc: HDC) {
    unsafe {
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let w = rect.right;
        let h = rect.bottom;

        let bg = CreateSolidBrush(cref(BG));
        let _ = FillRect(
            hdc,
            &RECT { left: 0, top: 0, right: w, bottom: h, ..Default::default() },
            bg,
        );
        let _ = DeleteObject(bg.into());

        let sel_bg = CreateSolidBrush(cref(SEL_BG));

        let dpi = GetDpiForWindow(hwnd);
        let scale = dpi as f32 / 96.0;
        let font = CreateFontW(
            (-16.0 * scale) as i32,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            FONT_CHARSET(134),
            FONT_OUTPUT_PRECISION(0),
            FONT_CLIP_PRECISION(0),
            FONT_QUALITY(0),
            0,
            PCWSTR::from_raw("Microsoft YaHei UI\0".as_ptr() as _),
        );
        let old_font = SelectObject(hdc, font.into());
        let _ = SetBkMode(hdc, TRANSPARENT);

        let lines = SNAPSHOT.get().map(|m| m.lock().unwrap().clone()).unwrap_or_default();
        let hl = HIGHLIGHT.get().map(|m| *m.lock().unwrap()).unwrap_or(-1);
        let pad = (6.0 * scale) as i32;
        let line_h = (22.0 * scale) as i32;
        let mut y = pad;
        for (i, line) in lines.iter().enumerate() {
            let idx = i as i32 - 1;
            if idx >= 0 && idx == hl {
                let _ = FillRect(
                    hdc,
                    &RECT { left: 0, top: y, right: w, bottom: y + line_h, ..Default::default() },
                    sel_bg,
                );
            }
            let color = if i == 0 && lines.len() > 1 { PREEDIT } else { TEXT };
            let _ = SetTextColor(hdc, cref(color));
            let wide: Vec<u16> = line.encode_utf16().collect();
            let _ = TextOutW(hdc, pad, y, &wide);
            y += line_h;
        }
        let _ = SelectObject(hdc, old_font);
        let _ = DeleteObject(font.into());
        let _ = DeleteObject(sel_bg.into());
    }
}
