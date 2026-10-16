//! Development-only staging and headless lifecycle. Public releases keep the signed Tauri updater.
use faber_engine::project::files::ScopedFile;
use faber_engine::project::git::{Executor, Invocation, valid_oid};
use faber_engine::project::lease::real_path;
pub use faber_engine::project::update::Coordinator;
use faber_engine::project::update::{
    CanaryVerdict, ExactTree, RollbackAction, RollbackReason, Slot, SlotLayout, SwapCheck, UpdateError, UpdateManager, UpdateState,
};
use faber_engine::project::{Effect, Grant, RepoIdentity, SCHEMA, clean_path};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, UpdateError>;
fn invalid(message: &'static str) -> UpdateError {
    UpdateError::InvalidState(message)
}
fn io_error(_: std::io::Error) -> UpdateError {
    UpdateError::Store(faber_engine::store::Fault::Write)
}
fn same_path(left: &Path, right: &Path) -> Result<bool> {
    let normalize = |path: PathBuf| {
        let text = path.to_string_lossy().replace('\\', "/");
        let text = text.trim_start_matches("//?/");
        #[cfg(any(windows, target_os = "macos"))]
        let text = text.to_lowercase();
        PathBuf::from(text)
    };
    Ok(normalize(real_path(left)?) == normalize(real_path(right)?))
}
/// The update flow's clock, in seconds since 1970.
pub fn now() -> u64 {
    let real = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    #[cfg(feature = "test-hooks")]
    if let Some((anchor, speed)) = test_clock() {
        let real_ms = u64::try_from(real.as_millis()).unwrap_or(u64::MAX);
        return anchor.saturating_add(real_ms.saturating_sub(anchor).saturating_mul(speed)) / 1000;
    }
    real.as_secs()
}

/// How long the supervisor has watched, on the same clock as [`now`].
fn watched(since: std::time::Instant) -> std::time::Duration {
    #[cfg(feature = "test-hooks")]
    if let Some((_, speed)) = test_clock() {
        return since.elapsed().saturating_mul(u32::try_from(speed).unwrap_or(u32::MAX));
    }
    since.elapsed()
}

/// Test builds only: `FABER_TEST_CLOCK=<anchor in ms since 1970>:<speed from 2 to 20>` runs the clock `speed` times faster
/// from the anchor, so the probation scenario proves the whole 300-second rule in a fraction of the time (R163). Every
/// process of a scenario gets the same anchor, so they agree; after the anchor this clock is never behind the real one.
#[cfg(feature = "test-hooks")]
fn test_clock() -> Option<(u64, u64)> {
    static CLOCK: std::sync::OnceLock<Option<(u64, u64)>> = std::sync::OnceLock::new();
    *CLOCK.get_or_init(|| {
        let text = std::env::var("FABER_TEST_CLOCK").ok()?;
        let (anchor, speed) = text.split_once(':')?;
        Some((anchor.parse().ok()?, speed.parse().ok().filter(|s| (2..=20).contains(s))?))
    })
}

#[derive(Clone)]
pub struct Development {
    layout: SlotLayout,
    installed_data: PathBuf,
    installed_release: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Enrollment {
    channel: String,
    installed_data: PathBuf,
    installed_release: PathBuf,
}

/// Variables a running slot needs from its person's session to reach the CLIs' sign-in and the build toolchain.
const PROFILE_AND_TOOLCHAIN: &[&str] = &[
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "HOMEDRIVE",
    "HOMEPATH",
    "ProgramData",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "RUSTUP_HOME",
    "CARGO_HOME",
    "LIB",
    "INCLUDE",
    "LIBPATH",
    "VCINSTALLDIR",
    "VCToolsInstallDir",
];

impl Development {
    pub fn new(channel: &str, root: PathBuf, installed_data: PathBuf, installed_release: PathBuf) -> Result<Self> {
        if channel != "development" {
            return Err(invalid("explicit development channel required"));
        }
        let layout = SlotLayout::new(&root, &installed_data, &installed_release)?;
        Ok(Self { layout, installed_data, installed_release })
    }

    pub fn from_environment() -> Result<Self> {
        let required = |name| std::env::var_os(name).map(PathBuf::from).ok_or(invalid("missing development root"));
        Self::new(
            &std::env::var("FABER_CHANNEL").unwrap_or_default(),
            required("FABER_DEV_ROOT")?,
            required("FABER_INSTALLED_DATA")?,
            required("FABER_INSTALLED_RELEASE")?,
        )
    }

    pub fn from_launcher(executable: &Path) -> Result<Self> {
        if faber_engine::project::update::is_forbidden_root(executable) {
            return Err(invalid("installed launcher excluded"));
        }
        let root = real_path(executable)?.parent().ok_or(invalid("missing launcher parent"))?.to_path_buf();
        let bytes = ScopedFile::new(&root.join("development.json"), false)?
            .read_limited(64 * 1024)?
            .ok_or(invalid("missing development enrollment"))?
            .0;
        let config: Enrollment = serde_json::from_slice(&bytes).map_err(|_| invalid("corrupt development enrollment"))?;
        let dev = Self::new(&config.channel, root, config.installed_data, config.installed_release)?;
        if !same_path(executable, &dev.helper())? {
            return Err(invalid("wrong development entry point"));
        }
        dev.verify_helper()?;
        Ok(dev)
    }

    pub fn from_slot(executable: &Path) -> Result<Option<Self>> {
        let Some(slot) = executable.parent() else { return Ok(None) };
        let Some(slots) = slot.parent() else { return Ok(None) };
        if executable.file_name().is_none_or(|name| !name.eq_ignore_ascii_case(format!("faber{}", std::env::consts::EXE_SUFFIX)))
            || slots.file_name().is_none_or(|name| !name.eq_ignore_ascii_case("slots"))
        {
            return Ok(None);
        }
        let root = slots.parent().ok_or(invalid("missing slot root"))?;
        let dev = Self::from_launcher(&root.join(format!("faber-local{}", std::env::consts::EXE_SUFFIX)))?;
        if !same_path(executable, &dev.binary(Slot::A))? && !same_path(executable, &dev.binary(Slot::B))? {
            return Err(invalid("unknown development slot"));
        }
        Ok(Some(dev))
    }

    pub fn manager(&self) -> Result<UpdateManager> {
        // Revalidate aliases on each entry, including after a restart.
        UpdateManager::new(SlotLayout::new(self.layout.dev_root(), &self.installed_data, &self.installed_release)?)
    }
    pub fn data(&self) -> PathBuf {
        self.layout.dev_root().join("data")
    }

    pub fn data_lease(&self) -> Result<ScopedFile> {
        let anchor = real_path(&self.data().join(".anchor"))?;
        ScopedFile::new(&anchor, true).map_err(UpdateError::from)
    }

    fn excluded(&self, path: &Path) -> bool {
        let normalized = |path: &Path| {
            let text = path.to_string_lossy().replace('\\', "/");
            let text = text.trim_start_matches("//?/");
            #[cfg(any(windows, target_os = "macos"))]
            let text = text.to_lowercase();
            PathBuf::from(text)
        };
        let path = normalized(path);
        faber_engine::project::update::is_forbidden_root(&path)
            || [&self.installed_data, &self.installed_release].iter().any(|root| path.starts_with(normalized(root)))
    }
    pub fn build_dir(&self) -> PathBuf {
        self.layout.staging_dir().join("build")
    }
    pub fn binary(&self, slot: Slot) -> PathBuf {
        self.layout.slot_dir(slot).join(format!("faber{}", std::env::consts::EXE_SUFFIX))
    }
    pub fn helper(&self) -> PathBuf {
        self.layout.dev_root().join(format!("faber-local{}", std::env::consts::EXE_SUFFIX))
    }

    pub fn view(&self) -> Result<Option<faber_engine::project::wire::BuildView>> {
        Ok(self.manager()?.manifest()?.map(|manifest| {
            let describe =
                |slot: Slot, tree: &ExactTree| format!("{} · {}", if slot == Slot::A { "A" } else { "B" }, &tree.commit_oid[..12]);
            let waiting = match manifest.state {
                UpdateState::PreparingNext { .. } => Some("projects.state.running"),
                UpdateState::CanaryPending { .. } => Some("projects.task.review"),
                UpdateState::Ready { .. } | UpdateState::Quiescent => Some("reason.update.quiescence"),
                UpdateState::Swapping { .. } if manifest.extra.get("cleanExit") == Some(&Value::Bool(true)) => {
                    Some("projects.state.paused")
                }
                UpdateState::Swapping { .. } | UpdateState::Probation { .. } => Some("projects.state.running"),
                UpdateState::Quarantined { .. } | UpdateState::RolledBack { .. } => Some("projects.state.blocked"),
                _ => None,
            };
            faber_engine::project::wire::BuildView {
                current: manifest.current_good.as_ref().map(|good| describe(good.slot, &good.tree)),
                next: manifest
                    .staged_tree
                    .as_ref()
                    .map(|tree| describe(manifest.last_good.as_ref().map_or(manifest.active_slot, |good| good.slot).other(), tree)),
                last_good: manifest.last_good.as_ref().map(|good| describe(good.slot, &good.tree)),
                waiting: waiting.map(String::from),
            }
        }))
    }

    pub fn lock(&self) -> Result<Coordinator> {
        self.manager()?;
        self.layout.coordinate()
    }

    pub fn ensure_missions_quiescent(&self) -> Result<()> {
        let result = self.layout.missions_quiescent();
        if matches!(result, Err(UpdateError::ActiveOperations)) {
            ScopedFile::new(&self.signal_path("quiesce"), true)?.write(b"{}", None)?;
        }
        result
    }

    pub async fn service_quiescence(&self, engine: &faber_engine::Engine) -> Result<()> {
        let signal = ScopedFile::new(&self.signal_path("quiesce"), false)?;
        if let Some(hash) = signal.hash()? {
            engine.pause_missions_for_update().await.map_err(|_| UpdateError::ActiveOperations)?;
            signal.claim(&hash)?.delete()?;
        }
        Ok(())
    }

    pub fn leaving_slot(&self, executable: &Path) -> Result<bool> {
        let manifest = self.manager()?.manifest()?.ok_or(invalid("missing manifest"))?;
        if let UpdateState::Swapping { from_slot, .. } = manifest.state {
            return same_path(executable, &self.binary(from_slot));
        }
        Ok(false)
    }

    pub fn slot_command(&self, slot: Slot) -> NativeCommand {
        let mut command = NativeCommand::new(self.binary(slot), self.layout.slot_dir(slot));
        command.args.push("--faber-local-start".into());
        self.environment(&mut command);
        // Folder names only, never a secret: the CLIs find their own sign-in through the profile, and a local build
        // finds its toolchain through these. Without them the slot cannot start a mission or build the next slot.
        for key in PROFILE_AND_TOOLCHAIN {
            if let Ok(value) = std::env::var(key) {
                command.environment.insert((*key).into(), value);
            }
        }
        command
    }

