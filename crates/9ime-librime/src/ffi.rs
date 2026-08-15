//! FFI bindings to librime (rime_api.h).
//!
//! Field order of every struct MUST match rime_api.h exactly; these are
//! version-controlled self-describing structs, so we also mirror the
//! RIME_STRUCT_INIT convention in Default impls (data_size = size of the
//! versioned portion, i.e. the whole struct minus the leading data_size).
#![allow(non_camel_case_types)]

use std::ffi::c_char;
use std::os::raw::{c_int, c_void};
use std::path::Path;

/// rime_api.h: `#define Bool int`
pub type Bool = c_int;
/// rime_api.h: `typedef uintptr_t RimeSessionId;`
pub type RimeSessionId = usize;

/// Callback invoked by librime on schema/option/deploy events.
pub type RimeNotificationHandler =
    Option<unsafe extern "C" fn(*mut c_void, RimeSessionId, *const c_char, *const c_char)>;

/// `RIME_API RimeApi * rime_get_api(void);`
pub type RimeGetApi = unsafe extern "C" fn() -> *const RimeApi;

/// rime_api.h: rime_traits_t
#[repr(C)]
pub struct RimeTraits {
    pub data_size: c_int,
    // v0.9
    pub shared_data_dir: *const c_char,
    pub user_data_dir: *const c_char,
    pub distribution_name: *const c_char,
    pub distribution_code_name: *const c_char,
    pub distribution_version: *const c_char,
    // v1.0
    pub app_name: *const c_char,
    pub modules: *const *const c_char,
    // v1.6
    pub min_log_level: c_int,
    pub log_dir: *const c_char,
    pub prebuilt_data_dir: *const c_char,
    pub staging_dir: *const c_char,
}

impl Default for RimeTraits {
    fn default() -> Self {
        Self {
            data_size: (std::mem::size_of::<Self>() - std::mem::size_of::<c_int>()) as c_int,
            shared_data_dir: std::ptr::null(),
            user_data_dir: std::ptr::null(),
            distribution_name: std::ptr::null(),
            distribution_code_name: std::ptr::null(),
            distribution_version: std::ptr::null(),
            app_name: std::ptr::null(),
            modules: std::ptr::null(),
            min_log_level: 0,
            log_dir: std::ptr::null(),
            prebuilt_data_dir: std::ptr::null(),
            staging_dir: std::ptr::null(),
        }
    }
}

/// rime_api.h: RimeComposition
#[repr(C)]
#[derive(Default)]
pub struct RimeComposition {
    pub length: c_int,
    pub cursor_pos: c_int,
    pub sel_start: c_int,
    pub sel_end: c_int,
    pub preedit: *mut c_char,
}

/// rime_api.h: rime_candidate_t
#[repr(C)]
#[derive(Default)]
pub struct RimeCandidate {
    pub text: *mut c_char,
    pub comment: *mut c_char,
    pub reserved: *mut c_void,
}

/// rime_api.h: RimeMenu
#[repr(C)]
#[derive(Default)]
pub struct RimeMenu {
    pub page_size: c_int,
    pub page_no: c_int,
    pub is_last_page: Bool,
    pub highlighted_candidate_index: c_int,
    pub num_candidates: c_int,
    pub candidates: *mut RimeCandidate,
    pub select_keys: *mut c_char,
}

/// rime_api.h: rime_commit_t (versioned)
#[repr(C)]
pub struct RimeCommit {
    pub data_size: c_int,
    // v0.9
    pub text: *mut c_char,
}

impl Default for RimeCommit {
    fn default() -> Self {
        Self {
            data_size: (std::mem::size_of::<Self>() - std::mem::size_of::<c_int>()) as c_int,
            text: std::ptr::null_mut(),
        }
    }
}

/// rime_api.h: rime_context_t (versioned)
#[repr(C)]
pub struct RimeContext {
    pub data_size: c_int,
    // v0.9
    pub composition: RimeComposition,
    pub menu: RimeMenu,
    // v0.9.2
    pub commit_text_preview: *mut c_char,
    pub select_labels: *mut *mut c_char,
}

