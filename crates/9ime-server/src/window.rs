//! Candidate window: topmost GDI popup, skin-aware (M4).
//!
//! The skin background and highlight images are decoded to HBITMAPs and
//! drawn 9-sliced; text uses the skin colors/font. Falls back to a flat
//! style when no skin is configured.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateDIBSection, CreateFontW,
    CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, FillRect,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    StretchBlt, TextOutW, TRANSPARENT, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, FONT_CHARSET, FONT_CLIP_PRECISION,
    FONT_OUTPUT_PRECISION, FONT_QUALITY, HDC, HBITMAP, PAINTSTRUCT, SRCCOPY,
    RGBQUAD,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect,
    GetMessageW, IsWindowVisible, LoadCursorW, MoveWindow, PostQuitMessage,
    RegisterClassW, SetWindowPos, ShowWindow, WNDCLASSW, CS_HREDRAW,
    CS_VREDRAW, HTCAPTION, HWND_TOPMOST, IDC_ARROW, MSG, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, SW_HIDE, WM_DESTROY,
    WM_ERASEBKGND, WM_NCHITTEST, WM_PAINT, WS_EX_NOACTIVATE,
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

/// Decoded skin graphics (GDI handles, UI thread only).
struct SkinGfx {
    bg: Option<HBITMAP>,
    bg_w: i32,
    bg_h: i32,
    hl: Option<HBITMAP>,
    hl_w: i32,
    hl_h: i32,
    sl: i32,
    sr: i32,
    st: i32,
    sb: i32,
    preedit_color: u32,
    cand_color: u32,
    hl_color: u32,
    font_name: String,
    font_size: i32,
}

struct PaintState {
    lines: Vec<String>,
    hl: i32,
    gfx: Option<SkinGfx>,
}

// GDI handles are created and used on the UI thread only; the value only
// crosses threads inside the OnceLock mutex (never used there).
unsafe impl Send for PaintState {}
unsafe impl Send for SkinGfx {}

static PAINT_STATE: OnceLock<Mutex<PaintState>> = OnceLock::new();

pub struct CandidateWindow {
    handle: Option<JoinHandle<()>>,
}

impl CandidateWindow {
    pub fn spawn(ui: Arc<Mutex<UiState>>, changed: Arc<AtomicBool>) -> Self {
        let _ = PAINT_STATE.set(Mutex::new(PaintState {
            lines: Vec::new(),
            hl: -1,
            gfx: None,
        }));
        let handle = std::thread::spawn(move || ui_thread(ui, changed));
        CandidateWindow { handle: Some(handle) }
    }

