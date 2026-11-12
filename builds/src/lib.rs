//! Lighter builds on one PC ([the build coordinator](../../docs/dev/build-folders.md#the-build-coordinator)). Faber's own
//! lanes get the same two parts from `scripts/cargo.mjs`; this crate gives them to the builds agents run in a person's
//! projects:
//!
//! - **One processor budget** ([`Budget`]): a make-style jobserver every Cargo and rustc started with [`Budget::env`]
//!   takes its tokens from, so three agents building at once share about half the processor instead of each taking its
//!   own `jobs`. It is one kernel object while someone builds: no process, no thread, no timer.
//! - **A ledger of passed proofs** ([`Ledger`]): a check that passed on exactly the same content, command, toolchain and
//!   variables is not run again, whichever copy or agent asks.
//!
//! Nothing here starts a process.

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The budget's name, the same for Faber's lanes and the app, so every build on the PC shares one budget.
pub const BUDGET_NAME: &str = "faber-build-jobs";

/// The shared tokens: half the logical processors less two, at least one. Each Cargo keeps one more of its own, so a
/// build alone keeps its speed. A configured positive number wins, at most 256.
pub fn budget_tokens(logical: usize, configured: Option<&str>) -> i32 {
    if let Some(set) = configured.and_then(|v| v.trim().parse::<i32>().ok()).filter(|n| *n > 0) {
        return set.min(256);
    }
    i32::try_from(logical / 2).unwrap_or(i32::MAX).saturating_sub(2).max(1)
}

/// Whether a child gets the shared budget: never where it already has a jobserver (it runs under make or another
/// Cargo, whose tokens it must use), and never on CI, whose machine is the job's alone.
pub fn wants_budget(var: impl Fn(&str) -> Option<String>) -> bool {
    var("CARGO_MAKEFLAGS").is_none() && var("MAKEFLAGS").is_none() && var("CI").as_deref() != Some("true")
}

/// The open budget. Hold it as long as builds may start; the variables of [`Budget::env`] name it.
pub struct Budget {
    tokens: i32,
    #[cfg(windows)]
    _held: faber_winproc::BuildBudget,
}

impl Budget {
    /// Opens the PC's budget, or `None` where there is none (other systems, or Windows refused it): builds then keep
    /// Cargo's own job count.
    pub fn open(logical: usize, configured: Option<&str>) -> Option<Self> {
        let tokens = budget_tokens(logical, configured);
        #[cfg(windows)]
        {
            faber_winproc::BuildBudget::open(BUDGET_NAME, tokens).ok().map(|held| Self { tokens, _held: held })
        }
        #[cfg(not(windows))]
        {
            let _ = tokens;
            None
        }
    }

    pub fn tokens(&self) -> i32 {
        self.tokens
    }

    /// What a child's environment gets. Only Cargo's own variable: `MAKEFLAGS` would make a plain `make` parallel.
    pub fn env(&self) -> [(&'static str, String); 1] {
        [("CARGO_MAKEFLAGS", format!("-j --jobserver-auth={BUDGET_NAME}"))]
    }
}

/// What a proof's result depends on.
#[derive(Clone, Copy)]
pub struct Proof<'a> {
    /// The content's own hash (a Git tree with uncommitted files, or the verifier's content hash).
    pub content: &'a str,
    pub command: &'a [String],
    /// What names the compiler and tools (`rustc -vV`, `node --version`).
    pub toolchain: &'a str,
    /// The variables that change the result, any order.
    pub variables: &'a [(String, String)],
}

impl Proof<'_> {
    /// The ledger key: the same for the same proof from any copy or agent, and different when any part differs.
    pub fn key(&self) -> String {
        let mut variables: Vec<&(String, String)> = self.variables.iter().collect();
        variables.sort();
        let mut hash = Sha256::new();
        let mut field = |bytes: &[u8]| {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        };
        field(b"proof-v1");
        field(self.content.as_bytes());
        field(self.toolchain.as_bytes());
        for arg in self.command {
            field(arg.as_bytes());
        }
        field(b"--variables--");
        for (name, value) in variables {
            field(name.as_bytes());
            field(value.as_bytes());
        }
        hash.finalize().iter().take(20).map(|b| format!("{b:02x}")).collect()
    }
}

/// A recorded pass: when (seconds since 1970), and what the proof printed, kept short by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pass {
    pub at: u64,
    pub output: String,
}

/// Passed proofs on disk, one small file each, the newest kept.
pub struct Ledger {
    dir: PathBuf,
}

