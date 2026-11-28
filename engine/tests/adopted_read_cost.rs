//! Supplemental reader measurements in their own process; not the complete session-start budget.
//! Fixture writes warm the OS file cache. Retained RSS is not peak RSS and excludes memory freed before the sample.

use faber_engine::project::memory::{read_adopted_source, read_adopted_tasks};
use std::path::Path;
use std::time::Instant;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

fn usage(system: &mut System) -> (u64, u64) {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory().with_cpu());
    let process = system.process(pid).expect("the test's own process is listed");
    (process.memory(), process.accumulated_cpu_time())
}

fn measure<T>(name: &str, operation: impl Fn() -> T, validate: impl Fn(&T)) {
    let mut system = System::new();
    let (before, cpu_before) = usage(&mut system);
    let started = Instant::now();
    let result = operation();
    let first = started.elapsed();
    let (retained, cpu_after) = usage(&mut system);
    validate(&result);
    drop(result);
    let mut times = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let result = operation();
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        validate(&result);
    }
    times.sort_by(f64::total_cmp);
    // Keep libtest's stdout result line intact when output capture is disabled.
    eprintln!(
        "adopted_read_cost {name}: first_read_ms={:.3} repeated_median_ms={:.3} repeated_max_ms={:.3} first_process_cpu_ms={} baseline_rss_bytes={before} retained_rss_bytes={retained} retained_growth_bytes={}",
        first.as_secs_f64() * 1000.0,
        (times[9] + times[10]) / 2.0,
        times[19],
        cpu_after.saturating_sub(cpu_before),
        retained.saturating_sub(before),
    );
}

fn tasks(project: &Path, count: usize, bytes: usize, incomplete: bool) {
    let loaded = read_adopted_tasks(project, "tasks").unwrap();
    assert_eq!(loaded.files.len(), count);
    assert_eq!(loaded.files.iter().map(|(_, text)| text.len()).sum::<usize>(), bytes);
    assert_eq!(loaded.incomplete, incomplete);
    assert!(loaded.files.windows(2).all(|pair| pair[0].0 < pair[1].0));
}

#[test]
fn adopted_reader_cost_preserves_bounded_results() {
    let temporary = std::env::temp_dir().canonicalize().unwrap();
    let project = tempfile::tempdir_in(temporary).unwrap();
    let root = project.path();
    let folder = root.join("tasks");
    std::fs::create_dir(&folder).unwrap();
    for index in 0..8 {
        std::fs::write(folder.join(format!("{index:03}.md")), "a".repeat(256)).unwrap();
    }
    measure(
        "typical_tasks",
        || read_adopted_tasks(root, "tasks").unwrap(),
        |result| {
            assert_eq!(result.files.len(), 8);
            assert!(!result.incomplete);
        },
    );
    for entry in std::fs::read_dir(&folder).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    let name = "ação".repeat(20);
    for index in 0..256 {
        std::fs::write(folder.join(format!("{index:03}-{name}.md")), "á".repeat(1024)).unwrap();
    }
    measure(
        "maximum_payload_unicode_names",
        || read_adopted_tasks(root, "tasks").unwrap(),
        |result| {
            assert_eq!(result.files.len(), 256);
            assert_eq!(result.files.iter().map(|(_, text)| text.len()).sum::<usize>(), 512 * 1024);
            assert!(!result.incomplete);
        },
    );
    std::fs::write(folder.join("overflow.md"), "overflow").unwrap();
    measure(
        "enumeration_overflow",
        || read_adopted_tasks(root, "tasks").unwrap(),
        |result| {
            assert_eq!(result.files.len(), 256);
            assert!(result.incomplete);
        },
    );
    for entry in std::fs::read_dir(&folder).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    std::fs::write(folder.join("good.md"), "kept").unwrap();
    std::fs::write(folder.join("invalid.md"), [0xff; 128]).unwrap();
    std::fs::create_dir(folder.join("directory.md")).unwrap();
    measure(
        "invalid_utf8_and_directory",
        || read_adopted_tasks(root, "tasks").unwrap(),
        |result| {
            assert_eq!(result.files, vec![("good.md".to_owned(), "kept".to_owned())]);
            assert!(result.incomplete);
        },
    );
    let source = "é".repeat(131 * 1024);
    std::fs::write(root.join("decisions.md"), &source).unwrap();
    measure("adopted_262_kib_file", || read_adopted_source(root, "decisions.md").unwrap(), |result| assert_eq!(result, &source));
    // Re-read the refused-entry fixture after all measurements to catch accidental source changes.
    tasks(root, 1, 4, true);
    assert_eq!(std::fs::read(folder.join("invalid.md")).unwrap(), [0xff; 128]);
}