impl Default for RimeContext {
    fn default() -> Self {
        Self {
            data_size: (std::mem::size_of::<Self>() - std::mem::size_of::<c_int>()) as c_int,
            composition: RimeComposition::default(),
            menu: RimeMenu::default(),
            commit_text_preview: std::ptr::null_mut(),
            select_labels: std::ptr::null_mut(),
        }
    }
}

/// rime_api.h: rime_candidate_preview_t (versioned)
#[repr(C)]
pub struct RimeCandidatePreview {
    pub data_size: c_int,
    pub text_before_selection: *mut c_char,
    pub selected_text: *mut c_char,
    pub text_after_selection: *mut c_char,
}

impl Default for RimeCandidatePreview {
    fn default() -> Self {
        Self {
            data_size: (std::mem::size_of::<Self>() - std::mem::size_of::<c_int>()) as c_int,
            text_before_selection: std::ptr::null_mut(),
            selected_text: std::ptr::null_mut(),
            text_after_selection: std::ptr::null_mut(),
        }
    }
}

/// rime_api.h: rime_status_t (versioned)
#[repr(C)]
pub struct RimeStatus {
    pub data_size: c_int,
    // v0.9
    pub schema_id: *mut c_char,
    pub schema_name: *mut c_char,
    pub is_disabled: Bool,
    pub is_composing: Bool,
    pub is_ascii_mode: Bool,
    pub is_full_shape: Bool,
    pub is_simplified: Bool,
    pub is_traditional: Bool,
    pub is_ascii_punct: Bool,
}

impl Default for RimeStatus {
    fn default() -> Self {
        Self {
            data_size: (std::mem::size_of::<Self>() - std::mem::size_of::<c_int>()) as c_int,
            schema_id: std::ptr::null_mut(),
            schema_name: std::ptr::null_mut(),
            is_disabled: 0,
            is_composing: 0,
            is_ascii_mode: 0,
            is_full_shape: 0,
            is_simplified: 0,
            is_traditional: 0,
            is_ascii_punct: 0,
        }
    }
}

/// rime_api.h: rime_candidate_list_iterator_t
#[repr(C)]
#[derive(Default)]
pub struct RimeCandidateListIterator {
    pub ptr: *mut c_void,
    pub index: c_int,
    pub candidate: RimeCandidate,
}

/// rime_api.h: rime_config_t
#[repr(C)]
#[derive(Default)]
pub struct RimeConfig {
    pub ptr: *mut c_void,
}

/// rime_api.h: rime_config_iterator_t
#[repr(C)]
#[derive(Default)]
pub struct RimeConfigIterator {
    pub list: *mut c_void,
    pub map: *mut c_void,
    pub index: c_int,
    pub key: *const c_char,
    pub path: *const c_char,
}

/// rime_api.h: rime_schema_list_item_t
#[repr(C)]
#[derive(Default)]
pub struct RimeSchemaListItem {
    pub schema_id: *mut c_char,
    pub name: *mut c_char,
    pub reserved: *mut c_void,
}

/// rime_api.h: rime_schema_list_t
#[repr(C)]
#[derive(Default)]
pub struct RimeSchemaList {
    pub size: usize,
    pub list: *mut RimeSchemaListItem,
}

/// rime_api.h: rime_string_slice_t
#[repr(C)]
#[derive(Default)]
pub struct RimeStringSlice {
    pub str: *const c_char,
    pub length: usize,
}

/// rime_api.h: rime_module_t
#[repr(C)]
pub struct RimeModule {
    pub data_size: c_int,
    pub module_name: *const c_char,
    pub initialize: Option<unsafe extern "C" fn()>,
    pub finalize: Option<unsafe extern "C" fn()>,
    pub get_api: Option<unsafe extern "C" fn() -> *mut RimeCustomApi>,
}

/// rime_api.h: rime_custom_api_t
#[repr(C)]
#[derive(Default)]
pub struct RimeCustomApi {
    pub data_size: c_int,
}

// ---------------------------------------------------------------------------
// RimeApi
// ---------------------------------------------------------------------------

/// The version-controlled API structure. Field order MUST match rime_api.h.
/// Optional function pointers let us check availability like
/// RIME_API_AVAILABLE(api, func).
#[repr(C)]
pub struct RimeApi {
    pub data_size: c_int,

