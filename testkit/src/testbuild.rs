//! Building the test app: the screen with Vite in test mode, then the shell with the Tauri CLI. Both are started as
//! `node <script>`, never through `npm`, a `.cmd` shim or a shell. The production `dist/` is never touched.

use crate::app::Paths;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::SystemTime;

/// Where Vite writes the test screen (relative to the repository). The shell embeds it.
pub const TEST_DIST: &str = "dist-test";

/// The arguments of the Vite build. `vite.config.ts` has `root: 'ui'`, so `--outDir` is relative to that folder.
pub fn vite_args() -> Vec<String> {
    ["node_modules/vite/bin/vite.js", "build", "--mode", "test", "--outDir", "../dist-test", "--emptyOutDir"].map(String::from).to_vec()
}

/// The arguments of the Tauri build. After the `--`, Cargo's: its own target folder (absolute, because Cargo reads it
/// from `src-tauri`) and the number of jobs.
pub fn tauri_args(target_dir: &Path, jobs: usize) -> Vec<String> {
    let mut args: Vec<String> = [
        "node_modules/@tauri-apps/cli/tauri.js",
        "build",
        "--debug",
        "--no-bundle",
        "--features",
        "test-hooks",
        "--config",
        "src-tauri/tauri.test.conf.json",
        "--",
        "--target-dir",
    ]
    .map(String::from)
    .to_vec();
    args.push(target_dir.to_string_lossy().into_owned());
    args.extend(["-j".to_string(), jobs.to_string()]);
    args
}

/// A fingerprint of a folder: every file's relative path, size and modification time. `absent` when it does not exist.
/// The test build must leave the production `dist/` exactly as it found it.
pub fn fingerprint(dir: &Path) -> String {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else if let Ok(meta) = entry.metadata() {
                let modified = meta.modified().ok().and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
                out.push(format!("{} {} {modified}", path.strip_prefix(base).unwrap_or(&path).display(), meta.len()));
            }
        }
    }
    if !dir.exists() {
        return "absent".into();
    }
    let mut lines = Vec::new();
    walk(dir, dir, &mut lines);
    lines.sort();
    faber_engine::hex(&Sha256::digest(lines.join("\n")))
}

/// What the Vite build of the test screen reads: the screen, its checks, the one shell file they import, and the build's
/// own configuration and packages. `a_screen_build_reads_nothing_outside_its_inputs` fails when an import reaches past them.
const VITE_INPUTS: [&str; 11] = [
    "ui",
    "tests/ui",
    "tests/ux",
    "src-tauri/src/themes.json",
    "src-tauri/src/pane/act.js",
    "vite.config.ts",
    "scripts/woff2-only.mjs",
    "package.json",
    "package-lock.json",
    "tsconfig.json",
    "node_modules/.package-lock.json",
];

/// Where a finished Vite build leaves the digest of what it read, inside its own output, which `--emptyOutDir` clears.
const VITE_STAMP: &str = ".faber-inputs";

/// A digest of every file the Vite build reads, by content (a checkout changes times, not contents), and of its arguments.
pub fn vite_inputs(root: &Path) -> String {
    fn walk(path: &Path, root: &Path, hash: &mut Sha256) {
        if path.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(path).into_iter().flatten().flatten().map(|e| e.path()).collect();
            entries.sort();
            for entry in entries {
                walk(&entry, root, hash);
            }
        } else if let Ok(bytes) = std::fs::read(path) {
            hash.update(path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/").as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(&bytes);
        }
    }
    let mut hash = Sha256::new();
    hash.update(vite_args().join(" ").as_bytes());
    for input in VITE_INPUTS {
        walk(&root.join(input), root, &mut hash);
    }
    faber_engine::hex(&hash.finalize())
}

/// Is the test screen in `dist-test` already the build of these inputs?
fn vite_is_fresh(root: &Path, inputs: &str) -> bool {
    let out = root.join(TEST_DIST);
    out.join("index.html").is_file() && std::fs::read_to_string(out.join(VITE_STAMP)).is_ok_and(|stamp| stamp == inputs)
}

