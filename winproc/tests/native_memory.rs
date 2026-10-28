//! Native macOS and Linux integration proof; unit tests keep using fake process readings. The measured process is this
//! test binary started again in a process group of its own, the way the engine starts a CLI (`engine/src/proc.rs`): the
//! test's own group may hold processes it cannot read (macOS refuses them).
#![cfg(any(target_os = "macos", target_os = "linux"))]

use faber_winproc::process_tree::{ProcessIdentity, group_process_identity, resident_bytes};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

const NAME: &str = "worker_memory_unix_observes_resident_pages_and_rejects_wrong_identity";
const CHILD: &str = "FABER_NATIVE_MEMORY_CHILD";

/// Ends the measured child even when an assertion fails first.
struct Measured(Child);
impl Drop for Measured {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn worker_memory_unix_observes_resident_pages_and_rejects_wrong_identity() {
    if std::env::var_os(CHILD).is_some() {
        return touch_pages_when_asked();
    }
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("measured child");
    let mut child = Measured(child);
    let pid = child.0.id();
    let (send, lines) = channel();
    let stdout = BufReader::new(child.0.stdout.take().unwrap());
    std::thread::spawn(move || stdout.lines().map_while(Result::ok).try_for_each(|line| send.send(line)));
    let mut ask = child.0.stdin.take().unwrap();
    ready(&lines);
    let identity = group_process_identity(pid, pid).expect("child process identity");
    assert_eq!(identity.pid, pid);
    let before = resident_bytes(identity).expect("native RSS before allocation");
    assert!(before > 0);
    writeln!(ask, "touch").unwrap();
    ready(&lines);
    let after = resident_bytes(identity).expect("native RSS after touching pages");
    assert!(after >= before + (32 << 20), "real resident pages must be observed");
    let reused = ProcessIdentity { created: identity.created.checked_add(1).unwrap(), ..identity };
    assert!(resident_bytes(reused).is_err(), "a different creation identity must be rejected");
    assert!(resident_bytes(ProcessIdentity { pid: u32::MAX, created: 1 }).is_err(), "a missing process must be rejected");
    drop(ask);
    assert!(child.0.wait().unwrap().success());
    eprintln!("Resident bytes: {before} before, {after} after touching 64 MiB");
}

/// The harness prints `test <name> ... ` without a line end before the test runs, so the first `ready` ends that line.
/// A child that never says it is ready fails the test within a minute instead of holding the run.
fn ready(lines: &Receiver<String>) {
    while !lines.recv_timeout(Duration::from_secs(60)).expect("child said ready within a minute").ends_with("ready") {}
}

/// The child: says it is ready, touches 64 MiB when asked, says so again and holds them until its input closes.
fn touch_pages_when_asked() {
    let mut input = std::io::stdin().lines();
    println!("ready");
    input.next();
    let mut allocation = vec![0u8; 64 << 20];
    for page in allocation.chunks_mut(4096) {
        page[0] = 1;
    }
    std::hint::black_box(&allocation);
    println!("ready");
    input.next();
    std::hint::black_box(&allocation);
}
