//! Registry registration (regsvr32): CLSID + TSF TIP language profile.

use windows::core::{GUID, HRESULT, PCWSTR};
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY,
    HKEY_CLASSES_ROOT, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, REG_DWORD,
    REG_OPTION_NON_VOLATILE, REG_SZ, REG_VALUE_TYPE,
};

use crate::{CLSID_NINEIME, PROFILE_NINEIME};

const TIP_ROOT: &str = "SOFTWARE\\Microsoft\\CTF\\TIP";
const LANG_ZH_CN: &str = "0x00000804";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn u16_bytes(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|c| c.to_le_bytes()).collect()
}

fn pcwstr(v: &[u16]) -> PCWSTR {
    PCWSTR(v.as_ptr())
}

fn ok(err: WIN32_ERROR) -> windows::core::Result<()> {
    if err.0 == 0 {
        Ok(())
    } else {
        Err(windows::core::Error::from_hresult(HRESULT::from_win32(err.0)))
    }
}

fn guid_string(g: &GUID) -> String {
    let d = g.data4;
    format!("{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1, g.data2, g.data3,
        d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7])
}

fn create_key(parent: HKEY, sub: &str) -> windows::core::Result<HKEY> {
    let sub_w = wide(sub);
    let mut key = HKEY(std::ptr::null_mut());
    let err = unsafe {
        RegCreateKeyExW(
            parent,
            pcwstr(&sub_w),
            Some(0),
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS,
            None,
            &mut key,
            None,
        )
    };
    ok(err)?;
    Ok(key)
}

fn set_value(hkey: HKEY, name: &str, data: &[u8], ty: REG_VALUE_TYPE) -> windows::core::Result<()> {
    let name_w = wide(name);
    ok(unsafe {
        RegSetValueExW(hkey, pcwstr(&name_w), None, ty, Some(data))
    })
}

fn set_string(hkey: HKEY, name: &str, value: &str) -> windows::core::Result<()> {
    let v = wide(value);
    set_value(hkey, name, &u16_bytes(&v), REG_SZ)
}

fn set_dword(hkey: HKEY, name: &str, value: u32) -> windows::core::Result<()> {
    set_value(hkey, name, &value.to_le_bytes(), REG_DWORD)
}

fn dll_path() -> Vec<u16> {
    let mut buf = vec![0u16; 2048];
    let n = unsafe { GetModuleFileNameW(None, &mut buf) };
    buf.truncate(n as usize);
    buf
}

/// Write CLSID InprocServer32 + TSF TIP language profile.
pub unsafe fn register() -> windows::core::Result<()> {
    let clsid = guid_string(&CLSID_NINEIME);
    let profile = guid_string(&PROFILE_NINEIME);

    // HKCR\CLSID\{clsid}
    let clsid_key = create_key(HKEY_CLASSES_ROOT, &format!("CLSID\\{clsid}"))?;
    set_string(clsid_key, "", "9IME Text Service")?;
    let inproc = create_key(clsid_key, "InprocServer32")?;
    let path_w = dll_path();
    set_value(inproc, "", &u16_bytes(&path_w), REG_SZ)?;
    set_string(inproc, "ThreadingModel", "Apartment")?;
    let _ = unsafe { RegCloseKey(inproc) };
    let _ = unsafe { RegCloseKey(clsid_key) };

    // HKLM\SOFTWARE\Microsoft\CTF\TIP\{clsid}\LanguageProfile\0x00000804\{profile}
    let tip = create_key(HKEY_LOCAL_MACHINE, &format!("{TIP_ROOT}\\{clsid}"))?;
    let lang = create_key(tip, &format!("LanguageProfile\\{LANG_ZH_CN}\\{profile}"))?;
    set_string(lang, "", "9IME")?;
    set_dword(lang, "Enable", 1)?;
    let _ = unsafe { RegCloseKey(lang) };
    let _ = unsafe { RegCloseKey(tip) };
    Ok(())
}

/// Remove CLSID + TIP registration.
pub unsafe fn unregister() -> windows::core::Result<()> {
    let clsid = guid_string(&CLSID_NINEIME);
    let clsid_w = wide(&clsid);
    let tip_w = wide(&format!("{TIP_ROOT}\\{clsid}"));
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CLASSES_ROOT, pcwstr(&clsid_w));
        let _ = RegDeleteTreeW(HKEY_LOCAL_MACHINE, pcwstr(&tip_w));
    }
    Ok(())
}