/// Builds the test screen with Vite, unless `dist-test` already holds the build of the same inputs: a change to the engine
/// or the shell alone left the 25-30 s Vite build to make the same files again.
fn build_screen(paths: &Paths) -> Result<(), String> {
    let inputs = vite_inputs(&paths.root);
    if vite_is_fresh(&paths.root, &inputs) {
        crate::timing::step("vite-skipped", std::time::Duration::ZERO);
        return Ok(());
    }
    let mut vite = build_priority(Command::new("node"));
    vite.current_dir(&paths.root).args(vite_args());
    run("Vite build", vite)?;
    std::fs::write(paths.root.join(TEST_DIST).join(VITE_STAMP), inputs).map_err(|e| format!("cannot stamp the Vite build: {e}"))
}

/// Is this one of the variables `cargo run` hands the program it runs? `cargo t` is such a program. Build scripts watch some
/// of them (ring's: `CARGO_MANIFEST_DIR`, `CARGO_PKG_NAME`, `CARGO_PKG_VERSION_*`), so a Cargo started with them rebuilt ring
/// and everything above it (rustls, reqwest, the engine, the app), and the next plain `cargo` rebuilt it all again.
pub fn is_run_env(name: &str) -> bool {
    name.starts_with("CARGO_PKG_")
        || name.starts_with("CARGO_MANIFEST_")
        || matches!(name, "CARGO_CRATE_NAME" | "CARGO_BIN_NAME" | "CARGO_PRIMARY_PACKAGE" | "OUT_DIR")
}

/// A command for Cargo, or for a tool that starts Cargo, without the variables of `cargo run` (`is_run_env`).
pub fn without_run_env(mut command: Command) -> Command {
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(is_run_env) {
            command.env_remove(name);
        }
    }
    command
}

/// Cargo, or the one Cargo that started this runner, at the build priority.
pub fn cargo_command() -> Command {
    build_priority(without_run_env(Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))))
}

/// `IDLE_PRIORITY_CLASS`: a program started with it, and every program it starts, runs only on a processor nothing else
/// wants.
#[cfg(windows)]
const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;

/// A build on a person's PC runs at idle priority, so a game or whatever else the person does keeps the processor; Windows
/// hands the class to every program the build starts (rustc, the linker). CI runners are the job's alone and keep the
/// normal class. Only builds get it: the app under test keeps the normal class, so its processor and frame readings mean
/// what they meant before.
#[cfg_attr(not(windows), allow(unused_mut))]
pub fn build_priority(mut command: Command) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Its output is piped, so it needs no console window either.
        command.creation_flags(build_flags(crate::plan::on_ci()));
    }
    command
}

/// The flags a build starts with: no console window, and idle priority unless on CI.
#[cfg(windows)]
fn build_flags(ci: bool) -> u32 {
    crate::contain::NO_WINDOW | if ci { 0 } else { IDLE_PRIORITY_CLASS }
}

/// Runs a command and reads what it prints while it prints it: `seen` hears each line of its standard output at once (Cargo
/// says there when its build ends and its tests begin), and both streams come back whole, as `Command::output` gives them.
pub fn streamed(command: &mut Command, mut seen: impl FnMut(&str)) -> std::io::Result<Output> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stderr = child.stderr.take();
    let errors = std::thread::spawn(move || {
        let mut text = Vec::new();
        if let Some(mut pipe) = stderr {
            let _ = pipe.read_to_end(&mut text);
        }
        text
    });
    let stdout = match child.stdout.take() {
        Some(pipe) => read_lines(pipe, &mut seen)?,
        None => Vec::new(),
    };
    let status = child.wait()?;
    Ok(Output { status, stdout, stderr: errors.join().unwrap_or_default() })
}

/// Reads `pipe` to its end, telling `seen` each line (with its line end) as soon as it is whole.
fn read_lines(pipe: impl Read, mut seen: impl FnMut(&str)) -> std::io::Result<Vec<u8>> {
    let (mut reader, mut text, mut line) = (BufReader::new(pipe), Vec::new(), Vec::new());
    while reader.read_until(b'\n', &mut line)? > 0 {
        seen(&String::from_utf8_lossy(&line));
        text.append(&mut line);
    }
    Ok(text)
}

