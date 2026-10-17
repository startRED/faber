//! What a project's mission needs besides the coordinator (R137): the proof the project itself declares and its owner
//! confirmed, the files the roles are shown, and the views of a mission's progress and final report.
//!
//! Nothing here runs a model or lands anything. It reads the project's own rules, turns them into the coordinator's
//! `Setup`, and turns the journal back into what the Projects screen shows.
use super::{
    Grant, Mission, MissionState,
    coordinator::{self, Pins, Setup, Stop},
    git::{self, Executor, GitError, Repository},
    lease::normalized,
    proof::Definition,
    store::Journal,
};
use crate::cli::Cli;
use crate::model::{Project, Vendor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// The most proof commands a project may declare.
pub const MAX_COMMANDS: usize = 12;
/// Where the roles' view of the project stops: bytes of file text, files, and bytes of one file.
const FILE_BUDGET: usize = 96 * 1024;
const MAX_SHOWN: usize = 48;
const FILE_MAX: u64 = 48 * 1024;
/// Blobs read at most while filling the budget; every read is a process.
const MAX_READS: usize = 160;
const MAX_WRITABLE: usize = 48;
const DOC_MAX: u64 = 256 * 1024;
/// The ceiling one mission is admitted under (R135), and the cost grant of one role turn, in millionths of an
/// API-equivalent dollar.
pub const MISSION_CAP_MICROS: u64 = 2_000_000;
const TURN_MICROS: u64 = 900_000;

// ----- the proof a project declares -----

/// What the Projects screen shows about a project's proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofView {
    /// The commands a mission will run: the confirmed ones, or the declared ones while nothing is confirmed.
    pub commands: Vec<String>,
    /// The commands the project declares now (its rules file, or a detected default).
    pub declared: Vec<String>,
    /// `agents`, `claude`, `detected` or `none`.
    pub source: String,
    /// The owner confirmed exactly what the project declares now.
    pub confirmed: bool,
    /// A list was confirmed before and the project declares something else now.
    pub changed: bool,
    /// What runs first in the clean copy, found from the project's lock files (not part of what the owner confirms).
    #[serde(default)]
    pub setup: Vec<String>,
}

/// The lines of the first code block under a heading that says "Proof commands". A line may chain commands with `&&`.
pub fn parse_declared(markdown: &str) -> Vec<String> {
    let mut found = false;
    let mut inside = false;
    let mut commands = vec![];
    for line in markdown.lines() {
        let trimmed = line.trim();
        if inside {
            if trimmed.starts_with("```") {
                return commands;
            }
            let line = trimmed.strip_prefix("$ ").unwrap_or(trimmed);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            commands.extend(line.split("&&").map(|c| c.trim().to_string()).filter(|c| !c.is_empty()));
        } else if found && trimmed.starts_with("```") {
            inside = true;
        } else if trimmed.starts_with('#') {
            if found && !commands.is_empty() {
                return commands;
            }
            found = trimmed.trim_start_matches('#').trim().to_lowercase().starts_with("proof commands");
        }
    }
    commands
}

fn read_doc(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > DOC_MAX {
        return None;
    }
    String::from_utf8(std::fs::read(path).ok()?).ok()
}

/// What the project declares: the "Proof commands" of AGENTS.md or CLAUDE.md, else a detected default.
pub fn declared(root: &Path) -> (Vec<String>, &'static str) {
    for (file, source) in [("AGENTS.md", "agents"), ("CLAUDE.md", "claude")] {
        let commands = read_doc(&root.join(file)).map(|text| parse_declared(&text)).unwrap_or_default();
        if !commands.is_empty() {
            return (commands.into_iter().take(MAX_COMMANDS).collect(), source);
        }
    }
    let mut detected = vec![];
    let scripts = read_doc(&root.join("package.json"))
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|json| json["scripts"]["test"].as_str().is_some_and(|s| !s.trim().is_empty()));
    if scripts {
        detected.push("npm test".to_string());
    }
    if root.join("Cargo.toml").is_file() {
        detected.push("cargo test".to_string());
    }
    let source = if detected.is_empty() { "none" } else { "detected" };
    (detected, source)
}

