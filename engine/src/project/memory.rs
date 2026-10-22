//! Project-local instruction entries. No CLI profile, import, or link inside a project is followed.
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

pub const MAX_BYTES: usize = 256 * 1024;
const MAX_ENTRIES: usize = 4096;
const READ: &str = "error.memory.read";
const CHANGED: &str = "error.memory.changed";
const REFUSED: &str = "error.memory.refused";
/// Soft targets of the project's own notes (own work method, "Staying small and current"). Over one, Memory says so and
/// the next close asks the agent to fold the file; Faber itself never cuts an item.
pub const TARGETS: [(&str, usize); 2] = [(".faber/decisions.md", 8 * 1024), (".faber/taste.md", 1024)];
/// The note kinds read under `.faber/`: today's, `decisions-old` (overridden decisions, searched and never loaded), and the
/// legacy `plans`, `recipes`, `learning` and `memory`, still read for folders that have them.
const KINDS: [&str; 7] = ["decisions", "decisions-old", "taste", "plans", "recipes", "learning", "memory"];
/// The file extensions a backticked name with no folder needs to be judged a path (`.js` is left out: `Next.js` is a name).
const EXTENSIONS: [&str; 30] = [
    "md", "rs", "ts", "tsx", "jsx", "mjs", "cjs", "json", "toml", "yaml", "yml", "py", "go", "java", "kt", "swift", "c", "h", "cpp", "cs",
    "rb", "php", "html", "css", "scss", "sql", "sh", "ps1", "txt", "lock",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFile {
    pub path: String,
    pub source: &'static str,
    pub revision: String,
    pub too_large: bool,
    /// Over its soft target (`TARGETS`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub over: bool,
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
    /// (an agent could write it): `mark_kept` sets it from what the person kept, which Faber stores outside the project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<&'static str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reviewed: bool,
    /// A note of `.faber/` naming a project file that is gone (`mark_stale`): `(stale?)` in Memory, and out of the card.
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
    /// Keeps an AI decision: nothing is written to the project, `apply` returns the hash of its words to store (`digest`).
    #[serde(default)]
    pub keep: bool,
}

fn source(path: &str) -> Option<&'static str> {
    let parts: Vec<_> = path.split('/').collect();
    if parts.iter().any(|p| p.is_empty() || matches!(*p, "." | ".." | ".git" | ".codex") || p.contains(['\\', ':', '\0'])) {
        return None;
    }
    let name = *parts.last()?;
    if let Some(i) = parts.iter().position(|p| *p == ".faber") {
        let kind = parts.get(i + 1)?;
        return (name.ends_with(".md") && KINDS.contains(&kind.trim_end_matches(".md"))).then_some("faber");
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

/// `.faber/decisions.md` (or a file under `.faber/decisions/`) and `.faber/decisions-old.md`: the lists whose entries carry
/// an author.
fn decisions(path: &str) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    parts
        .iter()
        .position(|p| *p == ".faber")
        .and_then(|i| parts.get(i + 1))
        .is_some_and(|kind| matches!(kind.trim_end_matches(".md"), "decisions" | "decisions-old"))
}

/// The backticked project-relative file paths in a note. Only these are judged: a span with a space, a quote, `=`, `:`, a
/// wildcard, `..` or a leading `/`, `~` or `-` is prose, a command or a setting, and so is a name whose last part has no
/// known file extension (`owner/repo`, `editor.fontSize`).
fn paths(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').skip(1).step_by(2).map(|span| span.strip_prefix("./").unwrap_or(span)).filter(|span| {
        let parts: Vec<_> = span.split('/').collect();
        let ext = parts.last().and_then(|name| name.rsplit_once('.'));
        (1..=200).contains(&span.len())
            && !span.starts_with(['/', '~', '-'])
            && !span.chars().any(|c| c.is_whitespace() || "\\\"'=:*?{}<>|$`".contains(c))
            && parts.iter().all(|p| !p.is_empty() && *p != "..")
            && ext.is_some_and(|(stem, ext)| !stem.is_empty() && (EXTENSIONS.contains(&ext) || parts.len() > 1 && ext == "js"))
    })
}

/// A project folder held open to judge its notes: each path is opened through it without following a link, so a link
/// planted in the project never makes Faber look outside it.
pub struct ProjectFiles {
    root: Directory,
    /// What each folder listed so far holds, by relative path. Notes name the same few folders, and one listing answers
    /// for all their files (opening each file costs about 75 µs on Windows).
    folders: std::cell::RefCell<std::collections::HashMap<String, Listing>>,
}

