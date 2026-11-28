//! The PC's one build budget: a named semaphore that every Cargo and rustc takes its tokens from, as a make-style
//! jobserver (`CARGO_MAKEFLAGS=-j --jobserver-auth=<name>`). It lives while one holder keeps it open; the first holder
//! sets how many tokens it has.
use std::io;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Threading::CreateSemaphoreW;

/// An open handle to the budget; dropping the last one anywhere on the PC ends it.
pub struct BuildBudget(HANDLE);

// SAFETY: a semaphore handle may be used and closed from any thread.
unsafe impl Send for BuildBudget {}
// SAFETY: nothing reads or writes through the handle after it is opened; it is only kept and closed.
unsafe impl Sync for BuildBudget {}

impl BuildBudget {
    /// Opens the budget `name`, creating it with `tokens` free tokens when no one holds it yet.
    pub fn open(name: &str, tokens: i32) -> io::Result<Self> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: the name is a NUL-terminated wide string that outlives the call; no security attributes means the default.
        let handle = unsafe { CreateSemaphoreW(std::ptr::null(), tokens, tokens, wide.as_ptr()) };
        if handle.is_null() { Err(io::Error::last_os_error()) } else { Ok(Self(handle)) }
    }
}

impl Drop for BuildBudget {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateSemaphoreW and is closed once, here.
        unsafe { CloseHandle(self.0) };
    }
}
