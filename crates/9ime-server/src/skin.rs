//! Server-side skin loading from the user skins directory.

use nineime_core::skin::Skin;
use nineime_core::ssf;

use nineime_core::config;

/// Load and parse the named .ssf skin. None when missing/unparseable.
pub fn load_skin(name: &str) -> Option<Skin> {
    if name.is_empty() {
        return None;
    }
    let path = config::skins_dir().join(name);
    let bytes = std::fs::read(&path).ok()?;
    let files = ssf::extract(&bytes)?;
    nineime_core::skin::parse(&files)
}
