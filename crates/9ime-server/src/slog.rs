//! Minimal file logger (server.log in the user data dir).

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

pub fn log(msg: &str) {
    let path = nineime_core::config::appdata_dir().join("server.log");
    let _ = std::fs::create_dir_all(nineime_core::config::appdata_dir());
    let _guard = LOCK.lock();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{}] {msg}", now());
    }
}

fn now() -> String {
    // wall clock without external deps
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02}")
}
