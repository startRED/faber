//! Incremental, offline Git housekeeping shared by every copy of a project. No history or working files are removed.
use super::git::{Executor, Invocation};
use super::lease::real_path;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Event-driven admission: at most one sweep per engine and hour, including failed attempts.
#[derive(Default)]
pub struct Sweeps {
    last: Option<Instant>,
    running: bool,
}

impl Sweeps {
    pub fn begin(&mut self, now: Instant) -> bool {
        if self.running || self.last.is_some_and(|last| now.saturating_duration_since(last) < Duration::from_secs(3600)) {
            return false;
        }
        self.last = Some(now);
        self.running = true;
        true
    }

    pub fn finish(&mut self) {
        self.running = false;
    }

    pub fn running(&self) -> bool {
        self.running
    }
}

pub(super) fn invocation(root: &Path) -> Invocation {
    Invocation::new(
        root,
        &[
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "pack.threads=1",
            "-c",
            "pack.windowMemory=16m",
            "-c",
            "maintenance.loose-objects.auto=1000",
            "maintenance",
            "run",
            "--auto",
            "--quiet",
            "--task=loose-objects",
            "--task=incremental-repack",
            "--task=commit-graph",
        ],
    )
}

/// Ask the installed Git only once per common directory. Missing Git, unsafe paths and unsupported tasks are harmless.
pub fn run(exec: &mut dyn Executor, roots: &[PathBuf], mut idle: impl FnMut() -> bool) -> usize {
    let mut seen = HashSet::new();
    let mut completed = 0;
    for root in roots {
        if !idle() {
            break;
        }
        let Ok(root) = root.canonicalize() else { continue };
        if real_path(&root).is_err() {
            continue;
        }
        let call = Invocation::new(&root, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .before(Instant::now() + Duration::from_secs(10));
        let Ok(out) = exec.execute_bounded(&call, 32 * 1024) else { continue };
        if !out.success || out.stdout.len() > 32 * 1024 {
            continue;
        }
        let Ok(common) = std::str::from_utf8(&out.stdout) else { continue };
        let common = PathBuf::from(common.trim_end_matches(['\r', '\n']));
        if !common.is_absolute()
            || ["", "objects", "objects/pack", "objects/info", "objects/info/commit-graphs"]
                .iter()
                .any(|path| real_path(&common.join(path)).is_err())
        {
            continue;
        }
        let Ok(common) = common.canonicalize() else { continue };
        if !common.join("objects").is_dir() || !seen.insert(common) || !idle() {
            continue;
        }
        if exec.maintain(&root).is_ok() {
            completed += 1;
        }
    }
    completed
}
