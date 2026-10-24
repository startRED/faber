//! Native macOS integration proof; unit tests keep using fake process readings.
#![cfg(target_os = "macos")]

use faber_winproc::process_tree::{ProcessIdentity, group_process_identity, resident_bytes};

#[test]
fn worker_memory_macos_observes_resident_pages_and_rejects_wrong_identity() {
    let pid = std::process::id();
    let group = rustix::process::getpgrp().as_raw_pid() as u32;
    let identity = group_process_identity(group, pid).expect("own process identity");
    assert_eq!(identity.pid, pid);
    let before = resident_bytes(identity).expect("native RSS before allocation");
    assert!(before > 0);
    let mut allocation = vec![0u8; 64 << 20];
    for page in allocation.chunks_mut(rustix::param::page_size()) {
        page[0] = 1;
    }
    std::hint::black_box(&allocation);
    let after = resident_bytes(identity).expect("native RSS after touching pages");
    assert!(after >= before + (32 << 20), "real resident pages must be observed");
    let reused = ProcessIdentity { created: identity.created.checked_add(1).unwrap(), ..identity };
    assert!(resident_bytes(reused).is_err(), "a different creation identity must be rejected");
    assert!(resident_bytes(ProcessIdentity { pid: i32::MAX as u32, created: 1 }).is_err(), "a missing process must be rejected");
    std::hint::black_box(&allocation);
    eprintln!("macOS resident bytes: {before} before, {after} after touching 64 MiB");
}
