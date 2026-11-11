//! Project-local instruction entries. No CLI profile, import, or link inside a project is followed.
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

pub const MAX_BYTES: usize = 256 * 1024;
// Adopted logs include the reference decision table, which exceeds Memory's ordinary file cap.
const ADOPTED_MAX_BYTES: usize = 512 * 1024;
const MAX_ENTRIES: usize = 4096;
const READ: &str = "error.memory.read";
const CHANGED: &str = "error.memory.changed";
const REFUSED: &str = "error.memory.refused";

/// Read-only sources approved by location; task folders use their bounded directory reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdoptedKind {
    Decisions,
    Taste,
    Tasks,
}

/// Only these two engine-owned notes accept an approved line, never an arbitrary path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LearningKind {
    Taste,
    Lessons,
}

pub type LearningRecords = std::collections::BTreeMap<LearningKind, Vec<String>>;

impl LearningKind {
    fn name(self) -> &'static str {
        match self {
            Self::Taste => "taste.md",
            Self::Lessons => "lessons.md",
        }
    }

    fn target(self) -> usize {
        match self {
            Self::Taste => 1024,
            Self::Lessons => 2048,
        }
    }

    pub fn for_path(path: &str) -> Option<Self> {
        match path {
            ".faber/taste.md" => Some(Self::Taste),
            ".faber/lessons.md" => Some(Self::Lessons),
            _ => None,
        }
    }
}

/// Authority lives outside the repository; body-only records from older versions remain readable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum KeptDecision {
    Item {
        id: String,
        hash: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        stale: bool,
    },
    Legacy(String),
}

impl KeptDecision {
    pub fn of(body: &str) -> Self {
        let line = body.lines().next().unwrap_or_default().trim();
        let line = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).or_else(|| line.strip_prefix("+ ")).unwrap_or(line);
        let normal = line.split_whitespace().collect::<Vec<_>>().join(" ");
        Self::Item { id: revision(normal.as_bytes()), hash: revision(body.trim().as_bytes()), files: vec![], stale: false }
    }

    pub fn matches(&self, record: &Self) -> bool {
        match (self, record) {
            (Self::Legacy(old), Self::Item { hash, .. }) => hash.get(..32) == Some(old.as_str()),
            (Self::Item { id, hash, .. }, Self::Item { id: other_id, hash: other_hash, .. }) => id == other_id && hash == other_hash,
            _ => self == record,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFile {
    pub path: String,
    pub source: &'static str,
    pub revision: String,
    pub too_large: bool,
    pub over_limit: Option<(u64, u64)>,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MemoryList {
    pub files: Vec<MemoryFile>,
    pub incomplete: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub text: String,
    /// Decision lists only (R16), from the trailing mark of `mark`: the text without it and its author ("ai" or "person").
    /// An entry with no mark has neither: its author is unknown, never the AI's. `reviewed` is never read from the file
    /// (an agent could write it): `mark_kept` and `mark_learning` use Faber's records outside the project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<&'static str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reviewed: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Edit {
    pub path: String,
    pub revision: String,
    pub start: usize,
    pub end: usize,
    /// None deletes the entry; Some replaces its text.
    pub text: Option<String>,
    /// Keeps an AI decision: nothing is written to the project, `apply` returns its identity and content hash to store.
    #[serde(default)]
    pub keep: bool,
    /// Separate person consent to exceed a taste or lesson file's target, as for appending.
    #[serde(default, rename = "overTarget")]
    pub over_target: bool,
}

fn source(path: &str) -> Option<&'static str> {
    let parts: Vec<_> = path.split('/').collect();
    if parts.iter().any(|p| p.is_empty() || matches!(*p, "." | ".." | ".git" | ".codex") || p.contains(['\\', ':', '\0'])) {
        return None;
    }
    let name = *parts.last()?;
    if let Some(i) = parts.iter().position(|p| *p == ".faber") {
        let kind = parts.get(i + 1)?;
        return (name.ends_with(".md")
            && ["decisions", "decisions-old", "taste", "plans", "recipes", "learning", "memory", "rules", "lessons", "tasks"]
                .contains(&kind.trim_end_matches(".md")))
        .then_some("faber");
    }
    if let Some(i) = parts.iter().position(|p| *p == ".claude") {
        return (i + 2 == parts.len() && name == "CLAUDE.md").then_some("claude");
    }
    match name {
        "CLAUDE.md" | "CLAUDE.local.md" => Some("claude"),
        "AGENTS.md" | "AGENTS.override.md" => Some("codex"),
        _ => None,
    }
}

/// `.faber/decisions.md` (or a file under `.faber/decisions/`): the one list whose entries carry an author.
fn decisions(path: &str) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    parts
        .iter()
        .position(|p| *p == ".faber")
        .and_then(|i| parts.get(i + 1))
        .is_some_and(|kind| matches!(kind.trim_end_matches(".md"), "decisions" | "decisions-old"))
}

struct Mark {
    open: usize,
    author: &'static str,
}

/// The mark closing a decision: `[by AI]` or `[by person]` (any case, any spacing). `[by AI, reviewed]` from an older
/// version is still a mark, but only the author in it counts: a review in the file would be one anyone could forge.
/// Anything else at the end is the person's own words and no mark.
fn mark(text: &str) -> Option<Mark> {
    let end = text.trim_end().len();
    let open = text[..end].rfind('[')?;
    let inner = text[..end].strip_suffix(']')?[open + 1..].to_ascii_lowercase();
    let mut parts = inner.split(',').map(str::trim);
    let author = match parts.next()?.strip_prefix("by ")?.trim() {
        "ai" => "ai",
        "person" => "person",
        _ => return None,
    };
    // "reviewed" is read only after "ai", for files an older version wrote.
    if !matches!((author, parts.next()), (_, None) | ("ai", Some("reviewed"))) {
        return None;
    }
    parts.next().is_none().then_some(Mark { open, author })
}

fn normal(path: &Path) -> String {
    let path: PathBuf = path.components().collect();
    let path = path.to_string_lossy().replace('\\', "/");
    let path = path.trim_start_matches("//?/").trim_end_matches('/');
    if cfg!(windows) { path.to_lowercase() } else { path.into() }
}

fn private_roots() -> Vec<String> {
    // Compare names only: never open a sign-in folder to discover its contents.
    let homes = [std::env::var_os("USERPROFILE"), std::env::var_os("HOME")];
    let protected = homes
        .into_iter()
        .flatten()
        .flat_map(|home| [".claude", ".claude.json", ".codex", ".gemini"].map(|name| normal(&PathBuf::from(&home).join(name))));
    let relocated =
        crate::permissions::relocated_credentials(&|key| std::env::var(key).ok()).into_iter().map(|(p, _)| normal(Path::new(&p)));
    protected.chain(relocated).collect()
}

fn private(path: &Path, roots: &[String]) -> bool {
    let target = normal(path);
    roots.iter().any(|p| target == *p || target.strip_prefix(p).is_some_and(|suffix| suffix.starts_with('/')))
}

fn root(path: &Path, protected: &[String]) -> Result<PathBuf, &'static str> {
    if !path.is_absolute() || private(path, protected) {
        return Err(REFUSED);
    }
    // Resolve only parents: the root itself and its children still pass the no-follow handle walk.
    let root = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent.canonicalize().map_err(|_| REFUSED)?.join(name),
        // A filesystem root or terminal `..` selects the directory resolved by the filesystem.
        _ => path.canonicalize().map_err(|_| REFUSED)?,
    };
    if private(&root, protected) {
        return Err(REFUSED);
    }
    Ok(root)
}

fn resolved_private(protected: &[String]) -> Vec<String> {
    let mut paths = protected.to_vec();
    for path in protected {
        // Resolve locations using metadata only; never enumerate or read private CLI contents.
        if let Ok(path) = root(Path::new(path), &[]) {
            paths.push(normal(&path));
        }
        if let Ok(target) = Path::new(path).canonicalize() {
            paths.push(normal(&target));
        }
    }
    paths
}

/// Holds a directory's identity through enumeration and atomic replacement.
struct Directory {
    #[cfg(windows)]
    dir: faber_winproc::MemoryDirectory,
    #[cfg(unix)]
    fd: rustix::fd::OwnedFd,
}

fn project_directory(project: &Path, protected: &[String]) -> Result<(PathBuf, Directory), &'static str> {
    let root = root(project, protected)?;
    let directory = Directory::open(&root, Path::new("")).map_err(|_| REFUSED)?;
    // Expand Windows short names only after opening the root without following links. All content I/O
    // stays relative to that held directory; its long name also protects private folders and files.
    #[cfg(windows)]
    let root = directory.dir.resolved_path().map_err(|_| REFUSED)?;
    if private(&root, protected) {
        return Err(REFUSED);
    }
    Ok((root, directory))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveStage {
    Created,
    Prepared,
    Checked,
    Installed,
}

impl Directory {
    fn open(root: &Path, relative: &Path) -> std::io::Result<Self> {
        if relative.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        #[cfg(windows)]
        return Ok(Self { dir: faber_winproc::MemoryDirectory::open(root, relative)? });
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, open, openat};
            let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
            let mut fd = open("/", flags, Mode::empty())?;
            for part in root.components().chain(relative.components()) {
                match part {
                    Component::Normal(name) => fd = openat(&fd, name, flags, Mode::empty())?,
                    Component::RootDir => {}
                    _ => return Err(std::io::ErrorKind::PermissionDenied.into()),
                }
            }
            Ok(Self { fd })
        }
    }

    fn under(&self, relative: &Path) -> std::io::Result<Self> {
        #[cfg(windows)]
        return Ok(Self { dir: self.dir.under(relative)? });
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, openat};
            let mut fd = rustix::io::dup(&self.fd)?;
            for part in relative.components() {
                let Component::Normal(name) = part else {
                    return Err(std::io::ErrorKind::PermissionDenied.into());
                };
                fd = openat(&fd, name, OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC, Mode::empty())?;
            }
            Ok(Self { fd })
        }
    }

    /// A new child folder, created through this handle and opened without following a link put in its place.
    fn create_dir(&self, name: &str) -> std::io::Result<Self> {
        #[cfg(windows)]
        return Ok(Self { dir: self.dir.create_under(name)? });
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, mkdirat};
            mkdirat(&self.fd, name, Mode::RWXU | Mode::RGRP | Mode::XGRP | Mode::ROTH | Mode::XOTH)?;
            self.under(Path::new(name))
        }
    }

    fn identity(&self) -> std::io::Result<String> {
        #[cfg(windows)]
        return self.dir.identity();
        #[cfg(unix)]
        {
            let stat = rustix::fs::fstat(&self.fd)?;
            Ok(format!("{:x}-{:x}", stat.st_dev, stat.st_ino))
        }
    }

    fn replacement_file(&self, name: &str) -> std::io::Result<File> {
        #[cfg(windows)]
        return self.dir.replacement_file(name);
        #[cfg(unix)]
        self.file(name)
    }

    fn names(&self) -> std::io::Result<Vec<String>> {
        #[cfg(windows)]
        return self.dir.names(20_001);
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            rustix::fs::Dir::read_from(&self.fd)?
                .take(20_001)
                .filter_map(|e| match e {
                    Ok(e) => std::ffi::OsStr::from_bytes(e.file_name().to_bytes())
                        .to_str()
                        .filter(|s| !matches!(*s, "." | ".."))
                        .map(|s| Ok(s.to_string())),
                    Err(e) => Some(Err(e.into())),
                })
                .collect()
        }
    }

    fn names_bounded(&self, limit: usize) -> std::io::Result<(Vec<String>, bool)> {
        #[cfg(windows)]
        return self.dir.names_bounded(limit);
        #[cfg(unix)]
        {
            let mut names = Vec::new();
            let mut incomplete = false;
            let mut examined = 0;
            if limit == 0 {
                return Ok((names, true));
            }
            for entry in rustix::fs::Dir::read_from(&self.fd)? {
                let entry = entry?;
                let bytes = entry.file_name().to_bytes();
                if bytes == b"." || bytes == b".." {
                    continue;
                }
                examined += 1;
                match std::str::from_utf8(bytes) {
                    Ok(name) => names.push(name.to_string()),
                    Err(_) => incomplete = true,
                }
                if examined >= limit {
                    return Ok((names, true));
                }
            }
            Ok((names, incomplete))
        }
    }

    fn file(&self, name: &str) -> std::io::Result<File> {
        #[cfg(windows)]
        return self.dir.file(name, false);
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, openat};
            use std::os::unix::fs::MetadataExt;
            let file: File =
                openat(&self.fd, name, OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC, Mode::empty())?.into();
            if !file.metadata()?.is_file() || file.metadata()?.nlink() != 1 {
                return Err(std::io::ErrorKind::PermissionDenied.into());
            }
            Ok(file)
        }
    }

    /// A new, empty file; an existing name (or a link in its place) is an error.
    fn create(&self, name: &str) -> std::io::Result<File> {
        #[cfg(windows)]
        return self.dir.file(name, true);
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, openat};
            let flags = OFlags::RDONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
            Ok(openat(&self.fd, name, flags, Mode::RUSR | Mode::WUSR | Mode::RGRP | Mode::ROTH)?.into())
        }
    }

    fn replace(&self, name: &str, bytes: &[u8], permissions: std::fs::Permissions, expected: &str) -> std::io::Result<()> {
        self.replace_staged(name, bytes, permissions, expected, |_| {})
    }

    #[cfg(all(test, windows))]
    fn replace_checked(
        &self,
        name: &str,
        bytes: &[u8],
        permissions: std::fs::Permissions,
        expected: &str,
        race: impl FnOnce(),
    ) -> std::io::Result<()> {
        let mut race = Some(race);
        self.replace_staged(name, bytes, permissions, expected, |stage| {
            if stage == SaveStage::Checked {
                race.take().unwrap()();
            }
        })
    }

    #[cfg(windows)]
    fn replace_file(&self, file: &File, _from: &str, to: &str) -> std::io::Result<()> {
        self.dir.replace(file, to)
    }

    #[cfg(unix)]
    fn replace_file(&self, _file: &File, from: &str, to: &str) -> std::io::Result<()> {
        rustix::fs::renameat(&self.fd, from, &self.fd, to)?;
        Ok(())
    }

    #[cfg(windows)]
    fn discard(&self, file: &File, _name: &str) -> std::io::Result<()> {
        self.dir.discard(file)
    }

    #[cfg(unix)]
    fn discard(&self, _file: &File, name: &str) -> std::io::Result<()> {
        rustix::fs::unlinkat(&self.fd, name, rustix::fs::AtFlags::empty())?;
        Ok(())
    }

    fn replace_staged(
        &self,
        name: &str,
        bytes: &[u8],
        permissions: std::fs::Permissions,
        expected: &str,
        mut race: impl FnMut(SaveStage),
    ) -> std::io::Result<()> {
        let temp = format!(".faber-memory-{}.tmp", uuid::Uuid::new_v4());
        #[cfg(windows)]
        let mut file = self.dir.file(&temp, true)?;
        #[cfg(unix)]
        let mut file: File = {
            use rustix::fs::{Mode, OFlags, openat};
            openat(
                &self.fd,
                temp.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )?
            .into()
        };
        race(SaveStage::Created);
        let result = (|| {
            file.write_all(bytes)?;
            file.set_permissions(permissions)?;
            file.sync_all()?;
            race(SaveStage::Prepared);
            // Windows excludes in-place writes until replacement. Atomic rename-over after this check, and
            // already-open writers on Unix, remain the accepted normal editor window (project-memory R333).
            let mut guard = self.replacement_file(name).map_err(|_| std::io::ErrorKind::AlreadyExists)?;
            let final_check = (|| {
                if guard.metadata()?.permissions().readonly() {
                    return Err(std::io::ErrorKind::AlreadyExists.into());
                }
                let mut original = Vec::new();
                (&mut guard).take(MAX_BYTES as u64 + 1).read_to_end(&mut original)?;
                if original.len() > MAX_BYTES || revision(&original) != expected {
                    return Err(std::io::ErrorKind::AlreadyExists.into());
                }
                Ok(())
            })();
            final_check.map_err(|_: std::io::Error| std::io::ErrorKind::AlreadyExists)?;
            race(SaveStage::Checked);
            self.replace_file(&file, &temp, name)?;
            race(SaveStage::Installed);
            drop(guard);
            Ok(())
        })();
        if result.is_err() {
            drop(self.discard(&file, &temp));
        }
        drop(file);
        result
    }
}