    fn environment(&self, command: &mut NativeCommand) {
        for (key, value) in [
            ("FABER_CHANNEL", "development".into()),
            ("FABER_DEV_ROOT", self.layout.dev_root().to_string_lossy().into_owned()),
            ("FABER_DATA_DIR", self.data().to_string_lossy().into_owned()),
            ("FABER_INSTALLED_DATA", self.installed_data.to_string_lossy().into_owned()),
            ("FABER_INSTALLED_RELEASE", self.installed_release.to_string_lossy().into_owned()),
        ] {
            command.environment.insert(key.into(), value);
        }
        // Test builds only: the next process keeps the faster test clock, or it would disagree with this one.
        #[cfg(feature = "test-hooks")]
        if let Ok(clock) = std::env::var("FABER_TEST_CLOCK") {
            command.environment.insert("FABER_TEST_CLOCK".into(), clock);
        }
    }

    pub fn verify_slot(&self, slot: Slot, tree: &ExactTree) -> Result<()> {
        let hash = ScopedFile::new(&real_path(&self.binary(slot))?, false)?.hash()?;
        if hash.as_deref() != Some(&tree.build_hash) {
            return Err(invalid("slot payload changed"));
        }
        Ok(())
    }

    pub fn verify_helper(&self) -> Result<()> {
        let expected =
            ScopedFile::new(&self.layout.dev_root().join("helper.sha256"), false)?.read()?.ok_or(invalid("trusted helper missing"))?.0;
        let actual = ScopedFile::new(&self.helper(), false)?.hash()?.ok_or(invalid("trusted helper missing"))?;
        if expected != actual.as_bytes() {
            return Err(invalid("trusted helper changed"));
        }
        Ok(())
    }

    fn slot_helper(&self, slot: Slot) -> Result<Option<PathBuf>> {
        let dir = self.layout.slot_dir(slot);
        let Some((expected, _)) = ScopedFile::new(&dir.join("helper.sha256"), false)?.read()? else {
            return Ok(None);
        };
        let path = dir.join(format!("faber-local{}", std::env::consts::EXE_SUFFIX));
        let hash = ScopedFile::new(&real_path(&path)?, false)?.hash()?.ok_or(invalid("future helper missing"))?;
        if expected != hash.as_bytes() {
            return Err(invalid("future helper changed"));
        }
        Ok(Some(path))
    }

    pub fn resume_allowed(&self) -> Result<()> {
        let manifest = self.manager()?.manifest()?.ok_or(invalid("missing manifest"))?;
        match manifest.state {
            UpdateState::Probation { health_acknowledged: true, .. } if manifest.first_operation_completed => Ok(()),
            UpdateState::Swapping { .. } | UpdateState::Probation { .. } => {
                Err(invalid("health and first operation required before resume"))
            }
            _ if manifest.current_good.as_ref().is_some_and(|good| good.slot == manifest.active_slot) => Ok(()),
            _ => Err(invalid("health and first operation required before resume")),
        }
    }

    pub fn startup_lease(&self, supervised: bool) -> Result<File> {
        let manager = self.manager()?;
        let loaded = manager.manifest();
        if loaded.is_err() {
            if !supervised {
                ScopedFile::new(&self.signal_path("open"), false)?.write(b"{}", None)?;
                if let Ok(coordinator) = self.lock() {
                    drop(coordinator);
                    self.launch_recovery()?;
                }
            }
            return Err(invalid("corrupt startup delegated to helper"));
        }
        let manifest = loaded?.ok_or(invalid("development baseline not enrolled"))?;
        if matches!(manifest.state, UpdateState::Swapping { .. } | UpdateState::Probation { .. }) && !supervised {
            self.request_open()?;
            if let Ok(coordinator) = self.lock() {
                drop(coordinator);
                self.launch_recovery()?;
            }
            return Err(invalid("interrupted transition delegated to helper"));
        }
        let tree = if let UpdateState::Probation { tree, .. } = &manifest.state {
            tree
        } else {
            &manifest.current_good.as_ref().ok_or(UpdateError::NoLastGood)?.tree
        };
        if !same_path(&std::env::current_exe().map_err(io_error)?, &self.binary(manifest.active_slot))? {
            return Err(invalid("development launch outside active slot"));
        }
        self.verify_slot(manifest.active_slot, tree)?;
        match self.slot_lease(manifest.active_slot) {
            Ok(lease) => Ok(lease),
            Err(error) => {
                if !supervised {
                    ScopedFile::new(&self.signal_path("open"), false)?.write(b"{}", None)?;
                }
                Err(error)
            }
        }
    }

    fn launch_recovery(&self) -> Result<()> {
        self.verify_helper()?;
        let mut command = NativeCommand::new(self.helper(), self.layout.dev_root().into());
        command.args.push("--recover".into());
        self.environment(&mut command);
        command.spawn()?;
        Ok(())
    }

    pub fn open_pending(&self) -> Result<bool> {
        Ok(ScopedFile::new(&self.signal_path("open"), false)?.hash()?.is_some())
    }

    pub fn request_open(&self) -> Result<bool> {
        let manager = self.manager()?;
        let manifest = manager.manifest()?.ok_or(invalid("missing manifest"))?;
        if manifest.state == UpdateState::Quiescent {
            manager.cancel_quiescence()?;
            return Ok(true);
        }
        if matches!(manifest.state, UpdateState::Swapping { .. } | UpdateState::Probation { .. }) && self.resume_allowed().is_err() {
            ScopedFile::new(&self.signal_path("open"), false)?.write(b"{}", None)?;
            return Ok(false);
        }
        let file = ScopedFile::new(&self.signal_path("open"), false)?;
        if let Some(hash) = file.hash()? {
            file.claim(&hash)?.delete()?;
        }
        Ok(true)
    }

    pub fn begin_build_output(&self) -> Result<()> {
        self.clean_build_output()?;
        ScopedFile::new(&self.build_dir().join(".faber-owned"), true)?.write(b"faber-local-build-v1", None)?;
        Ok(())
    }

    pub fn clean_build_output(&self) -> Result<()> {
        if !self.build_dir().exists() {
            return Ok(());
        }
        let marker = ScopedFile::new(&self.build_dir().join(".faber-owned"), false)?.read()?;
        if marker.as_ref().map(|(bytes, _)| bytes.as_slice()) != Some(b"faber-local-build-v1") {
            real_path(&self.build_dir())?;
            if fs::read_dir(self.build_dir()).map_err(io_error)?.next().is_none() {
                return fs::remove_dir(self.build_dir()).map_err(io_error);
            }
            return Err(invalid("uncertain build output retained"));
        }
        remove_owned_tree(&self.build_dir(), &self.layout.staging_dir())
    }

    pub fn remove_slot(&self, slot: Slot) -> Result<()> {
        let manifest = self.manager()?.manifest()?.ok_or(invalid("missing manifest"))?;
        if slot == manifest.active_slot
            || manifest.last_good.as_ref().is_some_and(|good| good.slot == slot)
            || manifest.staged_tree.is_some()
        {
            return Err(invalid("retained slot cannot be deleted"));
        }
        let _running = self.slot_lease(slot)?;
        self.clear_slot(slot)
    }

    pub fn slot_lease(&self, slot: Slot) -> Result<File> {
        self.manager()?;
        let path = real_path(&self.layout.slot_dir(slot).join("running.lock"))?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x0020_0000);
        }
        let file = options.open(path).map_err(io_error)?;
        file.try_lock().map_err(|_| invalid("slot still running"))?;
        Ok(file)
    }

    fn clear_slot(&self, slot: Slot) -> Result<()> {
        for entry in fs::read_dir(self.layout.slot_dir(slot)).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.file_name().is_some_and(|name| name == "running.lock") {
                continue;
            }
            real_path(&path)?;
            if path.is_dir() {
                remove_owned_tree(&path, &self.layout.slot_dir(slot))?;
            } else {
                let file = ScopedFile::new(&path, false)?;
                if let Some(hash) = file.hash()? {
                    file.claim(&hash)?.delete()?;
                }
            }
        }
        Ok(())
    }

    /// Called by the trusted runner after flushing journals and settling every owned process/cost reservation.
    pub fn transition(&self, grant: &Grant, repo: &RepoIdentity, revision: u64, check: SwapCheck<'_>, time: u64) -> Result<()> {
        grant.allows(repo, revision, Effect::SelfUpdate).map_err(UpdateError::Grant)?;
        let lock = self.lock()?;
        self.ensure_missions_quiescent()?;
        let manager = self.manager()?;
        self.verify_helper()?;
        // The enrolled launcher never changes. A healthy slot's helper controls only its next transition.
        let current = manager.manifest()?.ok_or(invalid("missing manifest"))?.current_good.ok_or(UpdateError::NoLastGood)?;
        self.verify_slot(current.slot, &current.tree)?;
        let supervisor = self.slot_helper(current.slot)?.unwrap_or_else(|| self.helper());
        manager.enter_quiescence(check.active_window, check.active_operations)?;
        manager.begin_swap(grant, repo, revision, check, time)?;
        let mut command = NativeCommand::new(supervisor, self.layout.dev_root().into());
        command.args.push("--transition".into());
        self.environment(&mut command);
        // The helper retries the coordinator lock until this process releases it. It never inherits a kill-on-close job.
        command.spawn()?;
        drop(lock);
        Ok(())
    }
}

pub struct NativeCommand {
    pub program: PathBuf,
    pub cwd: PathBuf,
    pub args: Vec<String>,
    pub environment: BTreeMap<String, String>,
}
impl NativeCommand {
    pub fn new(program: PathBuf, cwd: PathBuf) -> Self {
        // A fixed allowlist excludes inherited provider credentials, updater/signing variables and injected CLI options.
        let environment = ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR", "LANG"]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok().map(|value| (key.into(), value)))
            .collect();
        Self { program, cwd, args: vec![], environment }
    }
    pub fn spawn(&self) -> Result<Child> {
        real_path(&self.program)?;
        real_path(&self.cwd)?;
        if cfg!(windows) && self.program.extension().is_none_or(|e| !e.eq_ignore_ascii_case("exe")) {
            return Err(invalid("native executable required"));
        }
        let mut command = Command::new(&self.program);
        command
            .env_clear()
            .envs(&self.environment)
            .args(&self.args)
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        command.spawn().map_err(io_error)
    }
}

