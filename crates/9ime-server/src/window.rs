//! Candidate window: topmost layered popup rendered with per-pixel alpha.
//!
//! Skin PNGs (background / highlight) are decoded once and 9-sliced into a
//! premultiplied BGRA frame buffer by our own compositor, so rounded
//! corners and shadows keep their transparency. Text is drawn with GDI on
//! top; a small alpha-repair pass inside the text rectangles keeps the
//! glyphs opaque (GDI may zero the alpha byte of pixels it touches).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC,
    DeleteObject, EndPaint, GetTextExtentPoint32W, GetTextMetricsW, SelectObject,
    SetBkMode, SetTextColor, TextOutW, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, FONT_CHARSET, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION,
    FONT_QUALITY, HDC, PAINTSTRUCT, RGBQUAD, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    LoadCursorW, PeekMessageW, PostQuitMessage, RegisterClassW, ShowWindow,
    SystemParametersInfoW, UpdateLayeredWindow, WNDCLASSW, CS_HREDRAW, CS_VREDRAW,
    IDC_ARROW, MSG, PM_REMOVE, SPI_GETWORKAREA, SW_HIDE, SW_SHOWNOACTIVATE,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, ULW_ALPHA, WM_DESTROY, WM_NCHITTEST,
    WM_PAINT, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};

use crate::UiState;

const CLASS_NAME: PCWSTR = w!("NineImeCandWnd");

// Fallback palette (COLORREF 0x00BBGGRR) used when no skin is configured.
const FB_BG: u32 = 0x00FFFFFF;
const FB_BORDER: u32 = 0x00C8C8C8;
const FB_SEL_BG: u32 = 0x00E4D6B5;
const FB_TEXT: u32 = 0x00202020;
const FB_PREEDIT: u32 = 0x008B5A00;
const FB_COMMENT: u32 = 0x00808080;

fn cref(v: u32) -> COLORREF {
    COLORREF(v)
}

/// Decoded skin image (premultiplied BGRA pixels, UI thread only).
struct Img {
    w: i32,
    h: i32,
    px: Vec<u8>,
}

struct SkinGfx {
    bg: Option<Img>,
    hl: Option<Img>,
    sl: i32,
    sr: i32,
    st: i32,
    sb: i32,
    preedit_left: i32,
    preedit_top: i32,
    preedit_right: i32,
    candidate_left: i32,
    candidate_right: i32,
    candidate_bottom: i32,
    gap: i32,
    separator: Option<u32>,
    preedit_color: u32,
    cand_color: u32,
    hl_color: u32,
    font_name: String,
    font_size: i32,
}

struct Frame {
    lines: Vec<Line>,
    hl: i32,
    page_no: i32,
    page_size: i32,
    is_last_page: bool,
}

struct Line {
    text: String,
    kind: LineKind,
}

#[derive(Clone, Copy, PartialEq)]
enum LineKind {
    Preedit,
    Candidate(i32),
}

struct UiCache {
    shown: bool,
}

// GDI handles/pointers are created and used on the UI thread only; values
// cross threads solely inside the OnceLock mutex (never dereferenced there).
unsafe impl Send for SkinGfx {}
unsafe impl Send for UiCache {}

static CACHE: OnceLock<Mutex<UiCache>> = OnceLock::new();

pub struct CandidateWindow {
    handle: Option<JoinHandle<()>>,
}

