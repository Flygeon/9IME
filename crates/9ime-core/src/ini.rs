//! Minimal INI parser for skin.ini (case-insensitive section/key).

use std::collections::HashMap;

pub type Ini = HashMap<String, HashMap<String, String>>;

pub fn parse(text: &str) -> Ini {
    let mut out: Ini = HashMap::new();
    let mut section = String::new();
    for raw in text.split("\n") {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(";") || line.starts_with("#") {
            continue;
        }
        if line.starts_with("[") {
            if let Some(close) = line.find("]") {
                section = line[1..close].trim().to_lowercase();
            }
            continue;
        }
        if section.is_empty() {
            continue;
        }
        let (key, value) = if let Some(eq) = line.find("=") {
            (line[..eq].trim().to_lowercase(), line[eq + 1..].trim().to_string())
        } else if let Some(sp) = line.find(char::is_whitespace) {
            (line[..sp].trim().to_lowercase(), line[sp + 1..].trim().to_string())
        } else {
            (line.to_lowercase(), "1".to_string())
        };
        out.entry(section.clone()).or_default().insert(key, value);
    }
    out
}

pub fn get(ini: &Ini, section: &str, key: &str) -> Option<String> {
    ini.get(&section.to_lowercase())?
        .get(&key.to_lowercase())
        .cloned()
}

pub fn get_int(ini: &Ini, section: &str, key: &str, def: i32) -> i32 {
    get(ini, section, key)
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(def)
}

/// Parse a comma-separated integer list.
pub fn get_int_list(ini: &Ini, section: &str, key: &str) -> Vec<i32> {
    get(ini, section, key)
        .map(|v| {
            v.split(",")
                .map(|p| p.trim().parse::<i32>().unwrap_or(0))
                .collect()
        })
        .unwrap_or_default()
}

/// Skin colors are written as 0xRRGGBB but stored in BGR byte order
/// (Sogou convention). Returns 0x00BBGGRR (COLORREF-compatible).
pub fn get_color(ini: &Ini, section: &str, key: &str, def: u32) -> u32 {
    let Some(v) = get(ini, section, key) else { return def };
    let t = v.trim();
    let n = if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<u32>().ok()
    };
    match n {
        Some(c) => ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF),
        None => def,
    }
}