pub fn release_updates(channel: &str, hidden: bool, custom_data: bool) -> bool {
    channel == "release" && !hidden && !custom_data
}
pub fn check_space(available: Option<u64>, required: u64) -> Result<()> {
    if required == 0 || available.is_none_or(|space| space < required) { Err(invalid("insufficient build space")) } else { Ok(()) }
}
pub fn available_space(root: &Path) -> Option<u64> {
    #[cfg(windows)]
    let normalized = PathBuf::from(root.to_string_lossy().trim_start_matches(r"\\?\"));
    #[cfg(windows)]
    let root = normalized.as_path();
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|disk| root.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space())
}

// Delete leaves through the existing anchored claim/hash API; never follow links or sweep an unrelated directory.
fn remove_owned_tree(path: &Path, parent: &Path) -> Result<()> {
    real_path(parent)?;
    real_path(path)?;
    if path == parent || !path.starts_with(parent) {
        return Err(invalid("unowned cleanup path"));
    }
    if !path.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(path).map_err(io_error)?.collect::<std::io::Result<Vec<_>>>().map_err(io_error)?;
    // Keep the ownership marker until every output is gone, so an interrupted cleanup remains recoverable.
    entries.sort_by_key(|entry| entry.file_name() == ".faber-owned");
    for entry in entries {
        let path = entry.path();
        real_path(&path)?;
        if path.is_dir() {
            remove_owned_tree(&path, parent)?;
        } else {
            let file = ScopedFile::new(&path, false)?;
            if let Some(hash) = file.hash()? {
                file.claim(&hash)?.delete()?;
            }
        }
    }
    fs::remove_dir(path).map_err(io_error)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Monitor {
    Waiting,
    Healthy,
    Rollback(Slot),
    Stopped,
}
fn restored(dev: &Development, action: RollbackAction) -> Result<Monitor> {
    match action {
        RollbackAction::RollbackToLastGood { target_slot, quarantine_slot } => {
            let manifest = dev.manager()?.manifest()?.ok_or(invalid("missing rollback"))?;
            dev.verify_slot(target_slot, &manifest.last_good.ok_or(UpdateError::NoLastGood)?.tree)?;
            // Cleanup is retried at startup/staging; it must never suppress a verified last-good launch.
            let _ = dev.remove_slot(quarantine_slot);
            let _ = dev.clean_build_output();
            Ok(Monitor::Rollback(target_slot))
        }
        _ => Ok(Monitor::Stopped),
    }
}
pub fn recover(dev: &Development, time: u64) -> Result<Monitor> {
    let manager = dev.manager()?;
    if manager.manifest().is_err()
        || manager.manifest()?.is_some_and(|m| matches!(m.state, UpdateState::Swapping { .. } | UpdateState::Probation { .. }))
    {
        dev.ensure_missions_quiescent()?;
    }
    if manager.manifest().is_err() {
        let bytes = ScopedFile::new(&dev.layout.dev_root().join("rollback.json"), false)?
            .read_limited(16 * 1024 * 1024)?
            .ok_or(invalid("missing rollback checkpoint"))?
            .0;
        let checkpoint: faber_engine::project::update::UpdateManifest =
            serde_json::from_slice(&bytes).map_err(|_| invalid("corrupt rollback checkpoint"))?;
        let good = if matches!(checkpoint.state, UpdateState::Swapping { .. }) { checkpoint.last_good } else { checkpoint.current_good };
        let good = good.ok_or(UpdateError::NoLastGood)?;
        dev.verify_slot(good.slot, &good.tree)?;
    }
    match manager.recover_transition(time)? {
        Some(action) => restored(dev, action),
        None => Ok(Monitor::Stopped),
    }
}
/// Caller must settle the child tree before invoking this with `alive=false`.
pub fn monitor(dev: &Development, alive: bool, time: u64) -> Result<Monitor> {
    let manager = dev.manager()?;
    let manifest = manager.manifest()?.ok_or(invalid("missing manifest"))?;
    if let UpdateState::Probation { health_ack_deadline, probation_deadline, health_acknowledged, .. } = manifest.state {
        dev.ensure_missions_quiescent()?;
        let reason = if time < manifest.updated_at {
            Some(RollbackReason::StartupCorrupt)
        } else if !alive {
            if health_acknowledged && manifest.first_operation_completed && dev.has_clean_exit()? {
                manager.pause_probation(time)?;
                return Ok(Monitor::Stopped);
            }
            Some(RollbackReason::ProbationCrash)
        } else if !health_acknowledged && time > health_ack_deadline {
            Some(RollbackReason::HealthTimeout)
        } else if time >= probation_deadline && !manifest.first_operation_completed {
            Some(RollbackReason::FirstOperationFailed)
        } else {
            None
        };
        if let Some(reason) = reason {
            return restored(dev, manager.trigger_rollback(reason, time)?);
        }
        if time >= probation_deadline {
            manager.finish_probation(time)?;
            let _ = dev.clean_build_output();
            return Ok(Monitor::Healthy);
        }
        return Ok(Monitor::Waiting);
    }
    Ok(Monitor::Stopped)
}

pub fn compatible_round_trip(saved: &Value, old: &Value, next: &Value) -> Result<()> {
    if old != saved || next != saved { Err(invalid("rollback data round trip is lossy")) } else { Ok(()) }
}

fn saved_inventory(root: &Path) -> Result<Value> {
    fn collect(path: &Path, root: &Path, values: &mut BTreeMap<String, Value>) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(path).map_err(io_error)? {
            let path = real_path(&entry.map_err(io_error)?.path())?;
            if path.file_name().is_some_and(|name| name == "webview" || name == "workspaces") {
                continue;
            }
            if path.is_dir() {
                collect(&path, root, values)?;
            } else if path.extension().is_some_and(|extension| extension == "json") {
                let bytes = ScopedFile::new(&path, false)?.read_limited(16 * 1024 * 1024)?.ok_or(invalid("missing saved record"))?.0;
                let value = serde_json::from_slice(&bytes).map_err(|_| invalid("corrupt saved record"))?;
                values.insert(
                    path.strip_prefix(root).map_err(|_| invalid("outside saved data"))?.to_string_lossy().replace('\\', "/"),
                    value,
                );
            }
        }
        Ok(())
    }
    real_path(root)?;
    let mut values = BTreeMap::new();
    collect(root, root, &mut values)?;
    serde_json::to_value(values).map_err(|_| invalid("data serialization"))
}

fn round_trip<T: serde::de::DeserializeOwned + serde::Serialize>(value: &Value) -> Result<Value> {
    serde_json::to_value(serde_json::from_value::<T>(value.clone()).map_err(|_| invalid("incompatible saved record"))?)
        .map_err(|_| invalid("data serialization"))
}

/// Runs in each executable, on a private copy. Real record codecs must preserve grants, spend and extension fields.
pub fn probe_saved_data(root: &Path, output: &Path) -> Result<()> {
    validate_saved_data(root)?;
    let mut records = saved_inventory(root)?;
    for (path, record) in records.as_object_mut().ok_or(invalid("invalid saved inventory"))? {
        let name = path.rsplit('/').next().unwrap_or(path);
        *record = match name {
            "runtime.json" => round_trip::<faber_engine::project::Runtime>(record)?,
            "grant.json" => round_trip::<Grant>(record)?,
            // The engine writes its whole quota ledger to budget.json; the mission spending is a field inside it.
            "budget.json" => {
                let ledger = round_trip::<faber_engine::ledger::Ledger>(record)?;
                let parsed: faber_engine::ledger::Ledger =
                    serde_json::from_value(ledger.clone()).map_err(|_| invalid("incompatible saved record"))?;
                faber_engine::project::budget::Budget::load(&parsed).map_err(|_| invalid("incompatible saved record"))?;
                ledger
            }
            "ledger.json" => round_trip::<faber_engine::ledger::Ledger>(record)?,
            "mission.json" => round_trip::<faber_engine::project::Mission>(record)?,
            "plan.json" => round_trip::<faber_engine::project::Plan>(record)?,
            _ => record.clone(),
        };
    }
    ScopedFile::new(&real_path(output)?, true)?.write(&serde_json::to_vec(&records).expect("plain data serializes"), None)?;
    Ok(())
}

pub fn native_compatible(previous: &Path, next: &Path, saved_data: &Path) -> Result<()> {
    let saved = saved_inventory(saved_data)?;
    let parent = saved_data.parent().ok_or(invalid("missing compatibility parent"))?;
    let mut receipts = vec![];
    for (program, name) in [(previous, "old-data.json"), (next, "next-data.json")] {
        let output = parent.join(name);
        let mut command = NativeCommand::new(program.into(), parent.into());
        command.args =
            vec!["--faber-local-check-data".into(), saved_data.to_string_lossy().into_owned(), output.to_string_lossy().into_owned()];
        run_bounded(&command, std::time::Duration::from_secs(30))?;
        let bytes = ScopedFile::new(&output, false)?.read_limited(16 * 1024 * 1024)?.ok_or(invalid("missing compatibility receipt"))?.0;
        receipts.push(serde_json::from_slice(&bytes).map_err(|_| invalid("corrupt compatibility receipt"))?);
    }
    compatible_round_trip(&saved, &receipts[0], &receipts[1])
}

pub struct SnapshotEntry {
    pub path: PathBuf,
    pub oid: String,
    pub executable: bool,
}
pub fn snapshot_entries(bytes: &[u8], root: &Path) -> Result<Vec<SnapshotEntry>> {
    let mut entries = vec![];
    for record in bytes.split(|b| *b == 0).filter(|record| !record.is_empty()) {
        let record = std::str::from_utf8(record).map_err(|_| invalid("invalid tree path"))?;
        let (metadata, name) = record.split_once('\t').ok_or(invalid("invalid tree entry"))?;
        let parts: Vec<_> = metadata.split(' ').collect();
        let safe = clean_path(name).filter(|path| path == name && !path.is_empty()).ok_or(invalid("unsafe tree path"))?;
        if safe.split('/').any(|part| part.eq_ignore_ascii_case(".git"))
            || safe.starts_with("node_modules/")
            || parts.len() != 3
            || !["100644", "100755"].contains(&parts[0])
            || parts[1] != "blob"
            || !valid_oid(parts[2])
        {
            return Err(invalid("unsupported tree entry"));
        }
        let path = real_path(&root.join(safe))?;
        if entries.iter().any(|entry: &SnapshotEntry| entry.path == path) {
            return Err(invalid("duplicate tree path"));
        }
        entries.push(SnapshotEntry { path, oid: parts[2].into(), executable: parts[0] == "100755" });
    }
    if entries.is_empty() {
        return Err(invalid("empty source tree"));
    }
    Ok(entries)
}

fn git(executor: &mut impl Executor, repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let call = Invocation {
        cwd: repo.into(),
        args: args.iter().map(|arg| (*arg).into()).collect(),
        identity: false,
        author: None,
        private_index: None,
    };
    let output = executor.execute(&call).map_err(|_| invalid("source git failed"))?;
    if !output.success {
        return Err(invalid("source git failed"));
    }
    Ok(output.stdout)
}

/// The T5 runner supplies private acceptance/review receipts, reserved real/fake canaries and old/new data probes.
/// No IPC command accepts these assertions from a worker or a Projects screen.
pub trait Runner {
    fn available_space(&self, root: &Path) -> Option<u64> {
        available_space(root)
    }
    fn accepted(&mut self, repo: &RepoIdentity, commit: &str, tree: &str, snapshot: &Path) -> Result<String>;
    fn build(&mut self, snapshot: &Path, target: &Path) -> Result<(PathBuf, PathBuf)>;
    fn canary(&mut self, executable: &Path, tree: &ExactTree, data: &Path) -> Result<CanaryVerdict>;
    fn compatible(&mut self, previous: &Path, next: &Path, saved_data: &Path) -> Result<()> {
        native_compatible(previous, next, saved_data)
    }
}

pub struct Stage<'a> {
    pub repo: &'a RepoIdentity,
    pub commit: &'a str,
    pub tree: &'a str,
    pub grant: &'a Grant,
    pub revision: u64,
    pub required_space: u64,
}

