//! Safe-ish wrapper over the librime C API (see ffi).
//!
//! Threading: librime sessions must be driven from a single thread; the
//! wrapper does not enforce this - the caller (IPC server thread) must.

pub mod ffi;

use std::ffi::{c_char, CStr, CString};
use std::os::raw::c_void;
use std::path::Path;
use std::time::Duration;

use ffi::{
    RimeApi, RimeCommit, RimeContext, RimeLibrary, RimeNotificationHandler,
    RimeSessionId, RimeStatus, RimeTraits,
};

#[derive(Debug)]
pub enum Error {
    /// Failed to load the librime dynamic library.
    Load(String),
    /// The loaded librime is older than this binding: member missing.
    Unavailable(&'static str),
    /// An API returned NULL where a value was expected.
    NullPointer(&'static str),
    /// An API call failed (returned False).
    Failed(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Load(m) => write!(f, "librime load error: {m}"),
            Error::Unavailable(name) => write!(f, "librime API unavailable: {name}"),
            Error::NullPointer(name) => write!(f, "librime returned NULL: {name}"),
            Error::Failed(name) => write!(f, "librime call failed: {name}"),
        }
    }
}
impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

fn cstring(s: &str) -> CString {
    CString::new(s).expect("interior NUL in string passed to librime")
}

unsafe fn cstr_to_string(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(CStr::from_ptr(p).to_string_lossy().into_owned())
    }
}

/// Whether the loaded RimeApi provides the member at `member_offset`
/// (mirrors RIME_API_AVAILABLE).
pub fn api_has_member(api: &RimeApi, member_offset: usize) -> bool {
    (std::mem::size_of::<std::ffi::c_int>() + api.data_size as usize) > member_offset
}

/// The engine handle: owns the loaded librime library.
pub struct Rime {
    lib: RimeLibrary,
}

impl Rime {
    /// Load librime from `path` (dll/so path or directory containing it).
    pub fn load(path: &Path) -> Result<Self> {
        // SAFETY: RimeLibrary::load only reads the library and rime_get_api().
        let lib = unsafe { RimeLibrary::load(path) }.map_err(Error::Load)?;
        Ok(Rime { lib })
    }

    pub(crate) fn api(&self) -> &RimeApi {
        self.lib.api()
    }

    /// rime_api: setup + initialize. Call once at process start.
    pub fn initialize(&self, traits: &RimeTraits) -> Result<()> {
        let api = self.api();
        let setup = api.setup.ok_or(Error::Unavailable("setup"))?;
        let init = api.initialize.ok_or(Error::Unavailable("initialize"))?;
        // SAFETY: traits must outlive initialize; caller keeps it alive.
        unsafe {
            setup(traits as *const RimeTraits as *mut RimeTraits);
            init(traits as *const RimeTraits as *mut RimeTraits);
        }
        Ok(())
    }

    pub fn finalize(&self) -> Result<()> {
        let f = self.api().finalize.ok_or(Error::Unavailable("finalize"))?;
        // SAFETY: finalize tears down librime.
        unsafe { f() };
        Ok(())
    }

    pub fn version(&self) -> Option<String> {
        let v = self.api().get_version?;
        // SAFETY: librime returns a static version string.
        unsafe { cstr_to_string(v()) }
    }

