//! The hidden desktop (Windows). The test build of Faber never opens a window on the desktop the person is using: it
//! is started on a desktop of its own, made by the runner, that no monitor shows and no input reaches. Its window and its
//! WebView2 processes (which inherit the desktop of the app that starts them) live there, so they can neither appear
//! on top of a game nor take its focus.
//!
//! - `name` makes the desktop once per runner process. If it cannot be made, nothing is started: there is no fallback
//!   to the normal desktop. The desktop lives until the runner ends (its handle is leaked on purpose).
//! - `launch` starts a program on it. The standard library cannot do that (`STARTUPINFO.lpDesktop`), so this is the one
//!   place that calls `CreateProcess` itself. The program's error stream comes back through a pipe.
//! - `visible_windows_of` is the second safety net: it looks at the desktop the runner is on and reports any visible
//!   top-level window owned by the given processes. The runner ends the app's job if it finds one.

use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use winsafe::{CreateProcess, EnumWindows, HDESK, HPIPE, HPROCESS, SECURITY_ATTRIBUTES, STARTUPINFO, co};

/// GitHub Actions needs WebView2's default desktop; every local launch requires isolation.
pub fn hidden_required(actions: Option<&str>) -> bool {
    actions != Some("true")
}

/// The name of this runner's hidden desktop, made on first use.
pub fn name() -> Result<&'static str, String> {
    static NAME: OnceLock<Result<String, String>> = OnceLock::new();
    static COUNT: AtomicU32 = AtomicU32::new(0);
    NAME.get_or_init(|| {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let name = format!("faber-test-{}-{stamp}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::SeqCst));
        let desktop = HDESK::CreateDesktop(&name, None, co::DESKTOP_RIGHTS::GENERIC_ALL, None)
            .map_err(|e| format!("cannot create the hidden desktop, so the app is not started (it would not be hidden): {e}"))?;
        // The desktop is destroyed when its last handle closes and nothing is on it; this one is kept for the whole run.
        std::mem::forget(desktop);
        Ok(name)
    })
    .as_ref()
    .map(String::as_str)
    .map_err(Clone::clone)
}

/// A program running on the hidden desktop.
pub struct Launched {
    pub pid: u32,
    /// Our own handle to it: while it is open, `pid` cannot name anything else.
    process: winsafe::guard::CloseHandleGuard<HPROCESS>,
}

impl Launched {
    /// The exit code once it has ended.
    pub fn try_wait(&self) -> Option<u32> {
        match self.process.WaitForSingleObject(Some(0)) {
            Ok(co::WAIT::OBJECT_0) => self.process.GetExitCodeProcess().ok().or(Some(1)),
            _ => None,
        }
    }

    pub fn kill(&self) {
        let _ = self.process.TerminateProcess(1);
    }
}

/// Starts `program` on the desktop `desktop` with no console window, no input and no output, the environment and working
/// folder of this process, and its error stream copied to ours. Fails if the desktop is not usable.
pub fn launch(desktop: &str, program: &str, args: &[String]) -> Result<Launched, String> {
    let line: Vec<String> = std::iter::once(program).chain(args.iter().map(String::as_str)).map(quote).collect();
    let (program, line, desktop) = (program.to_string(), line.join(" "), desktop.to_string());
    let (sender, receiver) = std::sync::mpsc::channel();
    // The handles do not move between threads (they cannot), so one thread owns the pipe from its making to its end.
    std::thread::spawn(move || {
        let (read, mut write) = match HPIPE::CreatePipe(Some(&inheritable()), 0) {
            Ok(pipe) => pipe,
            Err(e) => return drop(sender.send(Err(format!("cannot make a pipe: {e}")))),
        };
        let mut si = STARTUPINFO::default();
        let mut desktop = winsafe::WString::from_str(&desktop);
        si.set_lpDesktop(Some(&mut desktop));
        si.dwFlags = co::STARTF::USESTDHANDLES;
        // The pipe's write end goes into the startup info for the call and comes back to its guard, which closes it.
        std::mem::swap(&mut *write, &mut si.hStdError);
        let started = CreateProcess(Some(&program), Some(&line), None, None, true, co::CREATE::NO_WINDOW, &[], None, &mut si);
        std::mem::swap(&mut *write, &mut si.hStdError);
        drop(write);
        let pi = match started {
            Ok(pi) => pi,
            Err(e) => return drop(sender.send(Err(format!("cannot start {program} on the hidden desktop: {e}")))),
        };
        let _ = sender.send(Ok(pi.dwProcessId));
        let mut chunk = [0u8; 4096];
        while let Ok(n) = read.ReadFile(&mut chunk) {
            if n == 0 {
                break;
            }
            let _ = std::io::stderr().write_all(&chunk[..n as usize]);
        }
        drop(pi);
    });
    let pid = receiver.recv().map_err(|e| e.to_string())??;
    let access = co::PROCESS::TERMINATE | co::PROCESS::SYNCHRONIZE | co::PROCESS::QUERY_LIMITED_INFORMATION;
    let process = HPROCESS::OpenProcess(access, false, pid).map_err(|e| format!("cannot hold process {pid}: {e}"))?;
    Ok(Launched { pid, process })
}

fn inheritable() -> SECURITY_ATTRIBUTES<'static> {
    let mut attrs = SECURITY_ATTRIBUTES::default();
    attrs.set_bInheritHandle(true);
    attrs
}

/// A command-line word the C runtime reads back as one argument.
fn quote(word: &str) -> String {
    if !word.is_empty() && !word.contains([' ', '\t', '"']) {
        return word.to_string();
    }
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in word.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            c => {
                out.push_str(&"\\".repeat(slashes));
                out.push(c);
                slashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

/// Every top-level window the calling thread's desktop has: (handle, owner process, visible).
pub fn windows() -> Result<Vec<(usize, u32, bool)>, String> {
    let mut found = Vec::new();
    EnumWindows(|hwnd| {
        found.push((hwnd.ptr() as usize, hwnd.GetWindowThreadProcessId().1, hwnd.IsWindowVisible()));
        true
    })
    // A desktop with no window at all makes EnumWindows return FALSE with no error code: that is an empty list.
    .or_else(|e| if e == co::ERROR::SUCCESS { Ok(()) } else { Err(format!("cannot list the windows: {e}")) })?;
    Ok(found)
}

/// The visible top-level windows on this thread's desktop that belong to one of `pids`, as handles.
pub fn visible_windows_of(pids: &[u32]) -> Result<Vec<usize>, String> {
    Ok(windows()?.into_iter().filter(|(_, pid, visible)| *visible && pids.contains(pid)).map(|(h, ..)| h).collect())
}

/// What the probe says: how many windows its own desktop shows, and which. Written by `fake-cli --desktop-probe`, which
/// is started on the hidden desktop; it makes no window, so nothing can ever be seen, wherever it ends up.
pub fn probe() -> i32 {
    match windows() {
        Ok(list) => {
            let handles: Vec<String> = list.iter().map(|(h, ..)| h.to_string()).collect();
            eprintln!("windows {}", handles.join(","));
            0
        }
        Err(e) => {
            eprintln!("error {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_github_actions_may_launch_without_desktop_isolation() {
        assert!(!hidden_required(Some("true")));
        for value in [None, Some("false"), Some("1"), Some("TRUE")] {
            assert!(hidden_required(value));
        }
    }
}