impl Development {
    /// Consume durable landing requests, including ones saved before a restart.
    pub fn service_landed_missions<R: Runner>(
        &self,
        engine: &faber_engine::Engine,
        active_window: bool,
        git: &mut impl Executor,
        mut make_runner: impl FnMut(&str, &Path) -> Result<R>,
    ) -> Result<bool> {
        let _chat = engine.local_build_admission().map_err(|_| UpdateError::ActiveOperations)?;
        self.layout.missions_quiescent()?;
        let projects = engine.projects_checked().map_err(|_| invalid("invalid project grants"))?;
        let root = self.data().join("missions");
        if !root.exists() {
            return Ok(false);
        }
        let mut pending = vec![];
        for entry in fs::read_dir(&root).map_err(io_error)? {
            let path = real_path(&entry.map_err(io_error)?.path())?;
            let Some((bytes, _)) = ScopedFile::new(&path.join("run.json"), false)?.read_limited(16 * 1024 * 1024)? else { continue };
            let run: faber_engine::project::run::Runner =
                serde_json::from_slice(&bytes).map_err(|_| invalid("corrupt mission checkpoint"))?;
            if path.file_name().and_then(|n| n.to_str()) != Some(&run.mission.id) {
                return Err(invalid("mission checkpoint mismatch"));
            }
            if run.promotion_pending && run.mission.state == faber_engine::project::MissionState::Done {
                pending.push(run);
            }
        }
        pending.sort_by_key(|r| r.landings.last().map_or(0, |l| l.at));
        let Some(recorded) = pending.pop() else { return Ok(false) };
        let project = projects.iter().find(|p| p.id == recorded.mission.project_id).ok_or(invalid("mission project missing"))?;
        let repo = project.repo.as_ref().filter(|r| r.common_dir == recorded.mission.repo).ok_or(invalid("mission repository changed"))?;
        let grant = project.grant.as_ref().ok_or(UpdateError::Grant("reason.grant.none"))?;
        let journal = faber_engine::project::store::Journal::open(Path::new(&project.path), &self.data(), &recorded.mission.id)
            .map_err(|_| invalid("invalid private mission ledger"))?;
        let mut run = faber_engine::project::run::Runner::load(&journal).map_err(|_| invalid("invalid private mission checkpoint"))?;
        let landing = run.landings.last().ok_or(invalid("missing accepted landing"))?;
        let receipt = journal
            .effect_receipt(&landing.commit_intent)
            .map_err(|_| invalid("invalid private landing receipt"))?
            .ok_or(invalid("missing private landing receipt"))?;
        let commit = receipt.evidence.get("head").ok_or(invalid("missing landed commit"))?;
        let tree = &landing.candidate.tree;
        let result = (|| {
            let mut runner = make_runner(&run.mission.id, Path::new(&project.path))?;
            self.stage_landed_mission(repo, grant, commit, tree, run.mission.grant_revision, git, &mut runner)?;
            if self.manager()?.manifest()?.is_some_and(|m| matches!(m.state, UpdateState::Ready { .. })) {
                runner.accepted(repo, commit, tree, &self.build_dir().join("source"))?;
                let manifest = self.manager()?.manifest()?.ok_or(invalid("missing manifest"))?;
                let payload = read_payload(&self.binary(manifest.active_slot.other()))?;
                self.transition(
                    grant,
                    repo,
                    run.mission.grant_revision,
                    SwapCheck { payload: &payload, active_window, active_operations: false },
                    now(),
                )?;
            }
            Ok(())
        })();
        if matches!(result, Err(UpdateError::ActiveOperations)) {
            return result.map(|()| true);
        }
        run.promotion_pending = false;
        if let Err(error) = &result {
            run.mission.reason = Some(error.key().into());
        }
        run.save(&journal).map_err(|_| invalid("failed to settle landing request"))?;
        for mut older in pending.into_iter().filter(|r| r.mission.repo == run.mission.repo) {
            let owner = projects.iter().find(|p| p.id == older.mission.project_id).ok_or(invalid("obsolete mission project missing"))?;
            if owner.repo.as_ref().is_none_or(|repo| repo.common_dir != older.mission.repo) {
                return Err(invalid("obsolete mission repository changed"));
            }
            let journal = faber_engine::project::store::Journal::open(Path::new(&owner.path), &self.data(), &older.mission.id)
                .map_err(|_| invalid("invalid obsolete landing ledger"))?;
            older.promotion_pending = false;
            older.save(&journal).map_err(|_| invalid("failed to coalesce obsolete landing"))?;
        }
        result.map(|()| true)
    }

    /// Baseline enrollment is explicit; never copy an installed release or a running slot.
    pub fn adopt(&self, tree: ExactTree, executable: &Path, helper: &Path, time: u64) -> Result<()> {
        let _lock = self.lock()?;
        if self.manager()?.manifest()?.is_some() {
            return Err(invalid("baseline already adopted"));
        }
        for path in [executable, helper] {
            if self.excluded(path) {
                return Err(invalid("installed payload excluded"));
            }
        }
        let bytes = read_payload(executable)?;
        if !tree.matches_payload(&bytes) {
            return Err(invalid("baseline payload mismatch"));
        }
        let helper_bytes = read_payload(helper)?;
        let enrollment = Enrollment {
            channel: "development".into(),
            installed_data: self.installed_data.clone(),
            installed_release: self.installed_release.clone(),
        };
        ScopedFile::new(&self.layout.dev_root().join("development.json"), false)?
            .write(&serde_json::to_vec(&enrollment).expect("plain data serializes"), None)?;
        write_payload(&self.binary(Slot::A), &bytes)?;
        write_payload(&self.helper(), &helper_bytes)?;
        ScopedFile::new(&self.layout.dev_root().join("helper.sha256"), false)?
            .write(ExactTree::compute_build_hash(&helper_bytes).as_bytes(), None)?;
        self.manager()?.adopt_baseline(Slot::A, tree, &bytes, time)
    }

