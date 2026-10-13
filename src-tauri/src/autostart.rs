//! Start with Windows (opt-in autostart).
//!
//! Registers Faber for the current user only in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//! Never touches HKLM, never requires admin privileges.
//! When launched with `--autostart`, Faber starts hidden in the tray without opening a window.
#![allow(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};

pub const AUTOSTART_FLAG: &str = "--autostart";

pub trait AutostartKeeper: Send + Sync {
    fn register(&self) -> Result<(), String>;
    fn unregister(&self) -> Result<(), String>;
    #[allow(dead_code)]
    fn is_registered(&self) -> Result<bool, String>;
}

/// Fake autostart implementation for tests and sandbox runs.
/// Never touches the real Windows registry.
pub struct FakeAutostart {
    registered: AtomicBool,
}

impl FakeAutostart {
    pub fn new(registered: bool) -> Self {
        Self { registered: AtomicBool::new(registered) }
    }
}

impl Default for FakeAutostart {
    fn default() -> Self {
        Self::new(false)
    }
}

impl AutostartKeeper for FakeAutostart {
    fn register(&self) -> Result<(), String> {
        self.registered.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn unregister(&self) -> Result<(), String> {
        self.registered.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_registered(&self) -> Result<bool, String> {
        Ok(self.registered.load(Ordering::SeqCst))
    }
}

#[allow(dead_code)]
pub struct NoopAutostart;

impl AutostartKeeper for NoopAutostart {
    fn register(&self) -> Result<(), String> {
        Ok(())
    }

    fn unregister(&self) -> Result<(), String> {
        Ok(())
    }

    fn is_registered(&self) -> Result<bool, String> {
        Ok(false)
    }
}

#[cfg(windows)]
pub struct WindowsAutostart;

#[cfg(windows)]
impl WindowsAutostart {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(windows)]
impl Default for WindowsAutostart {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
impl AutostartKeeper for WindowsAutostart {
    fn register(&self) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| format!("cannot determine current executable path: {e}"))?;
        let cmd = format!("\"{}\" {AUTOSTART_FLAG}", exe.display());
        set_run_value(&cmd)
    }

    fn unregister(&self) -> Result<(), String> {
        delete_run_value()
    }

    fn is_registered(&self) -> Result<bool, String> {
        has_run_value()
    }
}

#[cfg(windows)]
const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
#[cfg(windows)]
const VALUE_NAME: &str = "Faber";

#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn set_run_value(value: &str) -> Result<(), String> {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegSetKeyValueW(
            hKey: *mut std::ffi::c_void,
            lpSubKey: *const u16,
            lpValueName: *const u16,
            dwType: u32,
            lpData: *const std::ffi::c_void,
            cbData: u32,
        ) -> i32;
    }
    const HKEY_CURRENT_USER: isize = -2147483647; // 0x80000001
    const REG_SZ: u32 = 1;

    let subkey_w = to_wide(RUN_SUBKEY);
    let val_name_w = to_wide(VALUE_NAME);
    let val_data_w = to_wide(value);
    let cb_data = (val_data_w.len() * std::mem::size_of::<u16>()) as u32;

    // SAFETY: Valid wide null-terminated strings and buffer length.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER as *mut std::ffi::c_void,
            subkey_w.as_ptr(),
            val_name_w.as_ptr(),
            REG_SZ,
            val_data_w.as_ptr() as *const std::ffi::c_void,
            cb_data,
        )
    };
    if status == 0 { Ok(()) } else { Err(format!("RegSetKeyValueW failed with status {status}")) }
}

#[cfg(windows)]
fn delete_run_value() -> Result<(), String> {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegDeleteKeyValueW(hKey: *mut std::ffi::c_void, lpSubKey: *const u16, lpValueName: *const u16) -> i32;
    }
    const HKEY_CURRENT_USER: isize = -2147483647; // 0x80000001
    const ERROR_FILE_NOT_FOUND: i32 = 2;

    let subkey_w = to_wide(RUN_SUBKEY);
    let val_name_w = to_wide(VALUE_NAME);

    // SAFETY: Valid wide null-terminated strings.
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER as *mut std::ffi::c_void, subkey_w.as_ptr(), val_name_w.as_ptr()) };
    if status == 0 || status == ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(format!("RegDeleteKeyValueW failed with status {status}")) }
}

#[cfg(windows)]
#[allow(dead_code)]
fn has_run_value() -> Result<bool, String> {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            hKey: *mut std::ffi::c_void,
            lpSubKey: *const u16,
            lpValueName: *const u16,
            dwFlags: u32,
            pdwType: *mut u32,
            pvData: *mut std::ffi::c_void,
            pcbData: *mut u32,
        ) -> i32;
    }
    const HKEY_CURRENT_USER: isize = -2147483647; // 0x80000001
    const RRF_RT_REG_SZ: u32 = 0x00000002;
    const ERROR_FILE_NOT_FOUND: i32 = 2;

    let subkey_w = to_wide(RUN_SUBKEY);
    let val_name_w = to_wide(VALUE_NAME);
    let mut cb_data = 0u32;

    // SAFETY: Querying length only with null data pointer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER as *mut std::ffi::c_void,
            subkey_w.as_ptr(),
            val_name_w.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut cb_data,
        )
    };
    if status == 0 {
        Ok(true)
    } else if status == ERROR_FILE_NOT_FOUND {
        Ok(false)
    } else {
        Err(format!("RegGetValueW failed with status {status}"))
    }
}

/// Checks whether `--autostart` is in the arguments.
pub fn is_autostart_arg(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|a| a == AUTOSTART_FLAG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_autostart_records_state_without_touching_registry() {
        let fake = FakeAutostart::default();
        assert_eq!(fake.is_registered(), Ok(false));

        assert_eq!(fake.register(), Ok(()));
        assert_eq!(fake.is_registered(), Ok(true));

        assert_eq!(fake.unregister(), Ok(()));
        assert_eq!(fake.is_registered(), Ok(false));
    }

    #[test]
    fn autostart_flag_detection() {
        assert!(is_autostart_arg(vec!["faber.exe".into(), "--autostart".into()]));
        assert!(!is_autostart_arg(vec!["faber.exe".into(), "--other".into()]));
        assert!(!is_autostart_arg(vec![]));
    }
}
