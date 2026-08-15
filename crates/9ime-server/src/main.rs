//! 9IME server: owns librime + the candidate window; serves the TSF client
//! over a named pipe. One session, driven from one thread (librime rule).

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use nineime_ipc::{ContextMsg, MenuMsg, StatusMsg};
use nineime_librime::{ffi::RimeTraits, Rime};

mod pipe;
mod skin;
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
    let di = api.deployer_initialize.ok_or_else(|| {
        eprintln!("deploy failed: deployer_initialize unavailable");
        std::process::exit(1);
    });
    let pb = api.prebuild.ok_or_else(|| {
        eprintln!("deploy failed: prebuild unavailable");
        std::process::exit(1);
    });
    let dp = api.deploy.ok_or_else(|| {
        eprintln!("deploy failed: deploy unavailable");
        std::process::exit(1);
    });
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
    let base = exe_dir();
    let dll = base.join("rime.dll");
    let shared = if base.join("data").is_dir() {
        base.join("data")
    } else {
        base.clone()
    };
    let user = appdata_dir();
    let _ = std::fs::create_dir_all(&user);

    println!("9IME server: rime.dll = {}", dll.display());
    println!("9IME server: shared data = {}", shared.display());
    println!("9IME server: user data = {}", user.display());

    let rime = match Rime::load(&dll) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("server: cannot load librime: {e}");
            std::process::exit(1);
        }
    };
    let version = rime.version().unwrap_or_default();
    println!("server: librime {version}");

    // traits must outlive initialize.
    let shared_c = cstr(&shared.to_string_lossy());
    let user_c = cstr(&user.to_string_lossy());
    let app_c = cstr("rime.9ime");
    let mut traits = RimeTraits::default();
    traits.shared_data_dir = shared_c.as_ptr();
    traits.user_data_dir = user_c.as_ptr();
    traits.app_name = app_c.as_ptr();
    traits.min_log_level = 1;

    if let Err(e) = rime.initialize(&traits) {
        eprintln!("server: initialize failed: {e}");
        std::process::exit(1);
    }

    // deploy on first run (build dir missing)
    let build_dir = user.join("build");
    if !build_dir.join("default.yaml").exists() {
        println!("server: first run, deploying...");
        match rime.deploy(true) {
            Ok(ok) => println!("server: deploy ok={ok}"),
            Err(e) => eprintln!("server: deploy failed: {e}"),
        }
    } else {
        // lightweight maintenance check (merges user config changes)
        let _ = rime.deploy(false);
    }

    // one session, used only on this thread
    let mut sess = match rime.create_session() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("server: create_session failed: {e}");
            std::process::exit(1);
        }
    };
    let schema = sess.current_schema().unwrap_or_default();
    println!("server: session {}, schema {}", sess.id, schema);

    let ui = Arc::new(Mutex::new(UiState::new()));
    let changed = Arc::new(AtomicBool::new(false));
    let win = window::CandidateWindow::spawn(ui.clone(), changed.clone());
    println!("server: candidate window thread started");

    pipe::serve(&rime, &mut sess, ui.clone(), changed.clone());

    let _ = sess.destroy();
    let _ = rime.finalize();
    let _ = win.join();
    println!("server: exit");
}
