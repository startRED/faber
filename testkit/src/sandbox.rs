//! The sandbox every launched app lives in: its folders, the environment it is given and the check that nothing outside
//! it was touched. It keeps the whole contract of the setup test it replaces, for every launch.

use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Variables the child never inherits (matched on the start of the name, in any case): Faber's own test switches,
/// the CLIs' settings and provider keys, any proxy, and the webview's overrides (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`
/// can move the debugging port or the proxy, `WEBVIEW2_WAIT_FOR_SCRIPT_DEBUGGER` stops every script at startup). The
/// launch sets the webview values it needs itself.
const REMOVED_PREFIXES: [&str; 12] = [
    "FABER_",
    "CLAUDE",
    "CODEX",
    "OPENAI",
    "ANTHROPIC",
    "GEMINI",
    "GOOGLE_API",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "WEBVIEW2_",
];

/// A loopback address nothing listens on: a download or a request that slips through goes nowhere.
pub const DEAD_ORIGIN: &str = "http://127.0.0.1:1";

/// What `FABER_CLI_CLAUDE` or `FABER_CLI_CODEX` holds in a launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CliSlot {
    /// The empty string: discovery is off, so the CLI does not exist.
    #[default]
    Off,
    /// A launch config for `fake-cli`.
    Fake,
    /// The variable is not set, so the engine looks in the sandbox (the install scenarios).
    Absent,
}

/// What a scenario chooses for a launch; everything else comes from the sandbox.
#[derive(Debug, Clone, Default)]
pub struct Launch {
    /// The native git directory, only for contained Projects integration fixtures.
    pub git_bin: Option<PathBuf>,
    pub args: Vec<String>,
    /// Exercises raster costs that a local GPU can hide; used only by the motion benchmark.
    pub software_rendering: bool,
    pub claude: CliSlot,
    pub codex: CliSlot,
    /// `FABER_FAKE_RECORDING`, for the fake CLIs.
    pub recording: Option<PathBuf>,
    /// `FABER_SETUP_SOURCE`: a loopback server; a dead address when `None`.
    pub setup_source: Option<String>,
    pub cdp_port: Option<u16>,
    /// `FABER_CHECKS_OUT`: the `ui` suite's report file.
    pub checks_out: Option<PathBuf>,
}

pub struct Sandbox {
    /// Four folders above `scenario` are watched for stray files: `root`, `observed`, `enclosing`, `boundary`.
    pub root: PathBuf,
    pub observed: PathBuf,
    pub enclosing: PathBuf,
    pub boundary: PathBuf,
    pub scenario: PathBuf,
    pub sandbox: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    pub profile: PathBuf,
    pub path_dir: PathBuf,
    pub tmp: PathBuf,
    pub webview: PathBuf,
    pub release: PathBuf,
    /// A folder beside the sandbox that nothing may write to.
    pub canary: PathBuf,
    canary_before: BTreeMap<PathBuf, String>,
}

