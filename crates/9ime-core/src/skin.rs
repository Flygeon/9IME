//! Skin model: parses skin.ini from an extracted .ssf container.

use crate::ini::{self, Ini};
use crate::ssf::SkinFiles;
use encoding_rs::UTF_8;

#[derive(Debug, Clone, Default)]
pub struct Scheme {
    /// Background image bytes (png/bmp).
    pub pic: Option<Vec<u8>>,
    pub img_w: i32,
    pub img_h: i32,
    // 9-slice margins from layout_horizontal / layout_vertical.
    pub stretch_left: i32,
    pub stretch_right: i32,
    pub stretch_top: i32,
    pub stretch_bottom: i32,
    // text area insets from pinyin_marge / zhongwen_marge.
    pub preedit_left: i32,
    pub preedit_top: i32,
    pub preedit_right: i32,
    pub candidate_left: i32,
    pub candidate_right: i32,
    pub candidate_bottom: i32,
    /// Gap between preedit and candidates.
    pub gap: i32,
    pub separator_color: Option<u32>,
    pub preedit_highlight: Option<Vec<u8>>,
    pub candidate_highlight: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct Skin {
    pub name: String,
    pub font_size: i32,
    pub font_name: String,
    pub preedit_color: u32,
    pub candidate_color: u32,
    pub candidate_hl_color: u32,
    pub scheme: Scheme,
}

fn decode_ini_text(raw: &[u8]) -> Option<String> {
    // UTF-16LE BOM
    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE {
        let u16s: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16(&u16s).ok();
    }
    // UTF-8 BOM
    let body = if raw.len() >= 3 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF {
        &raw[3..]
    } else {
        raw
    };
    // strict UTF-8 first
    if let Some(text) =
        UTF_8.decode_without_bom_handling_and_without_replacement(body)
    {
        return Some(text.into_owned());
    }
    // else GBK (CP936)
    let (text, _, _) = encoding_rs::GBK.decode(body);
    Some(text.into_owned())
}

fn file(files: &SkinFiles, name: &str) -> Option<Vec<u8>> {
    files.get(name).cloned()
}

/// Byte-swap 0xRRGGBB into 0x00BBGGRR (Sogou stores colors in BGR order).
fn swap_bgr(v: u32) -> u32 {
    ((v & 0xFF) << 16) | (v & 0xFF00) | ((v >> 16) & 0xFF)
}

fn parse_scheme(ini: &Ini, section: &str, files: &SkinFiles) -> Scheme {
    let mut s = Scheme::default();
    if let Some(pic_name) = ini::get(ini, section, "pic") {
        s.pic = file(files, &pic_name.trim().to_lowercase());
    }
    let lh = ini::get_int_list(ini, section, "layout_horizontal");
    let lv = ini::get_int_list(ini, section, "layout_vertical");
    if lh.len() >= 3 && lv.len() >= 3 {
        s.stretch_left = lh[1];
        s.stretch_right = lh[2];
        s.stretch_top = lv[1];
        s.stretch_bottom = lv[2];
    }
    let pm = ini::get_int_list(ini, section, "pinyin_marge");
    let zm = ini::get_int_list(ini, section, "zhongwen_marge");
    if pm.len() >= 4 && zm.len() >= 4 {
        s.preedit_left = pm[2];
        s.preedit_top = pm[0];
        s.preedit_right = pm[3];
        s.candidate_left = zm[2];
        s.candidate_right = zm[3];
        s.candidate_bottom = zm[1];
        s.gap = pm[1] + zm[0];
    }
    let sep = ini::get_int_list(ini, section, "separator");
    if let Some(first) = sep.first() {
        s.separator_color = Some(swap_bgr(*first as u32));
        s.gap += 1;
    }
    if let Some(n) = ini::get(ini, section, "pinyin_pic") {
        s.preedit_highlight = file(files, &n.trim().to_lowercase());
    }
    if let Some(n) = ini::get(ini, section, "zhongwen_pic") {
        s.candidate_highlight = file(files, &n.trim().to_lowercase());
    }
    s
}

/// Parse a skin from extracted container files.
pub fn parse(files: &SkinFiles) -> Option<Skin> {
    let ini_raw = file(files, "skin.ini")?;
    let text = decode_ini_text(&ini_raw)?;
    let ini = ini::parse(&text);
    let mut skin = Skin::default();
    skin.name = ini::get(&ini, "General", "skin_name")
        .unwrap_or_else(|| "Sogou skin".to_string());
    skin.font_size = ini::get_int(&ini, "Display", "font_size", 12).clamp(4, 96);
    skin.font_name = ini::get(&ini, "Display", "font_ch")
        .filter(|s| !s.is_empty())
        .or_else(|| ini::get(&ini, "Display", "font_en"))
        .unwrap_or_else(|| "Microsoft YaHei UI".to_string());
    skin.preedit_color = ini::get_color(&ini, "Display", "pinyin_color", 0x004488);
    skin.candidate_color = ini::get_color(&ini, "Display", "zhongwen_color", 0x111111);
    skin.candidate_hl_color =
        ini::get_color(&ini, "Display", "zhongwen_first_color", 0x0044CC);
    // Sogou skins define several schemes; the default candidate bar is
    // horizontal, so Scheme_H1 (the main horizontal scheme) carries the
    // background image. Prefer the first scheme that has a pic, in display
    // priority order, and fall back to the first scheme with any layout
    // info (so margins/highlights survive even without a background).
    let mut fallback: Option<Scheme> = None;
    for sec in ["Scheme_H1", "Scheme_H2", "Scheme_V1", "Scheme_V2", "Scheme"] {
        let s = parse_scheme(&ini, sec, files);
        if s.pic.is_some() {
            skin.scheme = s;
            break;
        }
        if fallback.is_none() {
            fallback = Some(s);
        }
    }
    if skin.scheme.pic.is_none() {
        if let Some(fb) = fallback {
            skin.scheme = fb;
        }
    }
    Some(skin)
}
