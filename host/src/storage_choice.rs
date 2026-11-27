use crate::{Host, Res, check_revoked, lock, request_flag};
use faber_engine::project::{copy, git, lease::real_path};
use faber_engine::storage::{CHOICE_FILE, external_root_name};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

const FALLBACK: &str = "reason.storage.default";
const UNREADABLE: &str = "reason.storage.unreadable";
const CHANGE_UNREADABLE: &str = "error.storage.unreadable";
const LIMIT: u64 = 64 * 1024;

pub(crate) mod storage_move;

#[derive(Default, Deserialize, Serialize)]
struct Saved {
    folder: Option<PathBuf>,
    roots: BTreeSet<PathBuf>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    folder: PathBuf,
    custom: bool,
    locked: bool,
    notice: Option<&'static str>,
    ask: bool,
}

pub struct Choice {
    file: Option<PathBuf>,
    saved: Saved,
    status: Status,
}

fn read(file: &Path) -> io::Result<Option<Saved>> {
    let meta = match fs::symlink_metadata(file) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        result => result?,
    };
    if !meta.is_file() || meta.len() > LIMIT {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = Vec::new();
    fs::File::open(file)?.take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(io::ErrorKind::InvalidData.into());
    }
    serde_json::from_slice(&bytes).map(Some).map_err(io::Error::other)
}

