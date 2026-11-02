//! Creates a file whose access list names only the current user.
use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::path::Path;
use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL};

pub fn write_user_only(path: &Path, bytes: &[u8]) -> io::Result<()> {
    crate::pipe::with_user_security(|attributes| {
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: the name and descriptor stay valid during the call, and the returned handle is owned by File.
        let handle =
            unsafe { CreateFileW(name.as_ptr(), GENERIC_WRITE, 0, attributes, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, std::ptr::null_mut()) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut file = unsafe { std::fs::File::from_raw_handle(handle) };
        file.write_all(bytes)?;
        file.sync_all()
    })
}