/// One command as program and arguments, without a shell. A command that needs a shell (a pipe, a redirect, a
/// substitution, a quote) or that names a program by path is refused: the owner edits it into plain words.
pub fn command_tokens(line: &str) -> Result<Vec<String>, &'static str> {
    let tokens: Vec<String> = line.split_whitespace().map(String::from).collect();
    let Some(program) = tokens.first() else { return Err("reason.proof.command") };
    if tokens.len() > 32
        || line.len() > 512
        || line.chars().any(|c| c.is_control() || "|&;<>$`\"'(){}*?^%!".contains(c))
        || program.contains(['/', '\\', ':'])
    {
        return Err("reason.proof.command");
    }
    Ok(tokens)
}

/// The saved confirmation: the declared list, the confirmed commands, and the setup commands the owner saw (`None` for a
/// confirmation saved before the setup was part of it, which therefore asks again).
type Saved = (Vec<String>, Vec<String>, Option<Vec<String>>);

fn stored(project: &Project) -> Option<Saved> {
    let proof = project.extra.get("proof")?;
    let list = |key: &str| -> Option<Vec<String>> { proof[key].as_array()?.iter().map(|v| v.as_str().map(String::from)).collect() };
    Some((list("declared")?, list("commands")?, list("setup")))
}

/// The proof of a project as the screen shows it.
pub fn view(project: &Project) -> ProofView {
    let root = Path::new(&project.path);
    let (declared, source) = declared(root);
    let setup = setup_commands(&|path| root.join(path).is_file());
    match stored(project) {
        Some((was, commands, seen)) if was == declared && !commands.is_empty() && seen.as_ref() == Some(&setup) => {
            ProofView { commands, declared, source: source.into(), confirmed: true, changed: false, setup }
        }
        saved => {
            ProofView { commands: declared.clone(), declared, source: source.into(), confirmed: false, changed: saved.is_some(), setup }
        }
    }
}

/// What installs the project's dependencies before the proofs, found the way the proofs are (R137): `npm ci` when there is a
/// package lock, `cargo fetch` when there is a Cargo lock. `has` says whether a root file exists.
pub fn setup_commands(has: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut out = vec![];
    if has("package.json") && has("package-lock.json") {
        out.push("npm ci".to_string());
    }
    if has("Cargo.toml") && has("Cargo.lock") {
        out.push("cargo fetch".to_string());
    }
    out
}

/// The setup commands as steps: run like the proofs, with quiet flags so a long install stays inside the output limit.
pub fn setup_definitions(
    commands: &[String],
    target: &Path,
    resolve: &dyn Fn(&str) -> Option<Cli>,
) -> Result<Vec<Definition>, &'static str> {
    if commands.is_empty() {
        return Ok(vec![]);
    }
    let (mut steps, _) = definitions(commands, target, resolve).map_err(|_| "reason.setup.program")?;
    for (index, step) in steps.iter_mut().enumerate() {
        step.id = format!("setup-{}", index + 1);
        step.checks = vec![format!("S{}", index + 1)];
        match step.toolchain.as_str() {
            "npm" => step.argv.extend(["--no-audit".to_string(), "--no-fund".to_string()]),
            "cargo" => step.argv.push("--quiet".to_string()),
            _ => {}
        }
        step.validate()?;
    }
    Ok(steps)
}

/// Save the owner's confirmed list for the project. The list may differ from the declared one (the edit option); what is
/// kept is both, so a later change of the declared list asks again. `seen` is the setup and `seen_declared` the declared list
/// the owner was shown: if what the project would install or declare now is something else (a lock file appeared, or the
/// declaration was edited, meanwhile), nothing is saved and the screen reads and shows the list again.
pub fn confirm(project: &Project, commands: &[String], seen: &[String], seen_declared: &[String]) -> Result<Project, &'static str> {
    // Stale snapshots are refused first, so an invalid draft never hides that the files moved on.
    let (declared, _) = declared(Path::new(&project.path));
    if declared != seen_declared {
        return Err("reason.proof.declaredChanged");
    }
    let setup = setup_commands(&|path| Path::new(&project.path).join(path).is_file());
    if setup != seen {
        return Err("reason.proof.setupChanged");
    }
    if commands.is_empty() || commands.len() > MAX_COMMANDS {
        return Err("reason.proof.command");
    }
    for command in commands {
        command_tokens(command)?;
    }
    let mut next = project.clone();
    next.extra.insert("proof".into(), serde_json::json!({"declared": declared, "commands": commands, "setup": setup}));
    Ok(next)
}