fn run(what: &str, mut command: Command) -> Result<(), String> {
    let _build = crate::timing::span("build", what);
    let output = command.output().map_err(|e| format!("cannot start the {what}: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "the {what} failed ({})\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Builds the fake CLI next to the runner, with `jobs` cores for Cargo. `cargo run` builds only the binary it runs.
pub fn ensure_fake_cli(paths: &Paths, jobs: usize) -> Result<(), String> {
    let mut command = cargo_command();
    command.current_dir(&paths.root).args(["build", "--quiet", "-j", &jobs.to_string(), "--package", "faber-testkit", "--bin", "fake-cli"]);
    run("build of the fake CLI", command)?;
    #[cfg(windows)]
    {
        let mut example = cargo_command();
        // This guard-only host needs no PDB; MSVC hit LNK1318 LIMIT while generating it.
        example.current_dir(&paths.root).args([
            "rustc",
            "--quiet",
            "-j",
            &jobs.to_string(),
            "--package",
            "faber-engine",
            "--example",
            "cache_probe",
            "--",
            "-C",
            "debuginfo=0",
            "-C",
            "link-arg=/DEBUG:NONE",
        ]);
        run("build of the cache example guard host", example)?;
    }
    if paths.fake_cli.exists() { Ok(()) } else { Err(format!("the fake CLI is not at {}", paths.fake_cli.display())) }
}

/// The programs a screen run starts: what `--build-only` leaves and `--prebuilt` needs. CI hands
/// exactly these from its build job to its screen parts (R163), so a scenario that needs another one fails there at once.
pub fn built_programs(paths: &Paths) -> Vec<std::path::PathBuf> {
    let suffix = std::env::consts::EXE_SUFFIX;
    let debug = paths.fake_cli.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut list = vec![paths.app.clone(), paths.fake_cli.clone(), debug.join(format!("faber-test{suffix}"))];
    if cfg!(windows) {
        list.push(debug.join(format!("examples/cache_probe{suffix}")));
    }
    list
}

/// Every program a `--build-only` run leaves, or the ones that are missing.
pub fn prebuilt(paths: &Paths) -> Result<(), String> {
    let missing: Vec<String> = built_programs(paths).iter().filter(|p| !p.is_file()).map(|p| p.display().to_string()).collect();
    if missing.is_empty() { Ok(()) } else { Err(format!("--prebuilt, but these programs were not built: {}", missing.join(", "))) }
}

/// Builds the test app under Cargo's target directory, once per run, with `jobs` cores for Cargo. The Tauri build makes
/// the shell with the features and settings it ships with.
pub fn build_test_app(paths: &Paths, jobs: usize) -> Result<(), String> {
    let production = paths.root.join("dist");
    let before = fingerprint(&production);
    build_screen(paths)?;
    build_shell(paths, jobs, &production, &before)
}

/// The fake CLI and the test app for a run on this PC. Vite, which mostly waits on one core, builds the screen while Cargo
/// builds the fake CLI and the cache example; the Tauri build waits for both, so Cargo never runs twice at once here.
pub fn build_for_run(paths: &Paths, jobs: usize) -> Result<(), String> {
    let production = paths.root.join("dist");
    let before = fingerprint(&production);
    both(|| build_screen(paths), || ensure_fake_cli(paths, jobs))?;
    build_shell(paths, jobs, &production, &before)
}

/// The Tauri build of the shell, which embeds the test screen Vite left in `dist-test`.
fn build_shell(paths: &Paths, jobs: usize, production: &Path, before: &str) -> Result<(), String> {
    let mut tauri = build_priority(without_run_env(Command::new("node")));
    tauri.current_dir(&paths.root).args(tauri_args(&paths.target, jobs));
    run("Tauri build", tauri)?;
    if fingerprint(production) != before {
        return Err("the test build changed dist/, which only the production build may write".into());
    }
    if !paths.app.exists() {
        return Err(format!("the test build did not produce {}", paths.app.display()));
    }
    Ok(())
}

/// `--build-only`, CI's build job: the fake CLI and the cache example in Cargo's own folder while the test app builds in
/// its own (`paths.target`), so the two Cargo builds share no lock and each compiles the engine for itself at once (68-79 s
/// became 45-49 s with the engine's crates rebuilt from scratch). A local run keeps them one after the other, so it never
/// uses more than its `jobs` cores.
pub fn build_side_by_side(paths: &Paths, jobs: usize) -> Result<(), String> {
    both(|| ensure_fake_cli(paths, jobs), || build_test_app(paths, jobs))
}

/// Runs `first` on its own thread while `second` runs here, and reports every failure of the two.
fn both(first: impl FnOnce() -> Result<(), String> + Send, second: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    std::thread::scope(|scope| {
        let first = scope.spawn(first);
        let second = second();
        let first = first.join().unwrap_or_else(|_| Err("a build thread stopped".into()));
        let failures: Vec<String> = [first, second].into_iter().filter_map(Result::err).collect();
        if failures.is_empty() { Ok(()) } else { Err(failures.join("\n")) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_builds_run_at_the_same_time_and_both_failures_are_told() {
        use std::sync::mpsc::channel;
        use std::time::Duration;
        // Each side waits for the other's message: run one after the other, the first would wait in vain.
        let (to_second, from_first) = channel();
        let (to_first, from_second) = channel();
        let wait = Duration::from_secs(10);
        let met = both(
            move || {
                let _ = to_second.send(());
                from_second.recv_timeout(wait).map_err(|_| "the second build never ran meanwhile".to_string())
            },
            move || {
                let _ = to_first.send(());
                from_first.recv_timeout(wait).map_err(|_| "the first build never ran meanwhile".to_string())
            },
        );
        assert_eq!(met, Ok(()));
        assert_eq!(both(|| Err("fake CLI".into()), || Err("test app".into())), Err("fake CLI\ntest app".into()));
        assert_eq!(both(|| Ok(()), || Err("test app".into())), Err("test app".into()));
        assert_eq!(both(|| Err("fake CLI".into()), || Ok(())), Err("fake CLI".into()));
    }

    #[test]
    fn a_stream_tells_each_line_as_it_comes_and_keeps_the_whole_text() {
        let mut lines = Vec::new();
        let text =
            read_lines(std::io::Cursor::new("one\n{\"reason\":\"build-finished\"}\nlast without end"), |line| lines.push(line.to_string()))
                .unwrap();
        assert_eq!(lines, ["one\n", "{\"reason\":\"build-finished\"}\n", "last without end"]);
        assert_eq!(text, b"one\n{\"reason\":\"build-finished\"}\nlast without end");
    }

    #[cfg(windows)]
    #[test]
    fn a_build_on_a_persons_pc_starts_at_idle_priority_and_never_with_a_console_window() {
        assert_eq!(build_flags(true), crate::contain::NO_WINDOW);
        assert_eq!(build_flags(false), crate::contain::NO_WINDOW | IDLE_PRIORITY_CLASS);
    }

    #[test]
    fn cargo_starts_without_the_variables_cargo_run_hands_the_runner() {
        for name in
            ["CARGO_MANIFEST_DIR", "CARGO_MANIFEST_PATH", "CARGO_PKG_NAME", "CARGO_PKG_VERSION_MAJOR", "CARGO_CRATE_NAME", "OUT_DIR"]
        {
            assert!(is_run_env(name), "{name}");
        }
        for name in ["CARGO", "CARGO_HOME", "CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "CARGO_INCREMENTAL", "RUSTUP_TOOLCHAIN", "PATH"] {
            assert!(!is_run_env(name), "{name}");
        }
        // cargo test runs this test with CARGO_MANIFEST_DIR and CARGO_PKG_NAME set, as cargo run does for the runner.
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
        let command = cargo_command();
        let removed: Vec<_> = command.get_envs().filter(|(_, value)| value.is_none()).map(|(name, _)| name.to_owned()).collect();
        for name in ["CARGO_MANIFEST_DIR", "CARGO_PKG_NAME"] {
            assert!(removed.iter().any(|n| n == name), "{name} reaches Cargo");
        }
    }

    #[test]
    fn vite_builds_in_test_mode_into_its_own_folder_without_the_shell() {
        let args = vite_args();
        assert_eq!(args[0], "node_modules/vite/bin/vite.js", "node runs Vite's own script, not npm");
        let at = |flag: &str| args.iter().position(|a| a == flag).unwrap_or_else(|| panic!("{flag} is missing"));
        assert_eq!(args[at("--mode") + 1], "test");
        assert_eq!(args[at("--outDir") + 1], "../dist-test");
        assert!(args.contains(&"--emptyOutDir".to_string()));
        assert!(!args.iter().any(|a| a.contains("npm") || a.ends_with(".cmd")));
    }

    #[test]
    fn tauri_builds_a_debug_app_with_the_test_hooks_and_the_override_config() {
        let args = tauri_args(Path::new("/work/target/faber-test"), 4);
        assert_eq!(args[0], "node_modules/@tauri-apps/cli/tauri.js");
        let before_cargo = &args[..args.iter().position(|a| a == "--").unwrap()];
        for needed in ["build", "--debug", "--no-bundle"] {
            assert!(before_cargo.contains(&needed.to_string()), "{needed}");
        }
        let at = |flag: &str| before_cargo.iter().position(|a| a == flag).unwrap();
        assert_eq!(before_cargo[at("--features") + 1], "test-hooks");
        assert_eq!(before_cargo[at("--config") + 1], "src-tauri/tauri.test.conf.json");
        let cargo_args = &args[args.iter().position(|a| a == "--").unwrap() + 1..];
        assert_eq!(cargo_args, ["--target-dir", "/work/target/faber-test", "-j", "4"]);
    }

    #[test]
    fn the_fingerprint_changes_when_a_file_is_added_changed_or_removed() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(fingerprint(&dir.path().join("missing")), "absent");
        let empty = fingerprint(dir.path());
        std::fs::write(dir.path().join("index.html"), "one").unwrap();
        let one = fingerprint(dir.path());
        assert_ne!(one, empty);
        assert_eq!(fingerprint(dir.path()), one, "the same folder gives the same fingerprint");
        std::fs::write(dir.path().join("index.html"), "three").unwrap();
        assert_ne!(fingerprint(dir.path()), one, "a file that changed size");
        std::fs::create_dir(dir.path().join("assets")).unwrap();
        std::fs::write(dir.path().join("assets/app.js"), "x").unwrap();
        let two = fingerprint(dir.path());
        std::fs::remove_file(dir.path().join("assets/app.js")).unwrap();
        assert_ne!(fingerprint(dir.path()), two);
    }

    #[test]
    fn a_screen_build_reads_nothing_outside_its_inputs() {
        fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| ["ts", "tsx", "css", "mjs", "js"].contains(&e.to_str().unwrap())) {
                    out.push(path);
                }
            }
        }
        let root = Paths::discover().unwrap().root;
        let mut files = Vec::new();
        sources(&root.join("ui"), &mut files);
        sources(&root.join("tests/ui"), &mut files);
        let inputs: Vec<std::path::PathBuf> = VITE_INPUTS.iter().map(|i| root.join(i)).collect();
        let mut outside = Vec::new();
        for file in &files {
            let text = std::fs::read_to_string(file).unwrap();
            for opener in ["from '", "from \"", "import('", "import(\"", "import '", "import \"", "@import '", "@import \""] {
                for (at, _) in text.match_indices(opener) {
                    let rest = &text[at + opener.len()..];
                    let quote = opener.chars().last().unwrap();
                    let Some(end) = rest.find(quote) else { continue };
                    let spec = rest[..end].split('?').next().unwrap();
                    if !spec.starts_with('.') {
                        continue; // a package: its version is in node_modules/.package-lock.json
                    }
                    let mut target = file.parent().unwrap().to_path_buf();
                    for part in spec.split('/') {
                        match part {
                            "." => {}
                            ".." => {
                                target.pop();
                            }
                            part => target.push(part),
                        }
                    }
                    if !inputs.iter().any(|input| target.starts_with(input)) {
                        outside.push(format!("{} imports {spec}", file.display()));
                    }
                }
            }
        }
        assert!(files.len() > 50, "the scan found the screen's sources");
        assert!(outside.is_empty(), "add these to VITE_INPUTS, or the Vite build is skipped when they change: {outside:#?}");
    }

    #[test]
    fn the_screen_inputs_follow_contents_not_times_and_a_stamp_marks_the_build_fresh() {
        let root = std::env::temp_dir().join(format!("faber-vite-inputs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("ui")).unwrap();
        std::fs::write(root.join("ui/a.ts"), "one").unwrap();
        let first = vite_inputs(&root);
        std::fs::write(root.join("ui/a.ts"), "one").unwrap();
        assert_eq!(vite_inputs(&root), first, "writing the same text again changes nothing");
        std::fs::write(root.join("ui/a.ts"), "two").unwrap();
        let second = vite_inputs(&root);
        assert_ne!(second, first, "a changed file changes the digest");
        std::fs::write(root.join("tests_ui_absent"), "x").unwrap();
        assert_eq!(vite_inputs(&root), second, "a file the build does not read changes nothing");
        std::fs::create_dir_all(root.join("tests/ui")).unwrap();
        std::fs::write(root.join("tests/ui/b.ts"), "x").unwrap();
        assert_ne!(vite_inputs(&root), second, "a new check changes the digest");
        let inputs = vite_inputs(&root);
        assert!(!vite_is_fresh(&root, &inputs), "no build yet");
        std::fs::create_dir_all(root.join(TEST_DIST)).unwrap();
        std::fs::write(root.join(TEST_DIST).join(VITE_STAMP), &inputs).unwrap();
        assert!(!vite_is_fresh(&root, &inputs), "a stamp without the page is no build");
        std::fs::write(root.join(TEST_DIST).join("index.html"), "<html>").unwrap();
        assert!(vite_is_fresh(&root, &inputs));
        assert!(!vite_is_fresh(&root, &first), "a stamp of other inputs is stale");
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn repo_file(path: &str) -> String {
        let root = Paths::discover().unwrap().root;
        std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn the_test_override_config_embeds_dist_test_and_runs_no_hook_and_makes_no_bundle() {
        let config: serde_json::Value = serde_json::from_str(&repo_file("src-tauri/tauri.test.conf.json")).unwrap();
        assert_eq!(config["build"]["frontendDist"], "../dist-test");
        assert!(config["build"].get("beforeBuildCommand").is_some_and(|v| v.is_null()), "the production hook runs through a shell");
        assert!(config["build"].get("beforeDevCommand").is_some_and(|v| v.is_null()));
        assert_eq!(config["bundle"]["active"], false);
    }

    #[test]
    fn the_test_hooks_feature_exists_and_is_never_a_default() {
        let manifest: toml::Table = repo_file("src-tauri/Cargo.toml").parse().unwrap();
        let features = manifest["features"].as_table().expect("src-tauri/Cargo.toml has [features]");
        assert!(features.contains_key("test-hooks"));
        let default = features.get("default").and_then(|d| d.as_array()).map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>());
        assert!(!default.unwrap_or_default().contains(&"test-hooks"));
    }

    // The checks live only in a test build. When a production `dist/` exists, nothing in it may carry them.
    #[test]
    fn the_production_dist_holds_no_checks() {
        let dist = Paths::discover().unwrap().root.join("dist");
        if !dist.exists() {
            return;
        }
        fn files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let mut all = Vec::new();
        files(&dist, &mut all);
        for file in all.iter().filter(|f| matches!(f.extension().and_then(|e| e.to_str()), Some("js" | "html" | "css"))) {
            let text = std::fs::read_to_string(file).unwrap_or_default();
            for marker in ["__faberChecks", "__faberText", "report_checks"] {
                assert!(!text.contains(marker), "{} contains {marker}", file.display());
            }
        }
    }
}