    pub fn join(mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Decode PNG bytes to RGBA (expanded, 8-bit).
fn decode_png(data: &[u8]) -> Option<(i32, i32, Vec<u8>)> {
    let mut dec = png::Decoder::new(data);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = dec.read_info().ok()?;
    let mut out = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut out).ok()?;
    out.truncate(info.buffer_size());
    Some((info.width as i32, info.height as i32, out))
}

/// RGBA -> 32bpp bottom-up DIB section HBITMAP.
fn rgba_to_hbitmap(w: i32, h: i32, rgba: &[u8]) -> Option<HBITMAP> {
    if w <= 0 || h <= 0 {
        return None;
    }
    // premultiplied not needed for SRCCOPY; keep straight BGRA
    let mut bgra = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let bmi = BITMAPINFO {
        bmiHeader: header,
        bmiColors: [RGBQUAD::default()],
    };
    let mut bits = std::ptr::null_mut();
    unsafe {
        let hbmp = CreateDIBSection(
            None,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )
        .ok()?;
        if bits.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
        Some(hbmp)
    }
}

fn blt(
    hmem: HDC,
    hdc: HDC,
    sx: i32,
    sy: i32,
    sw: i32,
    sh: i32,
    dx: i32,
    dy: i32,
    dw: i32,
    dh: i32,
) {
    if dw > 0 && dh > 0 && sw > 0 && sh > 0 {
        let _ = unsafe { StretchBlt(hdc, dx, dy, dw, dh, Some(hmem), sx, sy, sw, sh, SRCCOPY) };
    }
}

/// 9-slice draw of a bitmap into (dx, dy, dw, dh).
unsafe fn draw_nine(
    hmem: HDC,
    hdc: HDC,
    bmp: HBITMAP,
    iw: i32,
    ih: i32,
    sl: i32,
    sr: i32,
    st: i32,
    sb: i32,
    dx: i32,
    dy: i32,
    dw: i32,
    dh: i32,
) {
    unsafe {
        let _ = SelectObject(hmem, bmp.into());
        let cw = (iw - sl - sr).max(1);
        let ch = (ih - st - sb).max(1);
        let mw = (dw - sl - sr).max(0);
        let mh = (dh - st - sb).max(0);
        let ml = sl.min(dw);
        let mr = sr.min(dw);
        let mt = st.min(dh);
        let mb = sb.min(dh);
        blt(hmem, hdc, 0, 0, sl, st, dx, dy, ml, mt);
        blt(hmem, hdc, iw - sr, 0, sr, st, dx + dw - mr, dy, mr, mt);
        blt(hmem, hdc, 0, ih - sb, sl, sb, dx, dy + dh - mb, ml, mb);
        blt(hmem, hdc, iw - sr, ih - sb, sr, sb, dx + dw - mr, dy + dh - mb, mr, mb);
        blt(hmem, hdc, sl, 0, cw, st, dx + ml, dy, mw, mt);
        blt(hmem, hdc, sl, ih - sb, cw, sb, dx + ml, dy + dh - mb, mw, mb);
        blt(hmem, hdc, 0, st, sl, ch, dx, dy + mt, ml, mh);
        blt(hmem, hdc, iw - sr, st, sr, ch, dx + dw - mr, dy + mt, mr, mh);
        blt(hmem, hdc, sl, st, cw, ch, dx + ml, dy + mt, mw, mh);
    }
}

fn build_skin_gfx(sk: &nineime_core::skin::Skin) -> Option<SkinGfx> {
    let scheme = &sk.scheme;
    let mut gfx = SkinGfx {
        bg: None,
        bg_w: scheme.img_w,
        bg_h: scheme.img_h,
        hl: None,
        hl_w: 0,
        hl_h: 0,
        sl: scheme.stretch_left,
        sr: scheme.stretch_right,
        st: scheme.stretch_top,
        sb: scheme.stretch_bottom,
        preedit_color: sk.preedit_color,
        cand_color: sk.candidate_color,
        hl_color: sk.candidate_hl_color,
        font_name: sk.font_name.clone(),
        font_size: sk.font_size,
    };
    if let Some(pic) = &scheme.pic {
        if let Some((w, h, rgba)) = decode_png(pic) {
            gfx.bg = rgba_to_hbitmap(w, h, &rgba);
            gfx.bg_w = w;
            gfx.bg_h = h;
        }
    }
    if let Some(hl) = &scheme.candidate_highlight {
        if let Some((w, h, rgba)) = decode_png(hl) {
            gfx.hl = rgba_to_hbitmap(w, h, &rgba);
            gfx.hl_w = w;
            gfx.hl_h = h;
        }
    }
    Some(gfx)
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

        let hmem = CreateCompatibleDC(None);
        let mut last_skin = String::new();
        let mut msg = MSG::default();
        loop {
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                DispatchMessageW(&msg);
                if msg.message == WM_DESTROY {
                    let _ = DeleteDC(hmem);
                    return;
                }
            }
            if changed.swap(false, Ordering::Relaxed) {
                let (visible, ax, ay, ctx, skin_name, skin) = {
                    let s = ui.lock().unwrap();
                    (
                        s.visible,
                        s.anchor_x,
                        s.anchor_y,
                        s.context.clone(),
                        s.loaded_skin.clone(),
                        s.skin.clone(),
                    )
                };
                if last_skin != skin_name {
                    last_skin = skin_name;
                    let new_gfx = skin.as_ref().and_then(build_skin_gfx);
                    if let Some(ps) = PAINT_STATE.get() {
                        let mut ps = ps.lock().unwrap();
                        ps.gfx = new_gfx;
                    }
                }
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
                    if let Some(ps) = PAINT_STATE.get() {
                        let mut ps = ps.lock().unwrap();
                        ps.lines = lines;
                        ps.hl = ctx.menu.highlighted;
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

        let ps = PAINT_STATE.get().map(|m| m.lock().unwrap());
        let Some(ps) = ps else { return };
        let gfx = ps.gfx.as_ref();

        let hmem = CreateCompatibleDC(Some(hdc));
        let dpi = GetDpiForWindow(hwnd);
        let scale = dpi as f32 / 96.0;

        if let Some(g) = gfx {
            if let Some(bg) = g.bg {
                draw_nine(
                    hmem,
                    hdc,
                    bg,
                    g.bg_w,
                    g.bg_h,
                    g.sl,
                    g.sr,
                    g.st,
                    g.sb,
                    0,
                    0,
                    w,
                    h,
                );
            } else {
                let bg = CreateSolidBrush(cref(BG));
                let _ = FillRect(
                    hdc,
                    &RECT { left: 0, top: 0, right: w, bottom: h, ..Default::default() },
                    bg,
                );
                let _ = DeleteObject(bg.into());
            }
        } else {
            let bg = CreateSolidBrush(cref(BG));
            let _ = FillRect(
                hdc,
                &RECT { left: 0, top: 0, right: w, bottom: h, ..Default::default() },
                bg,
            );
            let _ = DeleteObject(bg.into());
        }

        let font_size = gfx.map(|g| g.font_size).unwrap_or(12);
        let font_name = gfx.map(|g| g.font_name.clone()).unwrap_or_else(|| "Microsoft YaHei UI".to_string());
        let font_w: Vec<u16> = font_name.encode_utf16().chain(std::iter::once(0)).collect();
        let font = CreateFontW(
            -((font_size as f32) * 1.33 * scale).round() as i32,
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
            PCWSTR(font_w.as_ptr()),
        );
        let old_font = SelectObject(hdc, font.into());
        let _ = SetBkMode(hdc, TRANSPARENT);

        let lines = &ps.lines;
        let hl = ps.hl;
        let pad = (6.0 * scale) as i32;
        let line_h = (22.0 * scale) as i32;
        let ins_l = gfx.map(|g| g.sl + (8.0 * scale) as i32).unwrap_or(pad);
        let mut y = pad;
        for (i, line) in lines.iter().enumerate() {
            let idx = i as i32 - 1;
            if idx >= 0 && idx == hl {
                if let Some(g) = gfx {
                    if let Some(hlbmp) = g.hl {
                        draw_nine(
                            hmem,
                            hdc,
                            hlbmp,
                            g.hl_w,
                            g.hl_h,
                            0,
                            0,
                            0,
                            0,
                            ins_l,
                            y,
                            w - ins_l * 2,
                            line_h,
                        );
                    } else {
                        let sel_bg = CreateSolidBrush(cref(SEL_BG));
                        let _ = FillRect(
                            hdc,
                            &RECT { left: ins_l, top: y, right: w - ins_l, bottom: y + line_h, ..Default::default() },
                            sel_bg,
                        );
                        let _ = DeleteObject(sel_bg.into());
                    }
                } else {
                    let sel_bg = CreateSolidBrush(cref(SEL_BG));
                    let _ = FillRect(
                        hdc,
                        &RECT { left: 0, top: y, right: w, bottom: y + line_h, ..Default::default() },
                        sel_bg,
                    );
                    let _ = DeleteObject(sel_bg.into());
                }
            }
            let color = if i == 0 && lines.len() > 1 {
                gfx.map(|g| g.preedit_color).unwrap_or(PREEDIT)
            } else if idx == hl {
                gfx.map(|g| g.hl_color).unwrap_or(TEXT)
            } else {
                gfx.map(|g| g.cand_color).unwrap_or(TEXT)
            };
            let _ = SetTextColor(hdc, cref(color));
            let wide: Vec<u16> = line.encode_utf16().collect();
            let _ = TextOutW(hdc, ins_l, y + ((line_h - (font_size as f32 * 1.33 * scale).round() as i32) / 2), &wide);
            y += line_h;
        }
        let _ = SelectObject(hdc, old_font);
        let _ = DeleteObject(font.into());
        let _ = DeleteDC(hmem);
    }
}