/// The list a mission may run, or the key that refuses it: nothing is confirmed, or the project declares something else
/// than what was confirmed.
pub fn confirmed(project: &Project) -> Result<Vec<String>, &'static str> {
    let shown = view(project);
    if shown.confirmed { Ok(shown.commands) } else { Err("reason.proof.unconfirmed") }
}

/// The setup commands the owner confirmed together with the proofs: exactly what a mission installs with, or the key that
/// refuses it when the confirmation no longer holds (a lock file appeared or went away since).
pub fn confirmed_setup(project: &Project) -> Result<Vec<String>, &'static str> {
    let shown = view(project);
    if shown.confirmed { Ok(shown.setup) } else { Err("reason.proof.unconfirmed") }
}

/// The program a proof or setup command names, found without a shell. The official CLIs and native programs resolve as
/// everywhere (`cli::find_cli`); Node's own `npm` and `npx` are a `.cmd` shim that cannot start without a shell, so they
/// resolve to `node` with the tool's script from the same install, whatever the shim contains.
pub fn resolve_tool(name: &str) -> Option<Cli> {
    if let Some(cli) = crate::cli::find_cli(name) {
        return Some(cli);
    }
    let path = std::env::var("PATH").or_else(|_| std::env::var("Path")).unwrap_or_default();
    node_tool(name, &path, cfg!(windows), &|p| Path::new(p).is_file())
}

pub(crate) fn node_tool(name: &str, path_var: &str, windows: bool, exists: &dyn Fn(&str) -> bool) -> Option<Cli> {
    let script = match name {
        "npm" => "npm-cli.js",
        "npx" => "npx-cli.js",
        _ => return None,
    };
    let (sep, delim) = if windows { ('\\', ';') } else { ('/', ':') };
    let join = |dir: &str, file: &str| format!("{}{sep}{file}", dir.trim_end_matches(sep));
    for dir in path_var.split(delim).filter(|d| !d.is_empty()) {
        let shim = join(dir, &if windows { format!("{name}.cmd") } else { name.to_string() });
        let (node, cli) = if windows {
            (join(dir, "node.exe"), join(dir, &format!("node_modules{sep}npm{sep}bin{sep}{script}")))
        } else {
            // A Unix install keeps the package beside the prefix: <prefix>/bin/npm and <prefix>/lib/node_modules/npm.
            let prefix = dir.trim_end_matches(sep).rsplit_once(sep).map_or("", |(parent, _)| parent);
            (join(dir, "node"), format!("{prefix}{sep}lib{sep}node_modules{sep}npm{sep}bin{sep}{script}"))
        };
        if exists(&shim) && exists(&node) && exists(&cli) {
            return Some(Cli { command: node, prefix: vec![cli] });
        }
    }
    None
}

/// The proof of each command: the program is looked up by `resolve` (a bare name on the PATH in production), the
/// arguments are the command's own, and the process runs with the proof environment and a build folder outside the clone.
pub fn definitions(
    commands: &[String],
    target: &Path,
    resolve: &dyn Fn(&str) -> Option<Cli>,
) -> Result<(Vec<Definition>, crate::proc::Spawner), &'static str> {
    let (environment, folders) = super::canary::proof_environment(target);
    let mut out = vec![];
    for (index, line) in commands.iter().enumerate() {
        let tokens = command_tokens(line)?;
        let cli = resolve(&tokens[0]).ok_or("reason.proof.command")?;
        let definition = Definition {
            id: format!("proof-{}", index + 1),
            version: 1,
            program: PathBuf::from(cli.command),
            argv: cli.prefix.into_iter().chain(tokens[1..].iter().cloned()).collect(),
            checks: vec![format!("C{}", index + 1)],
            toolchain: tokens[0].clone(),
            dependencies: "project".into(),
            environment: environment.clone(),
            heavy: true,
        };
        definition.validate()?;
        out.push(definition);
    }
    if out.is_empty() || out.len() > MAX_COMMANDS {
        return Err("reason.proof.command");
    }
    Ok((out, crate::proc::isolated_spawner(environment, folders)))
}

// ----- the files the roles see -----

/// What the roles are told about the project.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shown {
    /// Files with their text, most useful first, within the budget.
    pub files: Vec<(String, String)>,
    /// Every usable tracked path.
    pub tree: Vec<String>,
    /// The folders (and root files) a plan may write under.
    pub writable: Vec<String>,
}