fn adopted_private_name(part: &str) -> bool {
    part.eq_ignore_ascii_case(".git") || crate::permissions::credential_piece(&part.to_ascii_lowercase())
}

fn adopted_relative(relative: &str) -> Result<&Path, &'static str> {
    if relative.contains(['\\', ':', '\0'])
        || relative.split('/').any(|part| part.is_empty() || matches!(part, "." | "..") || adopted_private_name(part))
    {
        return Err(REFUSED);
    }
    let path = Path::new(relative);
    if path.components().any(|part| !matches!(part, Component::Normal(_))) || crate::permissions::secret(path, true) {
        return Err(REFUSED);
    }
    Ok(path)
}

fn adopted_opened_path(root: &Path, opened: &Path, protected: &[String]) -> Result<(), &'static str> {
    opened.strip_prefix(root).map_err(|_| REFUSED)?;
    // Resolve aliases through the already-open file, before reading any bytes. Case folding is conservative on macOS.
    let key = |path: &Path| {
        let path = normal(path);
        if cfg!(target_os = "macos") { path.to_lowercase() } else { path }
    };
    let opened_key = key(opened);
    if crate::permissions::secret(opened, true)
        || opened.components().any(|part| part.as_os_str().to_str().is_none_or(adopted_private_name))
        || protected.iter().any(|path| {
            let path = key(Path::new(path));
            opened_key == path || opened_key.strip_prefix(&path).is_some_and(|tail| tail.starts_with('/'))
        })
    {
        return Err(REFUSED);
    }
    Ok(())
}

/// A bounded read-only snapshot of direct Markdown children; this contains no approval authority.
#[derive(Debug)]
pub struct AdoptedTasks {
    pub files: Vec<(String, String)>,
    pub incomplete: bool,
}

/// Read only through the approved folder's held handles; source text is never trusted instructions.
pub fn read_adopted_tasks(project: &Path, relative: &str) -> Result<AdoptedTasks, &'static str> {
    let path = adopted_relative(relative)?;
    let protected = resolved_private(&private_roots());
    // Unix reopens the root through the held handle below.
    #[cfg_attr(unix, allow(unused_variables))]
    let (root, project_dir) = project_directory(project, &protected)?;
    let dir = project_dir.under(path).map_err(|_| REFUSED)?;
    #[cfg(windows)]
    let opened = dir.dir.resolved_path().map_err(|_| REFUSED)?;
    #[cfg(unix)]
    let (root, opened) = (adopted_open_path(&project_dir.fd).map_err(|_| REFUSED)?, adopted_open_path(&dir.fd).map_err(|_| REFUSED)?);
    adopted_opened_path(&root, &opened, &protected)?;
    let (mut names, mut incomplete) = dir.names_bounded(257).map_err(|_| READ)?;
    names.sort();
    names.truncate(256);
    let mut files = Vec::new();
    let mut consumed = 0;
    for name in names {
        if !name.ends_with(".md") {
            continue;
        }
        if consumed >= ADOPTED_MAX_BYTES {
            incomplete = true;
            break;
        }
        let mut read = || -> Result<String, &'static str> {
            adopted_relative(&name)?;
            let file = dir.file(&name).map_err(|_| REFUSED)?;
            #[cfg(windows)]
            let opened = faber_winproc::rewind::resolved_path(&file).map_err(|_| REFUSED)?.ok_or(REFUSED)?;
            #[cfg(unix)]
            let (root, opened) = (adopted_open_path(&project_dir.fd).map_err(|_| REFUSED)?, adopted_open_path(&file).map_err(|_| REFUSED)?);
            adopted_opened_path(&root, &opened, &protected)?;
            let remaining = ADOPTED_MAX_BYTES - consumed;
            if file.metadata().map_err(|_| READ)?.len() > remaining as u64 {
                return Err(READ);
            }
            let mut bytes = Vec::new();
            let result = file.take(remaining as u64 + 1).read_to_end(&mut bytes);
            consumed += bytes.len();
            result.map_err(|_| READ)?;
            if bytes.len() > remaining {
                return Err(READ);
            }
            String::from_utf8(bytes).map_err(|_| READ)
        };
        match read() {
            Ok(text) => {
                files.push((name, text));
            }
            Err(_) => incomplete = true,
        }
    }
    Ok(AdoptedTasks { files, incomplete })
}

/// Read an externally approved relative file through held no-follow project handles. This supplies no approval authority.
/// Adopted formats stay read-only until their adapters exist; source bytes never become trusted instructions here.
/// Private CLI and VCS directories are refused even when selected as the project.
pub fn read_adopted_source(project: &Path, relative: &str) -> Result<String, &'static str> {
    let path = adopted_relative(relative)?;
    let protected = resolved_private(&private_roots());
    let (root, project_dir) = project_directory(project, &protected)?;
    if private(&root.join(path), &protected) {
        return Err(REFUSED);
    }
    let parent = path.parent().ok_or(REFUSED)?;
    let name = path.file_name().and_then(|name| name.to_str()).ok_or(REFUSED)?;
    let dir = project_dir.under(parent).map_err(|_| REFUSED)?;
    let file = dir.file(name).map_err(|_| REFUSED)?;
    #[cfg(windows)]
    let opened = faber_winproc::rewind::resolved_path(&file).map_err(|_| REFUSED)?.ok_or(REFUSED)?;
    #[cfg(unix)]
    let (root, opened) = (adopted_open_path(&project_dir.fd).map_err(|_| REFUSED)?, adopted_open_path(&file).map_err(|_| REFUSED)?);
    adopted_opened_path(&root, &opened, &protected)?;
    let mut bytes = Vec::new();
    file.take(ADOPTED_MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| READ)?;
    if bytes.len() > ADOPTED_MAX_BYTES {
        return Err(READ);
    }
    String::from_utf8(bytes).map_err(|_| READ)
}

#[cfg(target_os = "macos")]
fn adopted_open_path(file: &impl std::os::fd::AsFd) -> std::io::Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    rustix::fs::getpath(file).map(|path| std::ffi::OsString::from_vec(path.into_bytes()).into()).map_err(Into::into)
}

#[cfg(target_os = "linux")]
fn adopted_open_path(file: &impl std::os::fd::AsFd) -> std::io::Result<PathBuf> {
    use std::os::fd::AsRawFd;
    std::fs::read_link(format!("/proc/self/fd/{}", file.as_fd().as_raw_fd()))
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn adopted_open_path(_file: &impl std::os::fd::AsFd) -> std::io::Result<PathBuf> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// The files `rules.md` is built from, read through the same no-follow handles as the notes: the root files named in `names`,
/// opened by name, and the first `listed` files of `folder` whose name `wanted` accepts, in name order (a listing stops at
/// Memory's 20,000 names). A folder or file that is absent, a link, private, over `MAX_BYTES` or not UTF-8 is skipped.
/// Returns each file's relative path and text.
pub(super) fn read_sources(
    project: &Path,
    names: &[&str],
    folder: &str,
    wanted: &dyn Fn(&str) -> bool,
    listed: usize,
) -> Result<Vec<(String, String)>, &'static str> {
    let protected = resolved_private(&private_roots());
    let (root, project_dir) = project_directory(project, &protected)?;
    let read = |dir: &Directory, name: &str, path: String| -> Option<(String, String)> {
        if private(&root.join(&path), &protected) {
            return None;
        }
        let mut bytes = Vec::new();
        dir.file(name).ok()?.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).ok()?;
        (bytes.len() <= MAX_BYTES).then_some(())?;
        String::from_utf8(bytes).ok().map(|text| (path, text))
    };
    let mut out: Vec<_> = names.iter().filter_map(|name| read(&project_dir, name, name.to_string())).collect();
    if listed > 0
        && let Ok(dir) = project_dir.under(Path::new(folder))
    {
        let mut found: Vec<String> = dir.names().unwrap_or_default().into_iter().filter(|n| wanted(n)).collect();
        found.sort();
        out.extend(found.iter().take(listed).filter_map(|name| read(&dir, name, format!("{folder}/{name}"))));
    }
    Ok(out)
}

/// Hashes a bounded generated file through no-follow handles. Unsafe or missing output is never a cache hit.
pub(super) fn faber_file_hash(project: &Path, name: &str) -> Result<String, &'static str> {
    let protected = resolved_private(&private_roots());
    let (_, project_dir) = project_directory(project, &protected)?;
    let dir = project_dir.under(Path::new(".faber")).map_err(|_| REFUSED)?;
    let mut bytes = Vec::new();
    dir.file(name).map_err(|_| REFUSED)?.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| READ)?;
    if bytes.len() > MAX_BYTES {
        return Err(READ);
    }
    Ok(revision(&bytes))
}

/// Writes `.faber/<name>` in the project when its bytes differ, creating the folder and the file if needed, through the
/// same no-follow handles and atomic replacement as an edit: a link planted in place of either is refused, never followed.
/// Returns whether it wrote.
pub(super) fn write_faber_file(project: &Path, name: &str, bytes: &[u8]) -> Result<bool, &'static str> {
    update_faber_file(project, name, |_| Ok(bytes.to_vec()))
}

fn update_faber_file(
    project: &Path,
    name: &str,
    update: impl FnOnce(&[u8]) -> Result<Vec<u8>, &'static str>,
) -> Result<bool, &'static str> {
    let _write = WRITES.lock().map_err(|_| "error.memory.write")?;
    let protected = resolved_private(&private_roots());
    let (_, project_dir) = project_directory(project, &protected)?;
    let dir = match project_dir.under(Path::new(".faber")) {
        Ok(dir) => dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => project_dir.create_dir(".faber").map_err(|_| REFUSED)?,
        Err(_) => return Err(REFUSED),
    };
    let file = match dir.file(name) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => dir.create(name).map_err(|_| REFUSED)?,
        Err(_) => return Err(REFUSED),
    };
    let permissions = file.metadata().map_err(|_| READ)?.permissions();
    if permissions.readonly() {
        return Err("error.memory.readOnly");
    }
    let mut current = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut current).map_err(|_| READ)?;
    if current.len() > MAX_BYTES {
        return Err(REFUSED);
    }
    let bytes = update(&current)?;
    if bytes.len() > MAX_BYTES {
        return Err(REFUSED);
    }
    if current == bytes {
        return Ok(false);
    }
    dir.replace(name, &bytes, permissions, &revision(&current)).map_err(|_| "error.memory.write")?;
    Ok(true)
}

/// Called only after the person's approval. A failed record save leaves a proposal, never an approval.
pub fn append_approved(project: &Path, kind: LearningKind, line: &str, over_target: bool) -> Result<String, &'static str> {
    let line = line.trim();
    if line.is_empty() || line.contains(['\r', '\n', '\0']) || line.len() > MAX_BYTES - 3 {
        return Err(REFUSED);
    }
    let item = format!("- {line}\n");
    let parsed = entries(&item, 1);
    if parsed.len() != 1 || parsed[0].text != item.trim_end() {
        return Err(REFUSED);
    }
    if !over_target && item.len() > kind.target() {
        return Err(REFUSED);
    }
    update_faber_file(project, kind.name(), |bytes| {
        let text = std::str::from_utf8(bytes).map_err(|_| READ)?;
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let mut next = text.to_string();
        // A blank separator keeps a trailing paragraph or fenced block from absorbing the approved item.
        if !next.is_empty() {
            if !next.ends_with('\n') {
                next.push_str(newline);
            }
            next.push_str(newline);
        }
        let start = next.len();
        next.push_str(item.trim_end());
        next.push_str(newline);
        if !over_target && next.len() > kind.target() {
            return Err(REFUSED);
        }
        let parsed = entries(&next, MAX_ENTRIES);
        if !parsed.iter().any(|entry| entry.start == start && entry.text == item.trim_end()) {
            return Err(REFUSED);
        }
        Ok(next.into_bytes())
    })?;
    Ok(revision(item.trim_end().as_bytes()))
}

