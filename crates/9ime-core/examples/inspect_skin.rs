//! Debug: extract + parse an .ssf skin and dump the model.
fn main() {
    let path = std::env::args().nth(1).expect("usage: inspect_skin <file.ssf>");
    let bytes = std::fs::read(&path).expect("read skin");
    let files = nineime_core::ssf::extract(&bytes).expect("extract");
    println!("extracted {} files", files.len());
    for (k, v) in &files {
        let n = v.len();
        println!("  {k}: {n} bytes");
    }
    let skin = nineime_core::skin::parse(&files).expect("parse");
    println!("name = {}", skin.name);
    println!("font = {} @ {}", skin.font_name, skin.font_size);
    println!("preedit_color = {:#x}", skin.preedit_color);
    println!("candidate_color = {:#x}", skin.candidate_color);
    println!("hl_color = {:#x}", skin.candidate_hl_color);
    dump("HORIZONTAL", &skin.scheme);
    dump("VERTICAL", &skin.scheme_vertical);
}

fn dump(tag: &str, s: &nineime_core::skin::Scheme) {
    println!("[{tag}]");
    println!("  pic = {}", s.pic.as_ref().map(|p| p.len()).map(|n| format!("{n} bytes")).unwrap_or_else(|| "NONE".into()));
    println!("  candidate_highlight = {}", s.candidate_highlight.as_ref().map(|p| p.len()).map(|n| format!("{n} bytes")).unwrap_or_else(|| "NONE".into()));
    println!("  stretch L/R/T/B = {} {} {} {}", s.stretch_left, s.stretch_right, s.stretch_top, s.stretch_bottom);
    println!("  preedit insets L/T/R = {} {} {}", s.preedit_left, s.preedit_top, s.preedit_right);
    println!("  candidate insets L/R/B = {} {} {}", s.candidate_left, s.candidate_right, s.candidate_bottom);
    println!("  gap = {}, separator = {:?}", s.gap, s.separator_color);
}