    pub setup: Option<unsafe extern "C" fn(*mut RimeTraits)>,
    pub set_notification_handler:
        Option<unsafe extern "C" fn(RimeNotificationHandler, *mut c_void)>,
    pub initialize: Option<unsafe extern "C" fn(*mut RimeTraits)>,
    pub finalize: Option<unsafe extern "C" fn()>,
    pub start_maintenance: Option<unsafe extern "C" fn(Bool) -> Bool>,
    pub is_maintenance_mode: Option<unsafe extern "C" fn() -> Bool>,
    pub join_maintenance_thread: Option<unsafe extern "C" fn()>,

    pub deployer_initialize: Option<unsafe extern "C" fn(*mut RimeTraits)>,
    pub prebuild: Option<unsafe extern "C" fn() -> Bool>,
    pub deploy: Option<unsafe extern "C" fn() -> Bool>,
    pub deploy_schema: Option<unsafe extern "C" fn(*const c_char) -> Bool>,
    pub deploy_config_file:
        Option<unsafe extern "C" fn(*const c_char, *const c_char) -> Bool>,
    pub sync_user_data: Option<unsafe extern "C" fn() -> Bool>,

    pub create_session: Option<unsafe extern "C" fn() -> RimeSessionId>,
    pub find_session: Option<unsafe extern "C" fn(RimeSessionId) -> Bool>,
    pub destroy_session: Option<unsafe extern "C" fn(RimeSessionId) -> Bool>,
    pub cleanup_stale_sessions: Option<unsafe extern "C" fn()>,
    pub cleanup_all_sessions: Option<unsafe extern "C" fn()>,

    pub process_key: Option<unsafe extern "C" fn(RimeSessionId, c_int, c_int) -> Bool>,
    pub commit_composition: Option<unsafe extern "C" fn(RimeSessionId) -> Bool>,
    pub clear_composition: Option<unsafe extern "C" fn(RimeSessionId)>,

    pub get_commit: Option<unsafe extern "C" fn(RimeSessionId, *mut RimeCommit) -> Bool>,
    pub free_commit: Option<unsafe extern "C" fn(*mut RimeCommit) -> Bool>,
    pub get_context:
        Option<unsafe extern "C" fn(RimeSessionId, *mut RimeContext) -> Bool>,
    pub free_context: Option<unsafe extern "C" fn(*mut RimeContext) -> Bool>,
    pub get_status:
        Option<unsafe extern "C" fn(RimeSessionId, *mut RimeStatus) -> Bool>,
    pub free_status: Option<unsafe extern "C" fn(*mut RimeStatus) -> Bool>,

    pub set_option: Option<unsafe extern "C" fn(RimeSessionId, *const c_char, Bool)>,
    pub get_option: Option<unsafe extern "C" fn(RimeSessionId, *const c_char) -> Bool>,
    pub set_property:
        Option<unsafe extern "C" fn(RimeSessionId, *const c_char, *const c_char)>,
    pub get_property: Option<
        unsafe extern "C" fn(RimeSessionId, *const c_char, *mut c_char, usize) -> Bool,
    >,

    pub get_schema_list: Option<unsafe extern "C" fn(*mut RimeSchemaList) -> Bool>,
    pub free_schema_list: Option<unsafe extern "C" fn(*mut RimeSchemaList)>,
    pub get_current_schema:
        Option<unsafe extern "C" fn(RimeSessionId, *mut c_char, usize) -> Bool>,
    pub select_schema: Option<unsafe extern "C" fn(RimeSessionId, *const c_char) -> Bool>,