impl CandidateWindow {
    pub fn spawn(ui: Arc<Mutex<UiState>>, changed: Arc<AtomicBool>) -> Self {
        let _ = CACHE.set(Mutex::new(UiCache { shown: false }));
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

/// RGBA -> premultiplied BGRA pixel buffer.
fn to_img(w: i32, h: i32, rgba: &[u8]) -> Option<Img> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let mut px = Vec::with_capacity(rgba.len());
    for p in rgba.chunks_exact(4) {
        let a = p[3] as u32;
        px.push(((p[2] as u32 * a + 127) / 255) as u8);
        px.push(((p[1] as u32 * a + 127) / 255) as u8);
        px.push(((p[0] as u32 * a + 127) / 255) as u8);
        px.push(p[3]);
    }
    Some(Img { w, h, px })
}

fn load_img(png: &Option<Vec<u8>>) -> Option<Img> {
    let bytes = png.as_ref()?;
    let (w, h, rgba) = decode_png(bytes)?;
    to_img(w, h, &rgba)
}

fn build_skin_gfx(sk: &nineime_core::skin::Skin) -> SkinGfx {
    let sc = &sk.scheme;
    SkinGfx {
        bg: load_img(&sc.pic),
        hl: load_img(&sc.candidate_highlight),
        sl: sc.stretch_left,
        sr: sc.stretch_right,
        st: sc.stretch_top,
        sb: sc.stretch_bottom,
        preedit_left: sc.preedit_left,
        preedit_top: sc.preedit_top,
        preedit_right: sc.preedit_right,
        candidate_left: sc.candidate_left,
        candidate_right: sc.candidate_right,
        candidate_bottom: sc.candidate_bottom,
        gap: sc.gap,
        separator: sc.separator_color,
        preedit_color: sk.preedit_color,
        cand_color: sk.candidate_color,
        hl_color: sk.candidate_hl_color,
        font_name: sk.font_name.clone(),
        font_size: sk.font_size,
    }
}

// ---------------------------------------------------------------------------
// Frame compositor (premultiplied BGRA)
// ---------------------------------------------------------------------------

#[inline]
fn blend_px(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, b: u8, g: u8, r: u8, a: u8) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return;
    }
    let i = (y as usize * w + x as usize) * 4;
    let sa = a as u32;
    let inv = 255 - sa;
    // source is premultiplied: out = src + dst * (1 - sa)
    buf[i] = (b as u32 + buf[i] as u32 * inv / 255) as u8;
    buf[i + 1] = (g as u32 + buf[i + 1] as u32 * inv / 255) as u8;
    buf[i + 2] = (r as u32 + buf[i + 2] as u32 * inv / 255) as u8;
    buf[i + 3] = (sa + buf[i + 3] as u32 * inv / 255) as u8;
}

fn fill_rect(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, rw: i32, rh: i32, color: u32, a: u8) {
    let (b, g, r) = (
        (color & 0xFF) as u8,
        ((color >> 8) & 0xFF) as u8,
        ((color >> 16) & 0xFF) as u8,
    );
    let (b, g, r) = (
        ((b as u32 * a as u32 + 127) / 255) as u8,
        ((g as u32 * a as u32 + 127) / 255) as u8,
        ((r as u32 * a as u32 + 127) / 255) as u8,
    );
    for yy in y..y + rh {
        for xx in x..x + rw {
            blend_px(buf, w, h, xx, yy, b, g, r, a);
        }
    }
}

/// 9-slice blit of src (premultiplied) into dst at (dx,dy,dw,dh).
#[allow(clippy::too_many_arguments)]
fn blit_nine(
    dst: &mut [u8],
    dw_px: usize,
    dh_px: usize,
    src: &Img,
    sl: i32,
    sr: i32,
    st: i32,
    sb: i32,
    dx: i32,
    dy: i32,
    dw: i32,
    dh: i32,
) {
    if dw <= 0 || dh <= 0 {
        return;
    }
    let (sw, sh) = (src.w, src.h);
    let mut ml = sl.min(sw / 2).max(0);
    let mut mr = sr.min(sw / 2).max(0);
    let mut mt = st.min(sh / 2).max(0);
    let mut mb = sb.min(sh / 2).max(0);
    if ml + mr > dw {
        let f = dw as f32 / (ml + mr).max(1) as f32;
        ml = (ml as f32 * f) as i32;
        mr = dw - ml;
    }
    if mt + mb > dh {
        let f = dh as f32 / (mt + mb).max(1) as f32;
        mt = (mt as f32 * f) as i32;
        mb = dh - mt;
    }
    let map_x = |dxo: i32| -> i32 {
        if dxo < ml {
            dxo.min(sw - 1)
        } else if dxo >= dw - mr {
            (sw - (dw - dxo)).clamp(sw - mr, sw - 1)
        } else {
            let span_d = (dw - ml - mr).max(1);
            let span_s = (sw - ml - mr).max(1);
            ml + ((dxo - ml) * span_s / span_d).min(span_s - 1)
        }
    };
    let map_y = |dyo: i32| -> i32 {
        if dyo < mt {
            dyo.min(sh - 1)
        } else if dyo >= dh - mb {
            (sh - (dh - dyo)).clamp(sh - mb, sh - 1)
        } else {
            let span_d = (dh - mt - mb).max(1);
            let span_s = (sh - mt - mb).max(1);
            mt + ((dyo - mt) * span_s / span_d).min(span_s - 1)
        }
    };
    for y in 0..dh {
        let sy = map_y(y);
        let dyy = dy + y;
        if dyy < 0 || dyy >= dh_px as i32 {
            continue;
        }
        for x in 0..dw {
            let dxx = dx + x;
            if dxx < 0 || dxx >= dw_px as i32 {
                continue;
            }
            let sx = map_x(x);
            let si = (sy as usize * sw as usize + sx as usize) * 4;
            let a = src.px[si + 3];
            if a == 0 {
                continue;
            }
            blend_px(dst, dw_px, dh_px, dxx, dyy, src.px[si], src.px[si + 1], src.px[si + 2], a);
        }
    }
}

