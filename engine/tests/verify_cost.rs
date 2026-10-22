//! The verifier's own weight (its spec's part 5): idle it adds at most 15 MB to the engine, and after 200 fast checks its
//! memory is back within 5 MB of the level after the first. It sits alone in its own file, hence in its own process, so
//! no other test shares the memory it reads; the checks are scripted, so it measures the verifier and not a tool.

use faber_engine::verify::{Changed, Level, Request, Runner, Spec, Started, Verifier};
use std::sync::Arc;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const MIB: u64 = 1024 * 1024;

fn resident(system: &mut System, pid: Pid) -> u64 {
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory());
    system.process(pid).expect("the test's own process is listed").memory()
}

/// Every check passes at once with a few lines of output, like a quick lint.
struct Passing;

impl Runner for Passing {
    fn start(&self, _: &Spec) -> std::io::Result<Started> {
        Ok(Started {
            stdout: Box::new(std::io::Cursor::new(b"Checking formatting...\nAll files are formatted.\n".to_vec())),
            stderr: Box::new(tokio::io::empty()),
            exit: Box::pin(async { Some(0) }),
            confirmed: None,
            throttle: None,
            peak: None,
        })
    }
}

#[test]
fn the_verifier_idle_weighs_little_and_two_hundred_checks_leave_its_memory_flat() {
    let dir = tempfile::tempdir().unwrap();
    // The real path: macOS keeps temporary folders behind a link.
    let folder = dir.path().canonicalize().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let (mut system, pid) = (System::new(), Pid::from_u32(std::process::id()));
    // Each run changes the file's hash, so the content cutoff never skips the check and every run does the whole work.
    let request = |n: usize| Request {
        folder: folder.clone(),
        level: Some(Level::Fast),
        commands: vec!["cargo fmt --all --check".into()],
        changed: Some(Changed { base: "2".repeat(40), files: vec![("engine/src/a.rs".into(), Some(format!("h{n}")))] }),
        ..Request::default()
    };
    let before = resident(&mut system, pid);
    let verifier = Arc::new(Verifier::new(Arc::new(Passing)));
    let idle = resident(&mut system, pid);
    assert!(idle <= before + 15 * MIB, "the idle verifier added {} MiB", (idle.saturating_sub(before)) / MIB);
    assert!(rt.block_on(verifier.verify(request(0))).ok());
    let first = resident(&mut system, pid);
    for n in 1..=200 {
        assert!(rt.block_on(verifier.verify(request(n))).ok());
    }
    let after = resident(&mut system, pid);
    assert!(after <= first + 5 * MIB, "200 checks grew the memory by {} MiB", (after.saturating_sub(first)) / MIB);
}
