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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFile {
    pub path: String,
    pub source: &'static str,
    pub revision: String,
    pub too_large: bool,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct Edit {
    pub path: String,
    pub revision: String,
    pub start: usize,
    pub end: usize,
    /// None deletes the entry; Some replaces its text.
    pub text: Option<String>,
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
            && ["decisions", "taste", "plans", "recipes", "learning", "memory"].contains(&kind.trim_end_matches(".md")))
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
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
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
            Entry { start: range.start, end, line, text: text[range.start..content_end].into() }
        })
        .collect()
}

fn read_file(dir: &Directory, name: &str, path: String, source: &'static str) -> Result<MemoryFile, &'static str> {
    let file = dir.file(name).map_err(|_| REFUSED)?;
    let mut bytes = Vec::new();
    if file.metadata().map_err(|_| READ)?.len() <= MAX_BYTES as u64 {
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| READ)?;
    } else {
        return Ok(MemoryFile { path, source, revision: String::new(), too_large: true, entries: vec![] });
    }
    let too_large = bytes.len() > MAX_BYTES;
    let entries = if too_large { vec![] } else { entries(std::str::from_utf8(&bytes).map_err(|_| READ)?) };
    Ok(MemoryFile { path, source, revision: revision(&bytes), too_large, entries })
}

pub fn list(project: &Path) -> Result<MemoryList, &'static str> {
    list_with_private(project, &private_roots())
}

fn list_with_private(project: &Path, protected: &[String]) -> Result<MemoryList, &'static str> {
    let protected = resolved_private(protected);
    let (root, project_dir) = project_directory(project, &protected)?;
    let identity = project_dir.identity().map_err(|_| READ)?;
    let mut pending = vec![String::new()];
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
        let dir = project_dir.under(Path::new(&relative)).map_err(|_| REFUSED)?;
        let codex_path = if relative.is_empty() { "AGENTS.md".into() } else { format!("{relative}/AGENTS.md") };
        let codex = if source(&codex_path) == Some("codex") { codex_file(&dir, &root.join(&relative), &protected)? } else { None };
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
                let mut file = read_file(&dir, &name, path, source)?;
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
        return i + 1 == parts.len() || ["decisions", "taste", "plans", "recipes", "learning", "memory"].contains(&parts[i + 1]);
    }
    let name = parts.last().copied().unwrap_or_default();
    !matches!(name, ".git" | ".codex" | "node_modules" | "target" | "dist" | "vendor")
}

pub fn edit(project: &Path, edit: &Edit) -> Result<(), &'static str> {
    edit_with_private(project, edit, &private_roots())
}

fn edit_with_private(project: &Path, edit: &Edit, protected: &[String]) -> Result<(), &'static str> {
    static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());
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
    if permissions.readonly() {
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
        .map_err(|error| if error.kind() == std::io::ErrorKind::AlreadyExists { CHANGED } else { "error.memory.write" })
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
            &Edit { path: file.path, revision: file.revision, start: file.entries[0].start, end: file.entries[0].end, text: None },
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
}
