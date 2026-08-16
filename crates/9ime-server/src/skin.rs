//! Server-side skin loading from the user skins directory.

use nineime_core::skin::Skin;
use nineime_core::ssf;

use nineime_core::config;

/// Resolve the configured skin name to an actual file in the skins dir.
/// The config may hold a stale, differently-cased, or mojibake'd name
/// (e.g. written by an older build), so fall back progressively:
/// exact -> case-insensitive -> the only .ssf present.
fn resolve_skin_file(name: &str) -> Option<std::path::PathBuf> {
    let dir = config::skins_dir();
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    let mut ssf_files: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension()
                .map(|x| x.to_string_lossy().eq_ignore_ascii_case("ssf"))
                .unwrap_or(false)
            {
                ssf_files.push(p);
            }
        }
    }
    let want = name.to_lowercase();
    if let Some(p) = ssf_files
        .iter()
        .find(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()) == Some(want.clone()))
    {
        return Some(p.clone());
    }
    // a single installed skin is an unambiguous fallback
    if ssf_files.len() == 1 {
        return ssf_files.into_iter().next();
    }
    None
}

/// Load and parse the named .ssf skin. None when missing/unparseable.
pub fn load_skin(name: &str) -> Option<Skin> {
    if name.is_empty() {
        return None;
    }
    let path = resolve_skin_file(name)?;
    let bytes = std::fs::read(&path).ok()?;
    let files = ssf::extract(&bytes)?;
    let skin = nineime_core::skin::parse(&files);
    if skin.is_none() {
        crate::slog::log(&format!("skin parse failed: {}", path.display()));
    }
    skin
}
