//! The proof a project declares and its owner confirmed (R137): the commands of its rules file or a detected default,
//! what installs its dependencies first, and the confirmation Settings > Projects saves. Faber's checks read this to know
//! what to run (`crate::verify`).
//!
//! Nothing here runs a command. It reads the project's own rules and turns them into the list the screen shows.
use crate::cli::Cli;
use crate::model::Project;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The most proof commands a project may declare.
pub const MAX_COMMANDS: usize = 12;
const DOC_MAX: u64 = 256 * 1024;

/// What the Projects screen shows about a project's proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofView {
    /// The commands Faber's checks run: the confirmed ones, or the declared ones while nothing is confirmed.
    pub commands: Vec<String>,
    /// The commands the project declares now (its rules file, or a detected default).
    pub declared: Vec<String>,
    /// `agents`, `claude`, `detected` or `none`.
    pub source: String,
    /// The owner confirmed exactly what the project declares now.
    pub confirmed: bool,
    /// A list was confirmed before and the project declares something else now.
    pub changed: bool,
    /// What runs first in the clean copy, found from the project's lock files (shown with the proof; the owner does not edit it).
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
    view_in(project, Path::new(&project.path))
}

/// The proof as it holds in `folder`, the project folder or a copy of it: the declared list and the setup come from the
/// folder, but the confirmation is weighed against the setup the person saw, the project folder's. A copy's own state (a
/// gitignored `Cargo.lock` a build created there) adds setup lines the person never saw and does not drop their list.
pub fn view_in(project: &Project, folder: &Path) -> ProofView {
    let (declared, source) = declared(folder);
    let setup = setup_commands(&|path| folder.join(path).is_file());
    let person = Path::new(&project.path);
    let shown = if folder == person { setup.clone() } else { setup_commands(&|path| person.join(path).is_file()) };
    match stored(project) {
        Some((was, commands, seen)) if was == declared && !commands.is_empty() && seen.as_ref() == Some(&shown) => {
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

/// Node's own `npm` and `npx` are a `.cmd` shim that cannot start without a shell, so they resolve to `node` with the
/// tool's script from the same install, whatever the shim contains. Any other name is not found here.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
        let root = std::env::temp_dir().join(format!("faber-checks-{name}-{}", std::process::id()));
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
    fn an_unconfirmed_proof_shows_the_declared_list_and_a_changed_one_asks_again() {
        let root = folder("trust", &[("AGENTS.md", "## Proof commands\n```\nnpm test\n```")]);
        let project = Project::new("p1".into(), "P".into(), root.to_string_lossy().into_owned());
        assert!(!view(&project).confirmed);
        let shown = view(&project);
        assert_eq!((shown.confirmed, shown.changed, shown.commands.clone()), (false, false, vec!["npm test".to_string()]));

        // The owner confirms an edited list; it is what runs.
        let edited = confirm(&project, &["npm test".to_string(), "npm run lint".to_string()], &[], &["npm test".to_string()]).unwrap();
        assert_eq!(view(&edited).commands, ["npm test", "npm run lint"]);
        assert!(view(&edited).confirmed);

        // A list that needs a shell is never saved.
        assert_eq!(confirm(&project, &["npm test | tee x".to_string()], &[], &["npm test".to_string()]), Err("reason.proof.command"));
        assert_eq!(confirm(&project, &[], &[], &["npm test".to_string()]), Err("reason.proof.command"));

        // The project's declaration changes: the confirmation no longer holds.
        std::fs::write(root.join("AGENTS.md"), "## Proof commands\n```\nnpm test\nnpm run e2e\n```").unwrap();
        assert!(!view(&edited).confirmed);
        let again = view(&edited);
        assert!(!again.confirmed && again.changed);
        assert_eq!(again.commands, ["npm test", "npm run e2e"]);
    }

    #[test]
    fn the_setup_is_part_of_the_confirmation_and_a_change_asks_again() {
        let root = folder("setup", &[("AGENTS.md", "## Proof commands\n```\nnpm test\n```"), ("package.json", "{}")]);
        let project = Project::new("p1".into(), "P".into(), root.to_string_lossy().into_owned());
        let confirmed_now = confirm(&project, &["npm test".to_string()], &[], &["npm test".to_string()]).unwrap();
        assert!(view(&confirmed_now).confirmed && view(&confirmed_now).setup.is_empty());
        // A lock file appears: npm ci would now run, which the owner never saw.
        std::fs::write(root.join("package-lock.json"), "{}").unwrap();
        assert!(!view(&confirmed_now).confirmed);
        assert!(view(&confirmed_now).changed);
        // The screen still shows no setup: confirming it is refused, so the owner never approves what they did not see.
        assert_eq!(confirm(&confirmed_now, &["npm test".to_string()], &[], &["npm test".to_string()]), Err("reason.proof.setupChanged"));
        let again = confirm(&confirmed_now, &["npm test".to_string()], &["npm ci".to_string()], &["npm test".to_string()]).unwrap();
        assert!(view(&again).confirmed);
        assert_eq!(view(&again).setup, ["npm ci"]);
        // A confirmation saved before the setup was part of it asks again.
        let mut old = again.clone();
        old.extra.insert("proof".into(), serde_json::json!({"declared": ["npm test"], "commands": ["npm test"]}));
        assert!(!view(&old).confirmed);
    }

    #[test]
    fn a_copy_keeps_the_confirmed_list_when_its_own_lock_file_adds_a_setup_line() {
        let guide = "## Proof commands\n```\ncargo test\n```";
        let root = folder("person", &[("AGENTS.md", guide), ("Cargo.toml", "[package]")]);
        let project = Project::new("p1".into(), "P".into(), root.to_string_lossy().into_owned());
        let mine = ["cargo test".to_string(), "cargo fmt --check".to_string()];
        let confirmed_now = confirm(&project, &mine, &[], &["cargo test".to_string()]).unwrap();
        // The copy's build made a gitignored lock file: it fetches first, and still passes the person's whole list.
        let copy = folder("copy", &[("AGENTS.md", guide), ("Cargo.toml", "[package]"), ("Cargo.lock", "")]);
        let inside = view_in(&confirmed_now, &copy);
        assert!(inside.confirmed);
        assert_eq!((inside.commands, inside.setup), (mine.to_vec(), vec!["cargo fetch".to_string()]));
        // The person's folder gets a lock file the person never saw: that asks again, in the copy too.
        std::fs::write(root.join("Cargo.lock"), "").unwrap();
        assert!(!view_in(&confirmed_now, &copy).confirmed);
    }
}