    pub fn stage(&self, request: Stage<'_>, git_executor: &mut impl Executor, runner: &mut impl Runner) -> Result<Slot> {
        let _lock = self.lock()?;
        request.grant.allows(request.repo, request.revision, Effect::SelfUpdate).map_err(UpdateError::Grant)?;
        self.layout.missions_quiescent()?;
        if !valid_oid(request.commit) || !valid_oid(request.tree) {
            return Err(invalid("invalid accepted source identity"));
        }
        let manager = self.manager()?;
        let manifest = manager.manifest()?.ok_or(UpdateError::NoLastGood)?;
        if !matches!(
            manifest.state,
            UpdateState::Idle
                | UpdateState::Healthy
                | UpdateState::RolledBack { .. }
                | UpdateState::Quarantined { .. }
                | UpdateState::PreparingNext { .. }
                | UpdateState::CanaryPending { .. }
                | UpdateState::Ready { .. }
        ) {
            return Err(invalid("candidate or probation already pending"));
        }
        check_space(runner.available_space(self.layout.dev_root()), request.required_space.max(4 * 1024 * 1024 * 1024))?;
        let _inactive = self.slot_lease(manifest.active_slot.other())?;
        self.verify_slot(manifest.active_slot, &manifest.current_good.as_ref().ok_or(UpdateError::NoLastGood)?.tree)?;
        let source = real_path(Path::new(&request.repo.root))?;
        if self.excluded(&source) {
            return Err(invalid("installed source excluded"));
        }
        let resolved = git(git_executor, &source, &["rev-parse", "--verify", &format!("{}^{{tree}}", request.commit)])?;
        if std::str::from_utf8(&resolved).ok().map(str::trim) != Some(request.tree) {
            return Err(invalid("accepted tree changed"));
        }
        self.begin_build_output()?;
        let snapshot = self.build_dir().join("source");
        let result = (|| {
            let entries = snapshot_entries(&git(git_executor, &source, &["ls-tree", "-rz", request.tree])?, &snapshot)?;
            for entry in entries {
                let bytes = git(git_executor, &source, &["cat-file", "blob", &entry.oid])?;
                ScopedFile::new(&entry.path, true)?.write(&bytes, None)?;
                #[cfg(unix)]
                if entry.executable {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&entry.path, fs::Permissions::from_mode(0o755)).map_err(io_error)?;
                }
            }
            let frozen = source_inventory(&snapshot)?;
            let proof = runner.accepted(request.repo, request.commit, request.tree, &snapshot)?;
            if proof.trim().is_empty() {
                return Err(invalid("missing exact-tree proof and review"));
            }
            if source_inventory(&snapshot)? != frozen {
                return Err(invalid("proof modified accepted source"));
            }
            if let UpdateState::PreparingNext { slot, ref tree }
            | UpdateState::CanaryPending { slot, ref tree }
            | UpdateState::Ready { slot, ref tree, .. } = manifest.state
                && tree.commit_oid == request.commit
                && tree.tree_oid == request.tree
                && manifest.candidate_repo.as_ref() == Some(&request.repo.common_dir)
                && self.verify_slot(slot, tree).is_ok()
                && self.slot_helper(slot).is_ok_and(|helper| helper.is_some())
            {
                if tree.extra.get("proofReceipt").and_then(Value::as_str) != Some(&proof) {
                    return Err(invalid("accepted proof changed"));
                }
                if !matches!(manifest.state, UpdateState::Ready { .. }) {
                    let compatibility = self.build_dir().join("compatibility");
                    copy_saved_data(&self.data(), &compatibility)?;
                    runner.compatible(&self.binary(manifest.active_slot), &self.binary(slot), &compatibility)?;
                    self.run_canary(slot, tree, runner)?;
                }
                return Ok(slot);
            }
            let target = self.build_dir().join("target");
            let (executable, helper) = runner.build(&snapshot, &target)?;
            if source_inventory(&snapshot)? != frozen {
                return Err(invalid("build modified accepted source"));
            }
            for path in [&executable, &helper] {
                real_path(path)?;
                if !path.starts_with(&target) {
                    return Err(invalid("unowned build output"));
                }
            }
            let bytes = read_payload(&executable)?;
            let identity =
                ScopedFile::new(&target.join("local-identity.json"), false)?.read()?.ok_or(invalid("missing candidate identity"))?.0;
            let identity: Value = serde_json::from_slice(&identity).map_err(|_| invalid("invalid candidate identity"))?;
            let schema = identity["schema"].as_u64().and_then(|s| u32::try_from(s).ok()).ok_or(invalid("invalid candidate schema"))?;
            if identity["buildHash"].as_str() != Some(&ExactTree::compute_build_hash(&bytes)) {
                return Err(invalid("candidate identity payload mismatch"));
            }
            let mut tree = ExactTree::new(request.commit, request.tree, &ExactTree::compute_build_hash(&bytes), schema)?;
            tree.extra.insert("proofReceipt".into(), proof.clone().into());
            if schema > SCHEMA {
                return Err(UpdateError::Newer);
            }
            let compatibility = self.build_dir().join("compatibility");
            copy_saved_data(&self.data(), &compatibility)?;
            runner.compatible(&self.binary(manifest.active_slot), &executable, &compatibility)?;
            if matches!(manifest.state, UpdateState::PreparingNext { .. } | UpdateState::CanaryPending { .. } | UpdateState::Ready { .. }) {
                manager.supersede_next(request.grant, request.repo, request.revision, now())?;
            }
            let slot = manager.prepare_next(tree.clone(), request.grant, request.repo, request.revision, &[], now())?;
            // prepare_next atomically rotates current into last-good before the old inactive slot can be reused.
            self.clear_slot(slot)?;
            write_payload(&self.binary(slot), &bytes)?;
            write_payload(
                &self.layout.slot_dir(slot).join(format!("faber-local{}", std::env::consts::EXE_SUFFIX)),
                &read_payload(&helper)?,
            )?;
            let helper_hash = ScopedFile::new(&helper, false)?.hash()?.ok_or(invalid("missing helper"))?;
            ScopedFile::new(&self.layout.slot_dir(slot).join("helper.sha256"), false)?.write(helper_hash.as_bytes(), None)?;
            self.run_canary(slot, &tree, runner)?;
            Ok(slot)
        })();
        let cleanup = self.clean_build_output();
        match result {
            Err(error) => {
                cleanup?;
                Err(error)
            }
            Ok(slot) => {
                cleanup?;
                Ok(slot)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stage_landed_mission(
        &self,
        repo: &RepoIdentity,
        grant: &Grant,
        commit: &str,
        tree: &str,
        revision: u64,
        git_executor: &mut impl Executor,
        runner: &mut impl Runner,
    ) -> Result<Option<Slot>> {
        if !grant.self_update {
            return Ok(None);
        }
        let request = Stage { repo, commit, tree, grant, revision, required_space: 0 };
        let slot = self.stage(request, git_executor, runner)?;
        Ok(Some(slot))
    }

    fn run_canary(&self, slot: Slot, tree: &ExactTree, runner: &mut impl Runner) -> Result<()> {
        self.verify_slot(slot, tree)?;
        let bytes = read_payload(&self.binary(slot))?;
        let proof = tree.extra.get("proofReceipt").and_then(Value::as_str).ok_or(invalid("missing accepted proof receipt"))?;
        let canary = self.build_dir().join("canary");
        faber_engine::project::canary::lay_out(&canary).map_err(|_| invalid("canary fixtures unavailable"))?;
        let verdict = runner
            .canary(&self.binary(slot), tree, &canary)
            .unwrap_or_else(|_| CanaryVerdict::Fail { at: now(), reason: "error.update.canaryFailed".into() });
        if verdict == CanaryVerdict::NotRun {
            self.manager()?.wait_for_canary(now())?;
            return Ok(());
        }
        match &verdict {
            CanaryVerdict::Pass { tree: verified, proof_receipt, .. } if verified == tree && proof_receipt == proof => {}
            _ => {
                self.manager()?.record_canary_verdict(
                    CanaryVerdict::Fail { at: now(), reason: "error.update.canaryFailed".into() },
                    &bytes,
                    now(),
                )?;
                self.clear_slot(slot)?;
                return Err(invalid("mandatory canary unavailable or failed"));
            }
        }
        self.manager()?.record_canary_verdict(verdict, &bytes, now())
    }
}

fn read_payload(path: &Path) -> Result<Vec<u8>> {
    if faber_engine::project::update::is_forbidden_root(path) {
        return Err(invalid("protected release payload"));
    }
    ScopedFile::new(&real_path(path)?, false)?.read_limited(256 * 1024 * 1024)?.map(|(bytes, _)| bytes).ok_or(invalid("missing executable"))
}

/// What the build itself writes inside the snapshot and git ignores: installed packages, the screen's output and the
/// shell's generated schemas. Everything else must stay exactly as accepted.
const BUILD_OUTPUT: [&str; 3] = ["node_modules", "dist", "src-tauri/gen"];

fn source_inventory(root: &Path) -> Result<BTreeMap<PathBuf, String>> {
    fn collect(path: &Path, relative: &Path, entries: &mut BTreeMap<PathBuf, String>) -> Result<()> {
        for entry in fs::read_dir(path).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let child = relative.join(entry.file_name());
            if BUILD_OUTPUT.iter().any(|output| child == Path::new(output)) {
                continue;
            }
            let path = real_path(&entry.path())?;
            if path.is_dir() {
                collect(&path, &child, entries)?;
            } else {
                entries.insert(path.clone(), ScopedFile::new(&path, false)?.hash()?.ok_or(invalid("source file disappeared"))?);
            }
        }
        Ok(())
    }
    let mut entries = BTreeMap::new();
    collect(root, Path::new(""), &mut entries)?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_may_write_its_ignored_output_but_nothing_else() {
        let dir = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src-tauri/src")).unwrap();
        fs::write(root.join("src-tauri/src/main.rs"), "fn main() {}").unwrap();
        let before = source_inventory(root).unwrap();
        for output in ["dist/index.js", "node_modules/p/i.js", "src-tauri/gen/schemas/desktop.json"] {
            fs::create_dir_all(root.join(output).parent().unwrap()).unwrap();
            fs::write(root.join(output), "generated").unwrap();
        }
        assert_eq!(source_inventory(root).unwrap(), before);
        fs::create_dir_all(root.join("src-tauri/src/gen")).unwrap();
        fs::write(root.join("src-tauri/src/gen/x.rs"), "x").unwrap();
        assert_ne!(source_inventory(root).unwrap(), before, "only the exact generated folders are exempt");
    }
}

pub fn build_commands(snapshot: &Path, target: &Path, node: &Path, cargo: &Path) -> Vec<NativeCommand> {
    let mut ui = NativeCommand::new(node.into(), snapshot.into());
    ui.args = ["node_modules/vite/bin/vite.js", "build"].map(String::from).to_vec();
    let mut shell = NativeCommand::new(cargo.into(), snapshot.into());
    shell.args = ["build", "--offline", "--locked", "--release", "--package", "faber", "--bin", "faber", "--bin", "faber-local"]
        .map(String::from)
        .to_vec();
    for key in [
        "RUSTUP_HOME",
        "CARGO_HOME",
        "LIB",
        "INCLUDE",
        "LIBPATH",
        "VCINSTALLDIR",
        "VCToolsInstallDir",
        "ProgramData",
        "ProgramFiles",
        "ProgramFiles(x86)",
    ] {
        if let Ok(value) = std::env::var(key) {
            shell.environment.insert(key.into(), value);
        }
    }
    shell.environment.insert("CARGO_TARGET_DIR".into(), target.to_string_lossy().into_owned());
    shell.environment.insert("CARGO_INCREMENTAL".into(), "0".into());
    shell.environment.insert("CARGO_BUILD_JOBS".into(), "4".into());
    vec![ui, shell]
}

/// Offline native build used by the T5 Runner::build implementation. Dependencies must already be installed.
pub fn native_build(snapshot: &Path, target: &Path, dependencies: &Path, node: &Path, cargo: &Path) -> Result<(PathBuf, PathBuf)> {
    real_path(snapshot)?;
    real_path(target)?;
    real_path(dependencies)?;
    if target == snapshot || !target.starts_with(snapshot.parent().ok_or(invalid("missing build parent"))?) {
        return Err(invalid("unowned build target"));
    }
    // Pin the installed dependency tree to this exact source lockfile; never run npm install or package hooks.
    let source_lock =
        ScopedFile::new(&dependencies.parent().ok_or(invalid("missing dependency parent"))?.join("package-lock.json"), false)?;
    let snapshot_lock = ScopedFile::new(&snapshot.join("package-lock.json"), false)?;
    if source_lock.hash()?.is_none() || source_lock.hash()? != snapshot_lock.hash()? {
        return Err(invalid("installed dependencies do not match accepted lock"));
    }
    copy_dependencies(dependencies, &snapshot.join("node_modules"))?;
    for command in build_commands(snapshot, target, node, cargo) {
        let mut child = OwnedChild::start(&command)?;
        let started = std::time::Instant::now();
        loop {
            if let Some(status) = child.child.try_wait().map_err(io_error)? {
                if !status.success() {
                    return Err(invalid("local build failed"));
                }
                break;
            }
            if started.elapsed() >= std::time::Duration::from_secs(20 * 60) {
                return Err(invalid("local build timeout"));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    let executable = target.join(format!("release/faber{}", std::env::consts::EXE_SUFFIX));
    let mut inspect = NativeCommand::new(executable.clone(), target.into());
    inspect.args = vec!["--faber-local-identity".into(), target.join("local-identity.json").to_string_lossy().into_owned()];
    run_bounded(&inspect, std::time::Duration::from_secs(30))?;
    Ok((executable, target.join(format!("release/faber-local{}", std::env::consts::EXE_SUFFIX))))
}

/// How the candidate's own miniature mission runs. Only `Real` can earn a pass; `Offline` replays recorded replies and
/// can never promote a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanaryMode {
    Unavailable,
    Offline,
    Real,
}

pub struct ProductionRunner {
    pub dependencies: PathBuf,
    pub node: PathBuf,
    pub cargo: PathBuf,
    pub canary: CanaryMode,
    pub data: PathBuf,
    pub mission: String,
    _power: Option<crate::awake::MissionAwakeGuard>,
}

impl ProductionRunner {
    pub fn new(dependencies: PathBuf, node: PathBuf, cargo: PathBuf, data: PathBuf, mission: String) -> Self {
        Self { dependencies, node, cargo, canary: CanaryMode::Offline, data, mission, _power: None }
    }

    pub fn with_awake(mut self, awake: std::sync::Arc<std::sync::Mutex<crate::awake::CountedAwake>>) -> Self {
        self._power = Some(crate::awake::MissionAwakeGuard::new(awake, "local-update"));
        self
    }

    pub fn for_mission(repo: &Path, data: &Path, mission: &str) -> Result<Self> {
        let native = |name| {
            let cli =
                faber_engine::cli::find_cli(name).filter(|cli| cli.prefix.is_empty()).ok_or(invalid("native build tool unavailable"))?;
            Ok::<_, UpdateError>(PathBuf::from(cli.command))
        };
        let mut runner = Self::new(repo.join("node_modules"), native("node")?, native("cargo")?, data.into(), mission.into());
        if real_canary_allowed(data) {
            runner.canary = CanaryMode::Real;
        }
        Ok(runner)
    }
}

/// A real canary spends the person's subscription, so it runs only when a person put the allowance in the development
/// data folder: `real-canary.json` with a `capMicros` that covers what one canary may cost. Without it the candidate
/// still runs its replayed mission, stays ready and waits.
pub fn real_canary_allowed(data: &Path) -> bool {
    let Ok(Some((bytes, _))) = ScopedFile::new(&data.join("real-canary.json"), false).and_then(|f| f.read_limited(4096)) else {
        return false;
    };
    serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|v| v["capMicros"].as_u64())
        .is_some_and(|cap| cap >= faber_engine::project::canary::REAL_CAP_MICROS)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanaryHandshake {
    pub nonce: String,
    pub build_hash: String,
    pub schema: u32,
    pub receipt: String,
    pub verdict: CanaryVerdict,
    /// What the child says its miniature mission did. The supervisor reads the journal itself and believes none of it.
    #[serde(default)]
    pub evidence: Option<faber_engine::project::canary::Evidence>,
    /// The `reason.*` key the mission stopped with, when it did not finish.
    #[serde(default)]
    pub stopped: Option<String>,
}

/// This bounded headless operation runs inside the candidate before any GUI runtime or account discovery. It takes the
/// frozen canary goal through the production coordinator on a private data folder and reports what it did. It never
/// issues a verdict: the supervisor reads the mission's journal and ledger itself.
pub fn probe_canary(data: &Path, nonce: &str, output: &Path, mode: &str) -> Result<()> {
    use faber_engine::project::canary::{self, Mode};
    if !faber_engine::project::valid_id(nonce) || !output.starts_with(data) {
        return Err(invalid("invalid canary challenge"));
    }
    let mode = match mode {
        "offline" => Mode::Offline,
        "real" => Mode::Real,
        _ => return Err(invalid("unknown canary mode")),
    };
    if !canary::laid_out(data) {
        return Err(invalid("canary fixture changed"));
    }
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(io_error)?;
    let mut clis = std::collections::BTreeMap::new();
    if mode == Mode::Real {
        for (vendor, name) in [(faber_engine::model::Vendor::Claude, "claude"), (faber_engine::model::Vendor::Codex, "codex")] {
            clis.insert(vendor, faber_engine::cli::find_cli(name).ok_or(invalid("canary CLI unavailable"))?);
        }
    }
    let engine_data = data.join("data");
    canary::prepare_data(&engine_data, mode).map_err(|_| invalid("canary data could not be prepared"))?;
    let engine = faber_engine::Engine::new(
        faber_engine::EngineOptions {
            data_dir: engine_data.clone(),
            clis,
            spawn: faber_engine::proc::real_spawner(),
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: usize::from(mode == Mode::Real),
            idle: std::time::Duration::from_secs(30),
            runtime: runtime.handle().clone(),
            ask: None,
        },
        std::sync::Arc::new(|_| {}),
    );
    validate_saved_data(&engine_data)?;
    let _state = engine.state();
    let outcome = runtime.block_on(canary::run(&engine, &engine_data, &data.join("work"), mode));
    engine.shutdown();
    let executable = std::env::current_exe().map_err(io_error)?;
    let (evidence, stopped) = match outcome {
        Ok(evidence) => (Some(evidence), None),
        Err(key) => (None, Some(key)),
    };
    let handshake = CanaryHandshake {
        nonce: nonce.into(),
        build_hash: ExactTree::compute_build_hash(&read_payload(&executable)?),
        schema: SCHEMA,
        receipt: nonce.into(),
        verdict: CanaryVerdict::NotRun,
        evidence,
        stopped,
    };
    ScopedFile::new(output, true)?.write(&serde_json::to_vec(&handshake).expect("plain data serializes"), None)?;
    Ok(())
}

pub fn verify_canary_handshake(handshake: &CanaryHandshake, nonce: &str, tree: &ExactTree) -> Result<CanaryVerdict> {
    if handshake.nonce != nonce
        || handshake.receipt != nonce
        || handshake.build_hash != tree.build_hash
        || handshake.schema != tree.schema_version
    {
        return Err(invalid("canary handshake mismatch"));
    }
    // A startup acknowledgement cannot substitute for the mandatory writer/proof/other-vendor miniature mission.
    if handshake.verdict != CanaryVerdict::NotRun {
        return Err(invalid("missing private miniature mission receipt"));
    }
    Ok(handshake.verdict.clone())
}

impl Runner for ProductionRunner {
    fn accepted(&mut self, repo: &RepoIdentity, commit: &str, tree: &str, _snapshot: &Path) -> Result<String> {
        let journal = faber_engine::project::store::Journal::open(Path::new(&repo.root), &self.data, &self.mission)
            .map_err(|_| invalid("invalid private mission ledger"))?;
        let runner = faber_engine::project::run::Runner::load(&journal).map_err(|_| invalid("invalid private mission checkpoint"))?;
        if runner.mission.repo != repo.common_dir || runner.mission.state != faber_engine::project::MissionState::Done {
            return Err(invalid("mission is not accepted for this repository"));
        }
        let landing =
            runner.accepted_landing(&journal, commit, tree).map_err(|_| invalid("missing private exact-tree proof and review"))?;
        Ok(format!("{}-{}-{}", runner.mission.id, landing.commit_intent, landing.forward_intent))
    }

    fn build(&mut self, snapshot: &Path, target: &Path) -> Result<(PathBuf, PathBuf)> {
        native_build(snapshot, target, &self.dependencies, &self.node, &self.cargo)
    }

    fn canary(&mut self, executable: &Path, tree: &ExactTree, data: &Path) -> Result<CanaryVerdict> {
        if self.canary == CanaryMode::Unavailable {
            return Ok(CanaryVerdict::NotRun);
        }
        let bytes = read_payload(executable)?;
        if ExactTree::compute_build_hash(&bytes) != tree.build_hash {
            return Ok(CanaryVerdict::Fail { at: now(), reason: "error.update.payloadMismatch".into() });
        }
        let native = if cfg!(windows) {
            bytes.starts_with(b"MZ")
        } else {
            bytes.starts_with(b"\x7fELF")
                || matches!(
                    bytes.get(..4),
                    Some(
                        [0xfe, 0xed, 0xfa, 0xce]
                            | [0xfe, 0xed, 0xfa, 0xcf]
                            | [0xce, 0xfa, 0xed, 0xfe]
                            | [0xcf, 0xfa, 0xed, 0xfe]
                            | [0xca, 0xfe, 0xba, 0xbe]
                            | [0xca, 0xfe, 0xba, 0xbf]
                            | [0xbe, 0xba, 0xfe, 0xca]
                            | [0xbf, 0xba, 0xfe, 0xca]
                    )
                )
        };
        if !native {
            return Err(invalid("native canary executable required"));
        }
        let real = self.canary == CanaryMode::Real;
        // Each run starts from nothing of the last one: its own mission folder and a private copy of the engine's
        // spending and qualification records (the running engine keeps the original; a copy cannot be spent twice).
        for stale in ["work", "data"] {
            remove_owned_tree(&data.join(stale), data)?;
        }
        if real {
            seed_engine_data(&self.data, &data.join("data"))?;
        }
        let nonce = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| invalid("canary clock unavailable"))?.as_nanos()
        );
        let output = data.join(format!("handshake-{nonce}.json"));
        let mut command = NativeCommand::new(executable.into(), data.into());
        command.args = vec![
            "--faber-local-canary".into(),
            data.to_string_lossy().into_owned(),
            nonce.clone(),
            output.to_string_lossy().into_owned(),
            if real { "real" } else { "offline" }.into(),
        ];
        // The CLIs find their own sign-in through the user's profile; nothing of it is read here.
        let profile: &[&str] = if real { &["USERPROFILE", "APPDATA", "LOCALAPPDATA", "HOMEDRIVE", "HOMEPATH"] } else { &[] };
        for key in profile.iter().chain(&[
            "RUSTUP_HOME",
            "CARGO_HOME",
            "LIB",
            "INCLUDE",
            "LIBPATH",
            "VCINSTALLDIR",
            "VCToolsInstallDir",
            "ProgramData",
            "ProgramFiles",
            "ProgramFiles(x86)",
        ]) {
            if let Ok(value) = std::env::var(key) {
                command.environment.insert((*key).into(), value);
            }
        }
        run_bounded(&command, std::time::Duration::from_secs(if real { 20 * 60 } else { 10 * 60 }))?;
        let bytes = ScopedFile::new(&output, false)?.read_limited(64 * 1024)?.ok_or(invalid("canary did not acknowledge startup"))?.0;
        let handshake: CanaryHandshake = serde_json::from_slice(&bytes).map_err(|_| invalid("corrupt canary handshake"))?;
        verify_canary_handshake(&handshake, &nonce, tree)?;
        let receipt = self.data.join("missions").join(&self.mission).join(format!("canary-{nonce}.json"));
        ScopedFile::new(&receipt, true)?.write(&bytes, None)?;
        let (verdict, summary) = judge_canary(&handshake, data, tree, self.canary);
        let verified = self.data.join("missions").join(&self.mission).join(format!("canary-verified-{nonce}.json"));
        ScopedFile::new(&verified, true)?.write(&serde_json::to_vec(&summary).expect("plain data serializes"), None)?;
        Ok(verdict)
    }
}

/// Copy what the private engine needs to admit spending: the ledger and the qualification receipts. The CLIs' own
/// sign-in is never touched; each CLI finds it through the profile when it runs.
fn seed_engine_data(from: &Path, to: &Path) -> Result<()> {
    real_path(from)?;
    fs::create_dir_all(to).map_err(io_error)?;
    real_path(to)?;
    let ledger = from.join("budget.json");
    if ledger.is_file() {
        let (bytes, permissions) = ScopedFile::new(&ledger, false)?.read_limited(16 * 1024 * 1024)?.ok_or(invalid("missing ledger"))?;
        ScopedFile::new(&to.join("budget.json"), true)?.write(&bytes, Some(permissions))?;
    }
    let receipts = from.join("qualification");
    if receipts.is_dir() {
        for entry in fs::read_dir(&receipts).map_err(io_error)? {
            let path = real_path(&entry.map_err(io_error)?.path())?;
            if path.is_file() {
                let (bytes, permissions) = ScopedFile::new(&path, false)?.read_limited(1024 * 1024)?.ok_or(invalid("missing receipt"))?;
                let name = path.file_name().ok_or(invalid("receipt name"))?;
                ScopedFile::new(&to.join("qualification").join(name), true)?.write(&bytes, Some(permissions))?;
            }
        }
    }
    Ok(())
}

/// Read what the child's mission really left behind and decide. A mission that stopped is a wait (the build stays
/// ready and the canary runs again); a mission that claims success but cannot prove it is a failure. Only a verified
/// real run is a pass, and the pass echoes the accepted proof the staged tree already carries.
pub fn judge_canary(
    handshake: &CanaryHandshake,
    data: &Path,
    tree: &ExactTree,
    requested: CanaryMode,
) -> (CanaryVerdict, serde_json::Value) {
    use faber_engine::project::canary::{self, Mode};
    let fail =
        |why: &str| (CanaryVerdict::Fail { at: now(), reason: "error.update.canaryFailed".into() }, serde_json::json!({"fail": why}));
    let Some(evidence) = &handshake.evidence else {
        let why = handshake.stopped.clone().unwrap_or_else(|| "reason.canary.noMission".into());
        return (CanaryVerdict::NotRun, serde_json::json!({"wait": why}));
    };
    let expected = if requested == CanaryMode::Real { Mode::Real } else { Mode::Offline };
    let normal = |path: PathBuf| {
        let text = path.to_string_lossy().replace('\\', "/");
        let text = text.trim_start_matches("//?/");
        #[cfg(any(windows, target_os = "macos"))]
        let text = text.to_lowercase();
        PathBuf::from(text)
    };
    let inside = match (real_path(&data.join("work")), real_path(Path::new(&evidence.repo_root))) {
        (Ok(work), Ok(repo)) => normal(repo).starts_with(normal(work)),
        _ => false,
    };
    if evidence.mode != expected || !inside {
        return fail("evidence does not match the run");
    }
    let verified = match canary::verify(evidence, &data.join("data")) {
        Ok(verified) => verified,
        Err(key) => return fail(&key),
    };
    let summary = serde_json::json!({
        "mode": evidence.mode, "mission": verified.mission, "head": verified.head, "tree": verified.tree,
        "writer": verified.writer.name(), "reviewer": verified.reviewer.name(),
        "proofReceipt": verified.proof_receipt, "spentMicros": verified.spent_micros, "millis": evidence.millis,
    });
    if evidence.mode != Mode::Real {
        // Replayed replies prove the build can run a mission; they are never a promotion.
        return (CanaryVerdict::NotRun, summary);
    }
    let Some(proof) = tree.extra.get("proofReceipt").and_then(Value::as_str) else {
        return fail("the staged tree carries no accepted proof");
    };
    let mut schemas = vec![tree.schema_version];
    if !schemas.contains(&SCHEMA) {
        schemas.push(SCHEMA);
    }
    (
        CanaryVerdict::Pass {
            verified_at: now(),
            receipt_id: handshake.receipt.clone(),
            tree: tree.clone(),
            proof_receipt: proof.to_string(),
            compatible_schemas: schemas,
        },
        summary,
    )
}

fn copy_dependencies(source: &Path, target: &Path) -> Result<()> {
    real_path(source)?;
    real_path(target)?;
    for entry in fs::read_dir(source).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if entry.file_name() == ".bin" {
            continue;
        }
        let path = real_path(&entry.path())?;
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_dependencies(&path, &destination)?;
        } else {
            let (bytes, permissions) =
                ScopedFile::new(&path, false)?.read_limited(256 * 1024 * 1024)?.ok_or(invalid("missing dependency"))?;
            ScopedFile::new(&destination, true)?.write(&bytes, Some(permissions))?;
        }
    }
    Ok(())
}

