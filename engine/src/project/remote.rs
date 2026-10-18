//! Reading a pull request with the person's own gh, for the conversation that follows it (`crate::follow`).
//!
//! gh runs shell-free through [`Runner`], with the person's own login; Faber never reads a token or a credential file. It
//! only reads: nothing here pushes, opens or merges anything. Errors are `reason.*` keys.
//!
//! A repository is not trusted: its config can reroute a connection or run a command. So every call runs outside every
//! repository, in a folder of Faber's own (see [`Runner::neutral`]), and names the repository it reads.

use super::git::valid_oid;
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// What runs

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub cwd: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub code: i32,
    pub stdout: String,
}

/// A call that gave no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lost {
    /// The tool is not installed: nothing ran.
    Missing,
    /// It was started and gave no answer (a timeout, or output that never ended). A tool that exits with an error is an
    /// answer, not this.
    Interrupted,
    /// It was killed, and everything it started is proven gone.
    Killed,
}

pub trait Runner {
    fn run(&mut self, call: &Call) -> Result<Reply, Lost>;

    /// A folder of Faber's own where calls that must not see any repository run: empty, private, and under no
    /// repository. Never a shared folder such as the temp folder, which anything can turn into a repository.
    fn neutral(&self) -> PathBuf;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteError {
    GhMissing,
    /// A repository or number that could name something other than one pull request.
    Target,
    Unreachable,
    /// Output that is not what gh prints.
    Command,
}

impl RemoteError {
    pub fn key(self) -> &'static str {
        match self {
            Self::GhMissing => "reason.remote.ghMissing",
            Self::Target => "reason.remote.target",
            Self::Unreachable => "reason.remote.unreachable",
            Self::Command => "reason.git.command",
        }
    }
}

fn name_ok(name: &str) -> bool {
    (1..=100).contains(&name.len()) && name != "." && name != ".." && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
}

fn slug_ok(repo: &str) -> bool {
    repo.split_once('/').is_some_and(|(owner, name)| name_ok(owner) && name_ok(name))
}

fn gh(cwd: &Path, args: &[&str]) -> Call {
    Call { cwd: cwd.into(), args: args.iter().map(|s| (*s).to_string()).collect() }
}

/// A call whose answer is needed now. A call that gave none is a remote that cannot be reached.
fn ask(runner: &mut impl Runner, call: &Call) -> Result<Reply, RemoteError> {
    runner.run(call).map_err(|lost| match lost {
        Lost::Missing => RemoteError::GhMissing,
        Lost::Interrupted | Lost::Killed => RemoteError::Unreachable,
    })
}

// Pull requests

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Pr {
    number: u64,
    state: PrState,
    /// The commit the PR's branch points at.
    head: String,
}

fn parse_pr(value: &Value) -> Result<Pr, RemoteError> {
    let state = match value["state"].as_str() {
        Some("OPEN") => PrState::Open,
        Some("MERGED") => PrState::Merged,
        Some("CLOSED") => PrState::Closed,
        _ => return Err(RemoteError::Command),
    };
    let text = |key: &str| value[key].as_str().map(str::to_string).ok_or(RemoteError::Command);
    Ok(Pr {
        number: value["number"].as_u64().ok_or(RemoteError::Command)?,
        state,
        head: text("headRefOid").and_then(|oid| if valid_oid(&oid) { Ok(oid) } else { Err(RemoteError::Command) })?,
    })
}

fn json(reply: &Reply) -> Result<Value, RemoteError> {
    if reply.code != 0 {
        return Err(RemoteError::Unreachable);
    }
    serde_json::from_str(&reply.stdout).map_err(|_| RemoteError::Command)
}

