//! TSF registration (regsvr32): CLSID + TIP profile + categories.
//!
//! Mirrors the weasel deployer: the TIP profile is registered with
//! ITfInputProcessorProfileMgr::RegisterProfile and the keyboard category
//! with ITfCategoryMgr::RegisterCategory; without these the service does
//! not show up in Win+Space / language settings.

use windows::core::{GUID, PCWSTR};
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY,
    HKEY_CLASSES_ROOT, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS,
    REG_OPTION_NON_VOLATILE, REG_SZ, REG_VALUE_TYPE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
use windows::Win32::UI::TextServices::{
    CLSID_TF_CategoryMgr, CLSID_TF_InputProcessorProfiles,
    GUID_TFCAT_CATEGORY_OF_TIP, GUID_TFCAT_TIPCAP_COMLESS,
    GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT, GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT,
    GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT, GUID_TFCAT_TIP_KEYBOARD, ITfCategoryMgr,
    ITfInputProcessorProfileMgr,
};

use crate::{CLSID_NINEIME, PROFILE_NINEIME};

const TIP_ROOT: &str = "SOFTWARE\\Microsoft\\CTF\\TIP";

const CATEGORIES: [GUID; 6] = [
    GUID_TFCAT_CATEGORY_OF_TIP,
    GUID_TFCAT_TIP_KEYBOARD,
    GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT,
    GUID_TFCAT_TIPCAP_COMLESS,
    GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
    GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
];

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
        Err(windows::core::Error::from_hresult(
            windows::core::HRESULT::from_win32(err.0),
        ))
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

fn set_value(
    hkey: HKEY,
    name: &str,
    data: &[u8],
    ty: REG_VALUE_TYPE,
) -> windows::core::Result<()> {
    let name_w = wide(name);
    ok(unsafe {
        RegSetValueExW(hkey, pcwstr(&name_w), None, ty, Some(data))
    })
}

fn set_string(hkey: HKEY, name: &str, value: &str) -> windows::core::Result<()> {
    let v = wide(value);
    set_value(hkey, name, &u16_bytes(&v), REG_SZ)
}

fn dll_path() -> Vec<u16> {
    let mut buf = vec![0u16; 2048];
    let n = unsafe { GetModuleFileNameW(None, &mut buf) };
    buf.truncate(n as usize);
    buf
}

/// Write CLSID InprocServer32 + register TIP profile and categories.
pub unsafe fn register() -> windows::core::Result<()> {
    let clsid = guid_string(&CLSID_NINEIME);

    // 1. COM server registration (lets the OS load the DLL)
    let clsid_key = create_key(HKEY_CLASSES_ROOT, &format!("CLSID\\{clsid}"))?;
    set_string(clsid_key, "", "9IME Text Service")?;
    let inproc = create_key(clsid_key, "InprocServer32")?;
    let path_w = dll_path();
    set_value(inproc, "", &u16_bytes(&path_w), REG_SZ)?;
    set_string(inproc, "ThreadingModel", "Apartment")?;
    let _ = unsafe { RegCloseKey(inproc) };
    let _ = unsafe { RegCloseKey(clsid_key) };

    // 2. TIP language profile (Simplified Chinese, 0x0804), enabled by default
    let profiles: ITfInputProcessorProfileMgr = CoCreateInstance(
        &CLSID_TF_InputProcessorProfiles,
        None,
        CLSCTX_INPROC_SERVER,
    )?;
    let desc: Vec<u16> = "9IME".encode_utf16().collect();
    let icon = dll_path();
    unsafe {
        profiles.RegisterProfile(
            &CLSID_NINEIME,
            0x0804,
            &PROFILE_NINEIME,
            &desc,
            &icon,
            0,
            HKL(std::ptr::null_mut()),
            0,
            true,
            0,
        )
    }?;

    // 3. categories (keyboard TIP + capability flags)
    let catmgr: ITfCategoryMgr =
        CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)?;
    for cat in CATEGORIES {
        unsafe { catmgr.RegisterCategory(&CLSID_NINEIME, &cat, &CLSID_NINEIME) }?;
    }
    Ok(())
}

/// Remove profile, categories, and raw registry keys.
pub unsafe fn unregister() -> windows::core::Result<()> {
    if let Ok(profiles) = CoCreateInstance::<_, ITfInputProcessorProfileMgr>(
        &CLSID_TF_InputProcessorProfiles,
        None,
        CLSCTX_INPROC_SERVER,
    ) {
        let _ = unsafe { profiles.UnregisterProfile(&CLSID_NINEIME, 0x0804, &PROFILE_NINEIME, 0) };
    }
    if let Ok(catmgr) = CoCreateInstance::<_, ITfCategoryMgr>(
        &CLSID_TF_CategoryMgr,
        None,
        CLSCTX_INPROC_SERVER,
    ) {
        for cat in CATEGORIES {
            let _ = unsafe { catmgr.UnregisterCategory(&CLSID_NINEIME, &cat, &CLSID_NINEIME) };
        }
    }
    let clsid = guid_string(&CLSID_NINEIME);
    let clsid_w = wide(&clsid);
    let _ = unsafe { RegDeleteTreeW(HKEY_CLASSES_ROOT, pcwstr(&clsid_w)) };
    let tip_w = wide(&format!("{TIP_ROOT}\\{clsid}"));
    let _ = unsafe { RegDeleteTreeW(HKEY_LOCAL_MACHINE, pcwstr(&tip_w)) };
    Ok(())
}