impl Sandbox {
    pub fn new() -> io::Result<Sandbox> {
        let root = plain(tempfile::Builder::new().prefix("faber-test-").tempdir()?.keep().canonicalize()?);
        let observed = root.join("observed");
        let enclosing = observed.join("enclosing");
        let boundary = enclosing.join("boundary");
        let scenario = boundary.join("scenario");
        let sandbox = scenario.join("sandbox");
        let profile = sandbox.join("profile");
        let sb = Sandbox {
            data: sandbox.join("data"),
            state: sandbox.join("fake-state"),
            path_dir: sandbox.join("path"),
            tmp: sandbox.join("tmp"),
            webview: sandbox.join("webview"),
            release: sandbox.join("release"),
            canary: scenario.join("outside-sandbox"),
            canary_before: BTreeMap::new(),
            root,
            observed,
            enclosing,
            boundary,
            scenario,
            sandbox,
            profile,
        };
        let appdata = [sb.profile.join("AppData/Local"), sb.profile.join("AppData/Roaming")];
        for dir in [&sb.data, &sb.state, &sb.path_dir, &sb.tmp, &sb.release, &sb.canary].into_iter().chain(&appdata) {
            std::fs::create_dir_all(dir)?;
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::PermissionsExt;
            let runtime = sb.tmp.join("runtime");
            std::fs::create_dir(&runtime)?;
            std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700))?;
        }
        std::fs::write(sb.canary.join("untouched.txt"), "No installation or login may write here.")?;
        Ok(Sandbox { canary_before: snapshot(&sb.canary, None), ..sb })
    }

    /// Everything the sandbox is allowed to hold is inside `sandbox`. This checks the rest of the disk we can see:
    /// the canary folder is unchanged, `scenario` holds nothing but the sandbox and the canary, and nothing appeared in
    /// the four folders around the scenario.
    pub fn audit_files(&self) -> Result<(), String> {
        if snapshot(&self.canary, None) != self.canary_before {
            return Err(format!("write outside sandbox detected: the canary folder {} changed", self.canary.display()));
        }
        let mut stray = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.scenario) {
            stray.extend(
                entries.flatten().map(|e| e.path()).filter(|p| *p != self.sandbox && *p != self.canary).map(|p| p.display().to_string()),
            );
        }
        let mut child = self.scenario.clone();
        for _ in 0..4 {
            let Some(parent) = child.parent().map(Path::to_path_buf) else { break };
            stray.extend(snapshot(&parent, Some(&child)).into_keys().map(|p| p.display().to_string()));
            child = parent;
        }
        if stray.is_empty() { Ok(()) } else { Err(format!("write outside sandbox detected: {}", stray.join(", "))) }
    }

    /// Removes every folder. A webview may hold a file for a moment after it exits, so this retries. It fails if the
    /// folder is still there after that.
    pub fn dispose(&self) -> Result<(), String> {
        for _ in 0..10 {
            if remove_tree(&self.root).is_ok() || !self.root.exists() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(format!("the sandbox {} could not be removed", self.root.display()))
    }
}

/// Removes a folder and everything in it. `std::fs::remove_dir_all` gives up on the first folder it may not list, and
/// Windows makes one of those (`INetCache\Content.IE5`, a link the user may not open) in every profile folder the
/// webview touches. It can still be removed as it is, so a folder that cannot be listed is removed without looking
/// inside, and a link is removed as a folder when removing it as a file fails.
pub fn remove_tree(path: &Path) -> io::Result<()> {
    let Ok(entries) = std::fs::read_dir(path) else { return std::fs::remove_dir(path) };
    let mut first_error = None;
    for entry in entries.flatten() {
        let child = entry.path();
        let removed = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_tree(&child)
        } else {
            std::fs::remove_file(&child).or_else(|_| std::fs::remove_dir(&child))
        };
        if let Err(e) = removed {
            first_error.get_or_insert(e);
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => std::fs::remove_dir(path),
    }
}

/// `canonicalize` on Windows gives a `\\?\` path, which the webview and many programs reject.
fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().into_owned();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