// Checks

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiState {
    /// The PR has no checks.
    None,
    Pending,
    Passing,
    Failing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ci {
    pub state: CiState,
    /// The commit these results are for.
    pub head: String,
    pub failed: Vec<String>,
    pub total: usize,
}

fn classify(check: &Value) -> (CiState, String) {
    let name = check["name"].as_str().or(check["context"].as_str()).unwrap_or("check").to_string();
    let state = match check["__typename"].as_str() {
        Some("CheckRun") => match (check["status"].as_str(), check["conclusion"].as_str()) {
            (Some("COMPLETED"), Some("SUCCESS" | "NEUTRAL" | "SKIPPED")) => CiState::Passing,
            (Some("COMPLETED"), _) => CiState::Failing,
            _ => CiState::Pending,
        },
        Some("StatusContext") => match check["state"].as_str() {
            Some("SUCCESS") => CiState::Passing,
            Some("PENDING" | "EXPECTED") => CiState::Pending,
            _ => CiState::Failing,
        },
        _ => CiState::Pending,
    };
    (state, name)
}

fn summarize(checks: &[Value], head: &str) -> Ci {
    let seen: Vec<(CiState, String)> = checks.iter().map(classify).collect();
    let failed: Vec<String> = seen.iter().filter(|(s, _)| *s == CiState::Failing).map(|(_, n)| n.clone()).collect();
    let state = if !failed.is_empty() {
        CiState::Failing
    } else if seen.iter().any(|(s, _)| *s == CiState::Pending) {
        CiState::Pending
    } else if seen.is_empty() {
        CiState::None
    } else {
        CiState::Passing
    };
    Ci { state, head: head.into(), failed, total: seen.len() }
}

// Following a pull request

/// A pull request as a project conversation follows it (`crate::follow`): any repository the person's gh can read, and
/// only read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    pub state: PrState,
    pub head: String,
    pub ci: Ci,
    /// GitHub found a merge conflict with the base branch.
    pub conflict: bool,
    /// Each failed check, with the link and the short words GitHub gave it.
    pub failures: Vec<Failure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub name: String,
    pub link: Option<String>,
    pub said: Option<String>,
}

const LOOK_FIELDS: &str = "number,url,isDraft,state,headRefName,headRefOid,baseRefName,isCrossRepository,statusCheckRollup,mergeable";

/// One short printable line: what a check said about itself, never a block of its log.
fn one_line(text: Option<&str>, cap: usize) -> Option<String> {
    let text = text?.split_whitespace().collect::<Vec<_>>().join(" ");
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    (!text.is_empty()).then(|| if text.chars().count() > cap { text.chars().take(cap).collect::<String>() + "…" } else { text })
}

fn failure(check: &Value) -> Option<Failure> {
    let (state, name) = classify(check);
    if state != CiState::Failing {
        return None;
    }
    let link = check["detailsUrl"]
        .as_str()
        .or(check["targetUrl"].as_str())
        .filter(|l| l.starts_with("https://") && l.len() <= 300 && !l.chars().any(|c| c.is_whitespace() || c.is_control()));
    let said = one_line(check["description"].as_str().or(check["title"].as_str()), 160);
    Some(Failure { name: one_line(Some(&name), 80).unwrap_or_else(|| "check".into()), link: link.map(str::to_string), said })
}

/// Reads one pull request and its checks with gh, outside every repository.
pub fn look(runner: &mut impl Runner, repo: &str, number: u64) -> Result<Look, RemoteError> {
    if !slug_ok(repo) || repo.len() > 201 || number == 0 {
        return Err(RemoteError::Target);
    }
    let cwd = runner.neutral();
    let n = number.to_string();
    let reply = ask(runner, &gh(&cwd, &["pr", "view", &n, "--repo", repo, "--json", LOOK_FIELDS]))?;
    let value = json(&reply)?;
    let pr = parse_pr(&value)?;
    if pr.number != number {
        return Err(RemoteError::Command);
    }
    let checks = value["statusCheckRollup"].as_array().map_or(&[][..], Vec::as_slice);
    let ci = summarize(checks, &pr.head);
    let failures = checks.iter().filter_map(failure).take(8).collect();
    Ok(Look { state: pr.state, head: pr.head, ci, conflict: value["mergeable"].as_str() == Some("CONFLICTING"), failures })
}