/// Repository marks never confer approval on taste or lessons.
pub fn mark_learning(list: &mut MemoryList, records: &LearningRecords) {
    for file in &mut list.files {
        let Some(kind) = LearningKind::for_path(&file.path) else { continue };
        for entry in &mut file.entries {
            entry.reviewed = records.get(&kind).is_some_and(|hashes| hashes.contains(&revision(entry.text.trim().as_bytes())));
        }
    }
}

/// Fresh bounded reads for the card, independent of Memory's discovery window. Lessons have no card share.
pub fn taste_for_card(project: &Path, records: &LearningRecords) -> Result<Vec<String>, &'static str> {
    let Some(hashes) = records.get(&LearningKind::Taste).filter(|hashes| !hashes.is_empty()) else { return Ok(vec![]) };
    let protected = resolved_private(&private_roots());
    let (_, root) = project_directory(project, &protected)?;
    let dir = root.under(Path::new(".faber")).map_err(|_| REFUSED)?;
    let file = read_file(&dir, "taste.md", ".faber/taste.md".into(), "faber", MAX_ENTRIES)?;
    Ok(file.entries.into_iter().filter(|entry| hashes.contains(&revision(entry.text.trim().as_bytes()))).map(|entry| entry.text).collect())
}

fn codex_file(dir: &Directory, folder: &Path, protected: &[String]) -> Result<Option<&'static str>, &'static str> {
    for name in ["AGENTS.override.md", "AGENTS.md"] {
        if private(&folder.join(name), protected) {
            return Err(REFUSED);
        }
        let mut file = match dir.file(name) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(REFUSED),
        };
        // At the cap, retain only the file name. Never read an unbounded override to find a fallback.
        if file.metadata().map_err(|_| READ)?.len() > MAX_BYTES as u64 {
            return Ok(Some(name));
        }
        let mut bytes = Vec::new();
        (&mut file).take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| READ)?;
        if !std::str::from_utf8(&bytes).map_err(|_| READ)?.trim().is_empty() {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

fn task_directory(project: &Path, create: bool) -> Result<Directory, &'static str> {
    let protected = resolved_private(&private_roots());
    let (_, mut dir) = project_directory(project, &protected)?;
    for name in [".faber", "tasks"] {
        dir = match dir.under(Path::new(name)) {
            Ok(child) => child,
            Err(e) if create && e.kind() == std::io::ErrorKind::NotFound => dir.create_dir(name).map_err(|_| REFUSED)?,
            Err(_) => return Err(REFUSED),
        };
    }
    Ok(dir)
}

/// Creates one bounded task through pinned no-follow handles; a second worker never replaces the first one's task.
pub fn write_follow_up(project: &Path, task: &crate::progress::FollowUp) -> Result<crate::model::FollowUpTask, String> {
    let text = task.task_file()?;
    let _write = WRITES.lock().map_err(|_| "error.memory.write")?;
    let dir = task_directory(project, true)?;
    let (names, incomplete) = dir.names_bounded(MAX_ENTRIES).map_err(|_| READ)?;
    if incomplete {
        return Err(REFUSED.into());
    }
    let mut order = names.iter().filter_map(|n| n.split_once('-')?.0.parse::<u64>().ok()).max().unwrap_or(0);
    let slug: String = task.title.chars().take(64).map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    for _ in 0..32 {
        order = order.checked_add(1).ok_or(REFUSED)?;
        let name = format!("{order:03}-{}.md", slug.trim_matches('-'));
        let file = match dir.create(&name) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(REFUSED.into()),
        };
        let permissions = file.metadata().map_err(|_| READ)?.permissions();
        drop(file);
        if dir.replace(&name, text.as_bytes(), permissions, &revision(b"")).is_err() {
            return Err("error.memory.write".into());
        }
        return Ok(crate::model::FollowUpTask {
            title: task.title.trim().into(),
            file: format!(".faber/tasks/{name}"),
            revision: revision(text.as_bytes()),
            sent: false,
            message: None,
            ready: false,
        });
    }
    Err(REFUSED.into())
}

fn task_name(task: &crate::model::FollowUpTask) -> Result<&str, &'static str> {
    task.file
        .strip_prefix(".faber/tasks/")
        .filter(|n| !n.is_empty() && !n.contains(['/', '\\', ':', '\0']) && n.ends_with(".md"))
        .ok_or(REFUSED)
}

/// Readiness follows the current task folder, never the start card or an agent's claimed state.
pub fn ready_follow_ups(project: &Path, tasks: &[crate::model::FollowUpTask]) -> Result<Vec<crate::model::FollowUpTask>, &'static str> {
    let dir = task_directory(project, false)?;
    let (names, incomplete) = dir.names_bounded(MAX_ENTRIES).map_err(|_| READ)?;
    if incomplete {
        return Err(REFUSED);
    }
    let mut ready = Vec::new();
    for task in tasks.iter().filter(|t| !t.sent) {
        let mut text = String::new();
        let Ok(file) = dir.file(task_name(task)?) else { continue };
        file.take(6001).read_to_string(&mut text).map_err(|_| READ)?;
        if text.len() <= 6000 && revision(text.as_bytes()) == task.revision && crate::progress::follow_up_ready(&text, &names) {
            ready.push(crate::model::FollowUpTask { ready: true, ..task.clone() });
        }
    }
    Ok(ready)
}

/// Removes only the unchanged task that was offered. Links and edited briefs are preserved.
pub fn remove_follow_up(project: &Path, task: &crate::model::FollowUpTask) -> Result<(), &'static str> {
    let _write = WRITES.lock().map_err(|_| "error.memory.write")?;
    let dir = task_directory(project, false)?;
    let name = task_name(task)?;
    #[cfg(windows)]
    let mut file = match dir.dir.removal_file(name) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(REFUSED),
    };
    #[cfg(unix)]
    let mut file = match dir.file(name) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(REFUSED),
    };
    let mut bytes = Vec::new();
    (&mut file).take(6001).read_to_end(&mut bytes).map_err(|_| READ)?;
    if bytes.len() > 6000 || revision(&bytes) != task.revision {
        return Err(CHANGED);
    }
    // shortcut: Unix rename-over can race unlink; quarantine before supporting concurrent task replacement.
    dir.discard(&file, name).map_err(|_| "error.memory.write")
}

pub fn revision(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Upgrade only legacy approvals whose exact body is still present; absent approved words are never discarded.
pub fn upgrade_kept(list: &MemoryList, kept: &[KeptDecision]) -> Vec<KeptDecision> {
    let items: Vec<_> = list.files.iter().flat_map(|f| &f.entries).filter_map(|e| e.body.as_deref()).map(KeptDecision::of).collect();
    kept.iter()
        .map(|old| match old {
            KeptDecision::Legacy(_) => items.iter().find(|item| old.matches(item)).unwrap_or(old).clone(),
            _ => old.clone(),
        })
        .collect()
}

/// Marks the decisions the person kept or wrote (`kept` is what Faber stored outside the project). A `[by person]` mark
/// with no record is an agent's claim, so it reads as an AI decision not yet reviewed, whatever the file says.
pub fn mark_kept(list: &mut MemoryList, kept: &[KeptDecision]) {
    for entry in list.files.iter_mut().flat_map(|f| &mut f.entries) {
        let recorded = entry.body.as_deref().is_some_and(|b| {
            let item = KeptDecision::of(b);
            let recorded = kept.iter().any(|record| record.matches(&item));
            let stale = kept.iter().any(|record| record.matches(&item) && matches!(record, KeptDecision::Item { stale: true, .. }));
            entry.stale = if entry.author == Some("person") && recorded { stale } else { entry.stale || stale };
            recorded
        });
        if entry.author == Some("person") && !recorded {
            entry.author = Some("ai");
        }
        entry.reviewed = entry.author == Some("ai") && recorded;
    }
}

/// A partial read cannot prove that a decision was lost. Only active decisions count, as on the start card.
pub fn present_kept(list: &MemoryList, kept: &[KeptDecision]) -> Option<Vec<KeptDecision>> {
    if list.incomplete || list.files.iter().any(|file| decisions(&file.path) && file.too_large) {
        return None;
    }
    let items: Vec<_> = list
        .files
        .iter()
        .filter(|file| decisions(&file.path))
        .flat_map(|file| &file.entries)
        .filter_map(|entry| entry.body.as_deref())
        .map(KeptDecision::of)
        .collect();
    Some(kept.iter().filter(|record| items.iter().any(|item| record.matches(item))).cloned().collect())
}

fn entries(text: &str, limit: usize) -> Vec<Entry> {
    let mut depth = 0;
    let mut item = None;
    let mut paragraph = None;
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let bom = usize::from(text.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
    for (event, mut range) in Parser::new(&text[bom..]).into_offset_iter() {
        range.start += bom;
        range.end += bom;
        match event {
            Event::Start(tag) => {
                if matches!(tag, Tag::Item) && depth == 1 {
                    item = Some(range.start);
                }
                if matches!(tag, Tag::Paragraph) && depth == 0 {
                    paragraph = Some(range.start);
                }
                depth += 1;
            }
            Event::End(tag) => {
                depth -= 1;
                if tag == TagEnd::Item
                    && depth == 1
                    && let Some(start) = item.take()
                {
                    ranges.push(start..range.end);
                }
                if tag == TagEnd::Paragraph
                    && depth == 0
                    && let Some(start) = paragraph.take()
                {
                    ranges.push(start..range.end);
                }
                if ranges.len() > limit {
                    break;
                }
            }
            _ => {}
        }
    }
    let mut cursor = 0;
    let mut line = 1;
    ranges
        .into_iter()
        .map(|range| {
            // The entry owns its final newline, but never the blank separator after it.
            let mut end = range.start;
            let mut offset = range.start;
            for line in text[range.clone()].split_inclusive('\n') {
                offset += line.len();
                if !line.trim().is_empty() {
                    end = offset;
                }
            }
            let content_end = text[..end].trim_end_matches(['\r', '\n']).len();
            line += text[cursor..range.start].bytes().filter(|b| *b == b'\n').count();
            cursor = range.start;
            let text = &text[range.start..content_end];
            Entry { start: range.start, end, line, text: text.into(), body: None, author: None, reviewed: false, stale: false }
        })
        .collect()
}

fn note_target(path: &str) -> Option<u64> {
    const TARGETS: [(&str, u64); 5] = [("rules.md", 3), ("decisions.md", 8), ("taste.md", 1), ("lessons.md", 2), ("tasks/", 6)];
    let name = path.strip_prefix(".faber/").or_else(|| path.rfind("/.faber/").map(|at| &path[at + "/.faber/".len()..]))?;
    TARGETS
        .iter()
        .find(|(key, _)| if *key == "tasks/" { name.starts_with(key) && name.ends_with(".md") } else { name == *key })
        .map(|(_, kb)| kb * 1024)
}

fn read_file(dir: &Directory, name: &str, path: String, source: &'static str, entry_limit: usize) -> Result<MemoryFile, &'static str> {
    let file = dir.file(name).map_err(|_| REFUSED)?;
    let mut bytes = Vec::new();
    let size = file.metadata().map_err(|_| READ)?.len();
    let over_limit = note_target(&path).filter(|target| size > *target).map(|target| (size, target));
    if size <= MAX_BYTES as u64 {
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| READ)?;
    } else {
        return Ok(MemoryFile { path, source, revision: String::new(), too_large: true, over_limit, entries: vec![] });
    }
    let over_limit = note_target(&path).filter(|target| bytes.len() as u64 > *target).map(|target| (bytes.len() as u64, target));
    let too_large = bytes.len() > MAX_BYTES;
    let mut entries = if too_large { vec![] } else { entries(std::str::from_utf8(&bytes).map_err(|_| READ)?, entry_limit) };
    if decisions(&path) {
        for entry in &mut entries {
            if let Some(m) = mark(&entry.text) {
                (entry.body, entry.author) = (Some(entry.text[..m.open].trim_end().into()), Some(m.author));
            }
        }
    }
    for entry in &mut entries {
        entry.stale = entry.body.as_deref().unwrap_or(&entry.text).trim_end().ends_with(STALE);
    }
    Ok(MemoryFile { path, source, revision: revision(&bytes), too_large, over_limit, entries })
}

pub fn list(project: &Path) -> Result<MemoryList, &'static str> {
    list_with_private(project, &private_roots())
}

/// Only the notes in the project's root `.faber` folder, through the same no-follow handles: no other folder is walked, no
/// `CLAUDE.md` or `AGENTS.md` is read, and a note that cannot be read is skipped (the list says it may be incomplete).
pub fn list_faber(project: &Path) -> Result<MemoryList, &'static str> {
    scan(project, &private_roots(), true)
}

pub const STALE: &str = " (stale?)";

