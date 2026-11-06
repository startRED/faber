//! Runs a command inside a Windows job object that kills everything in it when this process ends, so a dead build
//! wrapper provably leaves no compiler behind. Compiled once and reused by `cargo.mjs`; other systems never use it.
#[cfg(windows)]
mod job {
    use std::ffi::c_void;

    #[repr(C)]
    struct Basic { per_process: i64, per_job: i64, flags: u32, min_ws: usize, max_ws: usize, active: u32, affinity: usize, priority: u32, scheduling: u32 }
    #[repr(C)]
    struct Extended { basic: Basic, io: [u64; 6], process_mem: usize, job_mem: usize, peak_process: usize, peak_job: usize }

    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(job: *mut c_void, class: i32, info: *const c_void, size: u32) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
    }

    /// Puts this process in a new job (kill on close, no breakaway). The handle stays open until the process ends.
    pub fn enter() -> bool {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if job.is_null() { return false; }
            let mut info: Extended = std::mem::zeroed();
            info.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            SetInformationJobObject(job, 9, &info as *const _ as *const c_void, std::mem::size_of::<Extended>() as u32) != 0
                && AssignProcessToJobObject(job, GetCurrentProcess()) != 0
        }
    }
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(program) = args.next() else { eprintln!("job-run: missing command"); std::process::exit(2) };
    #[cfg(windows)]
    if !job::enter() {
        eprintln!("job-run: could not create the job object");
        std::process::exit(126);
    }
    match std::process::Command::new(program).args(args).status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => { eprintln!("job-run: {error}"); std::process::exit(if error.kind() == std::io::ErrorKind::NotFound { 127 } else { 126 }) }
    }
}
