//! 9IME core: skin (.ssf) containers, skin.ini model, shared config.

pub mod config;
pub mod ini;
pub mod skin;
pub mod ssf;

pub const NAME: &str = "9IME";
pub const VERSION: &str = "0.1.0";

pub fn version() -> String {
    VERSION.to_string()
}