enum Listing {
    Names(std::collections::HashSet<String>),
    /// The folder is not found: every file named in it is gone.
    Gone,
    /// A link, refused, unreadable or over 20,000 names: nothing named in it is judged.
    Unknown,
}

/// A file name as the file system compares it: Windows folders ignore case.
fn compared(name: &str) -> String {
    if cfg!(windows) { name.to_lowercase() } else { name.to_string() }
}

impl ProjectFiles {
    pub fn open(project: &Path) -> Option<Self> {
        let (_, root) = project_directory(project, &resolved_private(&private_roots())).ok()?;
        Some(Self { root, folders: Default::default() })
    }

    fn file(&self, path: &str) -> std::io::Result<File> {
        match path.rsplit_once('/') {
            Some((parent, name)) => self.root.under(Path::new(parent))?.file(name),
            None => self.root.file(path),
        }
    }

    /// Whether a project file is known to be gone: its folder is not found, or listing it lacks the name. A folder that is
    /// a link, refused, unreadable or too large to list (20,000 names) answers "not gone".
    fn gone(&self, path: &str) -> bool {
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        let mut folders = self.folders.borrow_mut();
        // shortcut: at most 256 folder listings are kept; past that a path's note is not judged, until a later read.
        if folders.len() >= 256 && !folders.contains_key(parent) {
            return false;
        }
        let names = folders.entry(parent.to_string()).or_insert_with(|| {
            let listed = if parent.is_empty() { self.root.names() } else { self.root.under(Path::new(parent)).and_then(|d| d.names()) };
            match listed {
                Ok(names) if names.len() <= 20_000 => Listing::Names(names.iter().map(|n| compared(n)).collect()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Listing::Gone,
                _ => Listing::Unknown,
            }
        });
        match names {
            Listing::Names(names) => !names.contains(&compared(name)),
            Listing::Gone => true,
            Listing::Unknown => false,
        }
    }

    /// Whether a note names a project file that no longer exists. Only "not found" counts: a link, a refused or an
    /// unreadable folder keeps the note in force, and nothing is opened through a link. Checked when read, never written
    /// into the note, so a kept decision keeps its record and a file that comes back clears it.
    pub fn stale(&self, text: &str) -> bool {
        paths(text).any(|path| self.gone(path))
    }

    /// The size of a file of the project, or nothing for a missing file, a link or a folder.
    pub fn size(&self, path: &str) -> Option<u64> {
        self.file(path).ok()?.metadata().ok().map(|m| m.len())
    }
}

/// Marks the notes of the project's root `.faber/` that name a gone file.
pub fn mark_stale(list: &mut MemoryList, project: &Path) {
    let Some(files) = ProjectFiles::open(project) else { return };
    for file in list.files.iter_mut().filter(|f| f.path.starts_with(".faber/")) {
        for entry in &mut file.entries {
            entry.stale = files.stale(&entry.text);
        }
    }
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
        .flat_map(|home| [".claude", ".claude.json", ".codex"].map(|name| normal(&PathBuf::from(&home).join(name))));
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
    if let Ok(dir) = project_dir.under(Path::new(folder)) {
        let mut found: Vec<String> = dir.names().unwrap_or_default().into_iter().filter(|n| wanted(n)).collect();
        found.sort();
        out.extend(found.iter().take(listed).filter_map(|name| read(&dir, name, format!("{folder}/{name}"))));
    }
    Ok(out)
}

/// Writes `.faber/<name>` in the project when its bytes differ, creating the folder and the file if needed, through the
/// same no-follow handles and atomic replacement as an edit: a link planted in place of either is refused, never followed.
/// Returns whether it wrote.
pub(super) fn write_faber_file(project: &Path, name: &str, bytes: &[u8]) -> Result<bool, &'static str> {
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
    let mut current = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut current).map_err(|_| READ)?;
    if current == bytes {
        return Ok(false);
    }
    dir.replace(name, bytes, permissions, &revision(&current)).map_err(|_| "error.memory.write")?;
    Ok(true)
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

fn revision(bytes: &[u8]) -> String {
    crate::hex(&Sha256::digest(bytes))
}

/// What Faber stores when the person keeps an AI decision: a short hash of its words, so reworded words are not kept.
fn digest(body: &str) -> String {
    revision(body.trim().as_bytes())[..32].into()
}

