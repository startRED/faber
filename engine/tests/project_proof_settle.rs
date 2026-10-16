//! A proof that leaves a process running has ended before `execute` returns: nothing it started can still write in the folder.
#![cfg(windows)]
use faber_engine::project::proof::{Definition, execute};
use std::collections::BTreeMap;

#[tokio::test]
async fn a_process_a_proof_left_behind_is_gone_when_the_proof_returns() {
    let dir = tempfile::tempdir().unwrap();
    let system = std::env::var("SystemRoot").unwrap();
    let definition = Definition {
        id: "leaves-a-child".into(),
        version: 1,
        program: std::path::Path::new(&system).join("System32").join("cmd.exe"),
        // The child keeps hold.txt open for a minute; the proof itself ends at once.
        argv: vec!["/d".into(), "/c".into(), "start /b ping -n 60 127.0.0.1 > hold.txt 2>&1 <nul".into()],
        checks: vec!["C1".into()],
        toolchain: "cmd".into(),
        dependencies: "none".into(),
        environment: BTreeMap::new(),
        heavy: false,
    };
    let started = std::time::Instant::now();
    let seen = execute(&definition, dir.path(), &faber_engine::proc::real_spawner(), "proofs/x.log".into()).await;
    assert!(started.elapsed() < std::time::Duration::from_secs(30), "execute waited for the child's whole run");
    assert_eq!(seen.exit, Some(0));
    // A process still holding the file would make this fail with a sharing violation.
    std::fs::remove_file(dir.path().join("hold.txt")).expect("the left-behind process was still running");
}