fn write(file: &Path, saved: &Saved) -> io::Result<()> {
    let bytes = serde_json::to_vec(saved).map_err(io::Error::other)?;
    if bytes.len() as u64 > LIMIT {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut temp = tempfile::NamedTempFile::new_in(file.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    temp.write_all(&bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(file).map_err(|failure| failure.error)?;
    Ok(())
}

fn allowed_folder(folder: &Path, data: &Path, home: Option<&Path>, unix: bool) -> bool {
    if git::path_within(folder, data) || home.is_some_and(|home| git::same_path(folder, home)) {
        return false;
    }
    if unix {
        let mut parts = folder.components();
        if parts.next() == Some(std::path::Component::RootDir) {
            let name = |part: Option<std::path::Component>| part.map(|part| part.as_os_str().to_string_lossy().to_ascii_lowercase());
            let Some(first) = name(parts.next()) else { return false };
            // `/run` is the system's own and empties at logout, except the drives mounted under `/run/media`.
            if first == "run" {
                return name(parts.next()).as_deref() == Some("media") && parts.next().is_some();
            }
            return ![
                "system", "library", "usr", "bin", "sbin", "etc", "var", "private", "opt", "dev", "proc", "sys", "tmp", "boot", "lib",
                "lib64",
            ]
            .contains(&first.as_str());
        }
    }
    true
}

/// A folder inside a git work tree or another storage folder is refused: `git add -A` or `git clean` there, or retiring the
/// copy that holds it, would take every copy and tool in it along.
fn inside_repository_or_storage(folder: &Path) -> bool {
    folder
        .ancestors()
        .any(|dir| fs::symlink_metadata(dir.join(".git")).is_ok() || dir.file_name().is_some_and(|name| name == "Faber-storage"))
}

/// Agents write in a project's folder, and a session already running there keeps its writable folders: a chosen folder
/// inside a project would put the owner mark and every copy within that session's reach.
fn inside_a_project(folder: &Path, projects: &[PathBuf]) -> bool {
    projects.iter().any(|project| git::path_within(folder, project))
}

fn validate(folder: &Path, data: &Path, roots: &BTreeSet<PathBuf>, initialize: bool) -> Option<PathBuf> {
    // A link anywhere on the way is refused before anything follows it.
    real_path(folder).ok()?;
    let chosen = crate::folder::local_folder(folder.to_str()?).ok()?;
    faber_engine::permissions::checked_folder(&chosen)?;
    let base = crate::folder::local_folder(data.to_str()?).unwrap_or_else(|_| data.to_path_buf());
    let home = dirs::home_dir();
    let real_home = home.as_deref().and_then(|path| fs::canonicalize(path).ok());
    let root = chosen.join("Faber-storage");
    if inside_repository_or_storage(&chosen)
        || !allowed_folder(folder, data, home.as_deref(), cfg!(unix))
        || !allowed_folder(&chosen, &base, real_home.as_deref(), cfg!(unix))
        || git::path_within(&base, &root)
    {
        return None;
    }
    real_path(&root).ok()?;
    if !roots.contains(&root) {
        if !initialize {
            return None;
        }
        match fs::create_dir(&root) {
            Ok(()) => {
                if fs::write(root.join(".faber-owner"), data.to_str()?).is_err() {
                    let _ = fs::remove_dir(&root);
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    if !faber_engine::storage::owns_external_root(data, &root) {
        return None;
    }
    for child in ["copies", "tools"] {
        let path = root.join(child);
        real_path(&path).ok()?;
        if path.exists() {
            tempfile::NamedTempFile::new_in(path).ok()?;
        }
    }
    for child in ["plugin", "plugin/skills", "servers", "runtimes", "plugins", "disabled", "removed", "replacements"] {
        let path = root.join("tools").join(child);
        real_path(&path).ok()?;
        if path.exists() && !path.is_dir() {
            return None;
        }
    }
    tempfile::NamedTempFile::new_in(&root).ok()?;
    Some(root)
}

fn low_space(free: Option<u64>) -> bool {
    free.is_some_and(|bytes| bytes < 10_000_000_000)
}

fn new_profile(data: &Path) -> bool {
    match fs::read_dir(data) {
        Ok(mut entries) => entries.all(|entry| {
            entry.is_ok_and(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let Ok(kind) = entry.file_type() else { return false };
                // Electron can prepare its browser and an empty migration backup before the host opens.
                if kind.is_file() {
                    return name == "backup-before-electron.done";
                }
                kind.is_dir()
                    && (["webview", "pane-extension-profiles", "draft-profile-import"].contains(&name.as_ref())
                        || (name.strip_prefix("backup-before-electron-").is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
                            && fs::read_dir(entry.path()).is_ok_and(|mut entries| entries.next().is_none())))
            })
        }),
        Err(error) => error.kind() == io::ErrorKind::NotFound,
    }
}

/// When the choice is unreadable, also recover the roots of tools installed elsewhere from their location records
/// (`tools/locations/<id>.json`, each naming a `Faber-storage/tools` folder), so they stay protected.
fn tool_roots(data: &Path) -> BTreeSet<PathBuf> {
    let folder = data.join("tools").join("locations");
    if real_path(&folder).is_err() {
        return BTreeSet::new();
    }
    fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != "json" || !entry.file_type().ok()?.is_file() || real_path(&path).is_err() {
                return None;
            }
            let mut bytes = Vec::new();
            fs::File::open(&path).ok()?.take(LIMIT).read_to_end(&mut bytes).ok()?;
            let tools: PathBuf = serde_json::from_slice(&bytes).ok()?;
            let root = tools.parent()?;
            (tools.file_name()? == "tools" && external_root_name(root) && !git::path_within(root, data)).then(|| root.to_path_buf())
        })
        .collect()
}

/// When the choice is unreadable, recover the roots of existing copies from their own saved paths.
fn copy_roots(data: &Path) -> BTreeSet<PathBuf> {
    #[derive(Deserialize)]
    struct Record {
        id: String,
        copy: Option<copy::WorkCopy>,
    }
    let folder = data.join("conversations");
    if real_path(&folder).is_err() {
        return BTreeSet::new();
    }
    fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != "json" || !entry.file_type().ok()?.is_file() || real_path(&path).is_err() {
                return None;
            }
            let Record { id, copy } = serde_json::from_reader(BufReader::new(fs::File::open(&path).ok()?)).ok()?;
            if path.file_stem()?.to_str()? != id || !faber_engine::model::valid_id(&id) {
                return None;
            }
            let copy = copy?;
            let path = Path::new(&copy.path);
            let root = path.parent()?.parent()?;
            (external_root_name(root) && !git::path_within(root, data) && git::same_path(&copy::home(root).join(id), path))
                .then(|| root.to_path_buf())
        })
        .collect()
}

impl Choice {
    pub(crate) fn locked(data: &Path) -> Self {
        Self {
            file: None,
            saved: Saved::default(),
            status: Status { folder: data.into(), custom: false, locked: true, notice: None, ask: false },
        }
    }

    pub fn open(data: &Path, isolated: bool) -> Self {
        Self::resolve(data, data.with_file_name(CHOICE_FILE), isolated || std::env::var_os("FABER_DATA_DIR").is_some())
    }

    fn resolve(data: &Path, file: PathBuf, overridden: bool) -> Self {
        let mut choice = Self::locked(data);
        if overridden {
            return choice;
        }
        choice.file = Some(file.clone());
        choice.status.locked = false;
        let saved = match read(&file) {
            Ok(Some(saved)) => saved,
            Ok(None) => {
                if new_profile(data) {
                    let existing = data.ancestors().find(|path| path.is_dir());
                    choice.status.ask =
                        low_space(existing.and_then(|path| faber_winproc::rewind::Root::open(path).ok()?.available_space().ok()));
                }
                return choice;
            }
            Err(_) => {
                choice.status.notice = Some(UNREADABLE);
                choice.saved.roots = copy_roots(data);
                choice.saved.roots.extend(tool_roots(data));
                return choice;
            }
        };
        choice.status.custom = saved.folder.is_some();
        if let Some(folder) = &saved.folder {
            if let Some(root) = validate(folder, data, &saved.roots, false) {
                choice.status.folder = root;
            } else {
                choice.status.notice = Some(FALLBACK);
            }
        }
        choice.saved = saved;
        choice.saved.roots.retain(|root| external_root_name(root));
        choice
    }

    pub(crate) fn configuration(&self, data: &Path) -> (Option<PathBuf>, BTreeSet<PathBuf>) {
        ((self.status.folder != data).then(|| self.status.folder.clone()), self.saved.roots.clone())
    }

    pub(crate) fn status(&self) -> Status {
        self.status.clone()
    }

    pub(crate) fn publish_roots(&self, shell: &dyn crate::Shell) {
        shell.storage_roots(self.saved.roots.iter().cloned().collect());
    }

    fn writable_choice(&self) -> Result<Saved, &'static str> {
        let file = self.file.as_ref().ok_or("error.storage.choice")?;
        match read(file) {
            Ok(Some(mut saved)) => {
                saved.roots.retain(|root| external_root_name(root));
                saved.roots.extend(self.saved.roots.iter().cloned());
                Ok(saved)
            }
            Ok(None) if self.status.notice != Some(UNREADABLE) => Ok(Saved { folder: None, roots: self.saved.roots.clone() }),
            _ => Err(CHANGE_UNREADABLE),
        }
    }

    fn save_folder(&mut self, data: &Path, folder: Option<PathBuf>) -> Result<Option<PathBuf>, &'static str> {
        let mut saved = self.writable_choice()?;
        let file = self.file.as_ref().ok_or("error.storage.choice")?;
        let root = folder.as_ref().map(|folder| validate(folder, data, &saved.roots, true).ok_or("error.storage.choice")).transpose()?;
        let created = root.as_ref().filter(|root| !saved.roots.contains(*root));
        saved.folder = folder;
        saved.roots.extend(root.iter().cloned());
        if write(file, &saved).is_err() {
            if let Some(root) = created
                && faber_engine::storage::owns_external_root(data, root)
            {
                let _ = fs::remove_file(root.join(".faber-owner"));
                let _ = fs::remove_dir(root);
            }
            return Err("error.store.write");
        }
        self.status = Status {
            folder: root.clone().unwrap_or_else(|| data.to_path_buf()),
            custom: saved.folder.is_some(),
            locked: false,
            notice: None,
            ask: false,
        };
        self.saved = saved;
        Ok(root)
    }

    fn fall_back_on_read_error(&mut self, h: &Host, error: &str) {
        if error == CHANGE_UNREADABLE && self.status.notice != Some(UNREADABLE) {
            self.status = Status { folder: h.data_dir.clone(), custom: false, locked: false, notice: Some(UNREADABLE), ask: false };
            h.engine.set_new_storage(None, self.saved.roots.clone());
        }
    }
}

