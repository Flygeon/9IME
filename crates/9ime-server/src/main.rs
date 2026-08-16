//! 9IME server: owns librime + the candidate window; serves the TSF client
//! over a named pipe. One session, driven from one thread (librime rule).

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

use nineime_ipc::{ContextMsg, MenuMsg, StatusMsg};
use nineime_librime::{ffi::RimeTraits, Rime};

mod pipe;
mod skin;
mod slog;
mod window;

/// State shared with the UI thread (candidate window).
pub struct UiState {
    pub context: ContextMsg,
    pub status: StatusMsg,
    pub visible: bool,
    pub anchor_x: i32,
    pub anchor_y: i32,
    pub skin: Option<nineime_core::skin::Skin>,
    pub loaded_skin: String,
    /// Candidate window orientation: "vertical" | "horizontal".
    pub layout: String,
    /// Overall UI scale multiplier applied on top of DPI scaling.
    pub ui_scale: f32,
    pub quit: bool,
}

impl UiState {
    pub fn new() -> Self {
        UiState {
            context: ContextMsg::default(),
            status: StatusMsg::default(),
            visible: false,
            anchor_x: 0,
            anchor_y: 0,
            skin: None,
            loaded_skin: String::new(),
            layout: String::new(),
            ui_scale: 0.9,
            quit: false,
        }
    }
}

pub fn exe_dir() -> PathBuf {
    let mut buf = vec![0u16; 2048];
    let n = unsafe {
        windows::Win32::System::LibraryLoader::GetModuleFileNameW(None, &mut buf)
    };
    buf.truncate(n as usize);
    let p = String::from_utf16_lossy(&buf);
    Path::new(&p).parent().map(|d| d.to_path_buf()).unwrap_or_default()
}

pub fn appdata_dir() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    Path::new(&base).join("9IME")
}

pub fn cstr(s: &str) -> CString {
    CString::new(s).expect("interior NUL")
}

pub fn context_msg(ctx: &nineime_librime::Context) -> ContextMsg {
    let composition = ctx.composition.as_ref();
    ContextMsg {
        composing: composition.is_some(),
        preedit: composition.map(|c| c.preedit.clone()).unwrap_or_default(),
        cursor: composition.map(|c| c.cursor_pos).unwrap_or(0),
        menu: MenuMsg {
            page_size: ctx.menu.page_size,
            page_no: ctx.menu.page_no,
            is_last_page: ctx.menu.is_last_page,
            highlighted: ctx.menu.highlighted_candidate_index,
            candidates: ctx
                .menu
                .candidates
                .iter()
                .map(|c| nineime_ipc::CandidateMsg {
                    text: c.text.clone(),
                    comment: c.comment.clone(),
                })
                .collect(),
            select_keys: ctx.menu.select_keys.clone(),
        },
        commit_text_preview: ctx.commit_text_preview.clone(),
    }
}

pub fn status_msg(st: &nineime_librime::Status) -> StatusMsg {
    StatusMsg {
        schema_id: st.schema_id.clone(),
        schema_name: st.schema_name.clone(),
        ascii_mode: st.is_ascii_mode,
        composing: st.is_composing,
        disabled: st.is_disabled,
    }
}