    pub schema_open: Option<unsafe extern "C" fn(*const c_char, *mut RimeConfig) -> Bool>,
    pub config_open: Option<unsafe extern "C" fn(*const c_char, *mut RimeConfig) -> Bool>,
    pub config_close: Option<unsafe extern "C" fn(*mut RimeConfig) -> Bool>,
    pub config_get_bool:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char, *mut Bool) -> Bool>,
    pub config_get_int:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char, *mut c_int) -> Bool>,
    pub config_get_double:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char, *mut f64) -> Bool>,
    pub config_get_string: Option<
        unsafe extern "C" fn(*mut RimeConfig, *const c_char, *mut c_char, usize) -> Bool,
    >,
    pub config_get_cstring:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char) -> *const c_char>,
    pub config_update_signature:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char) -> Bool>,
    pub config_begin_map: Option<
        unsafe extern "C" fn(*mut RimeConfigIterator, *mut RimeConfig, *const c_char) -> Bool,
    >,
    pub config_next: Option<unsafe extern "C" fn(*mut RimeConfigIterator) -> Bool>,
    pub config_end: Option<unsafe extern "C" fn(*mut RimeConfigIterator)>,

    pub simulate_key_sequence:
        Option<unsafe extern "C" fn(RimeSessionId, *const c_char) -> Bool>,

    pub register_module: Option<unsafe extern "C" fn(*mut RimeModule) -> Bool>,
    pub find_module: Option<unsafe extern "C" fn(*const c_char) -> *mut RimeModule>,
    pub run_task: Option<unsafe extern "C" fn(*const c_char) -> Bool>,

    pub get_shared_data_dir: Option<unsafe extern "C" fn() -> *const c_char>,
    pub get_user_data_dir: Option<unsafe extern "C" fn() -> *const c_char>,
    pub get_sync_dir: Option<unsafe extern "C" fn() -> *const c_char>,
    pub get_user_id: Option<unsafe extern "C" fn() -> *const c_char>,
    pub get_user_data_sync_dir: Option<unsafe extern "C" fn(*mut c_char, usize)>,

    pub config_init: Option<unsafe extern "C" fn(*mut RimeConfig) -> Bool>,
    pub config_load_string:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char) -> Bool>,
    pub config_set_bool:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char, Bool) -> Bool>,
    pub config_set_int:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char, c_int) -> Bool>,
    pub config_set_double:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char, f64) -> Bool>,
    pub config_set_string: Option<
        unsafe extern "C" fn(*mut RimeConfig, *const c_char, *const c_char) -> Bool,
    >,
    pub config_get_item:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char, *mut RimeConfig) -> Bool>,
    pub config_set_item:
        Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char, *mut RimeConfig) -> Bool>,
    pub config_clear: Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char) -> Bool>,
    pub config_create_list: Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char) -> Bool>,
    pub config_create_map: Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char) -> Bool>,
    pub config_list_size: Option<unsafe extern "C" fn(*mut RimeConfig, *const c_char) -> usize>,
    pub config_begin_list: Option<
        unsafe extern "C" fn(*mut RimeConfigIterator, *mut RimeConfig, *const c_char) -> Bool,
    >,

    pub get_input: Option<unsafe extern "C" fn(RimeSessionId) -> *const c_char>,
    pub get_caret_pos: Option<unsafe extern "C" fn(RimeSessionId) -> usize>,
    pub select_candidate: Option<unsafe extern "C" fn(RimeSessionId, usize) -> Bool>,
    pub get_version: Option<unsafe extern "C" fn() -> *const c_char>,
    pub set_caret_pos: Option<unsafe extern "C" fn(RimeSessionId, usize)>,
    pub select_candidate_on_current_page:
        Option<unsafe extern "C" fn(RimeSessionId, usize) -> Bool>,

    pub candidate_list_begin:
        Option<unsafe extern "C" fn(RimeSessionId, *mut RimeCandidateListIterator) -> Bool>,
    pub candidate_list_next:
        Option<unsafe extern "C" fn(*mut RimeCandidateListIterator) -> Bool>,
    pub candidate_list_end:
        Option<unsafe extern "C" fn(*mut RimeCandidateListIterator)>,
    pub user_config_open:
        Option<unsafe extern "C" fn(*const c_char, *mut RimeConfig) -> Bool>,
    pub candidate_list_from_index: Option<
        unsafe extern "C" fn(RimeSessionId, *mut RimeCandidateListIterator, c_int) -> Bool,
    >,

    pub get_prebuilt_data_dir: Option<unsafe extern "C" fn() -> *const c_char>,
    pub get_staging_dir: Option<unsafe extern "C" fn() -> *const c_char>,
    pub commit_proto: Option<unsafe extern "C" fn(RimeSessionId, *mut c_void)>,
    pub context_proto: Option<unsafe extern "C" fn(RimeSessionId, *mut c_void)>,
    pub status_proto: Option<unsafe extern "C" fn(RimeSessionId, *mut c_void)>,
    pub get_state_label:
        Option<unsafe extern "C" fn(RimeSessionId, *const c_char, Bool) -> *const c_char>,

    pub delete_candidate: Option<unsafe extern "C" fn(RimeSessionId, usize) -> Bool>,
    pub delete_candidate_on_current_page:
        Option<unsafe extern "C" fn(RimeSessionId, usize) -> Bool>,
    pub get_state_label_abbreviated: Option<
        unsafe extern "C" fn(RimeSessionId, *const c_char, Bool, Bool) -> RimeStringSlice,
    >,
    pub set_input: Option<unsafe extern "C" fn(RimeSessionId, *const c_char) -> Bool>,

    pub get_shared_data_dir_s: Option<unsafe extern "C" fn(*mut c_char, usize)>,
    pub get_user_data_dir_s: Option<unsafe extern "C" fn(*mut c_char, usize)>,
    pub get_prebuilt_data_dir_s: Option<unsafe extern "C" fn(*mut c_char, usize)>,
    pub get_staging_dir_s: Option<unsafe extern "C" fn(*mut c_char, usize)>,
    pub get_sync_dir_s: Option<unsafe extern "C" fn(*mut c_char, usize)>,

    pub highlight_candidate: Option<unsafe extern "C" fn(RimeSessionId, usize) -> Bool>,
    pub highlight_candidate_on_current_page:
        Option<unsafe extern "C" fn(RimeSessionId, usize) -> Bool>,
    pub change_page: Option<unsafe extern "C" fn(RimeSessionId, Bool) -> Bool>,
    pub get_candidate_preview:
        Option<unsafe extern "C" fn(RimeSessionId, *mut RimeCandidatePreview) -> Bool>,
    pub free_candidate_preview:
        Option<unsafe extern "C" fn(*mut RimeCandidatePreview) -> Bool>,
}