pub(crate) async fn change(h: &Host, reset: bool) -> Res<Option<Status>> {
    {
        let mut choice = lock(&h.storage_choice);
        choice.writable_choice().inspect_err(|error| choice.fall_back_on_read_error(h, error))?;
    }
    let folder = if reset {
        None
    } else {
        let picked = h
            .shell
            .pick_folder()
            .await
            .map_err(|why| if why == crate::PickError::NoWindow { crate::NO_WINDOW } else { "error.storage.choice" })?;
        let Some(path) = picked else { return Ok(None) };
        let projects: Vec<PathBuf> = h.engine.projects().into_iter().map(|project| PathBuf::from(project.path)).collect();
        if inside_a_project(&path, &projects) {
            return Err("error.storage.choice".into());
        }
        Some(path)
    };
    check_revoked(request_flag().as_deref())?;
    let mut choice = lock(&h.storage_choice);
    check_revoked(request_flag().as_deref())?;
    let root = choice.save_folder(&h.data_dir, folder).inspect_err(|error| choice.fall_back_on_read_error(h, error))?;
    choice.publish_roots(h.shell.as_ref());
    h.engine.set_new_storage(root, choice.saved.roots.clone());
    Ok(Some(choice.status()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_choice_round_trips_and_the_override_wins() {
        let tmp = if cfg!(unix) { tempfile::tempdir_in(dirs::home_dir().unwrap()) } else { tempfile::tempdir() }.unwrap();
        let profile = fs::canonicalize(tmp.path()).unwrap();
        let profile = PathBuf::from(faber_engine::project::git::native_path(&profile));
        let data = profile.join("Faber");
        let folder = profile.join("Cópias e ferramentas");
        fs::create_dir(&folder).unwrap();
        let root = validate(&folder, &data, &BTreeSet::new(), true).unwrap();
        let file = data.with_file_name(CHOICE_FILE);
        write(&file, &Saved { folder: Some(folder.clone()), roots: BTreeSet::from([root.clone()]) }).unwrap();
        let saved = read(&file).unwrap().unwrap();
        assert_eq!(saved.folder, Some(folder));
        assert_eq!(saved.roots, BTreeSet::from([root.clone()]));
        assert_eq!(Choice::resolve(&data, file.clone(), false).status().folder, root);
        let overridden = Choice::resolve(&data, file, true).status();
        assert_eq!(overridden.folder, data);
        assert!(overridden.locked);
        assert_eq!(overridden.notice, None);
    }

    #[test]
    fn storage_choice_publishes_all_roots_at_start_and_after_a_change() {
        #[derive(Default)]
        struct Browser(std::sync::Mutex<Vec<Vec<PathBuf>>>);
        impl crate::Shell for Browser {
            fn storage_roots(&self, roots: Vec<PathBuf>) {
                lock(&self.0).push(roots);
            }
        }

        let tmp = if cfg!(unix) { tempfile::tempdir_in(dirs::home_dir().unwrap()) } else { tempfile::tempdir() }.unwrap();
        let profile = PathBuf::from(git::native_path(&tmp.path().canonicalize().unwrap()));
        let data = profile.join("Faber");
        let file = data.with_file_name(CHOICE_FILE);
        let earlier = profile.join("earlier/Faber-storage");
        let later = profile.join("later");
        fs::create_dir(&later).unwrap();
        write(&file, &Saved { folder: None, roots: BTreeSet::from([earlier.clone()]) }).unwrap();

        let browser = Browser::default();
        let mut choice = Choice::resolve(&data, file.clone(), false);
        choice.publish_roots(&browser);
        let current = choice.save_folder(&data, Some(later)).unwrap().unwrap();
        choice.publish_roots(&browser);
        Choice::resolve(&data, file, true).publish_roots(&browser);
        assert_eq!(*lock(&browser.0), [vec![earlier.clone()], vec![earlier, current], vec![]]);
    }

    #[test]
    fn storage_choice_falls_back_when_the_folder_is_unreachable_or_not_a_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("Faber");
        let file = data.with_file_name(CHOICE_FILE);
        let folder = tmp.path().join("gone");
        write(&file, &Saved { folder: Some(folder.clone()), roots: BTreeSet::from([folder.join("Faber-storage")]) }).unwrap();
        let check = || {
            let choice = Choice::resolve(&data, file.clone(), false).status();
            assert_eq!(choice.folder, data);
            assert!(choice.custom);
            assert_eq!(choice.notice, Some("reason.storage.default"));
        };
        check();
        fs::write(folder, b"not a directory").unwrap();
        check();
    }

    #[test]
    fn storage_choice_asks_below_ten_gigabytes_only_for_known_space() {
        assert!(low_space(Some(9_999_999_999)));
        assert!(!low_space(Some(10_000_000_000)));
        assert!(!low_space(None));
    }

    #[test]
    fn storage_choice_preserves_unreadable_records_and_recovers_when_they_are_readable() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap().join("Faber");
        let file = data.with_file_name(CHOICE_FILE);
        fs::create_dir(&file).unwrap();
        let mut locked = Choice::resolve(&data, file.clone(), false);
        assert_eq!(locked.status().notice, Some(UNREADABLE));
        assert_eq!(locked.save_folder(&data, None), Err(CHANGE_UNREADABLE));
        assert!(file.is_dir());

        fs::remove_dir(&file).unwrap();
        fs::write(&file, b"{ malformed").unwrap();
        let mut corrupted = Choice::resolve(&data, file.clone(), false);
        assert_eq!(corrupted.status().folder, data);
        assert_eq!(corrupted.status().notice, Some(UNREADABLE));
        assert_eq!(corrupted.save_folder(&data, None), Err(CHANGE_UNREADABLE));
        assert_eq!(fs::read(&file).unwrap(), b"{ malformed");
        let root = data.with_file_name("Faber-storage");
        write(&file, &Saved { folder: None, roots: BTreeSet::from([root.clone()]) }).unwrap();
        assert_eq!(corrupted.save_folder(&data, None), Ok(None));
        assert_eq!(read(&file).unwrap().unwrap().roots, BTreeSet::from([root]));
        fs::write(&file, b"corrupted since startup").unwrap();
        assert_eq!(corrupted.save_folder(&data, None), Err(CHANGE_UNREADABLE));
        assert_eq!(fs::read(&file).unwrap(), b"corrupted since startup");
    }

    #[test]
    fn storage_choice_recovers_saved_copy_roots_without_using_them_for_new_items() {
        let tmp = tempfile::tempdir().unwrap();
        let temp = tmp.path().canonicalize().unwrap();
        let data = temp.join("Faber");
        let root = PathBuf::from(git::native_path(&temp)).join("old/Faber-storage");
        let paths = data.join("conversations");
        fs::create_dir_all(&paths).unwrap();
        let record = |id, path: PathBuf| serde_json::json!({"id": id, "copy": {"path": path, "branch": "work", "root": "project"}});
        fs::write(paths.join("kept.json"), record("kept", copy::home(&root).join("kept")).to_string()).unwrap();
        fs::write(paths.join("wrong.json"), record("wrong", copy::home(&root.join("other")).join("wrong")).to_string()).unwrap();
        let file = data.with_file_name(CHOICE_FILE);
        fs::write(&file, b"malformed").unwrap();
        let choice = Choice::resolve(&data, file.clone(), false);
        assert_eq!(choice.configuration(&data), (None, BTreeSet::from([root.clone()])));

        // A root holding only tools is recovered from its location record.
        let tools_only = PathBuf::from(git::native_path(&temp)).join("tools-only/Faber-storage");
        let locations = data.join("tools/locations");
        fs::create_dir_all(&locations).unwrap();
        fs::write(locations.join("runtime.json"), serde_json::to_vec(&tools_only.join("tools")).unwrap()).unwrap();
        fs::write(locations.join("odd.json"), serde_json::to_vec(&tools_only.join("other")).unwrap()).unwrap();
        let choice = Choice::resolve(&data, file, false);
        assert_eq!(choice.configuration(&data), (None, BTreeSet::from([root, tools_only])));
    }

    #[test]
    fn storage_choice_refuses_a_folder_inside_a_repository_or_another_storage_folder() {
        // macOS keeps temporary folders under /private/var, a system folder the rule refuses.
        let tmp = if cfg!(unix) { tempfile::tempdir_in(dirs::home_dir().unwrap()) } else { tempfile::tempdir() }.unwrap();
        let profile = PathBuf::from(faber_engine::project::git::native_path(&fs::canonicalize(tmp.path()).unwrap()));
        let data = profile.join("Faber");
        let project = profile.join("project");
        fs::create_dir_all(project.join(".git")).unwrap();
        fs::create_dir_all(project.join("inner")).unwrap();
        let worktree = profile.join("worktree");
        fs::create_dir_all(&worktree).unwrap();
        fs::write(worktree.join(".git"), "gitdir: elsewhere").unwrap();
        let nested = profile.join("other/Faber-storage/copies/x");
        fs::create_dir_all(&nested).unwrap();
        let plain = profile.join("plain");
        fs::create_dir(&plain).unwrap();
        for refused in [&project, &project.join("inner"), &worktree, &nested] {
            assert_eq!(validate(refused, &data, &BTreeSet::new(), true), None, "{}", refused.display());
            assert!(!refused.join("Faber-storage").exists());
        }
        assert!(validate(&plain, &data, &BTreeSet::new(), true).is_some());
    }

    #[test]
    fn storage_choice_only_asks_for_a_new_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("Faber");
        assert!(new_profile(&data));
        for name in ["webview", "pane-extension-profiles", "draft-profile-import"] {
            fs::create_dir_all(data.join(name)).unwrap();
        }
        let backup = data.join("backup-before-electron-4a01b97c-8d7a-4a98-94d8-97eb19588604");
        fs::create_dir(&backup).unwrap();
        fs::write(data.join("backup-before-electron.done"), "").unwrap();
        assert!(new_profile(&data));
        for name in ["settings.json", "projects.json", "sharing.json", "startup.log"] {
            fs::write(data.join(name), "").unwrap();
            assert!(!new_profile(&data), "{name}");
            fs::remove_file(data.join(name)).unwrap();
        }
        for name in ["conversations", "tools", "copies", "attachments"] {
            fs::create_dir(data.join(name)).unwrap();
            assert!(!new_profile(&data), "{name}");
            fs::remove_dir(data.join(name)).unwrap();
        }
        fs::write(backup.join("projects.json"), "").unwrap();
        assert!(!new_profile(&data));
    }

    #[test]
    fn storage_choice_rejects_system_roots_home_and_the_profile() {
        // macOS keeps temporary folders under /private/var, a system folder the rule refuses.
        let tmp = if cfg!(unix) { tempfile::tempdir_in(dirs::home_dir().unwrap()) } else { tempfile::tempdir() }.unwrap();
        let home = tmp.path();
        let data = home.join("Faber");
        let allowed = |path: &Path| allowed_folder(path, &data, Some(home), cfg!(unix));
        assert!(!allowed(home));
        assert!(!allowed(&data));
        assert!(!allowed(&data.join("nested")));
        assert!(allowed(&home.join("backup")));
        for path in ["/", "/System", "/Library", "/usr", "/bin", "/etc", "/var", "/private", "/opt", "/usr/local"] {
            assert!(!allowed_folder(Path::new(path), &data, Some(home), true), "{path}");
        }
        assert!(allowed_folder(Path::new("/mount/var"), &data, Some(home), true));
        for path in ["/run/user/1000", "/run", "/run/media", "/tmp/x", "/boot", "/lib64"] {
            assert!(!allowed_folder(Path::new(path), &data, Some(home), true), "{path}: the system's own, or gone at logout");
        }
        assert!(allowed_folder(Path::new("/run/media/me/Drive"), &data, Some(home), true), "a drive mounted under /run/media");
    }

    #[test]
    fn storage_choice_refuses_a_folder_inside_a_project_an_agent_may_be_writing() {
        let projects = [PathBuf::from("/work/site"), PathBuf::from("/work/game")];
        assert!(inside_a_project(Path::new("/work/site"), &projects));
        assert!(inside_a_project(Path::new("/work/site/storage"), &projects));
        assert!(!inside_a_project(Path::new("/work"), &projects), "a folder that holds projects keeps its storage beside them");
        assert!(!inside_a_project(Path::new("/work/sitemap"), &projects));
    }
}
