//! 9IME console: engine smoke test / read-test harness (M1).
//!
//! Usage:
//!   9ime-console.exe <rime.dll path> <shared_data_dir> <user_data_dir>
//!
//! Prints librime version, deploys, then feeds the key sequence "nihao"
//! and dumps commit / context / status snapshots.

use std::ffi::CString;
use std::path::Path;

use nineime_librime::{ffi::RimeTraits, Rime};

const KEY_SPACE: u32 = 0x20;
const KEY_ENTER: u32 = 0x0D;
const MASK_NONE: u32 = 0;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: 9ime-console.exe <rime.dll> <shared_data_dir> <user_data_dir>");
        std::process::exit(2);
    }
    let dll_path = Path::new(&args[1]);
    let shared = &args[2];
    let user = &args[3];

    // --- load engine ---
    let rime = match Rime::load(dll_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("FAIL load: {e}");
            std::process::exit(1);
        }
    };
    match rime.version() {
        Some(v) => println!("librime version: {v}"),
        None => eprintln!("WARN: no version string"),
    }

    // --- traits (strings must outlive initialize) ---
    let shared_c = CString::new(shared.as_str()).unwrap();
    let user_c = CString::new(user.as_str()).unwrap();
    let app_c = CString::new("rime.9ime").unwrap();
    let dist_c = CString::new("9IME").unwrap();
    let mut traits = RimeTraits::default();
    traits.shared_data_dir = shared_c.as_ptr();
    traits.user_data_dir = user_c.as_ptr();
    traits.distribution_name = dist_c.as_ptr();
    traits.distribution_code_name = dist_c.as_ptr();
    traits.app_name = app_c.as_ptr();
    traits.min_log_level = 0;

    if let Err(e) = rime.initialize(&traits) {
        eprintln!("FAIL initialize: {e}");
        std::process::exit(1);
    }
    println!("initialized");

    // --- deploy (may be slow on first run) ---
    println!("deploying (full_check=false)...");
    match rime.deploy(false) {
        Ok(true) => println!("deploy ok"),
        Ok(false) => eprintln!("WARN: deploy returned false (maybe already deployed)"),
        Err(e) => {
            eprintln!("FAIL deploy: {e}");
            let _ = rime.finalize();
            std::process::exit(1);
        }
    }

    // --- session smoke test ---
    let mut sess = match rime.create_session() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("FAIL create_session: {e}");
            let _ = rime.finalize();
            std::process::exit(1);
        }
    };
    println!("session id = {}", sess.id);

    let keys: [(u32, u32); 6] = [
        (b'n' as u32, MASK_NONE),
        (b'i' as u32, MASK_NONE),
        (b'h' as u32, MASK_NONE),
        (b'a' as u32, MASK_NONE),
        (b'o' as u32, MASK_NONE),
        (KEY_SPACE, MASK_NONE),
    ];
    for (i, (kc, mk)) in keys.iter().enumerate() {
        let handled = sess.process_key(*kc, *mk);
        println!("key[{i}] 0x{kc:02X} handled={handled}");
    }

    let input = sess.get_input().unwrap_or_default();
    println!("input: {input:?}");
    if let Some(ctx) = sess.get_context() {
        println!("context: composing={:?}", ctx.composition.is_some());
        if let Some(comp) = &ctx.composition {
            println!("  preedit: {}", comp.preedit);
        }
        println!("  menu: {} candidates, page {} of last_page={}",
            ctx.menu.num_candidates, ctx.menu.page_no, ctx.menu.is_last_page);
        for (i, c) in ctx.menu.candidates.iter().enumerate() {
            println!("    [{i}] {}", c.text);
        }
    }
    if let Some(st) = sess.get_status() {
        println!("status: schema={} name={} ascii={} composing={}",
            st.schema_id, st.schema_name, st.is_ascii_mode, st.is_composing);
    }

    // --- commit what remains (e.g. return key) ---
    sess.process_key(KEY_ENTER, MASK_NONE);
    if let Some(commit) = sess.get_commit() {
        println!("commit: {commit}");
    } else {
        println!("commit: (none)");
    }
    sess.clear_composition();

    let _ = sess.destroy();
    let _ = rime.finalize();
    println!("console test done");
}