struct OwnedChild {
    child: Child,
    #[cfg(windows)]
    job: Option<win32job::Job>,
    detached: bool,
}
impl OwnedChild {
    fn start(command: &NativeCommand) -> Result<Self> {
        Self::start_guarded(command, &std::env::current_exe().map_err(io_error)?)
    }
    fn start_guarded(command: &NativeCommand, guard: &Path) -> Result<Self> {
        #[cfg(not(windows))]
        {
            let _ = guard;
            Ok(Self { child: command.spawn()?, detached: false })
        }
        #[cfg(windows)]
        {
            use std::io::{Read, Write};
            use std::os::windows::{io::AsRawHandle, process::CommandExt};
            real_path(&command.program)?;
            real_path(&command.cwd)?;
            if command.program.extension().is_none_or(|extension| !extension.eq_ignore_ascii_case("exe")) {
                return Err(invalid("native executable required"));
            }
            let job = win32job::Job::create().map_err(|_| invalid("process containment failed"))?;
            let mut limits = job.query_extended_limit_info().map_err(|_| invalid("process containment failed"))?;
            limits.limit_kill_on_job_close();
            job.set_extended_limit_info(&limits).map_err(|_| invalid("process containment failed"))?;
            // Reuse the engine's guard dispatch/handshake so the actual child cannot run before job assignment.
            let child = Command::new(guard)
                .arg("--faber-process-guard")
                .arg(&command.program)
                .args(&command.args)
                .current_dir(&command.cwd)
                .env_clear()
                .envs(&command.environment)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .creation_flags(0x0800_0000)
                .spawn()
                .map_err(io_error)?;
            let mut owned = Self { child, job: Some(job), detached: false };
            owned
                .job
                .as_ref()
                .unwrap()
                .assign_process(owned.child.as_raw_handle() as isize)
                .map_err(|_| invalid("process containment failed"))?;
            let mut input = owned.child.stdin.take().ok_or(invalid("missing guard input"))?;
            input.write_all(b"G").and_then(|()| input.flush()).map_err(io_error)?;
            let mut output = owned.child.stdout.take().ok_or(invalid("missing guard output"))?;
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut ack = [0];
                let result = output.read_exact(&mut ack).map(|()| ack == *b"R");
                let _ = send.send(result);
                // Builds may write more than a pipe buffer; discard output continuously without retaining it in RAM.
                let _ = std::io::copy(&mut output, &mut std::io::sink());
            });
            if !matches!(receive.recv_timeout(std::time::Duration::from_secs(5)), Ok(Ok(true))) {
                return Err(invalid("guard acknowledgment failed"));
            }
            Ok(owned)
        }
    }
    fn detach(&mut self) -> Result<()> {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            let mut limits = job.query_extended_limit_info().map_err(|_| invalid("process handoff failed"))?;
            // Only a healthy slot may outlive its independent supervisor.
            limits.clear_limits();
            job.set_extended_limit_info(&limits).map_err(|_| invalid("process handoff failed"))?;
        }
        self.detached = true;
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        #[cfg(windows)]
        {
            drop(self.job.take());
        }
        if self.child.try_wait().map_err(io_error)?.is_none() {
            self.child.kill().map_err(io_error)?;
        }
        self.child.wait().map_err(io_error)?;
        Ok(())
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.detached {
            let _ = self.stop();
        }
    }
}
fn write_payload(path: &Path, bytes: &[u8]) -> Result<()> {
    ScopedFile::new(&real_path(path)?, true)?.write(bytes, None)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(io_error)?;
    }
    Ok(())
}