// Bare words need a familiar file extension; a dotted setting name is not a file reference.
fn reference_path(name: &str) -> Option<String> {
    let name = name.replace('\\', "/");
    if name.chars().any(|c| c.is_whitespace() || matches!(c, '`' | '$' | '|' | '<' | '>' | '"' | '?' | '*'))
        || adopted_relative(&name).is_err()
    {
        return None;
    }
    let extension = Path::new(&name).extension().and_then(|e| e.to_str());
    let bare_file = matches!(
        extension,
        Some("rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "md" | "json" | "toml" | "yaml" | "yml" | "txt" | "py" | "go" | "html" | "css")
    );
    (bare_file || (name.contains('/') && extension.is_some())).then_some(name)
}

/// Close only: bounded metadata probes, then ordinary guarded edits. Person-authored records own their status.
pub fn refresh_stale(project: &Path, kept: &mut [KeptDecision]) -> Result<(), &'static str> {
    let protected = resolved_private(&private_roots());
    let (root, directory) = project_directory(project, &protected)?;
    let identity = directory.identity().map_err(|_| READ)?;
    let list = list_faber(project)?;
    // Duplicates must not move a shared approval away from the person's unchanged words.
    let person_words: Vec<_> = list
        .files
        .iter()
        .flat_map(|file| &file.entries)
        .filter(|entry| entry.author == Some("person"))
        .filter_map(|entry| entry.body.as_deref())
        .map(KeptDecision::of)
        .filter(|item| kept.iter().any(|record| record.matches(item)))
        .collect();
    let mut remaining = 200;
    for mut file in list.files.into_iter().filter(|f| f.path != ".faber/rules.md" && !f.path.starts_with(".faber/tasks/")) {
        if !file.revision.starts_with(&format!("{identity}:")) {
            continue;
        }
        let mut changes = vec![];
        for entry in &file.entries {
            let paths: Vec<_> = Parser::new(entry.body.as_deref().unwrap_or(&entry.text))
                .filter_map(|event| if let Event::Code(name) = event { reference_path(&name) } else { None })
                .take(remaining + 1)
                .collect();
            if paths.len() > remaining {
                break;
            }
            remaining -= paths.len();
            if paths.is_empty() {
                continue;
            }
            // The 200-path budget above counts every reference; one missing path is enough to mark the item.
            let stale = paths.iter().any(|name| !reference_exists(&directory, &root, name, &protected));
            changes.push((entry.clone(), stale));
        }
        // Reverse offsets survive edits to later entries; every write still rechecks the whole file revision.
        for (entry, stale) in changes.into_iter().rev() {
            let body = entry.body.as_deref().unwrap_or(&entry.text);
            let old = KeptDecision::of(body);
            let person = person_words.iter().any(|record| record.matches(&old));
            let plain = body.strip_suffix(STALE).unwrap_or(body);
            let next = if stale && !person {
                format!("{plain}{STALE}")
            } else if person {
                body.into()
            } else {
                plain.into()
            };
            if next != body {
                let text = format!("{next}{}", &entry.text[body.len()..]);
                let change = Edit {
                    path: file.path.clone(),
                    revision: file.revision.clone(),
                    start: entry.start,
                    end: entry.end,
                    text: Some(text),
                    keep: false,
                    over_target: false,
                };
                if edit(project, &change).is_err() {
                    break; // A concurrent edit or read-only file wins; never retry against words we did not read.
                }
            }
            for record in kept.iter_mut().filter(|record| record.matches(&old)) {
                let files = match record {
                    KeptDecision::Item { files, .. } => files.clone(),
                    KeptDecision::Legacy(_) => vec![],
                };
                *record = KeptDecision::of(&next);
                if let KeptDecision::Item { files: saved, stale: flag, .. } = record {
                    *saved = files;
                    *flag = stale;
                }
            }
            if next != body {
                let path = Path::new(&file.path);
                let Some(parent) = path.parent() else { break };
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else { break };
                let Ok(dir) = directory.under(parent) else { break };
                let Ok(updated) = read_file(&dir, name, file.path.clone(), "faber", MAX_ENTRIES) else { break };
                file.revision = format!("{identity}:{}", updated.revision);
            }
        }
        if remaining == 0 {
            break;
        }
    }
    Ok(())
}

fn reference_exists(directory: &Directory, root: &Path, name: &str, protected: &[String]) -> bool {
    let Ok(path) = adopted_relative(name) else { return false };
    if private(&root.join(path), protected) {
        return false;
    }
    let Some(parent) = path.parent() else { return false };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else { return false };
    let Ok(file) = directory.under(parent).and_then(|dir| dir.file(name)) else { return false };
    #[cfg(windows)]
    let Some(opened) = faber_winproc::rewind::resolved_path(&file).ok().flatten() else { return false };
    #[cfg(unix)]
    let (Ok(root), Ok(opened)) = (adopted_open_path(&directory.fd), adopted_open_path(&file)) else { return false };
    #[cfg(unix)]
    let root = root.as_path();
    adopted_opened_path(root, &opened, protected).is_ok()
}

/// Only explicit consent records existing file references; reading old approvals never invents them.
fn kept_item(body: &str, directory: &Directory, root: &Path, protected: &[String]) -> KeptDecision {
    let mut record = KeptDecision::of(body);
    let names: std::collections::BTreeSet<_> =
        Parser::new(body).filter_map(|event| if let Event::Code(name) = event { Some(name.replace('\\', "/")) } else { None }).collect();
    if let KeptDecision::Item { files, stale, .. } = &mut record {
        files.extend(names.into_iter().filter(|name| reference_exists(directory, root, name, protected)));
        *stale = body.trim_end().ends_with(STALE);
    }
    record
}

/// The card needs the newest decisions, not Memory's truncated display window or its other note files.
pub fn kept_for_card(project: &Path, kept: &[KeptDecision]) -> Result<Vec<String>, &'static str> {
    if kept.is_empty() {
        return Ok(vec![]);
    }
    let protected = resolved_private(&private_roots());
    let (root, directory) = project_directory(project, &protected)?;
    if private(&root.join(".faber/decisions.md"), &protected) {
        return Err(REFUSED);
    }
    let notes = directory.under(Path::new(".faber")).map_err(|_| REFUSED)?;
    // Each entry owns at least one source byte, so this includes every entry in the byte-capped file.
    let file = read_file(&notes, "decisions.md", ".faber/decisions.md".into(), "faber", MAX_BYTES)?;
    if file.too_large {
        return Err(READ);
    }
    Ok(file
        .entries
        .into_iter()
        .rev()
        .filter_map(|entry| entry.body.map(|body| (body, entry.author)))
        .filter(|(body, _)| {
            let item = KeptDecision::of(body);
            let mut matching = kept.iter().filter(|record| record.matches(&item)).peekable();
            matching.peek().is_some()
                && matching.all(|record| match record {
                    KeptDecision::Item { files, stale, .. } => {
                        !stale && files.iter().all(|name| reference_exists(&directory, &root, name, &protected))
                    }
                    KeptDecision::Legacy(_) => true,
                })
        })
        .map(|(body, author)| if author == Some("person") { body.strip_suffix(STALE).unwrap_or(&body).to_owned() } else { body })
        .collect())
}

fn list_with_private(project: &Path, protected: &[String]) -> Result<MemoryList, &'static str> {
    scan(project, protected, false)
}

fn scan(project: &Path, protected: &[String], faber_only: bool) -> Result<MemoryList, &'static str> {
    let protected = resolved_private(protected);
    let (root, project_dir) = project_directory(project, &protected)?;
    let identity = project_dir.identity().map_err(|_| READ)?;
    let mut pending = vec![if faber_only { ".faber".to_string() } else { String::new() }];
    let mut skipped = false;
    let mut result = MemoryList::default();
    let mut visited = 0;
    let mut bytes = 0;
    let mut count = 0;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while let Some(relative) = pending.pop() {
        if std::time::Instant::now() >= deadline {
            result.incomplete = true;
            break;
        }
        if private(&root.join(&relative), &protected) {
            continue;
        }
        let dir = match project_dir.under(Path::new(&relative)) {
            Ok(dir) => dir,
            // No `.faber` (or a link in its place) is no notes.
            Err(_) if faber_only => continue,
            Err(_) => return Err(REFUSED),
        };
        let codex_path = if relative.is_empty() { "AGENTS.md".into() } else { format!("{relative}/AGENTS.md") };
        let codex =
            if !faber_only && source(&codex_path) == Some("codex") { codex_file(&dir, &root.join(&relative), &protected)? } else { None };
        let mut names = dir.names().map_err(|_| READ)?;
        names.sort();
        for name in names {
            visited += 1;
            if visited > 20_000
                || result.files.len() >= 256
                || bytes >= 4 * 1024 * 1024
                || count >= MAX_ENTRIES
                || std::time::Instant::now() >= deadline
            {
                result.incomplete = true;
                break;
            }
            let path = if relative.is_empty() { name.clone() } else { format!("{relative}/{name}") };
            if private(&root.join(&path), &protected) {
                continue;
            }
            if let Some(source) = source(&path) {
                if source == "codex" && Some(name.as_str()) != codex {
                    continue;
                }
                let mut file = match read_file(&dir, &name, path, source, MAX_ENTRIES) {
                    Ok(file) => file,
                    Err(_) if faber_only => {
                        skipped = true;
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                if !file.too_large {
                    file.revision = format!("{identity}:{}", file.revision);
                }
                let mut used = 0;
                let keep = file
                    .entries
                    .iter()
                    .take(MAX_ENTRIES - count)
                    .take_while(|entry| {
                        used += entry.text.len();
                        used <= 4 * 1024 * 1024 - bytes
                    })
                    .count();
                if keep < file.entries.len() {
                    file.entries.truncate(keep);
                    result.incomplete = true;
                }
                count += file.entries.len();
                bytes += file.entries.iter().map(|entry| entry.text.len()).sum::<usize>();
                result.files.push(file);
            } else if descend(&path) {
                // Opening directories uses no-follow handles; links are skipped before enumeration.
                if project_dir.under(Path::new(&path)).is_ok() {
                    pending.push(path);
                }
            }
        }
        if result.incomplete {
            break;
        }
    }
    result.incomplete |= skipped;
    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

fn descend(path: &str) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() > 64 {
        return false;
    }
    if let Some(i) = parts.iter().position(|p| *p == ".claude") {
        return i + 1 == parts.len();
    }
    if let Some(i) = parts.iter().position(|p| *p == ".faber") {
        return i + 1 == parts.len()
            || ["decisions", "taste", "lessons", "plans", "recipes", "learning", "memory", "tasks"].contains(&parts[i + 1]);
    }
    let name = parts.last().copied().unwrap_or_default();
    !matches!(name, ".git" | ".codex" | "node_modules" | "target" | "dist" | "vendor")
}

pub fn edit(project: &Path, edit: &Edit) -> Result<(), &'static str> {
    apply(project, edit).map(drop)
}

/// Like `edit`, and for `keep` returns the identity and content hash instead of writing anything.
pub fn apply(project: &Path, edit: &Edit) -> Result<Option<KeptDecision>, &'static str> {
    edit_with_private(project, edit, &private_roots())
}

/// One writer at a time for the notes and `rules.md`.
static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub enum RemovedReview {
    Decision(KeptDecision),
    Learning { kind: LearningKind, hash: String },
}

/// The review a Memory edit takes away, read before it writes: an entry deleted, or rewritten into other words, loses
/// the review of its words, so the same words written again later are not reviewed by themselves. Every copy of them
/// loses it, since no listing can vouch that another copy is still there (an agent writes the project meanwhile); the
/// person keeps them again. The file is read as `apply` reads it, so a changed file is refused here as there.
pub fn removes(project: &Path, edit: &Edit) -> Result<Option<RemovedReview>, &'static str> {
    let learning = LearningKind::for_path(&edit.path);
    if edit.keep || (!decisions(&edit.path) && learning.is_none()) {
        return Ok(None);
    }
    // What `apply` would refuse takes nothing back.
    source(&edit.path).ok_or(REFUSED)?;
    if edit.text.as_deref().is_some_and(|text| text.trim().is_empty() || text.contains('\0') || text.len() > MAX_BYTES) {
        return Err(REFUSED);
    }
    let protected = resolved_private(&private_roots());
    let (root, project_dir) = project_directory(project, &protected)?;
    if private(&root.join(&edit.path), &protected) {
        return Err(REFUSED);
    }
    let path = Path::new(&edit.path);
    let dir = project_dir.under(path.parent().ok_or(REFUSED)?).map_err(|_| REFUSED)?;
    let name = path.file_name().and_then(|s| s.to_str()).ok_or(REFUSED)?;
    let mut file = dir.file(name).map_err(|_| REFUSED)?;
    if file.metadata().map_err(|_| READ)?.permissions().readonly() {
        return Err("error.memory.readOnly");
    }
    let mut bytes = Vec::new();
    (&mut file).take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| READ)?;
    if bytes.len() > MAX_BYTES || format!("{}:{}", project_dir.identity().map_err(|_| READ)?, revision(&bytes)) != edit.revision {
        return Err(CHANGED);
    }
    let original = std::str::from_utf8(&bytes).map_err(|_| READ)?;
    let entry = entries(original, MAX_ENTRIES).into_iter().find(|e| e.start == edit.start && e.end == edit.end).ok_or(REFUSED)?;
    if let Some(kind) = learning {
        if let Some(text) = &edit.text {
            let size = bytes.len() - (entry.end - entry.start) + replacement(original, &entry, text).len();
            if size > MAX_BYTES || (!edit.over_target && size > kind.target()) {
                return Err(REFUSED);
            }
        }
        let same = edit.text.as_deref().is_some_and(|text| text.trim().replace("\r\n", "\n") == entry.text.trim().replace("\r\n", "\n"));
        return Ok((!same).then(|| RemovedReview::Learning { kind, hash: revision(entry.text.trim().as_bytes()) }));
    }
    let old = mark(&entry.text).map_or(entry.text.as_str(), |m| entry.text[..m.open].trim_end());
    // Compared with the line ends `apply` writes, so saving the same words with other line ends takes nothing back.
    let lf = |text: &str| KeptDecision::of(&text.replace("\r\n", "\n"));
    let same = edit.text.as_deref().is_some_and(|text| lf(mark(text).map_or(text, |m| &text[..m.open])) == lf(old));
    Ok((!same).then(|| RemovedReview::Decision(KeptDecision::of(old))))
}

fn replacement(original: &str, entry: &Entry, text: &str) -> String {
    let old = &original[entry.start..entry.end];
    let first = old.find('\n').map(|i| &old[..=i]).or_else(|| original.find('\n').map(|i| &original[..=i]));
    let newline = if first.is_some_and(|line| line.ends_with("\r\n")) { "\r\n" } else { "\n" };
    let text = text.replace("\r\n", "\n").replace('\n', newline);
    let mut replacement = text.trim_end_matches(['\r', '\n']).to_string();
    if old.ends_with('\n') {
        replacement.push_str(if old.ends_with("\r\n") { "\r\n" } else { "\n" });
    }
    replacement
}