// ---------------------------------------------------------------------------
// Dynamic loading
// ---------------------------------------------------------------------------

/// Dynamically load librime and acquire the RimeApi struct.
pub struct RimeLibrary {
    // Kept alive for the process lifetime of the binding (drop guard).
    #[allow(dead_code)]
    lib: libloading::Library,
    pub api: *const RimeApi,
}

impl RimeLibrary {
    /// Load rime.dll / librime.so / librime.dylib from the given path.
    ///
    /// # Safety
    /// The library must be a librime build exposing rime_get_api().
    pub unsafe fn load(path: &Path) -> Result<Self, String> {
        let lib = libloading::Library::new(path)
            .map_err(|e| format!("cannot load {}: {e}", path.display()))?;
        let get_api: libloading::Symbol<RimeGetApi> = lib
            .get(b"rime_get_api")
            .map_err(|e| format!("rime_get_api not found in {}: {e}", path.display()))?;
        let api = get_api();
        if api.is_null() {
            return Err("rime_get_api() returned null".into());
        }
        Ok(RimeLibrary { lib, api })
    }

    pub fn api(&self) -> &RimeApi {
        // SAFETY: rime_get_api returns a pointer to a static, process-lifetime
        // API struct; the library is kept alive by self.lib.
        unsafe { &*self.api }
    }
}

// The API struct is plain data; sessions must be driven from a single thread
// (documented librime constraint), which the caller enforces.
unsafe impl Send for RimeLibrary {}

#[cfg(test)]
mod tests {
    use super::*;

    // data_size must equal the versioned portion (whole struct minus the
    // leading data_size int), matching RIME_STRUCT_INIT semantics.
    fn versioned_size<T>() -> c_int {
        (std::mem::size_of::<T>() - std::mem::size_of::<c_int>()) as c_int
    }

    #[test]
    fn versioned_structs_set_data_size() {
        assert_eq!(RimeTraits::default().data_size, versioned_size::<RimeTraits>());
        assert_eq!(RimeCommit::default().data_size, versioned_size::<RimeCommit>());
        assert_eq!(RimeContext::default().data_size, versioned_size::<RimeContext>());
        assert_eq!(RimeStatus::default().data_size, versioned_size::<RimeStatus>());
        assert_eq!(
            RimeCandidatePreview::default().data_size,
            versioned_size::<RimeCandidatePreview>(),
        );
    }

    #[test]
    fn rime_api_data_size_is_positive() {
        // sanity: the API struct is not empty
        assert!(std::mem::size_of::<RimeApi>() > 0);
    }
}