impl Development {
    fn signal_path(&self, name: &str) -> PathBuf {
        self.layout.dev_root().join(format!("{name}.json"))
    }

    /// Engine startup supplies health only after fallible data loading/reconciliation and its bounded first operation.
    pub fn startup_ready(&self, executable: &Path) -> Result<()> {
        let manifest = self.manager()?.manifest()?.ok_or(invalid("missing manifest"))?;
        if !same_path(executable, &self.binary(manifest.active_slot))? {
            return Err(invalid("wrong startup slot"));
        }
        let tree = match &manifest.state {
            UpdateState::Probation { tree, .. } => tree,
            UpdateState::Idle
            | UpdateState::Healthy
            | UpdateState::RolledBack { .. }
            | UpdateState::PreparingNext { .. }
            | UpdateState::CanaryPending { .. }
            | UpdateState::Ready { .. }
            | UpdateState::Quiescent
            | UpdateState::Quarantined { .. } => {
                self.verify_slot(manifest.active_slot, &manifest.current_good.as_ref().ok_or(UpdateError::NoLastGood)?.tree)?;
                return validate_saved_data(&self.data());
            }
            _ => return Err(invalid("startup outside probation")),
        };
        self.verify_slot(manifest.active_slot, tree)?;
        validate_saved_data(&self.data())?;
        let signal = serde_json::to_vec(&serde_json::json!({"tree": tree, "healthyAt": now(), "operationAt": now()}))
            .map_err(|_| invalid("health serialization"))?;
        ScopedFile::new(&self.signal_path("health"), false)?.write(&signal, None)?;
        Ok(())
    }

    fn accept_health_signal(&self) -> Result<()> {
        let manager = self.manager()?;
        let manifest = manager.manifest()?.ok_or(invalid("missing manifest"))?;
        let UpdateState::Probation { tree, health_acknowledged, .. } = &manifest.state else { return Ok(()) };
        if *health_acknowledged && manifest.first_operation_completed {
            return Ok(());
        }
        let Some((bytes, _)) = ScopedFile::new(&self.signal_path("health"), false)?.read()? else { return Ok(()) };
        let signal: Value = serde_json::from_slice(&bytes).map_err(|_| invalid("corrupt health signal"))?;
        if signal["tree"] != serde_json::to_value(tree).map_err(|_| invalid("health serialization"))? {
            return Err(invalid("stale health signal"));
        }
        let health = signal["healthyAt"].as_u64().ok_or(invalid("missing health time"))?;
        let operation = signal["operationAt"].as_u64().ok_or(invalid("missing operation time"))?;
        if health > now() || operation > now() {
            return Err(invalid("future health signal"));
        }
        manager.acknowledge_health(health)?;
        manager.complete_first_operation(operation)
    }

    pub fn record_clean_exit(&self) -> Result<()> {
        let manifest = self.manager()?.manifest()?.ok_or(invalid("missing manifest"))?;
        if let UpdateState::Probation { tree, health_acknowledged: true, .. } = &manifest.state
            && manifest.first_operation_completed
        {
            ScopedFile::new(&self.signal_path("exit"), false)?
                .write(&serde_json::to_vec(&serde_json::json!({"tree": tree})).expect("plain data serializes"), None)?;
        }
        Ok(())
    }
    fn has_clean_exit(&self) -> Result<bool> {
        let Some((bytes, _)) = ScopedFile::new(&self.signal_path("exit"), false)?.read()? else {
            return Ok(false);
        };
        let signal: Value = serde_json::from_slice(&bytes).map_err(|_| invalid("corrupt exit signal"))?;
        let manifest = self.manager()?.manifest()?.ok_or(invalid("missing manifest"))?;
        Ok(matches!(manifest.state, UpdateState::Probation { ref tree, .. }
            if signal["tree"] == serde_json::to_value(tree).expect("plain data serializes")))
    }

    pub fn cleanup_obsolete(&self) -> Result<()> {
        let manifest = self.manager()?.manifest()?.ok_or(invalid("missing manifest"))?;
        if let UpdateState::RolledBack { quarantined_slot, .. } = manifest.state {
            self.remove_slot(quarantined_slot)?;
        }
        self.clean_build_output()
    }
}

pub fn write_identity(output: &Path) -> Result<()> {
    let executable = std::env::current_exe().map_err(io_error)?;
    let hash = ScopedFile::new(&real_path(&executable)?, false)?.hash()?.ok_or(invalid("missing identity payload"))?;
    ScopedFile::new(&real_path(output)?, true)?
        .write(&serde_json::to_vec(&serde_json::json!({"schema": SCHEMA, "buildHash": hash})).expect("plain data serializes"), None)?;
    Ok(())
}