// The real tools

/// Runs the person's own gh, shell-free, hidden, never prompting.
pub struct Process {
    gh: PathBuf,
    /// Where reads run: Faber's own folder, see [`Runner::neutral`].
    neutral: PathBuf,
}

const OUTPUT_CAP: usize = 16 << 20;
/// The code of a call whose output went past the cap.
const OVER: i32 = 125;
const TIMEOUT: Duration = Duration::from_secs(180);
/// Variables that would aim the git that gh may start at another repository, other history or other config than the folder
/// it runs in has.
const REPO_VARIABLES: [&str; 13] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
    "GIT_GRAFT_FILE",
    "GIT_SHALLOW_FILE",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
];

/// The indexed config pairs (`GIT_CONFIG_KEY_<n>`, `GIT_CONFIG_VALUE_<n>`) among the names of an environment.
fn indexed_config_vars(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let indexed = |name: &str| {
        let upper = name.to_ascii_uppercase();
        upper.starts_with("GIT_CONFIG_KEY_") || upper.starts_with("GIT_CONFIG_VALUE_")
    };
    names.into_iter().filter(|name| indexed(name)).collect()
}

fn native(name: &str) -> PathBuf {
    let Some(cli) = crate::cli::find_cli(name) else { return PathBuf::new() };
    let path = PathBuf::from(&cli.command);
    if !cli.prefix.is_empty() || !path.is_absolute() || (cfg!(windows) && !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
    {
        return PathBuf::new();
    }
    path
}

/// Reads up to `cap` bytes and drains the rest, so the writer never blocks. The flag says whether more came.
fn read_capped(reader: &mut impl Read, cap: usize) -> (Vec<u8>, bool) {
    let (mut kept, mut over, mut chunk) = (Vec::new(), false, [0u8; 8192]);
    while let Ok(n) = reader.read(&mut chunk) {
        if n == 0 {
            break;
        }
        if kept.len() + n > cap {
            over = true;
        } else {
            kept.extend_from_slice(&chunk[..n]);
        }
    }
    (kept, over)
}

/// The code a call reports: output past the cap is a failure of its own, whatever the tool exited with.
fn reply_code(over: bool, exit: Option<i32>) -> i32 {
    if over { OVER } else { exit.unwrap_or(-1) }
}

/// Windows: a job that ends everything the tool started, so a killed call leaves no helper behind.
#[cfg(windows)]
fn confine(child: &std::process::Child) -> Option<win32job::Job> {
    use std::os::windows::io::AsRawHandle;
    let job = win32job::Job::create().ok()?;
    let mut limits = job.query_extended_limit_info().ok()?;
    limits.limit_kill_on_job_close();
    job.set_extended_limit_info(&limits).ok()?;
    // ponytail: assigned after the spawn, so a helper started in those first microseconds escapes; a suspended start would close it.
    job.assign_process(child.as_raw_handle() as isize).ok()?;
    Some(job)
}

impl Process {
    /// A missing tool is not an error here: its calls answer [`Lost::Missing`].
    pub fn find(neutral: PathBuf) -> Process {
        Process { gh: native("gh"), neutral }
    }

    #[cfg(test)]
    fn at(gh: PathBuf, neutral: PathBuf) -> Process {
        Process { gh, neutral }
    }

    /// The command for one call. The environment is the person's own, so their credential helper and gh login work,
    /// except for what would point git elsewhere or ask a question.
    fn command(&self, call: &Call) -> Command {
        self.command_with(call, indexed_config_vars(std::env::vars_os().map(|(name, _)| name.to_string_lossy().into_owned())))
    }

    /// [`Self::command`], given the indexed config variables of the environment to clear.
    fn command_with(&self, call: &Call, indexed: impl IntoIterator<Item = String>) -> Command {
        let mut command = Command::new(&self.gh);
        command.args(&call.args).current_dir(&call.cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        for name in REPO_VARIABLES {
            command.env_remove(name);
        }
        for name in indexed {
            command.env_remove(name);
        }
        // The environment's own list of transports beats every `protocol.*.allow`, so it names the ones gh may use.
        command.env("GIT_ALLOW_PROTOCOL", "https:ssh");
        if let Some(parent) = call.cwd.parent() {
            // git, which gh may start, looks at the working folder itself and never above it.
            command.env("GIT_CEILING_DIRECTORIES", parent);
        }
        command
            // A partial clone's promisor remote is the repository's choice: git may not fetch from it on its own.
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .env("SSH_ASKPASS_REQUIRE", "never")
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("GH_SPINNER_DISABLED", "1")
            .env("NO_COLOR", "1");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        command
    }
}

/// The code of a call that was refused before it started.
const REFUSED: i32 = 126;

/// The real path of the neutral folder, if it can be one: absolute, not the shared temp folder, made if missing, not a
/// link (a symlink or junction would put git somewhere else than the ceiling it is given), not a bare repository's
/// layout, and neither it nor any parent holds a `.git` (a folder, a file or a dangling link), so git finds no
/// repository and no config there. Git runs in the returned path.
fn private_path(dir: &Path) -> Option<PathBuf> {
    if !dir.is_absolute() || std::fs::create_dir_all(dir).is_err() || dir.symlink_metadata().ok()?.file_type().is_symlink() {
        return None;
    }
    let real = plain(std::fs::canonicalize(dir).ok()?);
    let shared = std::fs::canonicalize(std::env::temp_dir()).map(plain).unwrap_or_else(|_| std::env::temp_dir());
    let bare = real.join("HEAD").is_file() && real.join("objects").is_dir() && real.join("refs").is_dir();
    let free = real.ancestors().all(|folder| folder.join(".git").symlink_metadata().is_err());
    (real != shared && !bare && free).then_some(real)
}

/// Windows spells a canonical path with a `\\?\` prefix, which git and the ceiling comparison do not take.
fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with(r"UNC\") => PathBuf::from(rest),
        _ => path,
    }
}

/// What a call that had to be killed reports: killed and proven gone only if the whole tree was confined.
fn lost_on_timeout(confined: bool) -> Lost {
    if confined { Lost::Killed } else { Lost::Interrupted }
}

impl Runner for Process {
    fn neutral(&self) -> PathBuf {
        self.neutral.clone()
    }

    fn run(&mut self, call: &Call) -> Result<Reply, Lost> {
        if self.gh.as_os_str().is_empty() {
            return Err(Lost::Missing);
        }
        // A repository planted in or above the neutral folder, or a link out of it, would answer reads with its own config.
        let neutral;
        let call = if call.cwd == self.neutral {
            let Some(cwd) = private_path(&self.neutral) else { return Ok(Reply { code: REFUSED, stdout: String::new() }) };
            neutral = Call { cwd, ..call.clone() };
            &neutral
        } else {
            call
        };
        let mut child = self.command(call).spawn().map_err(|_| Lost::Missing)?;
        #[cfg(windows)]
        let job = confine(&child);
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            return Err(Lost::Interrupted);
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(read_capped(&mut stdout, OUTPUT_CAP));
        });
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                _ => {
                    // ponytail: only Windows confines the tree (a job object). Elsewhere a killed call stays
                    // unproven; process groups would prove it.
                    #[cfg(windows)]
                    let confined = job.is_some();
                    #[cfg(not(windows))]
                    let confined = false;
                    #[cfg(windows)]
                    drop(job);
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(lost_on_timeout(confined));
                }
            }
        };
        let (kept, over) = rx.recv_timeout(Duration::from_secs(5)).map_err(|_| Lost::Interrupted)?;
        // Output beyond the cap cannot be scanned or trusted: report it as a failure, never as a partial success.
        let code = reply_code(over, status.code());
        Ok(Reply { code, stdout: String::from_utf8_lossy(&kept).into_owned() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn envs_of(call: &Call) -> BTreeMap<String, Option<String>> {
        let process = Process::at("C:/tools/gh.exe".into(), "C:/faber/remote".into());
        process
            .command(call)
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned())))
            .collect()
    }

    #[test]
    fn ci_is_read_as_pending_passing_failing_or_absent() {
        use serde_json::json;
        let run = |status: &str, conclusion: &str| json!({"__typename": "CheckRun", "name": format!("{status}-{conclusion}"), "status": status, "conclusion": conclusion});
        let status = |state: &str| json!({"__typename": "StatusContext", "context": "ci/x", "state": state});
        let cases: Vec<(Vec<Value>, CiState)> = vec![
            (vec![], CiState::None),
            (vec![run("COMPLETED", "SUCCESS"), run("COMPLETED", "SKIPPED")], CiState::Passing),
            (vec![run("COMPLETED", "NEUTRAL")], CiState::Passing),
            (vec![run("COMPLETED", "SUCCESS"), run("IN_PROGRESS", "")], CiState::Pending),
            (vec![run("QUEUED", "")], CiState::Pending),
            (vec![run("COMPLETED", "FAILURE"), run("IN_PROGRESS", "")], CiState::Failing),
            (vec![run("COMPLETED", "CANCELLED")], CiState::Failing),
            (vec![status("PENDING")], CiState::Pending),
            (vec![status("EXPECTED")], CiState::Pending),
            (vec![status("SUCCESS")], CiState::Passing),
            (vec![status("ERROR")], CiState::Failing),
            (vec![json!({"__typename": "SomethingNew", "name": "x"})], CiState::Pending),
        ];
        for (checks, expected) in cases {
            let read = summarize(&checks, "abc");
            assert_eq!((read.state, read.head.as_str()), (expected, "abc"), "{checks:?}");
        }
        let read = summarize(&[run("COMPLETED", "FAILURE")], "abc");
        assert_eq!((read.failed, read.total), (vec!["COMPLETED-FAILURE".to_string()], 1));
    }

    fn outside_every_repository() -> bool {
        std::env::temp_dir().ancestors().all(|folder| folder.join(".git").symlink_metadata().is_err())
    }

    #[test]
    fn the_real_process_is_shell_free_hidden_and_never_prompts() {
        let process = Process::at("C:/tools/gh.exe".into(), "C:/faber/remote".into());
        let call = Call { cwd: PathBuf::from("/work/repo"), args: vec!["pr".into(), "view".into(), "7".into()] };
        let command = process.command(&call);
        assert_eq!(command.get_program(), "C:/tools/gh.exe");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["pr", "view", "7"]);
        let envs = envs_of(&call);
        for (name, value) in [
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GCM_INTERACTIVE", "never"),
            ("GH_PROMPT_DISABLED", "1"),
            ("SSH_ASKPASS_REQUIRE", "never"),
            ("GIT_ALLOW_PROTOCOL", "https:ssh"),
            // A partial clone's promisor remote is chosen by the repository: git must not fetch from it on its own.
            ("GIT_NO_LAZY_FETCH", "1"),
        ] {
            assert_eq!(envs.get(name), Some(&Some(value.to_string())), "{name}");
        }
        for name in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_NAMESPACE",
            "GIT_PREFIX",
        ] {
            assert_eq!(envs.get(name), Some(&None), "{name} must be removed");
        }
        // git looks at the folder it runs in and never above it.
        assert_eq!(envs.get("GIT_CEILING_DIRECTORIES"), Some(&Some(PathBuf::from("/work").to_string_lossy().into_owned())));
        // The person's own credential setup is neither replaced nor cleared.
        assert!(
            !envs.contains_key("GIT_CONFIG_GLOBAL")
                && !envs.contains_key("GIT_CONFIG_NOSYSTEM")
                && !envs.contains_key("GH_TOKEN")
                && !envs.contains_key("GIT_SSH_COMMAND")
        );
    }

    #[test]
    fn config_from_the_environment_never_reaches_a_call() {
        let process = Process::at("C:/tools/gh.exe".into(), "C:/faber/remote".into());
        // Also in the neutral folder, where the reads run.
        for cwd in ["/work/repo", "C:/faber/remote"] {
            let envs = envs_of(&Call { cwd: PathBuf::from(cwd), args: vec!["x".into()] });
            for name in ["GIT_CONFIG", "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_COUNT", "GIT_GRAFT_FILE", "GIT_SHALLOW_FILE"] {
                assert_eq!(envs.get(name), Some(&None), "{name} must be removed in {cwd}");
            }
        }
        // The indexed pairs are found by name, whatever their number, and nothing else is touched.
        let names = [
            "PATH",
            "GIT_CONFIG_KEY_0",
            "git_config_value_12",
            "GIT_CONFIG_KEYS",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_GLOBAL",
            "GIT_CONFIG_SYSTEM",
        ];
        assert_eq!(indexed_config_vars(names.map(String::from)), ["GIT_CONFIG_KEY_0", "git_config_value_12"]);
        let call = Call { cwd: PathBuf::from("/work/repo"), args: vec!["x".into()] };
        let command = process.command_with(&call, ["GIT_CONFIG_KEY_0".to_string(), "GIT_CONFIG_VALUE_0".to_string()]);
        let envs: BTreeMap<_, _> = command.get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.is_some())).collect();
        assert_eq!((envs.get("GIT_CONFIG_KEY_0"), envs.get("GIT_CONFIG_VALUE_0")), (Some(&false), Some(&false)));
    }

    #[test]
    fn the_output_cap_keeps_the_start_and_says_more_came() {
        let (kept, over) = read_capped(&mut std::io::Cursor::new(vec![b'a'; 20_000]), 16_384);
        assert!(over && kept.len() <= 16_384 && kept.len() >= 8_192);
        let (kept, over) = read_capped(&mut std::io::Cursor::new(vec![b'a'; 100]), 16_384);
        assert!(!over && kept.len() == 100);
        // Exactly the cap fits; one byte more is over.
        let (kept, over) = read_capped(&mut std::io::Cursor::new(vec![b'a'; 100]), 100);
        assert!(!over && kept.len() == 100);
        let (kept, over) = read_capped(&mut std::io::Cursor::new(vec![b'a'; 101]), 100);
        assert!(over && kept.len() <= 100);
        // Too much output is its own failure, even when the tool said all was well.
        assert_eq!(
            (reply_code(true, Some(0)), reply_code(false, Some(0)), reply_code(false, Some(3)), reply_code(false, None)),
            (125, 0, 3, -1)
        );
    }

    #[test]
    fn only_a_private_folder_outside_every_repository_can_be_the_neutral_one() {
        let private = |dir: &Path| private_path(dir).is_some();
        let base = tempfile::tempdir().unwrap();
        // The folder is made when it is missing, and is fine when nothing above it is a repository.
        let fresh = base.path().join("faber").join("remote");
        assert_eq!(private(&fresh), outside_every_repository());
        assert!(fresh.is_dir());
        // A repository in the folder, or in any folder above it, spoils it: its config would answer the reads.
        for planted in ["dir", "file"] {
            let top = tempfile::tempdir().unwrap();
            let git = top.path().join(".git");
            if planted == "dir" {
                std::fs::create_dir(&git).unwrap()
            } else {
                std::fs::write(&git, "gitdir: elsewhere").unwrap()
            }
            assert!(!private(&top.path().join("a").join("b")), "{planted} above");
            assert!(!private(top.path()), "{planted} in it");
        }
        // A link called .git that points nowhere is still a .git to the next tool that resolves it.
        let top = tempfile::tempdir().unwrap();
        let missing = top.path().join("nowhere");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&missing, top.path().join(".git")).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&missing, top.path().join(".git")).is_ok();
        if linked {
            assert!(!private(&top.path().join("work")), "a dangling .git link above");
        }
        // The shared temp folder and a relative path never qualify.
        assert!(!private(&std::env::temp_dir()));
        assert!(!private(Path::new("relative/folder")));
    }

    #[test]
    fn a_bare_repository_layout_cannot_be_the_neutral_folder() {
        let private = |dir: &Path| private_path(dir).is_some();
        let top = tempfile::tempdir().unwrap();
        let dir = top.path().join("faber").join("remote");
        let outside = outside_every_repository();
        assert_eq!(private(&dir), outside);
        // HEAD, objects and refs together are what git takes for a bare repository, in the folder git is started in.
        std::fs::write(dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(private(&dir), outside, "a lone HEAD is just a file");
        std::fs::create_dir(dir.join("objects")).unwrap();
        assert_eq!(private(&dir), outside, "no refs yet");
        std::fs::create_dir(dir.join("refs")).unwrap();
        assert!(!private(&dir));
        // Without HEAD it is not a repository, whatever else is there.
        std::fs::remove_file(dir.join("HEAD")).unwrap();
        assert_eq!(private(&dir), outside, "objects and refs without HEAD");
    }

    #[test]
    fn a_link_as_the_neutral_folder_cannot_lead_out_of_the_private_place() {
        let top = tempfile::tempdir().unwrap();
        let repo = top.path().join("checkout");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir(repo.join("inside")).unwrap();
        let plain = top.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        for (name, target) in [("into-a-repository", repo.join("inside")), ("to-a-plain-folder", plain.clone())] {
            let link = top.path().join("faber").join(name);
            std::fs::create_dir_all(link.parent().unwrap()).unwrap();
            #[cfg(unix)]
            let linked = std::os::unix::fs::symlink(&target, &link).is_ok();
            #[cfg(windows)]
            let linked = std::os::windows::fs::symlink_dir(&target, &link).is_ok();
            // Making a link takes a privilege on some machines.
            if linked {
                assert_eq!(private_path(&link), None, "{name}");
            }
        }
        // A plain folder is used as its real path, so git's ceiling is compared with where git really runs.
        let real = top.path().join("faber").join("real");
        let got = private_path(&real);
        assert_eq!(got.is_some(), outside_every_repository());
        if let Some(got) = got {
            assert_eq!(std::fs::canonicalize(&got).unwrap(), std::fs::canonicalize(&real).unwrap());
            assert!(!got.to_string_lossy().starts_with(r"\\?\"), "git takes no verbatim prefix");
        }
    }

    #[test]
    fn the_real_process_refuses_to_read_from_a_folder_a_repository_reaches() {
        let top = tempfile::tempdir().unwrap();
        std::fs::create_dir(top.path().join(".git")).unwrap();
        let neutral = top.path().join("remote");
        let gh = if cfg!(windows) { "C:\\none\\gh.exe" } else { "/none/gh" };
        let mut process = Process::at(gh.into(), neutral.clone());
        assert_eq!(process.neutral(), neutral);
        // gh does not exist, so a call that was not refused would come back as "missing" instead.
        let call = Call { cwd: neutral, args: vec!["pr".into(), "view".into()] };
        assert_eq!(process.run(&call), Ok(Reply { code: 126, stdout: String::new() }));
        let elsewhere = Call { cwd: top.path().to_path_buf(), args: vec!["pr".into(), "view".into()] };
        assert_eq!(process.run(&elsewhere), Err(Lost::Missing), "only the neutral folder is guarded");
    }

    #[test]
    fn a_timeout_proves_the_tree_gone_only_if_it_was_confined() {
        assert_eq!(lost_on_timeout(true), Lost::Killed);
        assert_eq!(lost_on_timeout(false), Lost::Interrupted);
    }
}