/// The environment of a launched app: the parent's, without anything that could reach a real account or the network,
/// with every place the app or a CLI could write pointed inside the sandbox.
pub fn child_env(parent: &[(String, String)], sb: &Sandbox, launch: &Launch, fake_cli: &Path) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = parent
        .iter()
        .filter(|(name, _)| {
            let upper = name.to_ascii_uppercase();
            name != "Path" && !REMOVED_PREFIXES.iter().any(|prefix| upper.starts_with(prefix))
        })
        .cloned()
        .collect();
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let mut set = |name: &str, value: String| {
        // Windows names are case-insensitive: `Temp` beside `TEMP` would leave it open which one the child sees.
        if cfg!(windows) {
            env.retain(|existing, _| !existing.eq_ignore_ascii_case(name));
        }
        env.insert(name.to_string(), value);
    };
    for name in ["HOME", "USERPROFILE"] {
        set(name, path(&sb.profile));
    }
    set("APPDATA", path(&sb.profile.join("AppData/Roaming")));
    set("LOCALAPPDATA", path(&sb.profile.join("AppData/Local")));
    let mut temp_names = vec!["TEMP", "TMP"];
    if cfg!(not(windows)) {
        // macOS and Linux have more places to write to, and `TMPDIR` is where they put temporary files.
        temp_names.push("TMPDIR");
        for (name, dir) in [
            ("XDG_CONFIG_HOME", ".config"),
            ("XDG_DATA_HOME", ".local/share"),
            ("XDG_CACHE_HOME", ".cache"),
            ("XDG_STATE_HOME", ".local/state"),
        ] {
            set(name, path(&sb.profile.join(dir)));
        }
    }
    for name in temp_names {
        set(name, path(&sb.tmp));
    }
    let mut paths = vec![sb.path_dir.clone()];
    paths.extend(launch.git_bin.clone());
    set("PATH", std::env::join_paths(paths).expect("sandbox paths are valid").to_string_lossy().into());
    if cfg!(target_os = "linux") {
        // WebKitGTK under xvfb has no usable dmabuf renderer; without this the test build's window stays blank.
        set("WEBKIT_DISABLE_DMABUF_RENDERER", "1".into());
        set("WEBKIT_DISABLE_COMPOSITING_MODE", "1".into());
        set("XDG_RUNTIME_DIR", path(&sb.tmp.join("runtime")));
    }
    set("CODEX_HOME", path(&sb.profile.join("fake-home")));
    set("WEBVIEW2_USER_DATA_FOLDER", path(&sb.webview));
    set("FABER_HIDDEN", "1".into());
    if launch.software_rendering {
        set("FABER_SOFTWARE_RENDERING", "1".into());
    }
    set("FABER_DATA_DIR", path(&sb.data));
    set("FABER_FAKE_STATE_DIR", path(&sb.state));
    set("FABER_SETUP_SOURCE", launch.setup_source.clone().unwrap_or_else(|| DEAD_ORIGIN.into()));
    if let Some(port) = launch.cdp_port {
        set("FABER_CDP_PORT", port.to_string());
    }
    if let Some(recording) = &launch.recording {
        set("FABER_FAKE_RECORDING", path(recording));
    }
    if let Some(out) = &launch.checks_out {
        set("FABER_CHECKS_OUT", path(out));
    }
    let slot = |slot: CliSlot| match slot {
        CliSlot::Off => Some(String::new()),
        CliSlot::Fake => Some(json!({ "command": path(fake_cli), "prefix": [] }).to_string()),
        CliSlot::Absent => None,
    };
    for (name, value) in [("FABER_CLI_CLAUDE", slot(launch.claude)), ("FABER_CLI_CODEX", slot(launch.codex))] {
        if let Some(value) = value {
            set(name, value);
        }
    }
    for name in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
        set(name, DEAD_ORIGIN.into());
    }
    set("NO_PROXY", "127.0.0.1,localhost".into());
    env
}

/// Whether `path` is `base` or inside it, judged by the path itself (`..` included), not by what exists on disk.
pub fn contained(base: &Path, path: &Path) -> bool {
    let (base, path) = (normal(base), normal(path));
    path.len() >= base.len() && path[..base.len()] == base[..]
}

fn normal(path: &Path) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                parts.pop();
            }
            Component::CurDir => {}
            other => {
                let text = other.as_os_str().to_string_lossy().into_owned();
                parts.push(if cfg!(windows) { text.to_lowercase() } else { text });
            }
        }
    }
    parts
}

