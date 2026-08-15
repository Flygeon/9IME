//! 9IME core: skin container/format parsing, DPI layout math, and shared
//! configuration. Populated in later milestones (M3/M4); the engine
//! milestones only need the crate to exist as the common foundation.

pub const NAME: &str = "9IME";
pub const VERSION: &str = "0.1.0";

pub fn version() -> String {
    VERSION.to_string()
}
