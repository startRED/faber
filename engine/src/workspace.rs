//! Each chat conversation has a private folder where the model runs code and creates files.

#[cfg(windows)]
pub mod relocation;

use crate::model::FileInfo;
use base64::Engine as _;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Keeps the Windows data-directory chain open through startup and engine shutdown.
/// This rejects existing links; it does not replace handle-relative I/O or a write sandbox.
pub struct DataDirectory {
    #[cfg(windows)]
    _directory: faber_winproc::MemoryDirectory,
}

impl DataDirectory {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        #[cfg(windows)]
        let directory = faber_winproc::MemoryDirectory::open_or_create(path)?;
        #[cfg(not(windows))]
        fs::create_dir_all(path)?;
        Ok(Self {
            #[cfg(windows)]
            _directory: directory,
        })
    }
}

/// A file the user attached, as it comes from the screen.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Attachment {
    pub name: String,
    pub base64: String,
}

/// Writes attached files into `dir` under safe, unique names and returns the full paths.
pub fn save_files(root: &Path, dir: &Path, files: &[Attachment]) -> Vec<PathBuf> {
    // Host-generated paths may have a trusted Windows 8.3 root (RUNNER~1). Check only the relative
    // operand, preserving the shared policy's traversal, junction and protected-file guards.
    let writable = |path: &Path| {
        path.strip_prefix(root).is_ok_and(|relative| {
            crate::permissions::allows(root, "Write", &serde_json::json!({ "file_path": relative.to_string_lossy() }))
        })
    };
    if !writable(dir) || fs::create_dir_all(dir).is_err() {
        return vec![];
    }
    files
        .iter()
        .filter_map(|file| {
            let name = file.name.replace('\\', "/");
            let name = name.rsplit('/').next().unwrap_or_default();
            let mut clean: String = name.chars().map(|c| if (c as u32) < 32 || "<>:\"|?*".contains(c) { '_' } else { c }).collect();
            if clean.is_empty() || clean == "." || clean == ".." {
                clean = "file".into();
            }
            let (stem, ext) = match clean.rfind('.') {
                Some(i) if i > 0 => (clean[..i].to_string(), clean[i..].to_string()),
                _ => (clean.clone(), String::new()),
            };
            let mut path = dir.join(&clean);
            let mut n = 1;
            if !writable(&path) {
                return None;
            }
            while path.exists() {
                path = dir.join(format!("{stem}-{n}{ext}"));
                n += 1;
                if !writable(&path) {
                    return None;
                }
            }
            let bytes = base64::engine::general_purpose::STANDARD.decode(&file.base64).ok()?;
            fs::write(&path, bytes).ok()?;
            Some(path)
        })
        .collect()
}

const MAX_FILES: usize = 50;

/// Files created or changed in a chat workspace since `since` (ms), as paths relative to it with forward slashes.
/// What the user attached is not something the answer created, so `attachments/` is skipped.
pub fn new_files(dir: &Path, since: u64) -> Vec<FileInfo> {
    let mut found = Vec::new();
    walk(dir, dir, since, &mut found);
    found
}

fn walk(root: &Path, folder: &Path, since: u64, found: &mut Vec<FileInfo>) {
    let Ok(entries) = fs::read_dir(folder) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if found.len() >= MAX_FILES {
            return;
        }
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
        if matches!(entry.file_name().to_str(), Some(".git" | ".faber"))
            || !crate::permissions::allows(root, "Read", &serde_json::json!({ "file_path": rel }))
            || entry.file_type().is_ok_and(|kind| kind.is_symlink())
        {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            if !(folder == root && entry.file_name() == "attachments") {
                walk(root, &path, since, found);
            }
        } else if meta.is_file() {
            let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64);
            // one second of slack for file systems that round modification times
            if modified + 1000 >= since {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                let rel = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
                found.push(FileInfo { path: rel, size: meta.len() });
            }
        }
    }
}

/// Resolves a path the screen asks for, refusing anything outside the folder.
pub fn inside_folder(dir: &Path, path: &str) -> Option<PathBuf> {
    let candidate = Path::new(path);
    if candidate.is_absolute() || candidate.has_root() {
        return None;
    }
    let mut full = dir.to_path_buf();
    let mut depth = 0usize;
    for part in candidate.components() {
        match part {
            Component::Normal(p) => {
                full.push(p);
                depth += 1;
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    (depth > 0 && crate::permissions::allows(dir, "Read", &serde_json::json!({ "file_path": path }))).then_some(full)
}