/// The SHA-256 of every file under `dir`, by path, leaving out the folder `exclude`.
pub fn snapshot(dir: &Path, exclude: Option<&Path>) -> BTreeMap<PathBuf, String> {
    let mut found = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        if exclude.is_some_and(|skip| path == skip) {
            continue;
        }
        if path.is_dir() {
            found.extend(snapshot(&path, exclude));
        } else if let Ok(bytes) = std::fs::read(&path) {
            found.insert(path, format!("{:x}", Sha256::digest(bytes)));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn the_child_environment_keeps_nothing_real_and_points_every_profile_variable_into_the_sandbox() {
        let sb = Sandbox::new().unwrap();
        let real = pairs(&[
            ("USERPROFILE", r"C:\Users\real"),
            ("Path", r"C:\Windows;C:\Users\real\.local\bin"),
            ("CODEX_HOME", r"C:\Users\real\.codex"),
            ("ANTHROPIC_API_KEY", "sk-ant-real"),
            ("OPENAI_API_KEY", "sk-real"),
            ("HTTPS_PROXY", "http://real-proxy:8080"),
            ("claude_oauth_token", "lowercase-secret"),
            ("FABER_CLI_CLAUDE", "stale"),
            ("FABER_SOFTWARE_RENDERING", "stale"),
            ("NO_PROXY", "internal.example"),
            ("SystemRoot", r"C:\Windows"),
            ("DISPLAY", ":99"),
            ("WEBVIEW2_WAIT_FOR_SCRIPT_DEBUGGER", "1"),
            ("webview2_additional_browser_arguments", "--remote-debugging-port=1 --proxy-server=real"),
            ("WEBVIEW2_USER_DATA_FOLDER", "elsewhere"),
        ]);
        let launch = Launch { cdp_port: Some(9333), recording: Some(PathBuf::from("rec.jsonl")), ..Launch::default() };
        let env = child_env(&real, &sb, &launch, Path::new("fake-cli.exe"));
        for secret in ["real", "sk-ant-real", "sk-real", "lowercase-secret", "stale", "internal.example"] {
            assert!(env.values().all(|v| !v.contains(secret)), "{secret} reached the child: {env:?}");
        }
        assert!(!env.contains_key("Path") && !env.contains_key("claude_oauth_token"));
        assert!(!env.contains_key("FABER_SOFTWARE_RENDERING"));
        let software = child_env(&real, &sb, &Launch { software_rendering: true, ..launch.clone() }, Path::new("fake-cli.exe"));
        assert_eq!(software.get("FABER_SOFTWARE_RENDERING").map(String::as_str), Some("1"));
        assert!(
            env.keys().filter(|k| k.to_ascii_uppercase().starts_with("WEBVIEW2_")).all(|k| k == "WEBVIEW2_USER_DATA_FOLDER"),
            "{env:?}"
        );
        for name in [
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "TEMP",
            "TMP",
            "CODEX_HOME",
            "WEBVIEW2_USER_DATA_FOLDER",
            "PATH",
            "FABER_DATA_DIR",
        ] {
            assert!(contained(&sb.root, Path::new(&env[name])), "{name} = {} is outside the sandbox", env[name]);
        }
        if cfg!(not(windows)) {
            for name in ["TMPDIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"] {
                assert!(contained(&sb.root, Path::new(&env[name])), "{name} = {} is outside the sandbox", env[name]);
            }
        } else {
            assert!(!env.contains_key("TMPDIR") && !env.contains_key("XDG_CONFIG_HOME"), "those only exist on macOS and Linux");
        }
        assert_eq!(env["PATH"], sb.path_dir.to_string_lossy());
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(env["TMPDIR"], sb.tmp.to_string_lossy());
            assert_eq!(env["XDG_RUNTIME_DIR"], sb.tmp.join("runtime").to_string_lossy());
            assert_eq!(std::fs::metadata(&env["XDG_RUNTIME_DIR"]).unwrap().permissions().mode() & 0o777, 0o700);
            assert_eq!(env["WEBKIT_DISABLE_DMABUF_RENDERER"], "1");
            assert_eq!(env["WEBKIT_DISABLE_COMPOSITING_MODE"], "1");
        }
        assert_eq!(std::fs::read_dir(&sb.path_dir).unwrap().count(), 0, "the PATH folder must be empty, so no real CLI is on it");
        assert_eq!((env["SystemRoot"].as_str(), env["DISPLAY"].as_str()), (r"C:\Windows", ":99"));
        assert_eq!((env["FABER_HIDDEN"].as_str(), env["FABER_CDP_PORT"].as_str()), ("1", "9333"));
        assert_eq!(
            (env["HTTP_PROXY"].as_str(), env["HTTPS_PROXY"].as_str(), env["ALL_PROXY"].as_str()),
            (DEAD_ORIGIN, DEAD_ORIGIN, DEAD_ORIGIN)
        );
        assert_eq!(env["NO_PROXY"], "127.0.0.1,localhost");
        assert_eq!(env["FABER_SETUP_SOURCE"], DEAD_ORIGIN, "a scenario with no server must not be able to download from anywhere");
        sb.dispose().unwrap();
    }

    #[test]
    fn each_cli_slot_is_off_a_fake_or_left_for_discovery() {
        let sb = Sandbox::new().unwrap();
        let go = |claude, codex| child_env(&[], &sb, &Launch { claude, codex, ..Launch::default() }, Path::new("fake-cli.exe"));
        let env = go(CliSlot::Off, CliSlot::Absent);
        assert_eq!(env["FABER_CLI_CLAUDE"], "");
        assert!(!env.contains_key("FABER_CLI_CODEX"), "absent means the engine may find the CLI installed in the data folder");
        let env = go(CliSlot::Fake, CliSlot::Off);
        let config: serde_json::Value = serde_json::from_str(&env["FABER_CLI_CLAUDE"]).unwrap();
        assert_eq!(config, json!({ "command": "fake-cli.exe", "prefix": [] }));
        assert_eq!(env["FABER_CLI_CODEX"], "");
        sb.dispose().unwrap();
    }

    // Windows spells a variable however it likes (`Path`, `Temp`, `userprofile`) and looks it up without case. A second
    // spelling next to the sandbox value would leave the child free to read the real one.
    #[cfg(windows)]
    #[test]
    fn on_windows_no_other_spelling_of_a_sandboxed_name_survives() {
        let sb = Sandbox::new().unwrap();
        let real =
            pairs(&[("path", r"C:\real\bin"), ("Temp", r"C:\real\temp"), ("userprofile", r"C:\Users\real"), ("Windir", r"C:\Windows")]);
        let env = child_env(&real, &sb, &Launch::default(), Path::new("fake-cli.exe"));
        for name in ["PATH", "TEMP", "USERPROFILE"] {
            let spellings: Vec<&String> = env.keys().filter(|k| k.eq_ignore_ascii_case(name)).collect();
            assert_eq!(spellings, [name], "only the sandbox spelling of {name} may be left");
            assert!(contained(&sb.root, Path::new(&env[name])), "{name} = {}", env[name]);
        }
        assert_eq!(env["Windir"], r"C:\Windows", "a name the sandbox does not own is kept as it was");
        sb.dispose().unwrap();
    }

    #[test]
    fn contained_judges_the_path_itself() {
        let base = Path::new("/work/sandbox");
        assert!(contained(base, Path::new("/work/sandbox")));
        assert!(contained(base, Path::new("/work/sandbox/data/x.txt")));
        assert!(contained(base, Path::new("/work/sandbox/data/../data/x.txt")));
        assert!(!contained(base, Path::new("/work/sandbox/../other")));
        assert!(!contained(base, Path::new("/work/sandbox2/x")), "a sibling that starts with the same letters is outside");
        assert!(!contained(base, Path::new("/work")));
    }

    #[test]
    fn remove_tree_removes_nested_folders_files_and_a_read_only_file() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("sandbox");
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        std::fs::write(root.join("a/one.txt"), "1").unwrap();
        std::fs::write(root.join("a/b/c/two.txt"), "2").unwrap();
        let locked = root.join("a/b/locked.txt");
        std::fs::write(&locked, "3").unwrap();
        let mut permissions = std::fs::metadata(&locked).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&locked, permissions).unwrap();
        remove_tree(&root).unwrap();
        assert!(!root.exists());
        assert!(remove_tree(&root).is_err(), "a folder that is already gone is reported, and dispose treats that as done");
    }

    // R5.5: the audit rejects a write outside the scenario's temporary folders, whatever the file is called.
    #[test]
    fn r5_5_the_audit_rejects_writes_outside_the_sandbox() {
        let sb = Sandbox::new().unwrap();
        sb.audit_files().unwrap();
        std::fs::write(sb.data.join("settings.json"), "{}").unwrap();
        sb.audit_files().expect("a write inside the sandbox is fine");
        let beyond_observed = sb.root.join("external-write.txt");
        std::fs::write(&beyond_observed, "unauthorized absolute write beyond observed root").unwrap();
        assert!(sb.audit_files().unwrap_err().to_lowercase().contains("write outside sandbox"));
        std::fs::remove_file(&beyond_observed).unwrap();
        let named_like_a_test = sb.enclosing.join("unauthorized.test.ts");
        std::fs::write(&named_like_a_test, "unauthorized write named test.ts").unwrap();
        assert!(sb.audit_files().unwrap_err().to_lowercase().contains("write outside sandbox"));
        std::fs::remove_file(&named_like_a_test).unwrap();
        let beside_the_sandbox = sb.scenario.join("stray.txt");
        std::fs::write(&beside_the_sandbox, "written next to the sandbox, inside none of the checked folders").unwrap();
        assert!(sb.audit_files().unwrap_err().to_lowercase().contains("write outside sandbox"), "scenario itself is watched");
        std::fs::remove_file(&beside_the_sandbox).unwrap();
        std::fs::write(sb.canary.join("untouched.txt"), "changed").unwrap();
        assert!(sb.audit_files().unwrap_err().contains("canary"));
        sb.dispose().unwrap();
    }
}