fn edit_with_private(project: &Path, edit: &Edit, protected: &[String]) -> Result<Option<KeptDecision>, &'static str> {
    let _write = WRITES.lock().map_err(|_| "error.memory.write")?;
    if edit.path == ".faber/rules.md" {
        return Err(REFUSED);
    }
    let source = source(&edit.path).ok_or(REFUSED)?;
    let protected = resolved_private(protected);
    let (root, project_dir) = project_directory(project, &protected)?;
    if private(&root.join(&edit.path), &protected) {
        return Err(REFUSED);
    }
    let path = Path::new(&edit.path);
    let parent = path.parent().ok_or(REFUSED)?;
    let identity = project_dir.identity().map_err(|_| READ)?;
    let expected = edit.revision.strip_prefix(&format!("{identity}:")).ok_or(CHANGED)?;
    let dir = project_dir.under(parent).map_err(|_| REFUSED)?;
    let name = path.file_name().and_then(|s| s.to_str()).ok_or(REFUSED)?;
    let mut file = dir.file(name).map_err(|_| REFUSED)?;
    let permissions = file.metadata().map_err(|_| READ)?.permissions();
    if permissions.readonly() && !edit.keep {
        return Err("error.memory.readOnly");
    }
    if source == "codex" && codex_file(&dir, &root.join(parent), &protected)? != Some(name) {
        return Err(CHANGED);
    }
    let mut bytes = Vec::new();
    (&mut file).take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| READ)?;
    if bytes.len() > MAX_BYTES || revision(&bytes) != expected {
        return Err(CHANGED);
    }
    let original = std::str::from_utf8(&bytes).map_err(|_| READ)?;
    let entry = entries(original, MAX_ENTRIES).into_iter().find(|e| e.start == edit.start && e.end == edit.end).ok_or(REFUSED)?;
    if edit.keep {
        let text = &original[entry.start..entry.end];
        let kept = decisions(&edit.path).then(|| mark(text)).flatten().ok_or(REFUSED)?;
        return Ok(Some(kept_item(&text[..kept.open], &project_dir, &root, &protected)));
    }
    let replacement = if let Some(text) = &edit.text {
        if text.trim().is_empty() || text.contains('\0') || text.len() > MAX_BYTES {
            return Err(REFUSED);
        }
        replacement(original, &entry, text)
    } else {
        String::new()
    };
    let mut next = bytes[..entry.start].to_vec();
    next.extend_from_slice(replacement.as_bytes());
    next.extend_from_slice(&bytes[entry.end..]);
    if next.len() > MAX_BYTES {
        return Err(REFUSED);
    }
    if edit.text.is_some() && !edit.over_target && LearningKind::for_path(&edit.path).is_some_and(|kind| next.len() > kind.target()) {
        return Err(REFUSED);
    }
    drop(file);
    dir.replace(name, &next, permissions, expected)
        .map_err(|error| if error.kind() == std::io::ErrorKind::AlreadyExists { CHANGED } else { "error.memory.write" })?;
    // Words the person wrote themselves in Memory and marked theirs are recorded as theirs, like a Keep.
    let written = decisions(&edit.path).then(|| replacement.trim_end());
    Ok(written
        .and_then(|text| mark(text).filter(|m| m.author == "person").map(|m| kept_item(&text[..m.open], &project_dir, &root, &protected))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_memory_stale_paths_mark_restore_and_preserve_person_words() {
        let (project, file) = decisions_file(
            "- Missing `src/gone.rs`. [by AI]\n- Existing `src/live.rs`. [by AI]\n- Mine `src/mine.rs`. [by person]\n- Ignore `../x`, `https://host/x.rs`, `cargo test`, `setting.name` and `/absolute.rs`.\n",
        );
        std::fs::create_dir(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/live.rs"), "").unwrap();
        let mut kept = vec![keep(project.path(), &file, 2).unwrap().unwrap()];
        refresh_stale(project.path(), &mut kept).unwrap();
        let file = listed(project.path(), ".faber/decisions.md", &kept);
        assert!(file.entries[0].text.contains(" (stale?)"));
        assert_eq!(file.entries[1].text, "- Existing `src/live.rs`. [by AI]");
        assert_eq!(file.entries[2].text, "- Mine `src/mine.rs`. [by person]");
        assert_eq!(file.entries[2].author, Some("person"));
        assert!(file.entries[2].stale, "a person's unchanged words take their status from the approval record");
        assert!(!file.entries[3].text.contains(" (stale?)"));
        assert!(kept_for_card(project.path(), &kept).unwrap().is_empty());
        std::fs::write(project.path().join("src/gone.rs"), "").unwrap();
        std::fs::write(project.path().join("src/mine.rs"), "").unwrap();
        refresh_stale(project.path(), &mut kept).unwrap();
        let file = listed(project.path(), ".faber/decisions.md", &kept);
        assert!(!file.entries.iter().any(|e| e.text.contains(" (stale?)")));
        assert_eq!(kept_for_card(project.path(), &kept).unwrap(), ["- Mine `src/mine.rs`."]);
    }

    #[test]
    fn project_memory_stale_paths_refuse_links_and_stop_at_two_hundred() {
        let project = folder();
        let outside = folder();
        std::fs::create_dir(project.path().join(".faber")).unwrap();
        std::fs::write(outside.path().join("secret.rs"), "synthetic fixture, never read").unwrap();
        #[cfg(windows)]
        junction::create(outside.path(), project.path().join("linked")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), project.path().join("linked")).unwrap();
        let text = format!("- Refuse `linked/secret.rs`.\n{}", (0..201).map(|i| format!("- Missing `src/{i}.rs`.\n")).collect::<String>());
        std::fs::write(project.path().join(".faber/memory.md"), text).unwrap();
        let mut system = sysinfo::System::new();
        let pid = sysinfo::Pid::from_u32(std::process::id());
        let mut usage = || {
            system.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::Some(&[pid]),
                true,
                sysinfo::ProcessRefreshKind::nothing().with_memory().with_cpu(),
            );
            let process = system.process(pid).unwrap();
            (process.memory(), process.accumulated_cpu_time())
        };
        let (memory, cpu) = usage();
        let started = std::time::Instant::now();
        refresh_stale(project.path(), &mut []).unwrap();
        let wall = started.elapsed();
        let (after, cpu_after) = usage();
        eprintln!(
            "stale close: 200 probes and guarded edits, {wall:?} wall, {} ms process CPU, {} B resident growth sampled after (not peak)",
            cpu_after.saturating_sub(cpu),
            after.saturating_sub(memory)
        );
        let file = listed(project.path(), ".faber/memory.md", &[]);
        assert_eq!(file.entries.iter().filter(|e| e.text.contains(" (stale?)")).count(), 200);
        assert!(file.entries[0].text.contains(" (stale?)"));
        assert!(!file.entries[200].text.contains(" (stale?)"));
        assert_eq!(std::fs::read_to_string(outside.path().join("secret.rs")).unwrap(), "synthetic fixture, never read");
        #[cfg(windows)]
        junction::delete(project.path().join("linked")).unwrap();
    }

    #[test]
    fn project_memory_stale_paths_keep_ai_approvals_bound_to_the_edited_words() {
        let (project, file) = decisions_file("- Kept `src/lib.rs`. [by AI]\n");
        std::fs::create_dir(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/lib.rs"), "").unwrap();
        let mut kept = vec![keep(project.path(), &file, 0).unwrap().unwrap()];
        let original = kept.clone();
        std::fs::remove_file(project.path().join("src/lib.rs")).unwrap();
        refresh_stale(project.path(), &mut kept).unwrap();
        let file = listed(project.path(), ".faber/decisions.md", &kept);
        assert!(file.entries[0].reviewed);
        assert!(file.entries[0].text.contains(" (stale?)"));
        std::fs::write(project.path().join("src/lib.rs"), "").unwrap();
        refresh_stale(project.path(), &mut kept).unwrap();
        assert_eq!(kept, original);
        assert_eq!(kept_for_card(project.path(), &kept).unwrap(), ["- Kept `src/lib.rs`."]);
    }

    #[test]
    fn project_memory_stale_paths_duplicate_person_words_keep_their_record_in_both_orders() {
        for reverse in [false, true] {
            let person = "- Use `src/gone.rs`. [by person]\n";
            let ai = "- Use `src/gone.rs`. [by AI]\n";
            let (project, file) = decisions_file(&if reverse { format!("{ai}{person}") } else { format!("{person}{ai}") });
            let person_index = usize::from(reverse);
            let mut kept = vec![keep(project.path(), &file, person_index).unwrap().unwrap()];
            refresh_stale(project.path(), &mut kept).unwrap();
            let file = listed(project.path(), ".faber/decisions.md", &kept);
            assert_eq!(file.entries[person_index].text, person.trim_end());
            assert_eq!(file.entries[person_index].author, Some("person"));
            assert!(file.entries.iter().all(|entry| entry.stale));
        }
    }

    #[test]
    fn project_memory_stale_paths_leave_follow_up_task_files_unchanged() {
        let project = folder();
        let task: crate::progress::FollowUp = serde_json::from_value(serde_json::json!({
            "title": "Create export", "files": "`src/new.rs`", "done": "`src/new.rs` exists"
        }))
        .unwrap();
        let saved = write_follow_up(project.path(), &task).unwrap();
        refresh_stale(project.path(), &mut []).unwrap();
        assert_eq!(ready_follow_ups(project.path(), std::slice::from_ref(&saved)).unwrap().len(), 1);
        remove_follow_up(project.path(), &saved).unwrap();
    }

    #[test]
    fn project_memory_stale_paths_kept_person_mark_clears_without_rewriting_their_words() {
        let (project, _) = decisions_file("- Mine `src/lib.rs`. [by person]\n");
        refresh_stale(project.path(), &mut []).unwrap();
        let file = listed(project.path(), ".faber/decisions.md", &[]);
        let mut kept = vec![keep(project.path(), &file, 0).unwrap().unwrap()];
        assert!(listed(project.path(), ".faber/decisions.md", &kept).entries[0].stale);
        std::fs::create_dir(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/lib.rs"), "").unwrap();
        refresh_stale(project.path(), &mut kept).unwrap();
        let restored = listed(project.path(), ".faber/decisions.md", &kept);
        assert_eq!(restored.entries[0].text, file.entries[0].text, "Keep protects the exact source, including a prior machine mark");
        assert!(!restored.entries[0].stale);
        assert_eq!(kept_for_card(project.path(), &kept).unwrap(), ["- Mine `src/lib.rs`."]);
    }

    #[test]
    fn follow_up_files_are_bounded_ready_and_excluded_from_memory_and_start_sources() {
        let project = folder();
        let mut task = crate::progress::FollowUp {
            title: "Fix export".into(),
            area: "Projects".into(),
            files: "ui/Export.tsx".into(),
            depends: vec![],
            size: "small".into(),
            done: "Export opens correctly.".into(),
        };
        let first = write_follow_up(project.path(), &task).unwrap();
        assert_eq!(std::fs::read_to_string(project.path().join(&first.file)).unwrap(), task.task_file().unwrap());
        task.depends = vec![first.file.rsplit('/').next().unwrap().into()];
        let second = write_follow_up(project.path(), &task).unwrap();
        assert_ne!(first.file, second.file);
        assert_eq!(ready_follow_ups(project.path(), &[first.clone(), second.clone()]).unwrap().len(), 1);
        // Memory lists task files since #648 (it marks one over its 6 KB target); the start card below still leaves them out.
        assert!(list_faber(project.path()).unwrap().files.iter().any(|f| f.path.contains("tasks/")));
        let facts = crate::project::rules::facts(project.path()).unwrap();
        assert!(!crate::project::rules::card(&facts, &[], &[]).contains("Export opens correctly"));
        remove_follow_up(project.path(), &first).unwrap();
        assert_eq!(ready_follow_ups(project.path(), std::slice::from_ref(&second)).unwrap().len(), 1);
        task.done = "é".repeat(3000);
        assert!(write_follow_up(project.path(), &task).unwrap_err().contains("6,000 bytes"));
        assert_eq!(std::fs::read_dir(project.path().join(".faber/tasks")).unwrap().count(), 1);
        std::fs::write(project.path().join(&second.file), "Person edited this task").unwrap();
        assert_eq!(remove_follow_up(project.path(), &second), Err(CHANGED));
        assert_eq!(std::fs::read_to_string(project.path().join(&second.file)).unwrap(), "Person edited this task");
        assert!(ready_follow_ups(project.path(), &[second]).unwrap().is_empty());
    }

    #[cfg(any(windows, unix))]
    #[test]
    fn follow_up_files_refuse_linked_task_folders() {
        let project = folder();
        let outside = folder();
        std::fs::create_dir(project.path().join(".faber")).unwrap();
        #[cfg(windows)]
        junction::create(outside.path(), project.path().join(".faber/tasks")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), project.path().join(".faber/tasks")).unwrap();
        let task: crate::progress::FollowUp = serde_json::from_value(serde_json::json!({"title":"Fix it", "done":"Test passes"})).unwrap();
        assert!(write_follow_up(project.path(), &task).is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
        #[cfg(windows)]
        junction::delete(project.path().join(".faber/tasks")).unwrap();
    }

    fn folder() -> tempfile::TempDir {
        tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
    }

    #[test]
    fn taste_lesson_memory_edits_require_separate_over_target_consent() {
        let project = folder();
        std::fs::create_dir(project.path().join(".faber")).unwrap();
        for kind in [LearningKind::Taste, LearningKind::Lessons] {
            let path = format!(".faber/{}", kind.name());
            let target = project.path().join(&path);
            let rest = "- Other.\n";
            std::fs::write(&target, format!("- Small.\n{rest}")).unwrap();
            let request = |text: Option<String>| -> Edit {
                let file = list(project.path()).unwrap().files.into_iter().find(|file| file.path == path).unwrap();
                serde_json::from_value(serde_json::json!({
                    "path": path, "revision": file.revision, "start": file.entries[0].start, "end": file.entries[0].end, "text": text
                }))
                .unwrap()
            };
            let at_target = format!("- {}", "x".repeat(kind.target() - rest.len() - 3));
            edit(project.path(), &request(Some(at_target.clone()))).unwrap();
            assert_eq!(std::fs::metadata(&target).unwrap().len(), kind.target() as u64);
            let before = std::fs::read(&target).unwrap();
            let mut over = request(Some(format!("{at_target}x")));
            assert!(!over.over_target, "old callers do not consent by omission");
            assert_eq!(edit(project.path(), &over), Err(REFUSED));
            assert_eq!(std::fs::read(&target).unwrap(), before);
            over.over_target = true;
            edit(project.path(), &over).unwrap();
            assert_eq!(std::fs::metadata(&target).unwrap().len(), kind.target() as u64 + 1);
            let before = std::fs::read(&target).unwrap();
            let mut huge = request(Some(format!("- {}", "x".repeat(MAX_BYTES - 3))));
            huge.over_target = true;
            assert_eq!(edit(project.path(), &huge), Err(REFUSED));
            assert_eq!(std::fs::read(&target).unwrap(), before);
            edit(project.path(), &request(None)).unwrap();
            assert_eq!(std::fs::read_to_string(&target).unwrap(), rest);
        }
    }

    #[test]
    fn taste_lesson_project_records_never_approve_hand_written_card_text() {
        let project = folder();
        let data = folder();
        let store = crate::store::Store::new(data.path());
        let id = "project";
        let kept = "- Approved taste.";
        let hand_written = "- Hand-written taste. [by person]";
        std::fs::create_dir(project.path().join(".faber")).unwrap();
        std::fs::write(project.path().join(".faber/taste.md"), format!("{kept}\n{hand_written}\n")).unwrap();
        let forged = serde_json::json!({(id): {"taste": [revision(hand_written.as_bytes())]}});
        for parent in [project.path().to_path_buf(), project.path().join(".faber")] {
            let trust = parent.join("faber-trust");
            std::fs::create_dir(&trust).unwrap();
            std::fs::write(trust.join("learning-kept.json"), serde_json::to_vec(&forged).unwrap()).unwrap();
        }
        assert!(store.approved_learning_reader(id)().is_empty());
        assert!(taste_for_card(project.path(), &store.approved_learning_reader(id)()).unwrap().is_empty());
        store.keep_learning(id, LearningKind::Taste, revision(kept.as_bytes())).unwrap();
        assert!(data.path().join("faber-trust/learning-kept.json").is_file());
        let records = store.approved_learning_reader(id)();
        assert_eq!(records[&LearningKind::Taste], [revision(kept.as_bytes())]);
        assert_eq!(taste_for_card(project.path(), &records).unwrap(), [kept]);
        std::fs::write(project.path().join(".faber/taste.md"), format!("{kept}\n  Changed continuation.\n{hand_written}\n")).unwrap();
        assert!(taste_for_card(project.path(), &records).unwrap().is_empty(), "the complete entry must still match its recorded hash");
        let mut displayed = list(project.path()).unwrap();
        mark_learning(&mut displayed, &records);
        assert!(displayed.files.iter().flat_map(|file| &file.entries).all(|entry| !entry.reviewed));
    }

    #[test]
    fn taste_lesson_append_refuses_links_and_unclosed_blocks_and_keeps_the_target_soft() {
        let project = folder();
        let outside = folder();
        std::fs::create_dir(project.path().join(".faber")).unwrap();
        let target = project.path().join(".faber/taste.md");
        let other = outside.path().join("note.md");
        std::fs::write(&other, "Outside.").unwrap();
        std::fs::hard_link(&other, &target).unwrap();
        assert!(append_approved(project.path(), LearningKind::Taste, "New.", false).is_err());
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "Outside.");
        std::fs::remove_file(&target).unwrap();
        std::fs::write(&target, "```\nunclosed\n").unwrap();
        assert_eq!(append_approved(project.path(), LearningKind::Taste, "New.", false), Err(REFUSED));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "```\nunclosed\n");
        std::fs::remove_file(&target).unwrap();
        append_approved(project.path(), LearningKind::Taste, &"é".repeat(510), false).unwrap();
        assert_eq!(std::fs::metadata(&target).unwrap().len(), 1023);
        assert_eq!(append_approved(project.path(), LearningKind::Taste, "New.", false), Err(REFUSED));
        append_approved(project.path(), LearningKind::Taste, "New.", true).unwrap();
        let records = LearningRecords::from([(LearningKind::Taste, vec![revision("- New.".as_bytes())])]);
        assert_eq!(taste_for_card(project.path(), &records).unwrap(), ["- New."]);
        std::fs::remove_file(&target).unwrap();
        std::fs::remove_dir(project.path().join(".faber")).unwrap();
        #[cfg(windows)]
        junction::create(outside.path(), project.path().join(".faber")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), project.path().join(".faber")).unwrap();
        assert!(append_approved(project.path(), LearningKind::Lessons, "New.", false).is_err());
        assert!(!outside.path().join("lessons.md").exists());
        #[cfg(windows)]
        junction::delete(project.path().join(".faber")).unwrap();
    }

    #[test]
    fn adopted_private_folders_and_files_are_never_read() {
        let project = folder();
        for relative in [
            ".ssh/id_ed25519",
            ".aws/credentials",
            ".docker/config.json",
            ".azure/session.json",
            ".kube/config",
            ".credentials.json",
            "auth.json",
            "credentials.json",
            ".env",
            ".env.local",
            "private.pem",
            "private.key",
            "private.p12",
            "private.pfx",
            ".npmrc",
            ".git-credentials",
            "hosts.yml",
            "update/updater.key",
        ] {
            let file = project.path().join(relative);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, "synthetic private record, never a real credential").unwrap();
            assert!(read_adopted_source(project.path(), relative).is_err(), "private source accepted: {relative}");
        }
        let ordinary = "notes about .ssh.md";
        std::fs::write(project.path().join(ordinary), "ordinary project notes").unwrap();
        assert_eq!(read_adopted_source(project.path(), ordinary).unwrap(), "ordinary project notes");
        for example in [".env.example", ".env.sample", ".env.template", ".env.dist"] {
            std::fs::write(project.path().join(example), "ordinary project example").unwrap();
            assert_eq!(read_adopted_source(project.path(), example).unwrap(), "ordinary project example");
        }
    }

    #[test]
    fn adopted_task_folder_reads_sorted_direct_markdown_without_changing_the_project() {
        let project = folder();
        let tasks = project.path().join("docs/tasks");
        std::fs::create_dir_all(tasks.join("nested")).unwrap();
        std::fs::write(tasks.join("020-second.md"), "# Second\n\nKeep this format.\n").unwrap();
        std::fs::write(tasks.join("010-first.md"), "# First\n").unwrap();
        std::fs::write(tasks.join("queue.txt"), "Not a Markdown task.").unwrap();
        std::fs::write(tasks.join("nested/hidden.md"), "Not a direct child.").unwrap();
        let loaded = read_adopted_tasks(project.path(), "docs/tasks").unwrap();
        assert_eq!(
            loaded.files,
            vec![
                ("010-first.md".to_string(), "# First\n".to_string()),
                ("020-second.md".to_string(), "# Second\n\nKeep this format.\n".to_string())
            ]
        );
        assert!(!loaded.incomplete);
        assert_eq!(std::fs::read_to_string(tasks.join("020-second.md")).unwrap(), "# Second\n\nKeep this format.\n");
        assert!(!project.path().join(".faber").exists());
        for target in ["", ".", "../outside", "docs/../docs/tasks", "docs\\tasks"] {
            assert!(read_adopted_tasks(project.path(), target).is_err(), "{target:?}");
        }
        assert!(read_adopted_tasks(project.path(), tasks.to_str().unwrap()).is_err());
    }

    #[test]
    fn adopted_task_folder_reports_refused_candidates_without_reading_outside_bytes() {
        let owned = folder();
        let project = owned.path().join("project");
        let tasks = project.join("tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        let outside = owned.path().join("outside.md");
        std::fs::write(&outside, "SYNTHETIC OUTSIDE BYTES MUST NOT BE READ").unwrap();
        std::fs::hard_link(&outside, tasks.join("linked.md")).unwrap();
        std::fs::write(tasks.join("safe.md"), "# Keep this task\n").unwrap();
        std::fs::write(tasks.join(".env.md"), "synthetic private record").unwrap();
        std::fs::write(tasks.join("invalid.md"), [0xff, 0xfe]).unwrap();
        let loaded = read_adopted_tasks(&project, "tasks").unwrap();
        assert!(loaded.incomplete);
        assert_eq!(loaded.files, vec![("safe.md".to_string(), "# Keep this task\n".to_string())]);
        assert_eq!(std::fs::read_to_string(outside).unwrap(), "SYNTHETIC OUTSIDE BYTES MUST NOT BE READ");
    }

    #[test]
    fn adopted_task_folder_bounds_total_payload_and_reports_truncation() {
        let project = folder();
        let tasks = project.path().join("tasks");
        std::fs::create_dir(&tasks).unwrap();
        std::fs::write(tasks.join("010-large.md"), "a".repeat(300 * 1024)).unwrap();
        std::fs::write(tasks.join("020-large.md"), "b".repeat(300 * 1024)).unwrap();
        std::fs::write(tasks.join("030-small.md"), "# Small\n").unwrap();
        let loaded = read_adopted_tasks(project.path(), "tasks").unwrap();
        assert!(loaded.incomplete);
        assert_eq!(loaded.files.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["010-large.md", "030-small.md"]);
        assert!(loaded.files.iter().map(|(_, text)| text.len()).sum::<usize>() <= 512 * 1024);
        assert_eq!(std::fs::metadata(tasks.join("020-large.md")).unwrap().len(), 300 * 1024);
    }

    #[test]
    fn adopted_task_folder_bounds_enumeration_even_when_entries_are_not_tasks() {
        let project = folder();
        let tasks = project.path().join("tasks");
        std::fs::create_dir(&tasks).unwrap();
        for index in 0..270 {
            std::fs::write(tasks.join(format!("{index:03}.txt")), "ignored").unwrap();
        }
        let loaded = read_adopted_tasks(project.path(), "tasks").unwrap();
        assert!(loaded.incomplete);
        assert!(loaded.files.is_empty());
        assert_eq!(std::fs::read_dir(&tasks).unwrap().count(), 270);
    }

    #[test]
    fn adopted_task_folder_refuses_private_directories_before_enumeration() {
        let project = folder();
        for relative in [".git/tasks", ".claude/tasks", ".codex/tasks", ".gemini/tasks", ".env"] {
            let tasks = project.path().join(relative);
            std::fs::create_dir_all(&tasks).unwrap();
            std::fs::write(tasks.join("task.md"), "synthetic private task").unwrap();
            assert!(read_adopted_tasks(project.path(), relative).is_err(), "{relative}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn adopted_task_folder_refuses_a_private_folder_selected_by_short_name() {
        let project = folder();
        let private = project.path().join(".claude");
        std::fs::create_dir(&private).unwrap();
        std::fs::write(private.join("task.md"), "synthetic private task").unwrap();
        let alias = project.path().join("CLAUDE~1");
        if alias.canonicalize().ok() != Some(private.canonicalize().unwrap()) {
            eprintln!("8.3 names unavailable; alias condition not exercised");
            return;
        }
        assert!(read_adopted_tasks(project.path(), "CLAUDE~1").is_err());
    }

    #[test]
    fn adopted_task_folder_counts_discarded_bytes_against_its_read_budget() {
        let project = folder();
        let tasks = project.path().join("tasks");
        std::fs::create_dir(&tasks).unwrap();
        std::fs::write(tasks.join("010-invalid.md"), vec![0xff; 512 * 1024]).unwrap();
        std::fs::write(tasks.join("020-later.md"), "must not be read after the budget is consumed").unwrap();
        let loaded = read_adopted_tasks(project.path(), "tasks").unwrap();
        assert!(loaded.incomplete);
        assert!(loaded.files.is_empty(), "invalid source bytes consume the same read budget as valid source bytes");
        assert_eq!(std::fs::metadata(tasks.join("010-invalid.md")).unwrap().len(), 512 * 1024);
    }

    #[test]
    fn adopted_source_reads_existing_project_files_without_changing_them() {
        let project = folder();
        let target = project.path().join("docs/decisions.md");
        std::fs::create_dir(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "# Existing decisions\n\nKeep the original format.\n").unwrap();
        let before = std::fs::read(&target).unwrap();
        assert_eq!(read_adopted_source(project.path(), "docs/decisions.md").unwrap().as_bytes(), before);
        assert_eq!(std::fs::read(&target).unwrap(), before);
        for refused in ["", ".", "../outside.md", "docs/../decisions.md", "docs\\decisions.md", "docs/missing.md"] {
            assert!(read_adopted_source(project.path(), refused).is_err(), "{refused:?}");
        }
        assert!(read_adopted_source(project.path(), target.to_str().unwrap()).is_err());
        for directory in [".claude", ".codex", ".gemini", ".git"] {
            std::fs::create_dir(project.path().join(directory)).unwrap();
            std::fs::write(project.path().join(directory).join("private.json"), "synthetic private data").unwrap();
            assert!(read_adopted_source(project.path(), &format!("{directory}/private.json")).is_err());
            assert!(read_adopted_source(&project.path().join(directory), "private.json").is_err(), "private project root: {directory}");
        }
    }

    #[test]
    fn adopted_source_rechecks_replaced_directories_and_refuses_unbounded_or_linked_files() {
        let owned = folder();
        let project = owned.path().join("project");
        let outside = owned.path().join("outside");
        std::fs::create_dir_all(project.join("docs")).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(project.join("docs/decisions.md"), "inside").unwrap();
        std::fs::write(outside.join("decisions.md"), "outside").unwrap();
        assert_eq!(read_adopted_source(&project, "docs/decisions.md").unwrap(), "inside");
        std::fs::rename(project.join("docs"), project.join("original-docs")).unwrap();
        #[cfg(windows)]
        junction::create(&outside, project.join("docs")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, project.join("docs")).unwrap();
        assert!(read_adopted_source(&project, "docs/decisions.md").is_err());
        #[cfg(windows)]
        junction::delete(project.join("docs")).unwrap();
        #[cfg(unix)]
        std::fs::remove_file(project.join("docs")).unwrap();
        assert_eq!(std::fs::read_to_string(outside.join("decisions.md")).unwrap(), "outside");
        std::fs::hard_link(outside.join("decisions.md"), project.join("linked.md")).unwrap();
        assert!(read_adopted_source(&project, "linked.md").is_err());
        std::fs::write(project.join("large.md"), vec![b'x'; ADOPTED_MAX_BYTES + 1]).unwrap();
        assert!(read_adopted_source(&project, "large.md").is_err());
        std::fs::write(project.join("invalid.md"), [0xff]).unwrap();
        assert!(read_adopted_source(&project, "invalid.md").is_err());
        assert!(read_adopted_source(&project, "original-docs").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn adopted_source_refuses_a_private_folder_opened_by_its_short_name() {
        let project = folder();
        let private = project.path().join(".claude");
        std::fs::create_dir(&private).unwrap();
        std::fs::write(private.join("private.json"), "synthetic private data").unwrap();
        let alias = project.path().join("CLAUDE~1");
        if alias.canonicalize().ok() != Some(private.canonicalize().unwrap()) {
            eprintln!("8.3 names are unavailable for this temporary folder; alias case not exercised");
            return;
        }
        assert!(read_adopted_source(project.path(), "CLAUDE~1/private.json").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn project_memory_keeps_short_project_root_names_in_the_private_exclusions() {
        let base = folder();
        let project = base.path().join("MEMORY-PROJECT-LONG-FOLDER");
        std::fs::create_dir(&project).unwrap();
        let alias = base.path().join("MEMORY~1");
        if alias.canonicalize().ok() != Some(project.canonicalize().unwrap()) {
            eprintln!("8.3 names are unavailable for this temporary folder");
            return;
        }
        std::fs::write(project.join("CLAUDE.md"), "Global instructions.\n").unwrap();
        let protected = [normal(&project)];
        assert_eq!(list_with_private(&alias, &protected).unwrap_err(), REFUSED);
        std::fs::write(project.join("AGENTS.override.md"), [0xff, 0x00]).unwrap();
        let protected = [normal(&project.join("AGENTS.override.md"))];
        assert_eq!(list_with_private(&alias, &protected).unwrap_err(), REFUSED);
    }

    #[test]
    fn project_memory_excludes_linked_cli_folder_targets_from_listing_and_edits() {
        for name in [".claude", ".codex", "relocated-profile"] {
            let base = folder();
            let project = base.path().join("repo");
            let profile = project.join("profile");
            let home = base.path().join("home");
            std::fs::create_dir_all(&profile).unwrap();
            std::fs::create_dir(&home).unwrap();
            let path = profile.join("CLAUDE.md");
            std::fs::write(&path, "Global instructions.\n").unwrap();
            // A row listed before this folder becomes a private CLI target must also stop being editable.
            let file = list_with_private(&project, &[]).unwrap().files.remove(0);
            let linked = home.join(name);
            #[cfg(windows)]
            junction::create(&profile, &linked).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&profile, &linked).unwrap();
            let protected = [normal(&linked)];
            let listed = list_with_private(&project, &protected);
            let selected = list_with_private(&profile, &protected);
            let saved = edit_with_private(
                &project,
                &Edit {
                    path: file.path,
                    revision: file.revision,
                    start: file.entries[0].start,
                    end: file.entries[0].end,
                    text: Some("Changed global instructions.".into()),
                    keep: false,
                    over_target: false,
                },
                &protected,
            );
            #[cfg(windows)]
            junction::delete(&linked).unwrap();
            #[cfg(unix)]
            std::fs::remove_file(&linked).unwrap();
            assert!(listed.unwrap().files.is_empty(), "the target of {name} must never be listed");
            assert_eq!(selected.unwrap_err(), REFUSED, "the target of {name} cannot be selected as a project");
            assert_eq!(saved, Err(REFUSED), "a stale row under {name} must not change global instructions");
            assert_eq!(std::fs::read(&path).unwrap(), b"Global instructions.\n");
        }
    }

    #[test]
    fn project_memory_never_probes_a_private_codex_override_for_listing_or_editing() {
        let project = folder();
        let path = project.path().join("AGENTS.md");
        std::fs::write(&path, "Project instructions.\n").unwrap();
        let file = list_with_private(project.path(), &[]).unwrap().files.remove(0);
        let target = project.path().join("AGENTS.override.md");
        std::fs::write(&target, [0xff, 0x00]).unwrap();
        // Supply the resolved private target directly: no Windows file-symlink privilege is needed.
        let protected = [normal(&target)];
        let listed = list_with_private(project.path(), &protected);
        let saved = edit_with_private(
            project.path(),
            &Edit {
                path: file.path,
                revision: file.revision,
                start: file.entries[0].start,
                end: file.entries[0].end,
                text: None,
                keep: false,
                over_target: false,
            },
            &protected,
        );
        assert_eq!(listed.unwrap_err(), REFUSED, "private override bytes must not be decoded while choosing the active file");
        assert_eq!(saved, Err(REFUSED), "a stale base row must not probe a private override either");
        assert_eq!(std::fs::read(&target).unwrap(), [0xff, 0x00]);
        assert_eq!(std::fs::read(&path).unwrap(), b"Project instructions.\n");
    }

    #[test]
    fn project_memory_keeps_cli_folders_private_under_a_linked_home() {
        let base = folder();
        let physical = base.path().join("physical");
        let linked = base.path().join("linked");
        let home = physical.join("home");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::write(home.join(".claude/CLAUDE.md"), [0xff, 0x00]).unwrap();
        #[cfg(windows)]
        junction::create(&physical, &linked).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&physical, &linked).unwrap();
        let protected = [normal(&linked.join("home/.claude"))];
        let result = list_with_private(&linked.join("home"), &protected);
        #[cfg(windows)]
        junction::delete(&linked).unwrap();
        #[cfg(unix)]
        std::fs::remove_file(&linked).unwrap();
        assert!(result.unwrap().files.is_empty());
    }

    fn faber_notes(project: &Path) -> Result<MemoryList, &'static str> {
        scan(project, &[], true)
    }

    #[test]
    fn faber_note_targets_mark_only_files_over_their_limit() {
        let project = folder();
        std::fs::create_dir(project.path().join(".faber")).unwrap();
        let path = project.path().join(".faber/decisions.md");
        for (size, over) in [(7 * 1024, false), (8 * 1024, false), (9 * 1024, true)] {
            std::fs::write(&path, "x".repeat(size)).unwrap();
            let file = list_faber(project.path()).unwrap().files.into_iter().find(|file| file.path == ".faber/decisions.md").unwrap();
            assert_eq!(file.over_limit, over.then_some((size as u64, 8 * 1024)));
        }
        assert_eq!(note_target("sub/.faber/decisions.md"), Some(8 * 1024));
        assert_eq!(note_target("sub/decisions.md"), None);
    }

    #[test]
    fn faber_note_targets_cover_existing_files_and_rules_are_read_only() {
        let project = folder();
        std::fs::create_dir_all(project.path().join(".faber/tasks")).unwrap();
        for (name, target) in [("rules.md", 3), ("taste.md", 1), ("lessons.md", 2), ("tasks/next.md", 6)] {
            std::fs::write(project.path().join(".faber").join(name), "x".repeat(target * 1024 + 1)).unwrap();
        }
        let list = list(project.path()).unwrap();
        for (name, target) in [("rules.md", 3), ("taste.md", 1), ("lessons.md", 2), ("tasks/next.md", 6)] {
            let file = list.files.iter().find(|file| file.path == format!(".faber/{name}")).unwrap();
            assert_eq!(file.over_limit, Some(((target * 1024 + 1) as u64, (target * 1024) as u64)));
            if name == "rules.md" {
                let entry = &file.entries[0];
                let edit = Edit {
                    path: file.path.clone(),
                    revision: file.revision.clone(),
                    start: entry.start,
                    end: entry.end,
                    text: None,
                    keep: false,
                    over_target: false,
                };
                assert!(apply(project.path(), &edit).is_err());
            }
        }
    }

    #[test]
    fn the_faber_only_list_reads_the_notes_of_a_big_project_and_no_other_file() {
        let project = folder();
        let big = project.path().join("big");
        std::fs::create_dir(&big).unwrap();
        for i in 0..20_050 {
            std::fs::write(big.join(format!("f{i}.txt")), "").unwrap();
        }
        std::fs::create_dir(project.path().join(".faber")).unwrap();
        std::fs::write(project.path().join(".faber/decisions.md"), "- Use Postgres [by AI]\n").unwrap();
        // Not readable as text, and not a note: it must not be opened, let alone fail the list.
        std::fs::write(project.path().join("CLAUDE.md"), [0xff, 0x00]).unwrap();
        std::fs::write(project.path().join("AGENTS.md"), [0xff, 0x00]).unwrap();
        let list = faber_notes(project.path()).unwrap();
        assert_eq!(list.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), [".faber/decisions.md"]);
        assert!(!list.incomplete);
        assert!(list_with_private(project.path(), &[]).is_err(), "the whole walk still fails on it");
    }

    #[test]
    fn t495_legacy_faber_notes_remain_listed() {
        let project = tempfile::tempdir().unwrap();
        let dir = project.path().join(".faber");
        std::fs::create_dir(&dir).unwrap();
        for name in ["plans", "recipes", "learning", "memory"] {
            std::fs::write(dir.join(format!("{name}.md")), "- Older note\n").unwrap();
        }
        let list = list_faber(project.path()).unwrap();
        for name in ["plans", "recipes", "learning", "memory"] {
            assert!(list.files.iter().any(|file| file.path == format!(".faber/{name}.md")), "{name}");
        }
    }

    #[test]
    fn t495_old_decisions_never_enter_the_card() {
        let project = tempfile::tempdir().unwrap();
        let dir = project.path().join(".faber");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("decisions.md"), "- Use local exports. [by person]\n").unwrap();
        std::fs::write(dir.join("decisions-old.md"), "- Use remote exports. [by person]\n").unwrap();
        let kept = [KeptDecision::of("- Use local exports."), KeptDecision::of("- Use remote exports.")];
        assert_eq!(kept_for_card(project.path(), &kept).unwrap(), ["- Use local exports."]);
    }

    #[test]
    fn the_faber_only_list_skips_a_note_it_cannot_read_and_says_the_list_may_be_incomplete() {
        let project = folder();
        std::fs::create_dir_all(project.path().join(".faber/plans")).unwrap();
        std::fs::write(project.path().join(".faber/taste.md"), [0xff, 0x00]).unwrap();
        std::fs::write(project.path().join(".faber/plans/one.md"), "- A plan\n").unwrap();
        let list = faber_notes(project.path()).unwrap();
        assert_eq!(list.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), [".faber/plans/one.md"]);
        assert!(list.incomplete);
        let none = folder();
        assert!(faber_notes(none.path()).unwrap().files.is_empty(), "no .faber is no notes");
    }

    #[test]
    fn the_faber_only_list_returns_nothing_from_outside_through_a_link() {
        let base = folder();
        let outside = base.path().join("outside");
        let project = base.path().join("project");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(project.join(".faber")).unwrap();
        std::fs::write(outside.join("decisions.md"), "- Secret [by AI]\n").unwrap();
        // A linked `.faber`.
        let linked = base.path().join("linked");
        std::fs::create_dir_all(&linked).unwrap();
        #[cfg(windows)]
        junction::create(&outside, linked.join(".faber")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, linked.join(".faber")).unwrap();
        let through_dir = faber_notes(&linked);
        #[cfg(windows)]
        junction::delete(linked.join(".faber")).unwrap();
        #[cfg(unix)]
        std::fs::remove_file(linked.join(".faber")).unwrap();
        assert!(through_dir.unwrap().files.is_empty());
        // A linked folder or file inside it.
        #[cfg(windows)]
        junction::create(&outside, project.join(".faber/decisions")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, project.join(".faber/decisions")).unwrap();
            std::os::unix::fs::symlink(outside.join("decisions.md"), project.join(".faber/taste.md")).unwrap();
        }
        let inside = faber_notes(&project);
        #[cfg(windows)]
        junction::delete(project.join(".faber/decisions")).unwrap();
        assert!(inside.unwrap().files.is_empty());
    }

    #[test]
    fn project_memory_crash_at_each_save_stage_keeps_old_or_new_under_its_name() {
        for crash in [SaveStage::Created, SaveStage::Prepared, SaveStage::Checked, SaveStage::Installed] {
            let project = folder();
            let path = project.path().join("CLAUDE.md");
            std::fs::write(&path, "Original.\n").unwrap();
            let dir = Directory::open(project.path(), Path::new("")).unwrap();
            let permissions = std::fs::metadata(&path).unwrap().permissions();
            let mut reached = false;
            let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                dir.replace_staged("CLAUDE.md", b"Replacement.\n", permissions, &revision(b"Original.\n"), |stage| {
                    if stage == crash {
                        reached = true;
                        panic!("simulated interruption without save recovery");
                    }
                })
            }));
            assert!(reached && stopped.is_err());
            let bytes = std::fs::read(&path).expect("the instruction file name must never disappear");
            assert!(bytes == b"Original.\n" || bytes == b"Replacement.\n");
        }
    }

    #[cfg(windows)]
    #[test]
    fn project_memory_review_keeps_the_final_revision_guard_through_replacement() {
        let project = folder();
        let path = project.path().join("CLAUDE.md");
        std::fs::write(&path, "Original.\n").unwrap();
        let dir = Directory::open(project.path(), Path::new("")).unwrap();
        let permissions = std::fs::metadata(&path).unwrap().permissions();
        let mut wrote = false;
        let result = dir.replace_checked("CLAUDE.md", b"Replacement.\n", permissions, &revision(b"Original.\n"), || {
            wrote = std::fs::write(&path, "Another program.\n").is_ok();
        });
        assert!(!wrote, "Windows must refuse an in-place writer during the final check and replacement");
        result.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"Replacement.\n");
        assert_eq!(dir.names().unwrap(), ["CLAUDE.md"]);
    }

    #[test]
    fn project_memory_never_opens_a_home_or_relocated_cli_folder_below_the_project() {
        let project = folder();
        let home = project.path().join("person");
        let moved = project.path().join("profiles/agent");
        let forbidden = [home.join(".claude"), home.join(".codex"), moved];
        for dir in &forbidden {
            std::fs::create_dir_all(dir).unwrap();
            for name in ["CLAUDE.md", "AGENTS.md"] {
                std::fs::write(dir.join(name), [0xff, 0x00]).unwrap();
            }
        }
        let protected: Vec<_> = forbidden.iter().map(|p| normal(&p.canonicalize().unwrap())).collect();
        assert!(list_with_private(project.path(), &protected).unwrap().files.is_empty());
        for dir in forbidden {
            assert_eq!(list_with_private(&dir, &protected).unwrap_err(), REFUSED);
            let dotted = dir.parent().unwrap().join(".").join(dir.file_name().unwrap());
            assert_eq!(list_with_private(&dotted, &protected).unwrap_err(), REFUSED);
        }
    }

    #[test]
    fn project_memory_rechecks_the_revision_after_preparing_the_temporary_file() {
        let project = folder();
        let project = project.path().canonicalize().unwrap();
        std::fs::write(project.join("AGENTS.md"), "Original.\n").unwrap();
        let expected = revision(b"Original.\n");
        let dir = Directory::open(&project, Path::new("")).unwrap();
        let permissions = dir.file("AGENTS.md").unwrap().metadata().unwrap().permissions();
        std::fs::write(project.join("AGENTS.md"), "Changed.\n").unwrap();
        assert_eq!(
            dir.replace("AGENTS.md", b"Replacement.\n", permissions, &expected).unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(project.join("AGENTS.md")).unwrap(), b"Changed.\n");
        assert_eq!(dir.names().unwrap(), ["AGENTS.md"]);
    }

    const DECISIONS: &str = "- Use SQLite for the cache. [by AI]\n- Keep the old export, it is\n  what customers use. [By Person]\n- Ship on Fridays\n- Drop the beta flag [by AI, reviewed]\n- Mixed [by robot]\n- Use [by AI] marks mid-line\n- Odd [by person, reviewed]\n";

    fn listed(project: &Path, path: &str, kept: &[KeptDecision]) -> MemoryFile {
        let mut list = list(project).unwrap();
        mark_kept(&mut list, kept);
        list.files.into_iter().find(|f| f.path == path).unwrap()
    }

    fn decisions_file(text: &str) -> (tempfile::TempDir, MemoryFile) {
        let project = folder();
        std::fs::create_dir(project.path().join(".faber")).unwrap();
        std::fs::write(project.path().join(".faber/decisions.md"), text).unwrap();
        std::fs::write(project.path().join("CLAUDE.md"), "- Note [by AI]\n").unwrap();
        let file = listed(project.path(), ".faber/decisions.md", &[]);
        (project, file)
    }

    fn keep(project: &Path, file: &MemoryFile, i: usize) -> Result<Option<KeptDecision>, &'static str> {
        apply(
            project,
            &Edit {
                path: file.path.clone(),
                revision: file.revision.clone(),
                start: file.entries[i].start,
                end: file.entries[i].end,
                text: None,
                keep: true,
                over_target: false,
            },
        )
    }

    #[test]
    fn old_decisions_are_listed_but_not_loaded_into_the_card() {
        let (project, file) = decisions_file("- Keep exports [by AI]\n");
        let record = keep(project.path(), &file, 0).unwrap().unwrap();
        std::fs::write(project.path().join(".faber/decisions-old.md"), "- Keep exports [by AI]\n").unwrap();
        std::fs::write(project.path().join(".faber/decisions.md"), "").unwrap();
        assert!(listed(project.path(), ".faber/decisions-old.md", std::slice::from_ref(&record)).entries[0].reviewed);
        assert!(kept_for_card(project.path(), &[record]).unwrap().is_empty());
    }

    #[test]
    fn project_memory_kept_item_records_bind_identity_and_full_content() {
        let (project, file) = decisions_file("- Keep exports\n  for older customers. [by AI]\n");
        let record = keep(project.path(), &file, 0).unwrap().unwrap();
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["id"], revision(b"Keep exports"));
        assert_eq!(value["hash"], revision(b"- Keep exports\n  for older customers."));
        assert_eq!(
            KeptDecision::of("*  Keep\t exports"),
            KeptDecision::Item { id: revision(b"Keep exports"), hash: revision(b"*  Keep\t exports"), files: vec![], stale: false }
        );
        let wrong_id =
            KeptDecision::Item { id: revision(b"another item"), hash: value["hash"].as_str().unwrap().into(), files: vec![], stale: false };
        assert!(!listed(project.path(), ".faber/decisions.md", &[wrong_id]).entries[0].reviewed);
        assert!(listed(project.path(), ".faber/decisions.md", std::slice::from_ref(&record)).entries[0].reviewed);
        std::fs::write(project.path().join(".faber/decisions.md"), "- Keep exports\n  but remove old customers. [by person]\n").unwrap();
        let changed = listed(project.path(), ".faber/decisions.md", &[record]);
        assert_eq!((changed.entries[0].author, changed.entries[0].reviewed), (Some("ai"), false));
    }

    #[test]
    fn project_memory_kept_file_references_preserve_old_records_and_validate_every_recorded_path() {
        let body = "- Preserve `src/lib.rs`, `src\\lib.rs`, `cargo test`, `setting.name` and `.env`.";
        let (project, file) = decisions_file(&format!("{body} [by AI]\n"));
        std::fs::create_dir(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/lib.rs"), "public API").unwrap();
        std::fs::write(project.path().join(".env"), "synthetic fixture, never read").unwrap();
        let record = keep(project.path(), &file, 0).unwrap().unwrap();
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["files"], serde_json::json!(["src/lib.rs"]));
        let old = KeptDecision::of(body);
        assert!(record.matches(&old));
        assert_ne!(record, old, "CAS compares metadata too");
        assert!(listed(project.path(), ".faber/decisions.md", std::slice::from_ref(&record)).entries[0].reviewed);
        assert_eq!(kept_for_card(project.path(), std::slice::from_ref(&record)).unwrap(), [body]);
        for path in ["../outside", "src/../src/lib.rs", ".env", "C:/outside", "/outside", "src/lib.rs:stream"] {
            let mut corrupt = json.clone();
            corrupt["files"] = serde_json::json!([path]);
            let corrupt: KeptDecision = serde_json::from_value(corrupt).unwrap();
            assert!(kept_for_card(project.path(), &[old.clone(), corrupt]).unwrap().is_empty(), "{path}");
        }
        let legacy = KeptDecision::Legacy(revision(body.as_bytes())[..32].into());
        std::fs::remove_file(project.path().join("src/lib.rs")).unwrap();
        assert!(kept_for_card(project.path(), &[legacy.clone(), record.clone()]).unwrap().is_empty());
        assert_eq!(kept_for_card(project.path(), &[old.clone(), legacy.clone()]).unwrap(), [body]);
        assert_eq!(upgrade_kept(&list(project.path()).unwrap(), &[legacy]), [old]);
        let roundtrip: KeptDecision = serde_json::from_value(json).unwrap();
        assert_eq!(roundtrip, record);
    }

    #[test]
    fn project_memory_kept_file_references_bind_person_edits_to_saved_crlf_words() {
        let (project, file) = decisions_file("- Old decision. [by AI]\r\n");
        std::fs::create_dir(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/lib.rs"), "public API").unwrap();
        let entry = &file.entries[0];
        let record = apply(
            project.path(),
            &Edit {
                path: file.path.clone(),
                revision: file.revision.clone(),
                start: entry.start,
                end: entry.end,
                text: Some("- Preserve `src/lib.rs`\n  and the public API. [by person]".into()),
                keep: false,
                over_target: false,
            },
        )
        .unwrap()
        .unwrap();
        let body = "- Preserve `src/lib.rs`\r\n  and the public API.";
        assert_eq!(kept_for_card(project.path(), std::slice::from_ref(&record)).unwrap(), [body]);
        assert_eq!(serde_json::to_value(&record).unwrap()["files"], serde_json::json!(["src/lib.rs"]));
        std::fs::remove_file(project.path().join("src/lib.rs")).unwrap();
        assert!(kept_for_card(project.path(), &[record]).unwrap().is_empty());
    }

    #[test]
    fn project_memory_kept_file_references_refuse_hardlinks_without_reading_contents() {
        let body = "- Preserve `src/lib.rs`.";
        let (project, file) = decisions_file(&format!("{body} [by AI]\n"));
        std::fs::create_dir(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/lib.rs"), "original fixture").unwrap();
        let record = keep(project.path(), &file, 0).unwrap().unwrap();
        let outside = folder();
        std::fs::write(outside.path().join("private"), "synthetic fixture, never read").unwrap();
        std::fs::remove_file(project.path().join("src/lib.rs")).unwrap();
        std::fs::hard_link(outside.path().join("private"), project.path().join("src/lib.rs")).unwrap();
        assert!(kept_for_card(project.path(), &[record]).unwrap().is_empty());
        let recaptured = keep(project.path(), &file, 0).unwrap().unwrap();
        assert!(serde_json::to_value(recaptured).unwrap().get("files").is_none());
    }

    #[test]
    fn project_memory_reads_the_author_of_each_decision_and_never_guesses_one() {
        let (project, file) = decisions_file(DECISIONS);
        let seen: Vec<_> = file.entries.iter().map(|e| (e.author, e.reviewed, e.body.as_deref())).collect();
        assert_eq!(
            seen,
            [
                (Some("ai"), false, Some("- Use SQLite for the cache.")),
                // A person's mark with no record of the person is an agent's claim: an AI decision not yet reviewed.
                (Some("ai"), false, Some("- Keep the old export, it is\n  what customers use.")),
                (None, false, None),
                // A review written in the file is no review: an agent could write it.
                (Some("ai"), false, Some("- Drop the beta flag")),
                (None, false, None),
                (None, false, None),
                (None, false, None),
            ],
            "a missing or unknown mark is an unknown author, never an AI decision"
        );
        assert_eq!(file.entries[0].text, "- Use SQLite for the cache. [by AI]", "the stored words stay whole");
        let other = listed(project.path(), "CLAUDE.md", &[]);
        assert_eq!((other.entries[0].author, other.entries[0].body.as_deref()), (None, None), "only the decision list has authors");
    }

    #[test]
    fn project_memory_keep_stores_the_hash_outside_the_file_and_binds_it_to_the_words() {
        let (project, file) = decisions_file(DECISIONS);
        for refused in [2, 4, 5, 6] {
            assert_eq!(keep(project.path(), &file, refused), Err(REFUSED), "only a marked decision can be kept (entry {refused})");
        }
        let hash = keep(project.path(), &file, 0).unwrap().expect("a kept decision returns its hash");
        assert_eq!(
            std::fs::read_to_string(project.path().join(".faber/decisions.md")).unwrap(),
            DECISIONS,
            "nothing is written to the project"
        );
        assert!(!listed(project.path(), ".faber/decisions.md", &[]).entries[0].reviewed, "nothing stored, nothing kept");
        let again = listed(project.path(), ".faber/decisions.md", std::slice::from_ref(&hash));
        assert_eq!((again.entries[0].author, again.entries[0].reviewed), (Some("ai"), true));
        assert_eq!(again.entries.iter().filter(|e| e.reviewed).count(), 1, "the other decisions stay as they were");
        // Keeping an entry marked as the person's makes it theirs; a legacy "[by AI, reviewed]" is plain AI.
        let person = keep(project.path(), &file, 1).unwrap().unwrap();
        let theirs = &listed(project.path(), ".faber/decisions.md", &[person]).entries[1];
        assert_eq!((theirs.author, theirs.reviewed), (Some("person"), false));
        let legacy = keep(project.path(), &file, 3).unwrap().unwrap();
        assert!(listed(project.path(), ".faber/decisions.md", &[legacy]).entries[3].reviewed);
        // The agent rewords the kept decision: the stored hash no longer matches, so it reads "not reviewed" again.
        let reworded = DECISIONS.replace("Use SQLite for the cache.", "Use Postgres for the cache.");
        std::fs::write(project.path().join(".faber/decisions.md"), reworded).unwrap();
        let after = listed(project.path(), ".faber/decisions.md", std::slice::from_ref(&hash));
        assert_eq!((after.entries[0].author, after.entries[0].reviewed), (Some("ai"), false));
        let note = listed(project.path(), "CLAUDE.md", &[]);
        let attempt = Edit {
            path: note.path.clone(),
            revision: note.revision.clone(),
            start: 0,
            end: note.entries[0].end,
            text: None,
            keep: true,
            over_target: false,
        };
        assert_eq!(apply(project.path(), &attempt), Err(REFUSED), "keeping is for the decision list only");
    }

    #[test]
    fn project_memory_words_the_person_writes_and_marks_theirs_are_recorded_as_theirs() {
        let (project, file) = decisions_file(DECISIONS);
        let edit = |text: &str, file: &MemoryFile| {
            let entry = &file.entries[0];
            let edit = Edit {
                path: file.path.clone(),
                revision: file.revision.clone(),
                start: entry.start,
                end: entry.end,
                text: Some(text.into()),
                keep: false,
                over_target: false,
            };
            apply(project.path(), &edit).unwrap()
        };
        assert_eq!(edit("- Use SQLite for the cache, always. [by AI]", &file), None, "an edit that leaves it the AI's records nothing");
        let file = listed(project.path(), ".faber/decisions.md", &[]);
        let hash = edit("- Use Postgres for the cache. [by person]", &file).expect("the person's own words are recorded");
        assert_eq!(listed(project.path(), ".faber/decisions.md", &[hash]).entries[0].author, Some("person"));
    }

    #[test]
    fn project_memory_keep_refuses_a_stale_file_and_reads_crlf_entries_the_same() {
        let (project, file) = decisions_file("- Use SQLite. [by AI]\r\n- Second [by AI]\r\n");
        let hash = keep(project.path(), &file, 0).unwrap().unwrap();
        let crlf = listed(project.path(), ".faber/decisions.md", std::slice::from_ref(&hash));
        assert!(crlf.entries[0].reviewed && !crlf.entries[1].reviewed);
        std::fs::write(project.path().join(".faber/decisions.md"), "- Use SQLite. [by AI]\r\n- Second [by AI]\r\n- Third\r\n").unwrap();
        assert_eq!(keep(project.path(), &file, 1), Err(CHANGED), "the person never keeps words they did not see");
    }
}