fn run_bounded(command: &NativeCommand, timeout: std::time::Duration) -> Result<()> {
    let mut child = OwnedChild::start(command)?;
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.child.try_wait().map_err(io_error)? {
            return if status.success() { Ok(()) } else { Err(invalid("local probe failed")) };
        }
        if started.elapsed() >= timeout {
            return Err(invalid("local probe timeout"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// The development entry point is the enrolled helper, never an unproven slot's main function.
pub fn launch(dev: &Development, open: bool) -> Result<()> {
    dev.verify_helper()?;
    let Ok(coordinator) = dev.lock() else {
        if open {
            ScopedFile::new(&dev.signal_path("open"), false)?.write(b"{}", None)?;
        }
        return Ok(());
    };
    if open {
        ScopedFile::new(&dev.signal_path("open"), false)?.write(b"{}", None)?;
    }
    let outcome = recover(dev, now())?;
    let manifest = dev.manager()?.manifest()?.ok_or(invalid("missing launch manifest"))?;
    if manifest.extra.get("cleanExit") == Some(&Value::Bool(true)) && matches!(manifest.state, UpdateState::Swapping { .. }) {
        let slot = manifest.current_good.ok_or(UpdateError::NoLastGood)?.slot;
        let supervisor = dev.slot_helper(slot)?.unwrap_or_else(|| dev.helper());
        if same_path(&supervisor, &std::env::current_exe().map_err(io_error)?)? {
            return helper_run(dev, false);
        }
        let mut command = NativeCommand::new(supervisor, dev.layout.dev_root().into());
        command.args.push("--transition".into());
        dev.environment(&mut command);
        command.spawn()?;
        return Ok(());
    }
    let slot = if let Monitor::Rollback(slot) = outcome { slot } else { manifest.active_slot };
    let tree = &manifest.current_good.as_ref().ok_or(UpdateError::NoLastGood)?.tree;
    dev.verify_slot(slot, tree)?;
    if let Ok(lease) = dev.slot_lease(slot) {
        drop(lease);
        dev.slot_command(slot).spawn()?;
    }
    let _ = dev.cleanup_obsolete();
    drop(coordinator);
    Ok(())
}

/// The bounded startup operation reads saved JSON without rewriting it or releasing a consumed grant.
pub fn validate_saved_data(root: &Path) -> Result<()> {
    real_path(root)?;
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        real_path(&path)?;
        if path.file_name().is_some_and(|name| name == "webview" || name == "workspaces") {
            continue;
        }
        if path.is_dir() {
            validate_saved_data(&path)?;
        } else if path.extension().is_some_and(|extension| extension == "json") {
            let bytes = ScopedFile::new(&path, false)?.read_limited(16 * 1024 * 1024)?.ok_or(invalid("missing saved record"))?.0;
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid("corrupt saved data"))?;
            if value.get("schema").is_some_and(|schema| schema.as_u64().is_none_or(|schema| schema == 0 || schema > u64::from(SCHEMA))) {
                return Err(UpdateError::Newer);
            }
        }
    }
    Ok(())
}

fn copy_saved_data(root: &Path, target: &Path) -> Result<()> {
    real_path(root)?;
    real_path(target)?;
    ScopedFile::new(&target.join(".anchor"), true)?;
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        // Process ownership locks are not saved application data.
        if entry.file_name() == "webview" || entry.file_name() == "workspaces" || entry.file_name() == "budget.lock" {
            continue;
        }
        let path = real_path(&entry.path())?;
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_saved_data(&path, &destination)?;
        } else {
            let bytes = ScopedFile::new(&path, false)?.read_limited(16 * 1024 * 1024)?.ok_or(invalid("missing saved data"))?.0;
            ScopedFile::new(&destination, true)?.write(&bytes, None)?;
        }
    }
    Ok(())
}

/// Runs in the independent helper, which owns only children it launched; no PID lookup or public installer.
pub fn helper_main(dev: &Development, recovering: bool) -> Result<()> {
    let started = std::time::Instant::now();
    let _coordinator = loop {
        match dev.lock() {
            Ok(lock) => break lock,
            Err(_) if started.elapsed() < std::time::Duration::from_secs(30) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(error) => return Err(error),
        }
    };
    let result = helper_run(dev, recovering);
    if result.is_err() {
        let outcome = recover(dev, now())?;
        let manifest = dev.manager()?.manifest()?.ok_or(invalid("missing recovery manifest"))?;
        let slot = match outcome {
            Monitor::Rollback(slot) => Some(slot),
            _ if matches!(manifest.state, UpdateState::Healthy | UpdateState::RolledBack { .. }) => Some(manifest.active_slot),
            _ => None,
        };
        if let Some(slot) = slot {
            dev.verify_slot(slot, &manifest.current_good.as_ref().ok_or(UpdateError::NoLastGood)?.tree)?;
            let started = std::time::Instant::now();
            loop {
                match dev.slot_lease(slot) {
                    Ok(lease) => {
                        drop(lease);
                        dev.slot_command(slot).spawn()?;
                        break;
                    }
                    Err(_) if started.elapsed() < std::time::Duration::from_secs(30) => {
                        std::thread::sleep(std::time::Duration::from_millis(100))
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    }
    result
}

#[cfg(windows)]
struct SessionWindow {
    window: winsafe::gui::WindowMain,
    ended: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl SessionWindow {
    fn start(dev: Development, lifecycle: std::sync::Arc<std::sync::Mutex<()>>) -> Result<Self> {
        use winsafe::{co, gui, prelude::*};
        let (send, receive) = std::sync::mpsc::channel();
        let ended = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ending = ended.clone();
        let thread = std::thread::spawn(move || {
            // A top-level window receives ENDSESSION; WS_VISIBLE is absent even during creation.
            let window = gui::WindowMain::new(gui::WindowMainOpts {
                class_name: "FaberLocalSession",
                style: co::WS::OVERLAPPED,
                ex_style: co::WS_EX::NOACTIVATE | co::WS_EX::TOOLWINDOW,
                size: (0, 0),
                ..Default::default()
            });
            let created = window.clone();
            window.on().wm_create(move |_| {
                if created.hwnd().IsWindowVisible() {
                    return Err(std::io::Error::other("visible session observer refused").into());
                }
                let _ = send.send(created.clone());
                Ok(0)
            });
            window.on().wm_end_session(move |event| {
                let _guard = lifecycle.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                session_ending(&dev, event.is_session_being_ended).map_err(|e| std::io::Error::other(e.key()))?;
                if event.is_session_being_ended {
                    ending.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                Ok(())
            });
            let _ = window.run_main(Some(co::SW::HIDE));
        });
        receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .map(|window| Self { window, ended, thread: Some(thread) })
            .map_err(|_| invalid("session observer unavailable"))
    }
}

#[cfg(windows)]
impl Drop for SessionWindow {
    fn drop(&mut self) {
        use winsafe::prelude::*;
        if self.window.hwnd() != &winsafe::HWND::NULL {
            self.window.close();
        }
        // No session callback may race recovery after the supervisor releases its lifecycle mutex.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(any(windows, feature = "test-hooks"))]
fn session_ending(dev: &Development, ending: bool) -> Result<()> {
    if ending && dev.manager()?.manifest()?.is_some_and(|m| matches!(m.state, UpdateState::Swapping { .. } | UpdateState::Probation { .. }))
    {
        dev.manager()?.pause_probation(now())?;
    }
    Ok(())
}

fn helper_run(dev: &Development, recovering: bool) -> Result<()> {
    let manager = dev.manager()?;
    if recovering {
        if let Monitor::Rollback(slot) = recover(dev, now())? {
            dev.slot_command(slot).spawn()?;
        }
        return Ok(());
    }
    #[cfg(windows)]
    let lifecycle = std::sync::Arc::new(std::sync::Mutex::new(()));
    #[cfg(windows)]
    let _session = SessionWindow::start(dev.clone(), lifecycle.clone())?;
    let manifest = manager.manifest()?.ok_or(invalid("missing manifest"))?;
    let UpdateState::Swapping { from_slot, to_slot, ref tree } = manifest.state else {
        return Err(invalid("helper needs pending swap"));
    };
    // Holding the old slot's OS lock proves the old process has exited, without adopting or killing any process by PID.
    let started = std::time::Instant::now();
    let old = loop {
        match dev.slot_lease(from_slot) {
            Ok(lease) => break lease,
            Err(_) if started.elapsed() < std::time::Duration::from_secs(30) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(error) => return Err(error),
        }
    };
    dev.ensure_missions_quiescent()?;
    dev.verify_slot(to_slot, tree)?;
    for name in ["health", "exit"] {
        let signal = ScopedFile::new(&dev.signal_path(name), false)?;
        if let Some(hash) = signal.hash()? {
            signal.claim(&hash)?.delete()?;
        }
    }
    {
        #[cfg(windows)]
        let _guard = lifecycle.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        #[cfg(windows)]
        if _session.ended.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }
        manager.begin_probation(now())?;
    }
    let command = dev.slot_command(to_slot);
    // A detached guard must not keep the independent supervisor's image mapped on Windows.
    let mut child = match OwnedChild::start_guarded(&command, &command.program) {
        Ok(child) => child,
        Err(_) => {
            #[cfg(windows)]
            let _guard = lifecycle.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            #[cfg(windows)]
            if _session.ended.load(std::sync::atomic::Ordering::SeqCst) {
                return Ok(());
            }
            drop(old);
            if let Monitor::Rollback(slot) = recover(dev, now())? {
                dev.slot_command(slot).spawn()?;
            }
            return Ok(());
        }
    };
    let supervision = std::time::Instant::now();
    loop {
        #[cfg(windows)]
        let guard = lifecycle.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        #[cfg(feature = "test-hooks")]
        if let Ok(ending) = fs::read_to_string(dev.data().join("test-session-end")) {
            fs::remove_file(dev.data().join("test-session-end")).map_err(io_error)?;
            session_ending(dev, ending == "true")?;
        }
        let status = child.child.try_wait().map_err(io_error)?;
        let alive = status.is_none();
        if status.is_some_and(|status| status.success()) {
            dev.record_clean_exit()?;
        }
        let signal = dev.accept_health_signal();
        let current = manager.manifest()?.ok_or(invalid("missing manifest"))?;
        let must_stop = !alive
            || signal.is_err()
            || now() < current.updated_at
            || (watched(supervision) > std::time::Duration::from_secs(301)
                && matches!(current.state, UpdateState::Probation { probation_deadline, .. } if now() < probation_deadline))
            || match current.state {
                UpdateState::Probation { health_ack_deadline, probation_deadline, health_acknowledged, .. } => {
                    (!health_acknowledged && now() > health_ack_deadline)
                        || (now() >= probation_deadline && !current.first_operation_completed)
                }
                _ => true,
            };
        if must_stop {
            match dev.ensure_missions_quiescent() {
                Ok(()) => {}
                Err(UpdateError::ActiveOperations) => {
                    #[cfg(windows)]
                    drop(guard);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
                Err(error) => return Err(error),
            }
            child.stop()?;
            drop(old);
            let outcome = if matches!(current.state, UpdateState::Probation { .. }) {
                monitor(dev, false, now())?
            } else if let UpdateState::RolledBack { restored_slot, quarantined_slot, .. } = current.state {
                let _ = dev.remove_slot(quarantined_slot);
                Monitor::Rollback(restored_slot)
            } else {
                Monitor::Stopped
            };
            if let Monitor::Rollback(slot) = outcome {
                dev.slot_command(slot).spawn()?;
            }
            return Ok(());
        }
        let probation_due = matches!(current.state, UpdateState::Probation { probation_deadline, .. } if now() >= probation_deadline);
        if (!probation_due || watched(supervision) >= std::time::Duration::from_secs(300))
            && match monitor(dev, true, now()) {
                Ok(outcome) => outcome == Monitor::Healthy,
                Err(error) => {
                    if matches!(error, UpdateError::ActiveOperations) {
                        #[cfg(windows)]
                        drop(guard);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        continue;
                    }
                    // Promotion may have committed its safe checkpoint before the live manifest write failed.
                    recover(dev, now())?;
                    if manager.manifest()?.is_some_and(|manifest| manifest.state == UpdateState::Healthy) {
                        true
                    } else {
                        return Err(error);
                    }
                }
            }
        {
            child.detach()?;
            return Ok(());
        }
        #[cfg(windows)]
        drop(guard);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
