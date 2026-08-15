//! 9IME TSF text service (M2: minimal runnable).
//!
//! COM server exports: DllGetClassObject / DllCanUnloadNow /
//! DllRegisterServer / DllUnregisterServer. Register with regsvr32 (admin).

mod engine;
mod factory;
mod register;
mod service;

use std::sync::atomic::{AtomicI64, Ordering};
use windows::core::{Interface, GUID, HRESULT};
use windows::Win32::System::Com::IClassFactory;

/// CLSID of the 9IME text service.
pub const CLSID_NINEIME: GUID = GUID::from_u128(0x39494d45_9eed_4a4d_9e45_000000000009);
/// GUID of the zh-CN language profile (registry / TSF profile).
pub const PROFILE_NINEIME: GUID = GUID::from_u128(0x39494d45_0001_4a4d_9e45_000000000009);

static LOCK_COUNT: AtomicI64 = AtomicI64::new(0);

pub(crate) fn lock_server(lock: bool) {
    if lock {
        LOCK_COUNT.fetch_add(1, Ordering::SeqCst);
    } else {
        LOCK_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
}

fn s_ok() -> HRESULT {
    HRESULT::from_win32(0)
}

fn s_false() -> HRESULT {
    HRESULT::from_win32(1)
}

fn e_pointer() -> HRESULT {
    HRESULT::from_win32(0x80004003)
}

fn e_nointerface() -> HRESULT {
    HRESULT::from_win32(0x80004002)
}

/// Standard COM server entry: hand out the IClassFactory for CLSID_NINEIME.
#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut core::ffi::c_void,
) -> HRESULT {
    unsafe {
        if rclsid.is_null() || riid.is_null() || ppv.is_null() {
            return e_pointer();
        }
        if *rclsid != CLSID_NINEIME {
            return e_nointerface();
        }
        let factory: IClassFactory = factory::NineImeFactory.into();
        let vtbl: &windows::Win32::System::Com::IClassFactory_Vtbl =
            <IClassFactory as Interface>::vtable(&factory);
        let raw = <IClassFactory as Interface>::as_raw(&factory);
        (vtbl.base__.QueryInterface)(raw, riid, ppv)
    }
}

/// Standard COM server entry: report whether the DLL can be unloaded.
#[no_mangle]
pub unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    if LOCK_COUNT.load(Ordering::SeqCst) == 0 {
        s_ok()
    } else {
        s_false()
    }
}

/// regsvr32 support: write CLSID + TSF TIP registration.
#[no_mangle]
pub unsafe extern "system" fn DllRegisterServer() -> HRESULT {
    match unsafe { register::register() } {
        Ok(()) => s_ok(),
        Err(e) => e.code(),
    }
}

/// regsvr32 /u support: remove registration.
#[no_mangle]
pub unsafe extern "system" fn DllUnregisterServer() -> HRESULT {
    match unsafe { register::unregister() } {
        Ok(()) => s_ok(),
        Err(e) => e.code(),
    }
}
