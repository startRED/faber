//! Start with Windows (opt-in autostart).
//!
//! Registers Faber for the current user only in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//! Never touches HKLM, never requires admin privileges.
//! Faber starts in the tray.
#![allow(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};

pub const AUTOSTART_FLAG: &str = "--autostart";

pub trait AutostartKeeper: Send + Sync {
    fn register(&self) -> Result<(), String>;
    fn unregister(&self) -> Result<(), String>;
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
impl AutostartKeeper for WindowsAutostart {
    fn register(&self) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| format!("cannot determine current executable path: {e}"))?;
        set_run_value(&format!("\"{}\" {AUTOSTART_FLAG}", exe.display()))
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

pub fn creates_window(args: impl IntoIterator<Item = String>) -> bool {
    !is_autostart_arg(args)
}

/// Changes the registration and settings together; tests supply both operations.
/// What the switch shows: whether Windows will really start Faber. The key can go without Faber (a cleanup tool, another
/// account tool), and the setting alone would then still say on. A key that cannot be read leaves the setting as saved.
pub fn shown(keeper: &dyn AutostartKeeper, saved: bool) -> bool {
    keeper.is_registered().unwrap_or(saved)
}

pub fn save<T>(keeper: &dyn AutostartKeeper, enabled: Option<bool>, persist: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let Some(enabled) = enabled else { return persist() };
    let failed = |error| {
        eprintln!("faber: autostart registration failed: {error}");
        "error.settings.autostartFailed".to_string()
    };
    let before = keeper.is_registered().map_err(failed)?;
    let apply = |on| if on { keeper.register() } else { keeper.unregister() };
    apply(enabled).map_err(failed)?;
    match persist() {
        Ok(saved) => Ok(saved),
        Err(error) => {
            apply(before).map_err(failed)?;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Refused;
    impl AutostartKeeper for Refused {
        fn register(&self) -> Result<(), String> {
            Err("access denied".into())
        }
        fn unregister(&self) -> Result<(), String> {
            Err("access denied".into())
        }
        fn is_registered(&self) -> Result<bool, String> {
            Ok(false)
        }
    }

    #[test]
    fn registry_failure_refuses_the_save() {
        for enabled in [false, true] {
            let mut persisted = false;
            let result = save(&Refused, Some(enabled), || {
                persisted = true;
                Ok(())
            });
            assert_eq!(result, Err("error.settings.autostartFailed".into()));
            assert!(!persisted);
        }
    }

    #[test]
    fn settings_failure_restores_the_previous_registration() {
        for before in [false, true] {
            let keeper = FakeAutostart::new(before);
            assert_eq!(
                save(&keeper, Some(!before), || Err::<(), _>("error.settings.saveFailed".into())),
                Err("error.settings.saveFailed".into())
            );
            assert_eq!(keeper.is_registered(), Ok(before));
        }
    }

    #[test]
    fn the_switch_shows_the_run_key_and_the_setting_only_when_the_key_cannot_be_read() {
        assert!(!shown(&FakeAutostart::new(false), true), "a key removed outside Faber turns the switch off");
        assert!(shown(&FakeAutostart::new(true), false));
        struct Unreadable;
        impl AutostartKeeper for Unreadable {
            fn register(&self) -> Result<(), String> {
                Ok(())
            }
            fn unregister(&self) -> Result<(), String> {
                Ok(())
            }
            fn is_registered(&self) -> Result<bool, String> {
                Err("access denied".into())
            }
        }
        assert!(shown(&Unreadable, true));
        assert!(!shown(&Unreadable, false));
    }

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

    #[test]
    fn autostart_never_creates_a_window_even_in_hidden_tests() {
        assert!(!creates_window(vec!["faber.exe".into(), AUTOSTART_FLAG.into()]));
        assert!(creates_window(vec!["faber.exe".into()]));
    }
}
