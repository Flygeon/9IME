use windows::core::{implement, Interface, Ref, GUID, Result, BOOL};
use windows::Win32::Foundation::E_NOINTERFACE;
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};

use crate::service::TextService;

/// COM class factory for the 9IME text service.
#[implement(IClassFactory)]
pub struct NineImeFactory;

impl IClassFactory_Impl for NineImeFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<windows::core::IUnknown>,
        riid: *const GUID,
        ppv: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        // Aggregation is not supported.
        if punkouter.cloned().is_some() {
            return Err(E_NOINTERFACE.into());
        }
        let unknown: windows::core::IUnknown = TextService::new().into();
        // SAFETY: riid/ppv were validated by the caller (COM).
        unsafe { unknown.query(riid, ppv) }.ok()
    }

    fn LockServer(&self, flock: BOOL) -> Result<()> {
        crate::lock_server(flock.as_bool());
        Ok(())
    }
}