/// Marks the decisions the person kept or wrote (`kept` is what Faber stored outside the project). A `[by person]` mark
/// with no record is an agent's claim, so it reads as an AI decision not yet reviewed, whatever the file says.
pub fn mark_kept(list: &mut MemoryList, kept: &[String]) {
    for entry in list.files.iter_mut().flat_map(|f| &mut f.entries) {
        let recorded = entry.body.as_deref().is_some_and(|b| kept.contains(&digest(b)));
        if entry.author == Some("person") && !recorded {
            entry.author = Some("ai");
        }
        entry.reviewed = entry.author == Some("ai") && recorded;
    }
}

fn entries(text: &str) -> Vec<Entry> {
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
                if ranges.len() > MAX_ENTRIES {
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
            Entry {
                start: range.start,
                end,
                line,
                text: text[range.start..content_end].into(),
                body: None,
                author: None,
                reviewed: false,
                stale: false,
            }
        })
        .collect()
}

fn read_file(dir: &Directory, name: &str, path: String, source: &'static str) -> Result<MemoryFile, &'static str> {
    let file = dir.file(name).map_err(|_| REFUSED)?;
    let mut bytes = Vec::new();
    if file.metadata().map_err(|_| READ)?.len() <= MAX_BYTES as u64 {
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| READ)?;
    } else {
        let over = TARGETS.iter().any(|(target, _)| *target == path);
        return Ok(MemoryFile { path, source, revision: String::new(), too_large: true, over, entries: vec![] });
    }
    let over = TARGETS.iter().any(|(target, cap)| *target == path && bytes.len() > *cap);
    let too_large = bytes.len() > MAX_BYTES;
    let mut entries = if too_large { vec![] } else { entries(std::str::from_utf8(&bytes).map_err(|_| READ)?) };
    if decisions(&path) {
        for entry in &mut entries {
            if let Some(m) = mark(&entry.text) {
                (entry.body, entry.author) = (Some(entry.text[..m.open].trim_end().into()), Some(m.author));
            }
        }
    }
    Ok(MemoryFile { path, source, revision: revision(&bytes), too_large, over, entries })
}

pub fn list(project: &Path) -> Result<MemoryList, &'static str> {
    list_with_private(project, &private_roots())
}

/// Only the notes in the project's root `.faber` folder, through the same no-follow handles: no other folder is walked, no
/// `CLAUDE.md` or `AGENTS.md` is read, and a note that cannot be read is skipped (the list says it may be incomplete).
pub fn list_faber(project: &Path) -> Result<MemoryList, &'static str> {
    scan(project, &private_roots(), true)
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
                let mut file = match read_file(&dir, &name, path, source) {
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
        return i + 1 == parts.len() || KINDS.contains(&parts[i + 1]);
    }
    let name = parts.last().copied().unwrap_or_default();
    !matches!(name, ".git" | ".codex" | "node_modules" | "target" | "dist" | "vendor")
}

pub fn edit(project: &Path, edit: &Edit) -> Result<(), &'static str> {
    apply(project, edit).map(drop)
}

/// Like `edit`, and for `keep` returns the hash of the kept words instead of writing anything.
pub fn apply(project: &Path, edit: &Edit) -> Result<Option<String>, &'static str> {
    edit_with_private(project, edit, &private_roots())
}

