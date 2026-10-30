//! A sampled lower bound on the public Facts reader's cost, in its own test process.
//! Fixtures warm the file cache. A sample above the budget can reject it; a lower sample cannot certify peak usage.
//! Only the two manifests exist: this exercises the same parser as kit_facts, with the public reader's extra name checks.

use faber_engine::project::{memory::MAX_BYTES, rules};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

static MEASUREMENT: Mutex<()> = Mutex::new(());

fn usage(system: &mut System) -> (u64, u64) {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory().with_cpu());
    let process = system.process(pid).expect("the test's own process is listed");
    (process.memory(), process.accumulated_cpu_time())
}

struct Sampler {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<(u64, u64)>>,
}

impl Sampler {
    fn start() -> (Self, u64, u64) {
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let mut system = System::new();
            let (baseline, cpu) = usage(&mut system);
            send.send((baseline, cpu)).unwrap();
            let mut peak = baseline;
            while !signal.load(Ordering::Acquire) {
                peak = peak.max(usage(&mut system).0);
                std::thread::sleep(Duration::from_millis(1));
            }
            let (after, cpu_after) = usage(&mut system);
            (peak.max(after), cpu_after)
        });
        let sampler = Self { stop, thread: Some(thread) };
        let (baseline, cpu) = receive.recv_timeout(Duration::from_secs(10)).expect("the memory sampler started");
        (sampler, baseline, cpu)
    }

    fn finish(mut self) -> (u64, u64) {
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().expect("the memory sampler completed")
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        // A failed read or assertion must not leave the sampler running during test shutdown.
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn measure(folder: &Path, label: &str) -> u64 {
    let (sampler, baseline, cpu_before) = Sampler::start();
    let mut times = Vec::new();
    for _ in 0..10 {
        let started = Instant::now();
        let facts = rules::facts(folder).unwrap();
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(facts.stacks, ["bevy", "stripe"], "the measured input must actually be parsed");
        assert_eq!(facts.description.as_deref(), Some("fixture"));
    }
    let (peak, cpu_after) = sampler.finish();
    let growth = peak.saturating_sub(baseline);
    eprintln!(
        "project_facts_cost {label}: first_ms={:.3} repeated_max_ms={:.3} baseline_rss_bytes={baseline} sampled_peak_rss_bytes={peak} sampled_growth_bytes={growth} process_cpu_ms={} (sampler included; warm file cache; lower-bound peak only)",
        times[0],
        times[1..].iter().copied().fold(0.0, f64::max),
        cpu_after.saturating_sub(cpu_before),
    );
    // The 20 ms budget is for the shipped build; the unoptimized test build parses the largest inputs several times slower.
    let budget = if cfg!(debug_assertions) { 120.0 } else { 20.0 };
    assert!(times.iter().all(|time| *time <= budget), "the measured reader exceeded {budget} ms: {times:?}");
    growth
}

#[test]
fn project_facts_cost_rejects_observed_growth_above_two_mib() {
    let _measurement = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let folder = tempfile::tempdir().unwrap();
    let prefix = r#"{"dependencies":{"stripe":"1"},"description":"fixture","data":["#;
    let suffix = "]}";
    let count = (MAX_BYTES - prefix.len() - suffix.len()) / 2;
    let mut manifest = format!("{prefix}{}1{suffix}", "1,".repeat(count - 1));
    manifest.extend(std::iter::repeat_n(' ', MAX_BYTES - manifest.len()));
    assert_eq!(manifest.len(), MAX_BYTES);
    std::fs::write(folder.path().join("package.json"), manifest).unwrap();
    std::fs::write(folder.path().join("Cargo.toml"), "[dependencies]\nbevy='0.17'\n").unwrap();
    let control = tempfile::tempdir().unwrap();
    std::fs::write(control.path().join("package.json"), r#"{"dependencies":{"stripe":"1"},"description":"fixture","data":[]}"#).unwrap();
    std::fs::write(control.path().join("Cargo.toml"), "[dependencies]\nbevy='0.17'\n").unwrap();
    // Warm the same sampler and reader with minimal input before measuring the large unknown value.
    let empty = tempfile::tempdir().unwrap();
    let mut counters = System::new();
    let _ = usage(&mut counters);
    assert!(rules::facts(empty.path()).unwrap().stacks.is_empty());
    let _control_growth = measure(control.path(), "minimal_control");
    let growth = measure(folder.path(), "max_unknown_array");
    assert!(growth <= 2 * 1024 * 1024, "the observed peak already exceeds 2 MiB: {growth} bytes");
}

#[test]
fn project_facts_toml_cost_rejects_observed_growth_above_two_mib() {
    let _measurement = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let folder = tempfile::tempdir().unwrap();
    let prefix = "data=[";
    let suffix = "]\n[dependencies]\nbevy='0.17'\n";
    let count = (MAX_BYTES - prefix.len() - suffix.len()) / 2;
    let mut manifest = format!("{prefix}{}1{suffix}", "1,".repeat(count - 1));
    manifest.extend(std::iter::repeat_n(' ', MAX_BYTES - manifest.len()));
    assert_eq!(manifest.len(), MAX_BYTES);
    std::fs::write(folder.path().join("Cargo.toml"), manifest).unwrap();
    let package = r#"{"dependencies":{"stripe":"1"},"description":"fixture"}"#;
    std::fs::write(folder.path().join("package.json"), package).unwrap();
    let control = tempfile::tempdir().unwrap();
    std::fs::write(control.path().join("Cargo.toml"), "data=[]\n[dependencies]\nbevy='0.17'\n").unwrap();
    std::fs::write(control.path().join("package.json"), package).unwrap();
    let _control_growth = measure(control.path(), "toml_minimal_control");
    let growth = measure(folder.path(), "toml_max_unknown_array");
    assert!(growth <= 2 * 1024 * 1024, "the observed peak already exceeds 2 MiB: {growth} bytes");
}

#[test]
fn project_facts_toml_string_cost_rejects_observed_growth_above_two_mib() {
    let _measurement = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let folder = tempfile::tempdir().unwrap();
    let prefix = "data=[";
    let suffix = "]\n[dependencies]\nbevy='0.17'\n";
    let count = (MAX_BYTES - prefix.len() - suffix.len()) / 4;
    let mut manifest = format!("{prefix}{}\"x\"{suffix}", "\"x\",".repeat(count - 1));
    manifest.extend(std::iter::repeat_n(' ', MAX_BYTES - manifest.len()));
    assert_eq!(manifest.len(), MAX_BYTES);
    std::fs::write(folder.path().join("Cargo.toml"), manifest).unwrap();
    let package = r#"{"dependencies":{"stripe":"1"},"description":"fixture"}"#;
    std::fs::write(folder.path().join("package.json"), package).unwrap();
    let control = tempfile::tempdir().unwrap();
    std::fs::write(control.path().join("Cargo.toml"), "data=[]\n[dependencies]\nbevy='0.17'\n").unwrap();
    std::fs::write(control.path().join("package.json"), package).unwrap();
    let _control_growth = measure(control.path(), "toml_string_minimal_control");
    let growth = measure(folder.path(), "toml_max_unknown_string_array");
    assert!(growth <= 2 * 1024 * 1024, "the observed peak already exceeds 2 MiB: {growth} bytes");
}

#[test]
fn project_facts_toml_escaped_string_cost_rejects_observed_growth_above_two_mib() {
    let _measurement = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let folder = tempfile::tempdir().unwrap();
    let prefix = "data=[";
    let suffix = "]\n[dependencies]\nbevy='0.17'\n";
    let value = r#""\t""#;
    let item = format!("{value},");
    let count = (MAX_BYTES - prefix.len() - suffix.len()) / item.len();
    let mut manifest = format!("{prefix}{}{value}{suffix}", item.repeat(count - 1));
    manifest.extend(std::iter::repeat_n(' ', MAX_BYTES - manifest.len()));
    assert_eq!(manifest.len(), MAX_BYTES);
    std::fs::write(folder.path().join("Cargo.toml"), manifest).unwrap();
    let package = r#"{"dependencies":{"stripe":"1"},"description":"fixture"}"#;
    std::fs::write(folder.path().join("package.json"), package).unwrap();
    let control = tempfile::tempdir().unwrap();
    std::fs::write(control.path().join("Cargo.toml"), "data=[]\n[dependencies]\nbevy='0.17'\n").unwrap();
    std::fs::write(control.path().join("package.json"), package).unwrap();
    let _control_growth = measure(control.path(), "toml_escaped_string_minimal_control");
    let growth = measure(folder.path(), "toml_max_unknown_escaped_string_array");
    assert!(growth <= 2 * 1024 * 1024, "the observed peak already exceeds 2 MiB: {growth} bytes");
}

#[test]
fn project_facts_toml_multiline_cost_rejects_observed_growth_above_two_mib() {
    let _measurement = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let folder = tempfile::tempdir().unwrap();
    let prefix = "data=[";
    let suffix = "]\n[dependencies]\nbevy='0.17'\n";
    let value = "\"\"\"x\"\"\"";
    let item = format!("{value},");
    let count = (MAX_BYTES - prefix.len() - suffix.len()) / item.len();
    let mut manifest = format!("{prefix}{}{value}{suffix}", item.repeat(count - 1));
    manifest.extend(std::iter::repeat_n(' ', MAX_BYTES - manifest.len()));
    assert_eq!(manifest.len(), MAX_BYTES);
    std::fs::write(folder.path().join("Cargo.toml"), manifest).unwrap();
    let package = r#"{"dependencies":{"stripe":"1"},"description":"fixture"}"#;
    std::fs::write(folder.path().join("package.json"), package).unwrap();
    let control = tempfile::tempdir().unwrap();
    std::fs::write(control.path().join("Cargo.toml"), "data=[]\n[dependencies]\nbevy='0.17'\n").unwrap();
    std::fs::write(control.path().join("package.json"), package).unwrap();
    let _control_growth = measure(control.path(), "toml_multiline_minimal_control");
    let growth = measure(folder.path(), "toml_max_unknown_multiline_array");
    assert!(growth <= 2 * 1024 * 1024, "the observed peak already exceeds 2 MiB: {growth} bytes");
}
#[test]
fn project_facts_toml_boolean_cost_rejects_observed_growth_above_two_mib() {
    let _measurement = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let folder = tempfile::tempdir().unwrap();
    let prefix = "data=[";
    let suffix = "]\n[dependencies]\nbevy='0.17'\n";
    let value = "true";
    let item = format!("{value},");
    let count = (MAX_BYTES - prefix.len() - suffix.len()) / item.len();
    let mut manifest = format!("{prefix}{}{value}{suffix}", item.repeat(count - 1));
    manifest.extend(std::iter::repeat_n(' ', MAX_BYTES - manifest.len()));
    assert_eq!(manifest.len(), MAX_BYTES);
    std::fs::write(folder.path().join("Cargo.toml"), manifest).unwrap();
    let package = r#"{"dependencies":{"stripe":"1"},"description":"fixture"}"#;
    std::fs::write(folder.path().join("package.json"), package).unwrap();
    let control = tempfile::tempdir().unwrap();
    std::fs::write(control.path().join("Cargo.toml"), "data=[]\n[dependencies]\nbevy='0.17'\n").unwrap();
    std::fs::write(control.path().join("package.json"), package).unwrap();
    let _control_growth = measure(control.path(), "toml_boolean_minimal_control");
    let growth = measure(folder.path(), "toml_max_unknown_boolean_array");
    assert!(growth <= 2 * 1024 * 1024, "the observed peak already exceeds 2 MiB: {growth} bytes");
}
#[test]
fn project_facts_toml_float_cost_rejects_observed_growth_above_two_mib() {
    let _measurement = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let folder = tempfile::tempdir().unwrap();
    let prefix = "data=[";
    let suffix = "]\n[dependencies]\nbevy='0.17'\n";
    let value = "1.0";
    let item = format!("{value},");
    let count = (MAX_BYTES - prefix.len() - suffix.len()) / item.len();
    let mut manifest = format!("{prefix}{}{value}{suffix}", item.repeat(count - 1));
    manifest.extend(std::iter::repeat_n(' ', MAX_BYTES - manifest.len()));
    assert_eq!(manifest.len(), MAX_BYTES);
    std::fs::write(folder.path().join("Cargo.toml"), manifest).unwrap();
    let package = r#"{"dependencies":{"stripe":"1"},"description":"fixture"}"#;
    std::fs::write(folder.path().join("package.json"), package).unwrap();
    let control = tempfile::tempdir().unwrap();
    std::fs::write(control.path().join("Cargo.toml"), "data=[]\n[dependencies]\nbevy='0.17'\n").unwrap();
    std::fs::write(control.path().join("package.json"), package).unwrap();
    let _control_growth = measure(control.path(), "toml_float_minimal_control");
    let growth = measure(folder.path(), "toml_max_unknown_float_array");
    assert!(growth <= 2 * 1024 * 1024, "the observed peak already exceeds 2 MiB: {growth} bytes");
}

#[test]
fn project_facts_toml_nested_cost_rejects_observed_growth_above_two_mib() {
    let _measurement = MEASUREMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let folder = tempfile::tempdir().unwrap();
    let prefix = "data=[";
    let suffix = "]\n[dependencies]\nbevy='0.17'\n";
    let value = "[true,false]";
    let item = format!("{value},");
    let count = (MAX_BYTES - prefix.len() - suffix.len()) / item.len();
    let mut manifest = format!("{prefix}{}{value}{suffix}", item.repeat(count - 1));
    manifest.extend(std::iter::repeat_n(' ', MAX_BYTES - manifest.len()));
    assert_eq!(manifest.len(), MAX_BYTES);
    std::fs::write(folder.path().join("Cargo.toml"), manifest).unwrap();
    let package = r#"{"dependencies":{"stripe":"1"},"description":"fixture"}"#;
    std::fs::write(folder.path().join("package.json"), package).unwrap();
    let control = tempfile::tempdir().unwrap();
    std::fs::write(control.path().join("Cargo.toml"), "data=[]\n[dependencies]\nbevy='0.17'\n").unwrap();
    std::fs::write(control.path().join("package.json"), package).unwrap();
    let _control_growth = measure(control.path(), "toml_nested_minimal_control");
    let growth = measure(folder.path(), "toml_max_unknown_nested_array");
    assert!(growth <= 2 * 1024 * 1024, "the observed peak already exceeds 2 MiB: {growth} bytes");
}