// ---------------------------------------------------------------------------
// Text measurement / font
// ---------------------------------------------------------------------------

struct Measure {
    line_h: i32,
    widths: Vec<i32>,
}

fn make_font(gfx: Option<&SkinGfx>, scale: f32) -> windows::Win32::Graphics::Gdi::HFONT {
    let size_pt = gfx.map(|g| g.font_size).unwrap_or(12).clamp(8, 48);
    let name = gfx
        .map(|g| g.font_name.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Microsoft YaHei UI".to_string());
    let height = -((size_pt as f32) * 96.0 / 72.0 * scale).round().max(8.0) as i32;
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            FONT_CHARSET(134), // GB2312_CHARSET: keep CJK glyphs for any font
            FONT_OUTPUT_PRECISION(0),
            FONT_CLIP_PRECISION(0),
            FONT_QUALITY(4), // ANTIALIASED_QUALITY
            0,
            PCWSTR(wide.as_ptr()),
        )
    }
}

fn measure(hdc: HDC, frame: &Frame, scale: f32) -> Measure {
    unsafe {
        let mut tm = TEXTMETRICW::default();
        let _ = GetTextMetricsW(hdc, &mut tm);
        let leading = tm.tmExternalLeading.max(1);
        let line_h = (tm.tmHeight + leading + 4).max((18.0 * scale) as i32);
        let mut widths = Vec::with_capacity(frame.lines.len());
        for line in &frame.lines {
            let wide: Vec<u16> = line.text.encode_utf16().collect();
            let mut sz = SIZE::default();
            let _ = GetTextExtentPoint32W(hdc, &wide, &mut sz);
            widths.push(sz.cx);
        }
        Measure { line_h, widths }
    }
}

// ---------------------------------------------------------------------------
// UI thread
// ---------------------------------------------------------------------------

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
        if RegisterClassW(&wc) == 0 {
            let err = windows::Win32::Foundation::GetLastError().0;
            // 1410 = ERROR_CLASS_ALREADY_EXISTS is fine on re-registration
            if err != 1410 {
                crate::slog::log(&format!("RegisterClassW failed: {err}"));
            }
        }

        let hwnd = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            CLASS_NAME,
            w!("9IME"),
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
                crate::slog::log(&format!("CreateWindowExW failed: {e}"));
                return;
            }
        };

        let mut last_skin = String::new();
        let mut gfx: Option<SkinGfx> = None;
        let mut msg = MSG::default();
        loop {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&msg);
                if msg.message == WM_DESTROY {
                    return;
                }
            }
            if ui.lock().map(|s| s.quit).unwrap_or(false) {
                let _ = DestroyWindow(hwnd);
                return;
            }
            if changed.swap(false, Ordering::Relaxed) {
                let (visible, ax, ay, frame, skin_name, skin) = {
                    let s = ui.lock().unwrap();
                    (
                        s.visible,
                        s.anchor_x,
                        s.anchor_y,
                        build_frame(&s.context),
                        s.loaded_skin.clone(),
                        s.skin.clone(),
                    )
                };
                if last_skin != skin_name {
                    last_skin = skin_name;
                    gfx = skin.as_ref().map(build_skin_gfx);
                }
                present(hwnd, visible, ax, ay, &frame, gfx.as_ref());
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    }
}

fn build_frame(ctx: &nineime_ipc::ContextMsg) -> Frame {
    let mut lines = Vec::new();
    if ctx.composing && !ctx.preedit.is_empty() {
        lines.push(Line {
            text: ctx.preedit.clone(),
            kind: LineKind::Preedit,
        });
    }
    for (i, c) in ctx.menu.candidates.iter().enumerate() {
        let key = ctx
            .menu
            .select_keys
            .chars()
            .nth(i)
            .map(|c| c.to_string())
            .unwrap_or_else(|| (i + 1).to_string());
        let text = if c.comment.is_empty() {
            format!("{key}. {}", c.text)
        } else {
            format!("{key}. {}  {}", c.text, c.comment)
        };
        lines.push(Line {
            text,
            kind: LineKind::Candidate(i as i32),
        });
    }
    Frame {
        lines,
        hl: ctx.menu.highlighted,
        page_no: ctx.menu.page_no,
        page_size: ctx.menu.page_size,
        is_last_page: ctx.menu.is_last_page,
    }
}