/// One writer at a time for the notes and `rules.md`.
static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn edit_with_private(project: &Path, edit: &Edit, protected: &[String]) -> Result<Option<String>, &'static str> {
    let _write = WRITES.lock().map_err(|_| "error.memory.write")?;
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
    let entry = entries(original).into_iter().find(|e| e.start == edit.start && e.end == edit.end).ok_or(REFUSED)?;
    let mut replacement = String::new();
    if edit.keep {
        let text = &original[entry.start..entry.end];
        let kept = decisions(&edit.path).then(|| mark(text)).flatten().ok_or(REFUSED)?;
        return Ok(Some(digest(&text[..kept.open])));
    }
    if let Some(text) = &edit.text {
        if text.trim().is_empty() || text.contains('\0') || text.len() > MAX_BYTES {
            return Err(REFUSED);
        }
        let old = &original[entry.start..entry.end];
        let first = old.find('\n').map(|i| &old[..=i]).or_else(|| original.find('\n').map(|i| &original[..=i]));
        let newline = if first.is_some_and(|line| line.ends_with("\r\n")) { "\r\n" } else { "\n" };
        replacement = text.replace("\r\n", "\n").replace('\n', newline);
        replacement = replacement.trim_end_matches(['\r', '\n']).into();
        if old.ends_with('\n') {
            replacement.push_str(if old.ends_with("\r\n") { "\r\n" } else { "\n" });
        }
    }
    let mut next = bytes[..entry.start].to_vec();
    next.extend_from_slice(replacement.as_bytes());
    next.extend_from_slice(&bytes[entry.end..]);
    if next.len() > MAX_BYTES {
        return Err(REFUSED);
    }
    drop(file);
    dir.replace(name, &next, permissions, expected)
        .map_err(|error| if error.kind() == std::io::ErrorKind::AlreadyExists { CHANGED } else { "error.memory.write" })?;
    // Words the person wrote themselves in Memory and marked theirs are recorded as theirs, like a Keep.
    let written = edit.text.as_deref().filter(|_| decisions(&edit.path)).map(str::trim_end);
    Ok(written.and_then(|text| mark(text).filter(|m| m.author == "person").map(|m| digest(&text[..m.open]))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> tempfile::TempDir {
        tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
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

    fn listed(project: &Path, path: &str, kept: &[String]) -> MemoryFile {
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

    fn keep(project: &Path, file: &MemoryFile, i: usize) -> Result<Option<String>, &'static str> {
        apply(
            project,
            &Edit {
                path: file.path.clone(),
                revision: file.revision.clone(),
                start: file.entries[i].start,
                end: file.entries[i].end,
                text: None,
                keep: true,
            },
        )
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
        let attempt =
            Edit { path: note.path.clone(), revision: note.revision.clone(), start: 0, end: note.entries[0].end, text: None, keep: true };
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

    #[test]
    fn faber_notes_over_their_target_old_decisions_and_legacy_names_are_read_and_gone_paths_are_stale() {
        let project = folder();
        let faber = project.path().join(".faber");
        std::fs::create_dir_all(faber.join("plans")).unwrap();
        std::fs::create_dir_all(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/kept.rs"), "").unwrap();
        let line = "- Keep `src/kept.rs` small, `npm run test`, `editor.fontSize`, `owner/repo`, `Next.js` [by AI]\n";
        std::fs::write(faber.join("decisions.md"), format!("{line}- Split `src/gone.rs` in two [by person]\n")).unwrap();
        std::fs::write(faber.join("taste.md"), "- x\n".repeat(300)).unwrap();
        std::fs::write(faber.join("decisions-old.md"), "- Use npm [by AI]\n").unwrap();
        std::fs::write(faber.join("plans/old.md"), "A plan from an older version.\n").unwrap();
        let mut list = list(project.path()).unwrap();
        mark_stale(&mut list, project.path());
        let file = |path: &str| list.files.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{path} is read"));
        let decisions = file(".faber/decisions.md");
        assert!(!decisions.over, "under 8 KB");
        assert!(!decisions.entries[0].stale, "only backticked file paths are judged, and this one exists");
        assert!(decisions.entries[1].stale, "a gone file makes the note stale");
        assert!(file(".faber/taste.md").over, "1.2 KB is over the 1 KB target");
        assert_eq!(file(".faber/decisions-old.md").entries[0].author, Some("ai"), "old decisions keep their authors");
        assert_eq!(file(".faber/plans/old.md").entries.len(), 1, "a legacy name stays readable");
        std::fs::write(faber.join("decisions.md"), line.repeat(100)).unwrap();
        assert!(list_faber(project.path()).unwrap().files.iter().any(|f| f.path == ".faber/decisions.md" && f.over), "over 8 KB");
        let files = ProjectFiles::open(project.path()).unwrap();
        assert!(!files.stale("`../outside.rs`, `/abs.rs`, `C:/x.rs`, `a b.rs`, `*.rs`, `.gitignore`"), "never judged");
        assert_eq!(files.size(".faber/decisions.md"), Some(line.len() as u64 * 100));
        // A link planted in the project is never followed: what lies behind it is neither found nor missing.
        let outside = folder();
        std::fs::write(outside.path().join("there.rs"), "").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), project.path().join("linked")).unwrap();
        #[cfg(windows)]
        junction::create(outside.path(), project.path().join("linked")).unwrap();
        assert!(!files.stale("`linked/missing.rs`") && files.size("linked/there.rs").is_none(), "nothing is probed through a link");
        assert!(!files.stale("`linked/other.rs`"), "a folder that cannot be listed judges none of its files");
        assert!(files.stale("`gone/a.rs`") && files.stale("`gone/b.rs`"), "a gone folder makes each file in it gone");
    }
}