/// At most this many passes are kept, none older than [`KEEP_FOR`].
pub const KEEP: usize = 300;
pub const KEEP_FOR: Duration = Duration::from_secs(3 * 24 * 60 * 60);

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Ledger {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file(&self, key: &str) -> Option<PathBuf> {
        (key.len() == 40 && key.bytes().all(|b| b.is_ascii_hexdigit())).then(|| self.dir.join(format!("{key}.pass")))
    }

    /// The pass recorded under `key`, if any and not too old.
    pub fn find(&self, key: &str) -> Option<Pass> {
        let text = fs::read_to_string(self.file(key)?).ok()?;
        let (at, output) = text.split_once('\n')?;
        let at: u64 = at.parse().ok()?;
        (now().saturating_sub(at) < KEEP_FOR.as_secs()).then(|| Pass { at, output: output.to_string() })
    }

    /// Records a pass, written whole or not at all, then drops the oldest beyond [`KEEP`] and any older than [`KEEP_FOR`].
    pub fn record(&self, key: &str, output: &str) -> io::Result<()> {
        let file = self.file(key).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a ledger key"))?;
        fs::create_dir_all(&self.dir)?;
        let temp = self.dir.join(format!("{key}.{}.new", std::process::id()));
        fs::write(&temp, format!("{}\n{output}", now()))?;
        fs::rename(&temp, &file)?;
        self.prune(KEEP, KEEP_FOR)
    }

    pub fn prune(&self, keep: usize, keep_for: Duration) -> io::Result<()> {
        let mut passes: Vec<(SystemTime, PathBuf)> = fs::read_dir(&self.dir)?
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "pass"))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        passes.sort_by_key(|pass| std::cmp::Reverse(pass.0));
        let oldest = SystemTime::now().checked_sub(keep_for).unwrap_or(UNIX_EPOCH);
        for (i, (modified, path)) in passes.iter().enumerate() {
            if i >= keep || *modified < oldest {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_is_about_half_the_processor_and_never_none() {
        assert_eq!(budget_tokens(16, None), 6);
        assert_eq!(budget_tokens(8, None), 2);
        assert_eq!(budget_tokens(4, None), 1);
        assert_eq!(budget_tokens(1, None), 1);
        assert_eq!(budget_tokens(16, Some("3")), 3);
        assert_eq!(budget_tokens(16, Some("9999")), 256);
        for ignored in ["0", "-2", "many", ""] {
            assert_eq!(budget_tokens(16, Some(ignored)), 6, "{ignored}");
        }
    }

    #[test]
    fn a_child_under_another_jobserver_or_on_ci_keeps_its_own() {
        let vars = |set: &'static [(&'static str, &'static str)]| {
            move |name: &str| set.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string())
        };
        assert!(wants_budget(vars(&[])));
        assert!(wants_budget(vars(&[("CI", "false")])));
        assert!(!wants_budget(vars(&[("CARGO_MAKEFLAGS", "-j")])));
        assert!(!wants_budget(vars(&[("MAKEFLAGS", "-j4")])));
        assert!(!wants_budget(vars(&[("CI", "true")])));
    }

    #[test]
    fn only_cargo_hears_of_the_budget() {
        let Some(budget) = Budget::open(16, None) else { return };
        assert_eq!(budget.env(), [("CARGO_MAKEFLAGS", format!("-j --jobserver-auth={BUDGET_NAME}"))]);
        assert!(budget.tokens() >= 1);
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_key_changes_with_every_part_and_not_with_variable_order() {
        let command = args(&["cargo", "test"]);
        let vars = vec![("RUSTFLAGS".to_string(), "-Dwarnings".to_string()), ("CI".to_string(), String::new())];
        let base = Proof { content: "tree1", command: &command, toolchain: "rustc 1.99", variables: &vars };
        let key = base.key();
        assert_eq!(key.len(), 40);
        let reversed: Vec<_> = vars.iter().rev().cloned().collect();
        assert_eq!(Proof { variables: &reversed, ..base }.key(), key);
        assert_ne!(Proof { content: "tree2", ..base }.key(), key);
        assert_ne!(Proof { toolchain: "rustc 2.0", ..base }.key(), key);
        let other = args(&["cargo", "clippy"]);
        assert_ne!(Proof { command: &other, ..base }.key(), key);
        // Fields cannot run into each other: "ab" then "c" is not "a" then "bc".
        let (one, two) = (args(&["ab", "c"]), args(&["a", "bc"]));
        assert_ne!(Proof { command: &one, ..base }.key(), Proof { command: &two, ..base }.key());
        assert_ne!(Proof { variables: &[], ..base }.key(), key);
    }

    #[test]
    fn a_recorded_pass_is_found_and_old_ones_go() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = Ledger::new(dir.path().join("ledger"));
        let command = args(&["cargo", "check"]);
        let key = Proof { content: "t", command: &command, toolchain: "r", variables: &[] }.key();
        assert_eq!(ledger.find(&key), None);
        ledger.record(&key, "Finished\n").unwrap();
        let pass = ledger.find(&key).unwrap();
        assert_eq!(pass.output, "Finished\n");
        assert!(now() - pass.at < 60);
        assert!(ledger.record("../escape", "x").is_err());
        assert_eq!(ledger.find("../escape"), None);
        for i in 0..5 {
            ledger.record(&format!("{i:040x}"), "ok").unwrap();
        }
        ledger.prune(3, KEEP_FOR).unwrap();
        assert_eq!(fs::read_dir(ledger.dir()).unwrap().count(), 3);
        ledger.prune(KEEP, Duration::ZERO).unwrap();
        assert_eq!(fs::read_dir(ledger.dir()).unwrap().count(), 0);
    }
}