fn present(hwnd: HWND, visible: bool, ax: i32, ay: i32, frame: &Frame, gfx: Option<&SkinGfx>) {
    unsafe {
        let Some(cache) = CACHE.get() else { return };
        let mut cache = cache.lock().unwrap();
        if !visible || frame.lines.is_empty() {
            if cache.shown {
                let _ = ShowWindow(hwnd, SW_HIDE);
                cache.shown = false;
            }
            return;
        }

        let dpi = GetDpiForWindow(hwnd);
        let scale = dpi as f32 / 96.0;
        let sc = |v: i32| ((v as f32) * scale) as i32;

        // insets (skin or fallback)
        let (pe_l, pe_t, pe_r, ca_l, ca_r, ca_b, gap) = if let Some(g) = gfx {
            (
                sc(g.preedit_left).max(sc(4)),
                sc(g.preedit_top).max(sc(3)),
                sc(g.preedit_right).max(sc(4)),
                sc(g.candidate_left).max(sc(4)),
                sc(g.candidate_right).max(sc(4)),
                sc(g.candidate_bottom).max(sc(3)),
                sc(g.gap).max(sc(1)),
            )
        } else {
            (sc(8), sc(6), sc(8), sc(8), sc(8), sc(6), sc(2))
        };

        // measure text
        let hdc_screen = CreateCompatibleDC(None);
        let font = make_font(gfx, scale);
        let old_font = SelectObject(hdc_screen, font.into());
        let m = measure(hdc_screen, frame, scale);
        let line_h = m.line_h;

        // layout
        let has_preedit = frame.lines.first().map(|l| l.kind) == Some(LineKind::Preedit);
        let mut content_w = 0i32;
        for (i, line) in frame.lines.iter().enumerate() {
            let w = m.widths[i]
                + if line.kind == LineKind::Preedit { pe_l + pe_r } else { ca_l + ca_r };
            content_w = content_w.max(w);
        }
        content_w = content_w.max(sc(60));
        let n_cand = frame.lines.len() as i32 - if has_preedit { 1 } else { 0 };
        let win_w = content_w;
        let win_h = if has_preedit {
            pe_t + line_h + gap + line_h * n_cand + ca_b
        } else {
            sc(3) + line_h * n_cand + ca_b
        };
        let (w, h) = (win_w as usize, win_h as usize);

        // compose the background / highlight pixels
        let mut buf = vec![0u8; w * h * 4];
        if let Some(g) = gfx {
            if let Some(bg) = &g.bg {
                blit_nine(&mut buf, w, h, bg, g.sl, g.sr, g.st, g.sb, 0, 0, win_w, win_h);
            } else {
                fill_rect(&mut buf, w, h, 0, 0, win_w, win_h, FB_BG, 255);
            }
        } else {
            fill_rect(&mut buf, w, h, 0, 0, win_w, win_h, FB_BORDER, 255);
            fill_rect(&mut buf, w, h, 1, 1, win_w - 2, win_h - 2, FB_BG, 255);
        }

        // collect text jobs: (x, y, text, color); highlight rectangles too
        let mut text_jobs: Vec<(i32, i32, String, u32)> = Vec::new();
        let mut y = if has_preedit { pe_t } else { sc(3) };
        for line in frame.lines.iter() {
            match line.kind {
                LineKind::Preedit => {
                    text_jobs.push((
                        pe_l,
                        y,
                        line.text.clone(),
                        gfx.map(|g| g.preedit_color).unwrap_or(FB_PREEDIT),
                    ));
                    if frame.page_size > 0 && (frame.page_no > 0 || !frame.is_last_page) {
                        let ind = format!("< {} >", frame.page_no + 1);
                        let wide: Vec<u16> = ind.encode_utf16().collect();
                        let mut sz = SIZE::default();
                        let _ = GetTextExtentPoint32W(hdc_screen, &wide, &mut sz);
                        text_jobs.push((win_w - pe_r - sz.cx, y, ind, FB_COMMENT));
                    }
                    y += line_h;
                    if n_cand > 0 {
                        if let Some(sep) = gfx.and_then(|g| g.separator) {
                            let sy = y + gap / 2;
                            fill_rect(&mut buf, w, h, ca_l, sy, win_w - ca_l - ca_r, 1, sep, 255);
                        }
                        y += gap;
                    }
                }
                LineKind::Candidate(idx) => {
                    if idx == frame.hl {
                        if let Some(g) = gfx {
                            if let Some(hl) = &g.hl {
                                let mg = (hl.w / 3).min(hl.h / 3).max(1);
                                blit_nine(
                                    &mut buf, w, h, hl, mg, mg, mg, mg,
                                    ca_l / 2, y, win_w - ca_l / 2 - ca_r / 2, line_h,
                                );
                            } else {
                                fill_rect(&mut buf, w, h, ca_l / 2, y, win_w - ca_l / 2 - ca_r / 2, line_h, FB_SEL_BG, 255);
                            }
                        } else {
                            fill_rect(&mut buf, w, h, 1, y, win_w - 2, line_h, FB_SEL_BG, 255);
                        }
                    }
                    let color = if idx == frame.hl {
                        gfx.map(|g| g.hl_color).unwrap_or(FB_TEXT)
                    } else {
                        gfx.map(|g| g.cand_color).unwrap_or(FB_TEXT)
                    };
                    text_jobs.push((ca_l, y, line.text.clone(), color));
                    y += line_h;
                }
            }
        }

        // upload pixels to a DIB and draw text with GDI
        let header = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: win_w,
            biHeight: -win_h, // top-down
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
        let hbmp = match CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) if !bits.is_null() => b,
            _ => {
                let _ = SelectObject(hdc_screen, old_font);
                let _ = DeleteObject(font.into());
                let _ = DeleteDC(hdc_screen);
                return;
            }
        };
        std::ptr::copy_nonoverlapping(buf.as_ptr(), bits as *mut u8, buf.len());
        drop(buf);

        let hdc_mem = CreateCompatibleDC(None);
        let old_bmp = SelectObject(hdc_mem, hbmp.into());
        SelectObject(hdc_mem, font.into());
        let _ = SetBkMode(hdc_mem, TRANSPARENT);

        let mut text_rects: Vec<(i32, i32, i32, i32)> = Vec::new();
        for (x, ty, text, color) in text_jobs.iter() {
            let _ = SetTextColor(hdc_mem, cref(*color));
            let wide: Vec<u16> = text.encode_utf16().collect();
            let _ = TextOutW(hdc_mem, *x, *ty + 2, &wide);
            let mut sz = SIZE::default();
            let _ = GetTextExtentPoint32W(hdc_screen, &wide, &mut sz);
            text_rects.push((*x - 2, *ty, *x + sz.cx + 4, *ty + line_h));
        }

        // Repair the alpha GDI clobbered inside the text rectangles: GDI
        // TextOut writes RGB but leaves the 4th byte zero on 32bpp DIBs, so
        // glyph pixels (including pure-black ones) come back with alpha=0 and
        // vanish under per-pixel alpha blending. The content area is opaque by
        // design, so any pixel GDI touched there must become opaque again.
        let stride = w * 4;
        let frame_px = std::slice::from_raw_parts_mut(bits as *mut u8, h * stride);
        for &(rx, ry, rx1, ry1) in &text_rects {
            let x0 = rx.clamp(0, win_w);
            let x1 = rx1.clamp(0, win_w);
            let y0 = ry.clamp(0, win_h);
            let y1 = ry1.clamp(0, win_h);
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let i = (yy as usize * w + xx as usize) * 4;
                    if frame_px[i + 3] == 0 {
                        frame_px[i + 3] = 255;
                    }
                }
            }
        }

        // position: below the caret, clamped into the work area
        let mut wa = RECT::default();
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut wa as *mut RECT as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let mut px = ax;
        let mut py = ay + sc(4);
        if px + win_w > wa.right {
            px = (wa.right - win_w).max(wa.left);
        }
        if px < wa.left {
            px = wa.left;
        }
        if py + win_h > wa.bottom {
            py = (ay - win_h - sc(4)).max(wa.top);
        }

        let pt_dst = POINT { x: px, y: py };
        let sz = SIZE { cx: win_w, cy: win_h };
        let pt_src = POINT { x: 0, y: 0 };
        let blend = windows::Win32::Graphics::Gdi::BLENDFUNCTION {
            BlendOp: 0,  // AC_SRC_OVER
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: 1, // AC_SRC_ALPHA
        };
        if !cache.shown {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            cache.shown = true;
        }
        let _ = UpdateLayeredWindow(
            hwnd,
            None,
            Some(&pt_dst),
            Some(&sz),
            Some(hdc_mem),
            Some(&pt_src),
            cref(0),
            Some(&blend),
            ULW_ALPHA,
        );

        let _ = SelectObject(hdc_mem, old_bmp);
        let _ = DeleteDC(hdc_mem);
        let _ = DeleteObject(hbmp.into());
        let _ = SelectObject(hdc_screen, old_font);
        let _ = DeleteObject(font.into());
        let _ = DeleteDC(hdc_screen);
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
                // layered windows manage their own pixels; just validate
                let mut ps = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut ps);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            // click-through: candidates must never steal clicks
            WM_NCHITTEST => LRESULT(-1), // HTTRANSPARENT
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