/// Run librime deployment and exit (used by the deployer / installer).
fn deploy_only() {
    let base = exe_dir();
    let dll = base.join("rime.dll");
    let shared = if base.join("data").is_dir() {
        base.join("data")
    } else {
        base.clone()
    };
    let user = appdata_dir();
    let _ = std::fs::create_dir_all(&user);
    let log_dir = user.join("log");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_c = cstr(&log_dir.to_string_lossy());
    let rime = match Rime::load(&dll) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("deploy: cannot load librime: {e}");
            std::process::exit(1);
        }
    };
    let shared_c = cstr(&shared.to_string_lossy());
    let user_c = cstr(&user.to_string_lossy());
    let app_c = cstr("rime.9ime");
    let mut traits = RimeTraits::default();
    traits.shared_data_dir = shared_c.as_ptr();
    traits.user_data_dir = user_c.as_ptr();
    traits.app_name = app_c.as_ptr();
    traits.min_log_level = 0;
    traits.log_dir = log_c.as_ptr();
    // Deployer mode: deployer_initialize + prebuild + deploy, no session
    // initialize (same pattern as the weasel deployer).
    let api = rime.api();
    let di = match api.deployer_initialize {
        Some(f) => f,
        None => {
            eprintln!("deploy failed: deployer_initialize unavailable");
            std::process::exit(1);
        }
    };
    let pb = match api.prebuild {
        Some(f) => f,
        None => {
            eprintln!("deploy failed: prebuild unavailable");
            std::process::exit(1);
        }
    };
    let dp = match api.deploy {
        Some(f) => f,
        None => {
            eprintln!("deploy failed: deploy unavailable");
            std::process::exit(1);
        }
    };
    // SAFETY: no concurrent librime use.
    unsafe { di(&traits as *const RimeTraits as *mut RimeTraits) };
    let prebuilt = unsafe { pb() } != 0;
    println!("prebuild result: {prebuilt}");
    if !prebuilt {
        eprintln!("prebuild failed - see log files in {}", user.join("log").display());
        std::process::exit(1);
    }
    let deployed = unsafe { dp() } != 0;
    println!("deploy result: {deployed}");
    if !deployed {
        eprintln!("deploy failed - see log files in {}", user.join("log").display());
        std::process::exit(1);
    }
    println!("deploy ok");
    std::process::exit(0);
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--deploy") {
        deploy_only();
        return;
    }
    // Make layered-window coordinates match the foreground app's screen pixels
    // instead of being virtualized by the OS.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let base = exe_dir();
    let dll = base.join("rime.dll");
    let shared = if base.join("data").is_dir() {
        base.join("data")
    } else {
        base.clone()
    };
    let user = appdata_dir();
    let _ = std::fs::create_dir_all(&user);

    slog::log(&format!("start: rime.dll={} shared={} user={}", dll.display(), shared.display(), user.display()));

    let rime = match Rime::load(&dll) {
        Ok(r) => r,
        Err(e) => {
            slog::log(&format!("FATAL cannot load librime: {e}"));
            eprintln!("server: cannot load librime: {e}");
            std::process::exit(1);
        }
    };
    let version = rime.version().unwrap_or_default();
    slog::log(&format!("librime {version} loaded"));

    // traits must outlive initialize.
    let log_dir = user.join("log");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_c = cstr(&log_dir.to_string_lossy());
    let dist_c = cstr("9IME");
    let shared_c = cstr(&shared.to_string_lossy());
    let user_c = cstr(&user.to_string_lossy());
    let app_c = cstr("rime.9ime");
    let mut traits = RimeTraits::default();
    traits.shared_data_dir = shared_c.as_ptr();
    traits.user_data_dir = user_c.as_ptr();
    traits.app_name = app_c.as_ptr();
    traits.distribution_name = dist_c.as_ptr();
    traits.distribution_code_name = dist_c.as_ptr();
    traits.min_log_level = 1;
    traits.log_dir = log_c.as_ptr();

    if let Err(e) = rime.initialize(&traits) {
        slog::log(&format!("FATAL initialize failed: {e}"));
        eprintln!("server: initialize failed: {e}");
        std::process::exit(1);
    }

    // Deploy in a background thread so the pipe can answer immediately.
    // The rime thread creates the session lazily once deploy_done is set,
    // so librime is never asked for a session while in maintenance mode.
    let deploy_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let deploy_done2 = deploy_done.clone();
    let deploy_rime = Rime::load(&dll);
    let deploy_user = user.clone();
    std::thread::spawn(move || {
        let Ok(dr) = deploy_rime else {
            slog::log("deploy thread: cannot load librime");
            deploy_done2.store(true, std::sync::atomic::Ordering::SeqCst);
            return;
        };
        let full = !deploy_user.join("build").join("default.yaml").exists();
        slog::log(if full { "first run, deploying..." } else { "maintenance deploy..." });
        let r = dr.deploy(full);
        slog::log(&format!("deploy result: {r:?}"));
        deploy_done2.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    let ui = Arc::new(Mutex::new(UiState::new()));
    let changed = Arc::new(AtomicBool::new(false));
    let win = window::CandidateWindow::spawn(ui.clone(), changed.clone());
    slog::log("candidate window thread started");

    // Pipe listener in the background; this thread is the rime thread.
    let rx = pipe::start_listener();
    pipe::run(&rime, ui.clone(), changed.clone(), deploy_done, rx);

    let _ = rime.finalize();
    let _ = win.join();
    slog::log("server exit");
}
