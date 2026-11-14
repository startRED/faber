//! One-shot available video-memory reading; callers cache it for the engine's lifetime.

#[cfg(windows)]
pub fn available_vram_mb() -> Option<u64> {
    use std::ffi::c_void;
    #[repr(C)]
    struct Guid {
        a: u32,
        b: u16,
        c: u16,
        d: [u8; 8],
    }
    #[repr(C)]
    #[derive(Default)]
    struct Memory {
        budget: u64,
        used: u64,
        reservation_available: u64,
        reservation: u64,
    }
    #[repr(C)]
    struct Description {
        name: [u16; 128],
        vendor: u32,
        device: u32,
        subsystem: u32,
        revision: u32,
        video: usize,
        system: usize,
        shared: usize,
        luid: i64,
        flags: u32,
    }
    #[link(name = "dxgi")]
    unsafe extern "system" {
        fn CreateDXGIFactory1(iid: *const Guid, factory: *mut *mut c_void) -> i32;
    }
    struct Com(*mut c_void);
    impl Com {
        unsafe fn method(&self, index: usize) -> *const c_void {
            // SAFETY: each object is a live DXGI interface, and callers use its documented vtable index.
            unsafe { *(*(self.0 as *const *const *const c_void)).add(index) }
        }
    }
    impl Drop for Com {
        fn drop(&mut self) {
            // SAFETY: every successful creation/query owns exactly one reference.
            unsafe {
                let release: unsafe extern "system" fn(*mut c_void) -> u32 = std::mem::transmute(self.method(2));
                release(self.0);
            }
        }
    }
    const FACTORY: Guid = Guid { a: 0x770aae78, b: 0xf26f, c: 0x4dba, d: [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87] };
    const ADAPTER3: Guid = Guid { a: 0x645967a4, b: 0x1392, c: 0x4310, d: [0xa7, 0x98, 0x80, 0x53, 0xce, 0x3e, 0x93, 0xfd] };
    // SAFETY: documented COM signatures, checked HRESULTs, initialized outputs and RAII references.
    unsafe {
        let mut raw = std::ptr::null_mut();
        if CreateDXGIFactory1(&FACTORY, &mut raw) < 0 || raw.is_null() {
            return None;
        }
        let factory = Com(raw);
        let enumerate: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32 = std::mem::transmute(factory.method(12));
        let mut best = None;
        for index in 0..32 {
            let mut raw = std::ptr::null_mut();
            if enumerate(factory.0, index, &mut raw) < 0 || raw.is_null() {
                break;
            }
            let adapter = Com(raw);
            let describe: unsafe extern "system" fn(*mut c_void, *mut Description) -> i32 = std::mem::transmute(adapter.method(10));
            let mut description: Description = std::mem::zeroed();
            // The curated GPU wheels use CUDA. Integrated, software and other vendors cannot run them.
            if describe(adapter.0, &mut description) < 0 || description.vendor != 0x10de || description.video == 0 {
                continue;
            }
            let query: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
                std::mem::transmute(adapter.method(0));
            let mut raw = std::ptr::null_mut();
            if query(adapter.0, &ADAPTER3, &mut raw) < 0 || raw.is_null() {
                continue;
            }
            let adapter3 = Com(raw);
            let memory: unsafe extern "system" fn(*mut c_void, u32, u32, *mut Memory) -> i32 = std::mem::transmute(adapter3.method(14));
            let mut info = Memory::default();
            if memory(adapter3.0, 0, 0, &mut info) >= 0 && info.budget > 0 {
                let free = info.budget.saturating_sub(info.used) / (1024 * 1024);
                best = Some(best.unwrap_or(free).min(free));
            }
        }
        best
    }
}

#[cfg(not(windows))]
pub fn available_vram_mb() -> Option<u64> {
    use std::process::{Command, Stdio};
    let program = nvidia_program(std::env::var_os("PATH").as_deref())?;
    let mut child = Command::new(program)
        .args(["--query-gpu=memory.free", "--format=csv,noheader,nounits"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut output = String::new();
    use std::io::Read;
    child.stdout.take()?.take(4096).read_to_string(&mut output).ok()?;
    output.lines().filter_map(|line| line.trim().parse::<u64>().ok()).min()
}

#[cfg(any(not(windows), test))]
fn nvidia_program(path: Option<&std::ffi::OsStr>) -> Option<std::path::PathBuf> {
    std::env::split_paths(path?)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join("nvidia-smi"))
        .find(|program| program.is_file())
}

#[cfg(test)]
mod tests {
    #[test]
    fn gpu_program_ignores_relative_path_entries() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("nvidia-smi"), "fixture").unwrap();
        let path = std::env::join_paths([std::path::Path::new("."), temp.path()]).unwrap();
        assert_eq!(super::nvidia_program(Some(&path)), Some(temp.path().join("nvidia-smi")));
        assert_eq!(super::nvidia_program(Some(std::ffi::OsStr::new("."))), None);
    }
}