    /// Run deployment (build schemas). Blocks until maintenance completes.
    /// Must not run concurrently with session use.
    pub fn deploy(&self, full_check: bool) -> Result<bool> {
        let api = self.api();
        let sm = api.start_maintenance.ok_or(Error::Unavailable("start_maintenance"))?;
        let is_mm = api.is_maintenance_mode.ok_or(Error::Unavailable("is_maintenance_mode"))?;
        let join = api.join_maintenance_thread.ok_or(Error::Unavailable("join_maintenance_thread"))?;
        // SAFETY: no concurrent librime use by caller contract.
        let started = unsafe { sm(if full_check { 1 } else { 0 }) } != 0;
        if !started {
            return Ok(false);
        }
        // Wait up to 180s (100ms x 1800) for maintenance to finish.
        let mut done = false;
        for _ in 0..1800 {
            // SAFETY: read-only flag.
            if unsafe { is_mm() } == 0 {
                done = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if done {
            // SAFETY: join the maintenance thread once it has finished.
            unsafe { join() };
            return Ok(true);
        }
        // Timed out: do not join (would block forever); report failure.
        eprintln!("librime: deploy timed out after 180s");
        Ok(false)
    }

    /// Deploy via the simpler deployer API: deployer_initialize + prebuild + deploy.
    pub fn deploy_direct(&self, traits: &RimeTraits) -> Result<bool> {
        let api = self.api();
        let di = api.deployer_initialize.ok_or(Error::Unavailable("deployer_initialize"))?;
        let pb = api.prebuild.ok_or(Error::Unavailable("prebuild"))?;
        let dp = api.deploy.ok_or(Error::Unavailable("deploy"))?;
        // SAFETY: caller guarantees no concurrent librime use.
        unsafe { di(traits as *const RimeTraits as *mut RimeTraits) };
        let ok = unsafe { pb() } != 0 && unsafe { dp() } != 0;
        Ok(ok)
    }

    pub fn is_maintenance_mode(&self) -> bool {
        let Some(f) = self.api().is_maintenance_mode else { return false };
        // SAFETY: read-only flag.
        (unsafe { f() }) != 0
    }

    /// Create a new input session. The returned handle borrows this Rime;
    /// the caller must keep the Rime alive and drive the session from a
    /// single thread.
    pub fn create_session(&self) -> Result<RimeSession> {
        let f = self.api().create_session.ok_or(Error::Unavailable("create_session"))?;
        // SAFETY: sessions are driven from one thread at a time.
        let id = unsafe { f() };
        if id == 0 {
            return Err(Error::Failed("create_session"));
        }
        Ok(RimeSession { rime: self as *const Rime, id })
    }

    /// Set the notification callback (schema/option/deploy events).
    pub fn set_notification_handler(
        &self,
        handler: RimeNotificationHandler,
        ctx: *mut c_void,
    ) -> Result<()> {
        let f = self.api().set_notification_handler
            .ok_or(Error::Unavailable("set_notification_handler"))?;
        // SAFETY: handler must remain valid until finalize.
        unsafe { f(handler, ctx) };
        Ok(())
    }

    pub fn shared_data_dir(&self) -> Option<String> {
        let f = self.api().get_shared_data_dir_s?;
        let mut buf = [0 as c_char; 1024];
        // SAFETY: buf is valid for 1024 bytes.
        unsafe { f(buf.as_mut_ptr(), buf.len()) };
        unsafe { cstr_to_string(buf.as_ptr()) }
    }

    pub fn user_data_dir(&self) -> Option<String> {
        let f = self.api().get_user_data_dir_s?;
        let mut buf = [0 as c_char; 1024];
        // SAFETY: buf is valid for 1024 bytes.
        unsafe { f(buf.as_mut_ptr(), buf.len()) };
        unsafe { cstr_to_string(buf.as_ptr()) }
    }
}

/// A librime input session. Use from a single thread only.
/// The Rime handle must outlive all sessions.
pub struct RimeSession {
    rime: *const Rime,
    pub id: RimeSessionId,
}

impl RimeSession {
    fn rime(&self) -> &Rime {
        // SAFETY: Rime outlives its sessions by contract.
        unsafe { &*self.rime }
    }

    pub fn process_key(&self, keycode: u32, mask: u32) -> bool {
        let Some(f) = self.rime().api().process_key else { return false };
        // SAFETY: single-threaded session use.
        (unsafe { f(self.id, keycode as std::os::raw::c_int, mask as std::os::raw::c_int) }) != 0
    }

    pub fn commit_composition(&self) -> bool {
        let Some(f) = self.rime().api().commit_composition else { return false };
        // SAFETY: single-threaded session use.
        (unsafe { f(self.id) }) != 0
    }

    pub fn clear_composition(&self) {
        if let Some(f) = self.rime().api().clear_composition {
            // SAFETY: single-threaded session use.
            unsafe { f(self.id) };
        }
    }

    /// Commit text pending for this session (if any).
    pub fn get_commit(&self) -> Option<String> {
        let f = self.rime().api().get_commit?;
        let mut c = RimeCommit::default();
        // SAFETY: c is a valid RimeCommit; librime fills it.
        if unsafe { f(self.id, &mut c) } == 0 {
            return None;
        }
        let text = unsafe { cstr_to_string(c.text) };
        if let Some(g) = self.rime().api().free_commit {
            // SAFETY: c was filled by get_commit.
            unsafe { g(&mut c) };
        }
        text
    }

    pub fn get_context(&self) -> Option<Context> {
        let f = self.rime().api().get_context?;
        let mut ctx = RimeContext::default();
        // SAFETY: ctx is a valid RimeContext; librime fills it.
        if unsafe { f(self.id, &mut ctx) } == 0 {
            return None;
        }
        let out = unsafe { Context::from_raw(&ctx) };
        if let Some(g) = self.rime().api().free_context {
            // SAFETY: ctx was filled by get_context.
            unsafe { g(&mut ctx) };
        }
        Some(out)
    }

    pub fn get_status(&self) -> Option<Status> {
        let f = self.rime().api().get_status?;
        let mut st = RimeStatus::default();
        // SAFETY: st is a valid RimeStatus; librime fills it.
        if unsafe { f(self.id, &mut st) } == 0 {
            return None;
        }
        let out = unsafe { Status::from_raw(&st) };
        if let Some(g) = self.rime().api().free_status {
            // SAFETY: st was filled by get_status.
            unsafe { g(&mut st) };
        }
        Some(out)
    }

    pub fn set_option(&self, name: &str, value: bool) -> bool {
        let Some(f) = self.rime().api().set_option else { return false };
        let n = cstring(name);
        // SAFETY: n lives for the call.
        unsafe { f(self.id, n.as_ptr(), if value { 1 } else { 0 }) };
        true
    }

    pub fn get_option(&self, name: &str) -> Option<bool> {
        let f = self.rime().api().get_option?;
        let n = cstring(name);
        // SAFETY: n lives for the call.
        let v = unsafe { f(self.id, n.as_ptr()) };
        Some(v != 0)
    }

    pub fn set_input(&self, input: &str) -> bool {
        let Some(f) = self.rime().api().set_input else { return false };
        let i = cstring(input);
        // SAFETY: i lives for the call.
        (unsafe { f(self.id, i.as_ptr()) }) != 0
    }

    pub fn get_input(&self) -> Option<String> {
        let f = self.rime().api().get_input?;
        // SAFETY: returned pointer valid until next edit.
        unsafe { cstr_to_string(f(self.id)) }
    }

    pub fn select_candidate(&self, index: usize) -> bool {
        let Some(f) = self.rime().api().select_candidate else { return false };
        // SAFETY: single-threaded session use.
        (unsafe { f(self.id, index) }) != 0
    }

    pub fn select_candidate_on_current_page(&self, index: usize) -> bool {
        let Some(f) = self.rime().api().select_candidate_on_current_page else { return false };
        // SAFETY: single-threaded session use.
        (unsafe { f(self.id, index) }) != 0
    }

    pub fn change_page(&self, backward: bool) -> bool {
        let Some(f) = self.rime().api().change_page else { return false };
        // SAFETY: single-threaded session use.
        (unsafe { f(self.id, if backward { 1 } else { 0 }) }) != 0
    }

    pub fn select_schema(&self, schema_id: &str) -> bool {
        let Some(f) = self.rime().api().select_schema else { return false };
        let s = cstring(schema_id);
        // SAFETY: s lives for the call.
        (unsafe { f(self.id, s.as_ptr()) }) != 0
    }

    pub fn current_schema(&self) -> Option<String> {
        let f = self.rime().api().get_current_schema?;
        let mut buf = [0 as c_char; 256];
        // SAFETY: buf is valid for 256 bytes.
        if unsafe { f(self.id, buf.as_mut_ptr(), buf.len()) } == 0 {
            return None;
        }
        unsafe { cstr_to_string(buf.as_ptr()) }
    }

    pub fn destroy(&mut self) -> bool {
        let Some(f) = self.rime().api().destroy_session else { return false };
        // SAFETY: session id is ours.
        let ok = unsafe { f(self.id) } != 0;
        self.id = 0;
        ok
    }
}

impl Drop for RimeSession {
    fn drop(&mut self) {
        if self.id != 0 {
            let _ = self.destroy();
        }
    }
}

// ---------------------------------------------------------------------------
// Owned snapshots
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct Composition {
    pub length: i32,
    pub cursor_pos: i32,
    pub sel_start: i32,
    pub sel_end: i32,
    pub preedit: String,
}

#[derive(Debug, Default, Clone)]
pub struct Candidate {
    pub text: String,
    pub comment: String,
}

#[derive(Debug, Default, Clone)]
pub struct Menu {
    pub page_size: i32,
    pub page_no: i32,
    pub is_last_page: bool,
    pub highlighted_candidate_index: i32,
    pub num_candidates: i32,
    pub candidates: Vec<Candidate>,
    pub select_keys: String,
}

#[derive(Debug, Default, Clone)]
pub struct Context {
    pub composition: Option<Composition>,
    pub menu: Menu,
    pub commit_text_preview: Option<String>,
    pub select_labels: Vec<String>,
}

impl Context {
    unsafe fn from_raw(ctx: &RimeContext) -> Self {
        let composing = ctx.composition.length > 0 && !ctx.composition.preedit.is_null();
        let composition = if composing {
            Some(Composition {
                length: ctx.composition.length,
                cursor_pos: ctx.composition.cursor_pos,
                sel_start: ctx.composition.sel_start,
                sel_end: ctx.composition.sel_end,
                preedit: cstr_to_string(ctx.composition.preedit).unwrap_or_default(),
            })
        } else {
            None
        };
        let menu = Menu {
            page_size: ctx.menu.page_size,
            page_no: ctx.menu.page_no,
            is_last_page: ctx.menu.is_last_page != 0,
            highlighted_candidate_index: ctx.menu.highlighted_candidate_index,
            num_candidates: ctx.menu.num_candidates,
            candidates: (0..ctx.menu.num_candidates.max(0) as usize)
                .filter_map(|i| unsafe { ctx.menu.candidates.add(i).as_ref() })
                .map(|c| Candidate {
                    text: cstr_to_string(c.text).unwrap_or_default(),
                    comment: cstr_to_string(c.comment).unwrap_or_default(),
                })
                .collect(),
            select_keys: cstr_to_string(ctx.menu.select_keys).unwrap_or_default(),
        };
        let select_labels = if ctx.select_labels.is_null() {
            Vec::new()
        } else {
            (0..ctx.menu.num_candidates.max(0) as usize)
                .filter_map(|i| unsafe { ctx.select_labels.add(i).as_ref() })
                .filter_map(|p| cstr_to_string(*p))
                .collect()
        };
        Context {
            composition,
            menu,
            commit_text_preview: cstr_to_string(ctx.commit_text_preview),
            select_labels,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Status {
    pub schema_id: String,
    pub schema_name: String,
    pub is_disabled: bool,
    pub is_composing: bool,
    pub is_ascii_mode: bool,
    pub is_full_shape: bool,
    pub is_simplified: bool,
    pub is_traditional: bool,
    pub is_ascii_punct: bool,
}

impl Status {
    unsafe fn from_raw(st: &RimeStatus) -> Self {
        Status {
            schema_id: cstr_to_string(st.schema_id).unwrap_or_default(),
            schema_name: cstr_to_string(st.schema_name).unwrap_or_default(),
            is_disabled: st.is_disabled != 0,
            is_composing: st.is_composing != 0,
            is_ascii_mode: st.is_ascii_mode != 0,
            is_full_shape: st.is_full_shape != 0,
            is_simplified: st.is_simplified != 0,
            is_traditional: st.is_traditional != 0,
            is_ascii_punct: st.is_ascii_punct != 0,
        }
    }
}
