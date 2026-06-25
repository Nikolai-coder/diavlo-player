//! Windows registry file association helpers.
//!
//! Registers DIAVLO PLAYER as a handler for audio file types so the user
//! can select it in Windows Settings > Default Apps.
//!
//! Windows 10+ does not allow programmatic default-app overrides.
//! This module registers the progid + extensions, then opens the
//! Settings page where the user can click "Set default".

#![allow(non_snake_case)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

type HKEY = isize;
type LSTATUS = i32;
type DWORD = u32;

const HKEY_CURRENT_USER: HKEY = -0x7FFFFFFF - 1;
const KEY_WRITE: i32 = 0x00020000 | 0x00000002;
const KEY_READ: i32 = 0x00020000 | 0x00000001;
const REG_SZ: DWORD = 1;
const ERROR_SUCCESS: LSTATUS = 0;

extern "system" {
    fn RegCreateKeyExW(
        hKey: HKEY,
        lpSubKey: *const u16,
        Reserved: DWORD,
        lpClass: *const u16,
        dwOptions: DWORD,
        samDesired: i32,
        lpSecurityAttributes: *const u8,
        phkResult: *mut HKEY,
        lpdwDisposition: *mut DWORD,
    ) -> LSTATUS;

    fn RegSetValueExW(
        hKey: HKEY,
        lpValueName: *const u16,
        Reserved: DWORD,
        dwType: DWORD,
        lpData: *const u8,
        cbData: DWORD,
    ) -> LSTATUS;

    fn RegCloseKey(hKey: HKEY) -> LSTATUS;

    fn ShellExecuteW(
        hwnd: isize,
        lpOperation: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShowCmd: i32,
    ) -> isize;
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn reg_set_sz(hkey: HKEY, name: &str, value: &str) -> bool {
    let wn = to_wide(name);
    let wv: Vec<u16> = OsStr::new(value).encode_wide().chain(std::iter::once(0)).collect();
    let data = wv.as_ptr() as *const u8;
    let len = (wv.len() * 2) as DWORD;
    unsafe { RegSetValueExW(hkey, wn.as_ptr(), 0, REG_SZ, data, len) == ERROR_SUCCESS }
}

fn reg_create(path: &str) -> Option<(HKEY, DWORD)> {
    let wp = to_wide(path);
    let mut hkey: HKEY = 0;
    let mut disp: DWORD = 0;
    let ok = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wp.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_WRITE,
            std::ptr::null(),
            &mut hkey,
            &mut disp,
        ) == ERROR_SUCCESS
    };
    if ok && hkey != 0 { Some((hkey, disp)) } else { None }
}

/// Register the `diavlo-player.Audio` progid and all supported file extensions.
/// Call this from an installer or from the app (elevated not required for HKCU).
pub unsafe fn register_file_associations() -> Result<(), String> {
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("Cannot get exe path: {e}"))?;
    let exe_str = exe_path.to_string_lossy().to_string();

    let progid = r"Software\Classes\diavlo-player.Audio";

    // ── ProgId: friendly name ──────────────────────────
    let (hk, _) = reg_create(progid)
        .ok_or("Failed to create progid key")?;
    if !reg_set_sz(hk, "", "DIAVLO PLAYER Audio File") {
        unsafe { RegCloseKey(hk); }
        return Err("Failed to set progid default value".into());
    }
    unsafe { RegCloseKey(hk); }

    // ── DefaultIcon ─────────────────────────────────────
    let (hk, _) = reg_create(&format!("{progid}\\DefaultIcon"))
        .ok_or("Failed to create DefaultIcon key")?;
    reg_set_sz(hk, "", &format!("\"{exe_str}\",0"));
    unsafe { RegCloseKey(hk); }

    // ── Shell > open > command ──────────────────────────
    let (hk, _) = reg_create(&format!("{progid}\\shell\\open\\command"))
        .ok_or("Failed to create shell command key")?;
    reg_set_sz(hk, "", &format!("\"{exe_str}\" \"%1\""));
    unsafe { RegCloseKey(hk); }

    // ── File extensions ─────────────────────────────────
    let extensions = ["wav", "mp3", "flac", "ogg", "m4a", "aiff", "aac", "opus"];

    for ext in &extensions {
        // .ext -> diavlo-player.Audio
        let (hk, _) = reg_create(&format!("Software\\Classes\\.{ext}\\OpenWithProgids"))
            .ok_or_else(|| format!("Failed to create .{ext} key"))?;
        reg_set_sz(hk, "diavlo-player.Audio", ""); // empty value = registered
        unsafe { RegCloseKey(hk); }

        // Also set the direct ProgID hint on the extension
        let (hk, _) = reg_create(&format!("Software\\Classes\\.{ext}"))
            .ok_or_else(|| format!("Failed to create .{ext} root key"))?;
        // Only set the default if not already set — don't overwrite existing associations
        // We use OpenWithProgids above which is the polite way
        unsafe { RegCloseKey(hk); }
    }

    // ── RegisteredApplications ──────────────────────────
    let (hk, _) = reg_create(r"Software\RegisteredApplications")
        .ok_or("Failed to create RegisteredApplications key")?;
    let cap_path = format!("Software\\Classes\\diavlo-player.Audio\\Capabilities");
    reg_set_sz(hk, "diavlo-player", &cap_path);
    unsafe { RegCloseKey(hk); }

    // ── Capabilities ────────────────────────────────────
    let (hk, _) = reg_create(&cap_path)
        .ok_or("Failed to create Capabilities key")?;
    reg_set_sz(hk, "ApplicationName", "DIAVLO PLAYER");
    reg_set_sz(hk, "ApplicationDescription", "A modern glass-style music player (WAV, FLAC, MP3, AAC, OGG, AIFF)");
    unsafe { RegCloseKey(hk); }

    // FileAssociations subkey
    let (hk, _) = reg_create(&format!("{cap_path}\\FileAssociations"))
        .ok_or("Failed to create FileAssociations key")?;
    for ext in &extensions {
        reg_set_sz(hk, &format!(".{ext}"), "diavlo-player.Audio");
    }
    unsafe { RegCloseKey(hk); }

    Ok(())
}

/// Open Windows Settings > Default Apps so the user can make DIAVLO PLAYER the default.
pub unsafe fn open_default_apps_settings() {
    let uri = to_wide("ms-settings:defaultapps");
    let verb = to_wide("open");
    ShellExecuteW(0, verb.as_ptr(), uri.as_ptr(), std::ptr::null(), std::ptr::null(), 1);
}

/// Check if file associations are already registered.
pub unsafe fn associations_registered() -> bool {
    let mut hkey: HKEY = 0;
    let path = to_wide(r"Software\Classes\diavlo-player.Audio");
    let ok = RegCreateKeyExW(
        HKEY_CURRENT_USER,
        path.as_ptr(),
        0,
        std::ptr::null(),
        0,
        KEY_READ,
        std::ptr::null(),
        &mut hkey,
        std::ptr::null_mut(),
    ) == ERROR_SUCCESS;
    if hkey != 0 { RegCloseKey(hkey); }
    ok
}
