//! Runs a command inside a Windows job object that kills everything in it when this process ends, so a dead build
//! wrapper provably leaves no compiler behind. Compiled once and reused by `cargo.mjs`; other systems never use it.
//!
//! Two more duties, both optional and both Windows only:
//! - `FABER_JOBSERVER=<name>:<tokens>` holds the PC's one build budget open: a named semaphore every lane's Cargo and
//!   rustc share as a jobserver (`CARGO_MAKEFLAGS=--jobserver-auth=<name>`), created with `tokens` slots by whichever
//!   lane builds first and gone once no build runs.
//! - `FABER_BUILD_PRIORITY=below` runs the build below normal priority, which its compilers inherit.
//! - `FABER_JOB_STATS=<file>` receives one JSON line when the command ends: the processor time of every process the job
//!   ever held (the compilers too), the wall time and the job's peak committed memory.
#[cfg(windows)]
mod job {
    use std::ffi::c_void;

    #[repr(C)]
    struct Basic { per_process: i64, per_job: i64, flags: u32, min_ws: usize, max_ws: usize, active: u32, affinity: usize, priority: u32, scheduling: u32 }
    #[repr(C)]
    struct Extended { basic: Basic, io: [u64; 6], process_mem: usize, job_mem: usize, peak_process: usize, peak_job: usize }
    #[repr(C)]
    struct Accounting { user: i64, kernel: i64, period_user: i64, period_kernel: i64, faults: u32, processes: u32, active: u32, terminated: u32 }

    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(job: *mut c_void, class: i32, info: *const c_void, size: u32) -> i32;
        fn QueryInformationJobObject(job: *mut c_void, class: i32, info: *mut c_void, size: u32, returned: *mut u32) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
        fn CreateSemaphoreW(attributes: *mut c_void, initial: i32, maximum: i32, name: *const u16) -> *mut c_void;
        fn SetPriorityClass(process: *mut c_void, class: u32) -> i32;
    }

    /// Below-normal priority for this process and, as Windows hands that class down, every compiler the build starts:
    /// a game or whatever else the person does is served first, and the build still moves.
    pub fn below_normal() {
        unsafe { SetPriorityClass(GetCurrentProcess(), 0x0000_4000) };
    }

    pub struct Job(*mut c_void);

    /// Puts this process in a new job (kill on close, no breakaway). The handle stays open until the process ends.
    pub fn enter() -> Option<Job> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if job.is_null() { return None; }
            let mut info: Extended = std::mem::zeroed();
            info.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            let ok = SetInformationJobObject(job, 9, &info as *const _ as *const c_void, std::mem::size_of::<Extended>() as u32) != 0
                && AssignProcessToJobObject(job, GetCurrentProcess()) != 0;
            ok.then_some(Job(job))
        }
    }

    impl Job {
        /// Processor milliseconds (user and kernel) of every process the job held, its process count and peak memory.
        pub fn stats(&self) -> (u64, u32, usize) {
            unsafe {
                let mut acc: Accounting = std::mem::zeroed();
                let mut ext: Extended = std::mem::zeroed();
                QueryInformationJobObject(self.0, 1, &mut acc as *mut _ as *mut c_void, std::mem::size_of::<Accounting>() as u32, std::ptr::null_mut());
                QueryInformationJobObject(self.0, 9, &mut ext as *mut _ as *mut c_void, std::mem::size_of::<Extended>() as u32, std::ptr::null_mut());
                (((acc.user + acc.kernel) / 10_000) as u64, acc.processes, ext.peak_job)
            }
        }
    }

    /// Opens, or creates with `tokens` slots, the named semaphore of the shared build budget. The handle is kept open
    /// for the life of this process by leaking it on purpose.
    pub fn budget(name: &str, tokens: i32) -> bool {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        !unsafe { CreateSemaphoreW(std::ptr::null_mut(), tokens, tokens, wide.as_ptr()) }.is_null()
    }
}

/// `name:tokens` of `FABER_JOBSERVER`, when both are sensible: a plain name and 1 to 256 tokens.
fn parse_budget(value: &str) -> Option<(&str, i32)> {
    let (name, tokens) = value.rsplit_once(':')?;
    let tokens: i32 = tokens.parse().ok().filter(|n| (1..=256).contains(n))?;
    (!name.is_empty() && name.len() < 200 && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))).then_some((name, tokens))
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(program) = args.next() else { eprintln!("job-run: missing command"); std::process::exit(2) };
    let started = std::time::Instant::now();
    #[cfg(windows)]
    let Some(job) = job::enter() else {
        eprintln!("job-run: could not create the job object");
        std::process::exit(126);
    };
    #[cfg(windows)]
    if std::env::var("FABER_BUILD_PRIORITY").is_ok_and(|v| v == "below") {
        job::below_normal();
    }
    #[cfg(windows)]
    if let Some((name, tokens)) = std::env::var("FABER_JOBSERVER").ok().as_deref().and_then(parse_budget) {
        if !job::budget(name, tokens) {
            eprintln!("job-run: the shared build budget could not be opened; this build uses its own job count");
        }
    }
    let code = match std::process::Command::new(program).args(args).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => { eprintln!("job-run: {error}"); if error.kind() == std::io::ErrorKind::NotFound { 127 } else { 126 } }
    };
    #[cfg(windows)]
    if let Some(file) = std::env::var_os("FABER_JOB_STATS") {
        use std::io::Write;
        let (cpu_ms, processes, peak) = job.stats();
        let line = format!("{{\"cpuMs\":{cpu_ms},\"wallMs\":{},\"processes\":{processes},\"peakBytes\":{peak},\"code\":{code}}}\n", started.elapsed().as_millis());
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(file).and_then(|mut f| f.write_all(line.as_bytes()));
    }
    #[cfg(not(windows))]
    let _ = started;
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_budget_is_a_plain_name_and_a_sane_count() {
        assert_eq!(super::parse_budget("faber-build-jobs:6"), Some(("faber-build-jobs", 6)));
        for bad in ["", "x", ":6", "a:0", "a:999", "a b:4", "a\\b:4", "a:-1"] {
            assert_eq!(super::parse_budget(bad), None, "{bad}");
        }
    }
}