/// A tracked path a role may be told about: a clean relative path that is not a credential or a refused place (R124).
pub fn usable(path: &str) -> bool {
    normalized(path).is_ok_and(|clean| clean == path) && !crate::permissions::secret(Path::new(path), true)
}

fn priority(path: &str) -> u8 {
    let lower = path.to_lowercase();
    if !lower.contains('/') && (lower == "agents.md" || lower == "claude.md" || lower.starts_with("readme")) {
        0
    } else if !lower.contains('/') {
        1
    } else {
        2
    }
}

fn noise(path: &str) -> bool {
    let lower = path.to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    name.ends_with(".lock")
        || name.ends_with(".min.js")
        || name.ends_with(".map")
        || ["package-lock.json", "pnpm-lock.yaml", "yarn.lock", "cargo.lock"].contains(&name)
}

/// Read the committed tree at `base`: all usable tracked paths, the texts that fit the budget, and where a plan may write.
pub fn shown(executor: &mut impl Executor, root: &Path, base: &str) -> Result<Shown, GitError> {
    let all = git::tracked(executor, root, base)?;
    let mut usable_files: Vec<_> = all.into_iter().filter(|f| usable(&f.path)).collect();
    usable_files.sort_by(|a, b| a.path.cmp(&b.path));
    let tree: Vec<String> = usable_files.iter().map(|f| f.path.clone()).collect();
    let mut dirs = std::collections::BTreeSet::new();
    let mut roots = std::collections::BTreeSet::new();
    for f in &usable_files {
        match f.path.split_once('/') {
            Some((dir, _)) if ![".github", ".faber"].contains(&dir) => {
                dirs.insert(dir.to_string());
            }
            None => {
                roots.insert(f.path.clone());
            }
            _ => {}
        }
    }
    let writable: Vec<String> = dirs.into_iter().chain(roots).take(MAX_WRITABLE).collect();

    let mut candidates: Vec<_> = usable_files.iter().filter(|f| f.size > 0 && f.size <= FILE_MAX && !noise(&f.path)).collect();
    candidates.sort_by(|a, b| (priority(&a.path), a.size, &a.path).cmp(&(priority(&b.path), b.size, &b.path)));
    let (mut files, mut used, mut reads) = (vec![], 0usize, 0usize);
    for f in candidates {
        if files.len() >= MAX_SHOWN || reads >= MAX_READS || used + 200 > FILE_BUDGET {
            break;
        }
        if used + f.size as usize > FILE_BUDGET {
            continue;
        }
        reads += 1;
        let bytes = git::blob(executor, root, &f.oid)?;
        if bytes.contains(&0) {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else { continue };
        used += text.len();
        files.push((f.path.clone(), text));
    }
    Ok(Shown { files, tree, writable })
}

// ----- the setup of a mission -----

/// Everything a mission start takes besides the engine's own state.
pub struct Launch {
    pub repository: Repository,
    /// The confirmed proof commands (see `confirmed`).
    pub commands: Vec<String>,
    /// The confirmed setup commands (see `confirmed_setup`): what is installed is what the owner saw.
    pub setup: Vec<String>,
    /// The engine's data folder: the proofs build under `proof-target/<project>` there, outside every checkout.
    pub data: PathBuf,
    pub project: String,
    pub pins: Pins,
    /// The current authority, read at every effect.
    pub grant: Arc<dyn Fn() -> Result<Option<Grant>, GitError> + Send + Sync>,
    pub revision: u64,
    /// Called whenever the mission's progress changed.
    pub notify: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// The coordinator's setup for a project's mission (R137).
pub fn setup(launch: Launch, executor: &mut impl Executor, resolve: &dyn Fn(&str) -> Option<Cli>) -> Result<Setup, String> {
    let target = launch.data.join("proof-target").join(&launch.project);
    let (proofs, proof_spawn) = definitions(&launch.commands, &target, resolve)?;
    let seen = shown(executor, Path::new(&launch.repository.identity.root), &launch.repository.base).map_err(|e| e.key().to_string())?;
    if seen.writable.is_empty() {
        return Err("reason.project.empty".into());
    }
    let installs = setup_definitions(&launch.setup, &target, resolve)?;
    Ok(Setup {
        pins: launch.pins,
        proofs,
        expect_red: false,
        tree: seen.tree,
        proof_spawn,
        sources: seen.files,
        writable: seen.writable,
        grant: launch.grant,
        revision: launch.revision,
        micros: TURN_MICROS,
        turn_limit: Duration::from_secs(900),
        plan: None,
        notify: launch.notify,
        stage: Some(super::workspace::Stage {
            workspace: super::workspace::Workspace::new(&launch.data, &launch.project).map_err(str::to_string)?,
            repo_root: PathBuf::from(&launch.repository.identity.root),
            setup: installs,
        }),
    })
}

/// The models of a project mission and the CLI versions they run on: Claude plans and writes, Codex reviews.
pub fn product_pins() -> Result<Pins, String> {
    use super::canary::{REVIEWER_MODEL, WRITER_MODEL, cli, cli_version, pin};
    let writer = pin(WRITER_MODEL.0, WRITER_MODEL.1, &cli_version(WRITER_MODEL.0, &|| cli(WRITER_MODEL.0))?);
    let reviewer = pin(REVIEWER_MODEL.0, REVIEWER_MODEL.1, &cli_version(REVIEWER_MODEL.0, &|| cli(REVIEWER_MODEL.0))?);
    Ok(Pins { lead: writer.clone(), writer, reviewer })
}

/// Remember which model each role runs on, for the board. The pins are the product's choice, not a model's claim.
pub fn remember_roles(mission: &mut Mission, pins: &Pins) {
    let role = |pin: &super::worker::ModelPin| serde_json::json!({"vendor": pin.vendor, "model": pin.model});
    mission.extra.insert(
        "roles".into(),
        serde_json::json!({"lead": role(&pins.lead), "writer": role(&pins.writer), "reviewer": role(&pins.reviewer)}),
    );
}

// ----- the views -----

/// The step a running mission is on and who works on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressView {
    /// One of `coordinator::STEPS`, 1-based position in `number`.
    pub step: String,
    pub number: usize,
    pub total: usize,
    /// `lead`, `writer` or `reviewer` while a model works; none while the engine does (a branch, a proof, the landing).
    pub role: Option<String>,
    pub vendor: Option<Vendor>,
    pub model: Option<String>,
}

pub fn progress(journal: &Journal, mission: &Mission) -> Option<ProgressView> {
    if matches!(mission.state, MissionState::Done) {
        return None;
    }
    let step = coordinator::current_step(journal)?;
    let role = match step {
        "plan" => Some("lead"),
        "write" => Some("writer"),
        "review" => Some("reviewer"),
        _ => None,
    };
    let pin = role.and_then(|r| mission.extra.get("roles").map(|roles| &roles[r]));
    Some(ProgressView {
        step: step.into(),
        number: coordinator::STEPS.iter().position(|s| *s == step).map_or(1, |i| i + 1),
        total: coordinator::STEPS.len(),
        role: role.map(String::from),
        vendor: pin.and_then(|p| serde_json::from_value(p["vendor"].clone()).ok()),
        model: pin.and_then(|p| p["model"].as_str().map(String::from)),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportCheck {
    pub id: String,
    pub result: String,
}

/// What a finished mission leaves for the owner: where the work is and what was proven. The owner's own branch is never
/// touched, so the report says which branch holds the work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    pub branch: String,
    pub head: Option<String>,
    pub checks: Vec<ReportCheck>,
    pub seconds: Option<u64>,
}

pub fn report_view(journal: &Journal, mission: &Mission) -> Option<ReportView> {
    use super::report::UnknownOr;
    if mission.state != MissionState::Done {
        return None;
    }
    let report = journal.report().ok().flatten()?;
    let known = |v: &UnknownOr<String>| match v {
        UnknownOr::Known(s) => Some(s.clone()),
        UnknownOr::Unknown(_) => None,
    };
    let landing = report.tasks.values().next();
    let head = landing.and_then(|t| known(&t.commit_receipt.head)).or_else(|| report.commit_receipt.as_ref().and_then(|c| known(&c.head)));
    let checks = landing
        .map(|t| &t.checks)
        .unwrap_or(&report.checks)
        .iter()
        .map(|c| ReportCheck { id: c.id.clone(), result: c.result.clone() })
        .collect();
    let seconds = match report.timings.duration_seconds {
        UnknownOr::Known(s) => Some(s),
        UnknownOr::Unknown(_) => None,
    };
    Some(ReportView { branch: mission.branch.clone(), head, checks, seconds })
}

/// The reason a mission waits for the owner: blocked for good, or parked on something only a person can settle. A wait
/// for budget or quota is not one; the owner resumes it when they like.
pub fn needs(mission: &Mission) -> Option<String> {
    let reason = mission.reason.clone()?;
    match mission.state {
        MissionState::Blocked => Some(reason),
        MissionState::Paused if matches!(coordinator::classify(&reason), Stop::Park(_)) => Some(reason),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGENTS: &str = "# Guide\n\nSome text.\n\n## Rules\n\n```\nnot a proof\n```\n\n## Proof commands\n\nAll exit 0 before any commit:\n\n```bash\nnpm run lint\n# a comment\n$ cargo fmt --all --check\ncargo clippy --workspace -- -D warnings && cargo t\n```\n\n## Stack\n\n```\nignored\n```\n";

    #[test]
    fn npm_resolves_to_node_and_its_own_script_not_to_a_shim_that_needs_a_shell() {
        let win = |p: &str| p.replace('/', "\\");
        let files: Vec<String> =
            ["C:/Node/npm.cmd", "C:/Node/node.exe", "C:/Node/node_modules/npm/bin/npm-cli.js"].iter().map(|p| win(p)).collect();
        let present = |p: &str| files.iter().any(|f| f == p);
        let found = node_tool("npm", &win("C:/Other;C:/Node"), true, &present).unwrap();
        assert_eq!(found.command, win("C:/Node/node.exe"));
        assert_eq!(found.prefix, [win("C:/Node/node_modules/npm/bin/npm-cli.js")]);
        assert!(node_tool("npx", &win("C:/Node"), true, &present).is_none(), "npx has no script here");
        assert!(node_tool("curl", &win("C:/Node"), true, &present).is_none(), "only npm and npx");
        assert!(node_tool("npm", &win("C:/Other"), true, &present).is_none());
        let unix = |p: &str| ["/opt/node/bin/npm", "/opt/node/bin/node", "/opt/node/lib/node_modules/npm/bin/npm-cli.js"].contains(&p);
        assert_eq!(
            node_tool("npm", "/usr/bin:/opt/node/bin", false, &unix).unwrap().prefix,
            ["/opt/node/lib/node_modules/npm/bin/npm-cli.js"]
        );
    }

    #[test]
    fn the_declared_list_is_the_code_block_under_the_proof_commands_heading() {
        assert_eq!(
            parse_declared(AGENTS),
            ["npm run lint", "cargo fmt --all --check", "cargo clippy --workspace -- -D warnings", "cargo t"]
        );
        assert!(parse_declared("# Proof\n```\nnpm test\n```").is_empty(), "the heading must say proof commands");
        assert!(parse_declared("## Proof commands\nno block here\n## Next\n```\nx\n```").is_empty());
        assert_eq!(parse_declared("### proof commands (CI)\n```\nmake test\n```"), ["make test"]);
    }

    #[test]
    fn a_command_is_plain_words_naming_a_program_not_a_path_or_a_shell_line() {
        assert_eq!(command_tokens("cargo test --workspace").unwrap(), ["cargo", "test", "--workspace"]);
        for bad in [
            "",
            "   ",
            "cargo test | tee out",
            "npm test > log",
            "echo $HOME",
            "./run.sh",
            "C:\\tools\\x.exe a",
            "a/b c",
            "x \"quoted\"",
            "rm -rf *",
        ] {
            assert_eq!(command_tokens(bad), Err("reason.proof.command"), "{bad:?}");
        }
    }

    fn folder(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("faber-mission-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for (path, text) in files {
            std::fs::write(root.join(path), text).unwrap();
        }
        root
    }

    #[test]
    fn the_declared_list_falls_back_to_the_detected_default() {
        let none = folder("none", &[]);
        assert_eq!(declared(&none), (vec![], "none"));
        let node = folder("node", &[("package.json", r#"{"scripts":{"test":"vitest"}}"#)]);
        assert_eq!(declared(&node), (vec!["npm test".to_string()], "detected"));
        let no_script = folder("noscript", &[("package.json", r#"{"scripts":{"build":"x"}}"#), ("Cargo.toml", "[package]")]);
        assert_eq!(declared(&no_script), (vec!["cargo test".to_string()], "detected"));
        let agents =
            folder("agents", &[("AGENTS.md", AGENTS), ("package.json", r#"{"scripts":{"test":"x"}}"#), ("CLAUDE.md", "@AGENTS.md")]);
        assert_eq!(declared(&agents).1, "agents");
        let claude = folder("claude", &[("AGENTS.md", "no proof here"), ("CLAUDE.md", "## Proof commands\n```\nmake check\n```")]);
        assert_eq!(declared(&claude), (vec!["make check".to_string()], "claude"));
    }

    #[test]
    fn an_unconfirmed_proof_refuses_a_mission_and_a_changed_list_asks_again() {
        let root = folder("trust", &[("AGENTS.md", "## Proof commands\n```\nnpm test\n```")]);
        let project = Project::new("p1".into(), "P".into(), root.to_string_lossy().into_owned());
        assert_eq!(confirmed(&project), Err("reason.proof.unconfirmed"));
        let shown = view(&project);
        assert_eq!((shown.confirmed, shown.changed, shown.commands.clone()), (false, false, vec!["npm test".to_string()]));

        // The owner confirms an edited list; it is what runs.
        let edited = confirm(&project, &["npm test".to_string(), "npm run lint".to_string()], &[], &["npm test".to_string()]).unwrap();
        assert_eq!(confirmed(&edited).unwrap(), ["npm test", "npm run lint"]);
        assert!(view(&edited).confirmed);

        // A list that needs a shell is never saved.
        assert_eq!(confirm(&project, &["npm test | tee x".to_string()], &[], &["npm test".to_string()]), Err("reason.proof.command"));
        assert_eq!(confirm(&project, &[], &[], &["npm test".to_string()]), Err("reason.proof.command"));

        // The project's declaration changes: the confirmation no longer holds.
        std::fs::write(root.join("AGENTS.md"), "## Proof commands\n```\nnpm test\nnpm run e2e\n```").unwrap();
        assert_eq!(confirmed(&edited), Err("reason.proof.unconfirmed"));
        let again = view(&edited);
        assert!(!again.confirmed && again.changed);
        assert_eq!(again.commands, ["npm test", "npm run e2e"]);
    }

    #[test]
    fn the_setup_is_part_of_the_confirmation_and_a_change_asks_again() {
        let root = folder("setup", &[("AGENTS.md", "## Proof commands\n```\nnpm test\n```"), ("package.json", "{}")]);
        let project = Project::new("p1".into(), "P".into(), root.to_string_lossy().into_owned());
        let confirmed_now = confirm(&project, &["npm test".to_string()], &[], &["npm test".to_string()]).unwrap();
        assert_eq!(confirmed_setup(&confirmed_now).unwrap(), Vec::<String>::new());
        // A lock file appears: npm ci would now run, which the owner never saw.
        std::fs::write(root.join("package-lock.json"), "{}").unwrap();
        assert_eq!(confirmed_setup(&confirmed_now), Err("reason.proof.unconfirmed"));
        assert_eq!(confirmed(&confirmed_now), Err("reason.proof.unconfirmed"));
        assert!(view(&confirmed_now).changed);
        // The screen still shows no setup: confirming it is refused, so the owner never approves what they did not see.
        assert_eq!(confirm(&confirmed_now, &["npm test".to_string()], &[], &["npm test".to_string()]), Err("reason.proof.setupChanged"));
        let again = confirm(&confirmed_now, &["npm test".to_string()], &["npm ci".to_string()], &["npm test".to_string()]).unwrap();
        assert_eq!(confirmed_setup(&again).unwrap(), ["npm ci"]);
        // A confirmation saved before the setup was part of it asks again.
        let mut old = again.clone();
        old.extra.insert("proof".into(), serde_json::json!({"declared": ["npm test"], "commands": ["npm test"]}));
        assert_eq!(confirmed(&old), Err("reason.proof.unconfirmed"));
    }

    /// A repository that answers the two reads the module makes: the tree listing and one blob by id.
    struct Repo(Vec<(String, String, &'static str, &'static str)>);
    impl Executor for Repo {
        fn execute(&mut self, call: &git::Invocation) -> Result<git::Output, GitError> {
            let oid = |n: usize| format!("{n:040x}");
            let stdout = if call.args.iter().any(|a| a == "ls-tree") {
                let mut out = String::new();
                for (n, (path, text, mode, kind)) in self.0.iter().enumerate() {
                    out.push_str(&format!("{mode} {kind} {} {:>7}\t{path}\0", oid(n + 1), text.len()));
                }
                out.into_bytes()
            } else {
                let wanted = call.args.last().unwrap();
                let n = self.0.iter().enumerate().find(|(n, _)| &oid(n + 1) == wanted).map(|(_, f)| f.1.clone());
                n.unwrap_or_default().into_bytes()
            };
            Ok(git::Output { success: true, stdout })
        }
    }

    #[test]
    fn the_roles_see_every_usable_tracked_path_and_the_texts_that_fit_most_useful_first() {
        let file = |path: &str, text: &str| (path.to_string(), text.to_string(), "100644", "blob");
        let mut files = vec![
            file("src/zeta.rs", "pub fn z() {}"),
            file("README.md", "# P"),
            file("AGENTS.md", "rules"),
            file(".env", "TOKEN=abc"),
            file(".claude/settings.json", "{}"),
            file("keys/server.pem", "-----BEGIN"),
            file("Cargo.lock", "lock"),
            file("src/big.rs", &"x".repeat(60 * 1024)),
            file("assets/blob.bin", "a\0b"),
            file(".github/workflows/ci.yml", "on: push"),
            file("docs/guide.md", "guide"),
        ];
        files.push(("link".into(), "target".into(), "120000", "blob"));
        files.push(("vendor/sub".into(), String::new(), "160000", "commit"));
        let seen = shown(&mut Repo(files), Path::new("."), &"1".repeat(40)).unwrap();

        // Every usable path is named, and nothing a role must not see.
        assert_eq!(
            seen.tree,
            [
                ".github/workflows/ci.yml",
                "AGENTS.md",
                "Cargo.lock",
                "README.md",
                "assets/blob.bin",
                "docs/guide.md",
                "src/big.rs",
                "src/zeta.rs"
            ]
        );
        // Texts: the project's own guides first (smallest first), then root files, then the rest by size; no lock, no oversize file, no binary.
        let names: Vec<&str> = seen.files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(names, ["README.md", "AGENTS.md", "docs/guide.md", ".github/workflows/ci.yml", "src/zeta.rs"]);
        // A plan may write under the project's folders and root files, never under CI or the mission records.
        assert_eq!(seen.writable, ["assets", "docs", "src", "AGENTS.md", "Cargo.lock", "README.md"]);
    }

    #[test]
    fn the_budget_bounds_what_the_roles_are_shown() {
        let many: Vec<_> = (0..300).map(|n| (format!("src/f{n:03}.rs"), "x".repeat(2000), "100644", "blob")).collect();
        let seen = shown(&mut Repo(many), Path::new("."), &"1".repeat(40)).unwrap();
        assert_eq!(seen.tree.len(), 300);
        assert!(seen.files.len() <= MAX_SHOWN, "{}", seen.files.len());
        assert!(seen.files.iter().map(|(_, t)| t.len()).sum::<usize>() <= FILE_BUDGET);
    }

    #[test]
    fn only_a_block_or_a_parked_effect_needs_the_owner() {
        let mut m = Mission::new("m1", "p1", "repo", "Goal", "base", 1, 100);
        m.reason = Some("reason.queue.review".into());
        assert_eq!(needs(&m), None, "a planning mission has nothing to settle");
        m.state = MissionState::Blocked;
        assert_eq!(needs(&m).as_deref(), Some("reason.queue.review"));
        m.state = MissionState::Paused;
        assert_eq!(needs(&m), None, "a paused mission with a block key is the owner's Resume, not a need");
        m.reason = Some("reason.lease.live".into());
        assert_eq!(needs(&m).as_deref(), Some("reason.lease.live"), "a parked effect is only the owner's to settle");
        m.reason = Some("reason.budget.cap".into());
        assert_eq!(needs(&m), None, "a wait for budget is not a need");
        m.state = MissionState::Running;
        m.reason = None;
        assert_eq!(needs(&m), None);
    }

    #[test]
    fn credential_and_refused_paths_are_never_shown_to_a_role() {
        for path in ["src/a.rs", "docs/readme.md", "README.md", ".github/workflows/ci.yml", ".env.example"] {
            assert!(usable(path), "{path}");
        }
        for path in [
            ".env",
            ".env.production",
            ".claude/settings.json",
            ".codex/config.toml",
            "sub/.ssh/id_rsa",
            "keys/server.pem",
            "a/credentials.json",
            ".npmrc",
            "x/../y",
            "C:/x",
            "/abs",
            "a\\b",
        ] {
            assert!(!usable(path), "{path}");
        }
    }
}
