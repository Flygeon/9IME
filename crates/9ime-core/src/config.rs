//! 9IME user configuration (%APPDATA%\9IME). Shared by server/deployer.

use std::path::PathBuf;

pub fn appdata_dir() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("9IME")
}

pub fn config_path() -> PathBuf {
    appdata_dir().join("9ime.json")
}

pub fn skins_dir() -> PathBuf {
    appdata_dir().join("skins")
}

/// Candidate-window orientation.
pub const LAYOUT_VERTICAL: &str = "vertical";
pub const LAYOUT_HORIZONTAL: &str = "horizontal";

#[derive(Debug, Clone)]
pub struct Config {
    pub skin: String,
    /// Candidate window layout: "vertical" (default) or "horizontal".
    pub layout: String,
    /// Overall UI scale multiplier applied on top of DPI scaling.
    pub ui_scale: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            skin: String::new(),
            layout: LAYOUT_VERTICAL.to_string(),
            ui_scale: 0.85,
        }
    }
}

impl Config {
    pub fn is_horizontal(&self) -> bool {
        self.layout == LAYOUT_HORIZONTAL
    }
}

/// Read the config file. Missing/broken files yield the default config.
pub fn load() -> Config {
    let path = config_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    let mut cfg = Config::default();
    // tiny JSON parser: {"skin": "x.ssf", "layout": "vertical"}
    if let Some(open) = text.find("{") {
        let body = &text[open + 1..];
        if let Some(close) = body.find("}") {
            let body = &body[..close];
            for part in body.split(",") {
                if let Some(eq) = part.find(":") {
                    let key = part[..eq].trim().trim_start_matches('"').trim_end_matches('"');
                    let value = part[eq + 1..].trim().trim_start_matches('"').trim_end_matches('"');
                    match key {
                        "skin" => cfg.skin = value.to_string(),
                        "layout" => cfg.layout = value.to_string(),
                        "ui_scale" => {
                            if let Ok(v) = value.parse::<f32>() {
                                cfg.ui_scale = v.clamp(0.5, 2.0);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    if cfg.layout.is_empty() {
        cfg.layout = LAYOUT_VERTICAL.to_string();
    }
    cfg
}

/// Write the config file.
pub fn save(cfg: &Config) -> std::io::Result<()> {
    let _ = std::fs::create_dir_all(&appdata_dir());
    let layout = if cfg.layout.is_empty() { LAYOUT_VERTICAL } else { &cfg.layout };
    let scale = cfg.ui_scale.clamp(0.5, 2.0);
    let text = format!(
        "{{\"skin\": \"{}\", \"layout\": \"{}\", \"ui_scale\": {:.2}}}\n",
        cfg.skin, layout, scale
    );
    std::fs::write(config_path(), text)
}
