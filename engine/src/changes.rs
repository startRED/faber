//! A conversation's complete git changes, with bounded output and patches loaded one file at a time.
use crate::project::copy::WorkCopy;
use crate::project::git::{Executor, Invocation, valid_oid};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};
mod cache;

pub const PATCH_BYTES: usize = 400 * 1024;
pub const PATCH_LINES: usize = 4_000;
const LIST_BYTES: usize = 2 * 1024 * 1024;
const MAX_FILES: usize = 2_000;
const COUNT_BYTES: usize = 4 * 1024 * 1024;
const PATH_ARGS: usize = 12 * 1024;
pub const FILE_BYTES: u64 = 4 * 1024 * 1024;
const REQUEST_TIME: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChangeList {
    pub files: Vec<FileChange>,
    pub incomplete: bool,
    pub next: Option<String>,
}

struct Request<'a> {
    exec: &'a mut dyn Executor,
    deadline: Instant,
}
impl Request<'_> {
    fn git(&mut self, root: &Path, args: &[&str], skip: usize, limit: usize) -> Option<Vec<u8>> {
        if Instant::now() >= self.deadline {
            return None;
        }
        let mut call = Invocation::new(root, args).before(self.deadline);
        call.args.splice(0..0, ["-c".to_string(), format!("core.worktree={}", crate::project::git::native_path(root))]);
        let out = self.exec.execute_window(&call, skip, limit).ok()?;
        out.success.then_some(out.stdout)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub added: u64,
    pub removed: u64,
    pub binary: bool,
    /// False for binary files or a new file that could not be counted within the read budget.
    pub counts_known: bool,
    pub too_large: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChangePatch {
    pub text: String,
    pub too_large: bool,
    pub binary: bool,
    pub error: Option<&'static str>,
}

impl ChangePatch {
    pub(crate) fn failed() -> Self {
        Self { error: Some("error.changes.read"), ..Self::default() }
    }
}

fn line(exec: &mut Request<'_>, root: &Path, args: &[&str]) -> Option<String> {
    let bytes = exec.git(root, args, 0, 256)?;
    (bytes.len() <= 256).then(|| String::from_utf8_lossy(&bytes).trim().to_string())
}

/// Same base candidates as verify, with HEAD as the fallback when no branch tells what was committed.
fn base(exec: &mut Request<'_>, copy: &WorkCopy) -> Option<String> {
    let root = Path::new(&copy.path);
    let head = line(exec, root, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]).filter(|s| valid_oid(s))?;
    for candidate in
        copy.base.as_deref().into_iter().chain(["refs/remotes/origin/HEAD", "@{upstream}", "refs/heads/main", "refs/heads/master"])
    {
        if let Some(found) = line(exec, root, &["merge-base", "HEAD", candidate]).filter(|s| valid_oid(s)) {
            return Some(found);
        }
    }
    Some(head)
}

pub(crate) fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|part| !part.is_empty() && !matches!(part, "." | "..") && !part.eq_ignore_ascii_case(".git"))
        && !crate::permissions::secret(Path::new(path), true)
        && !crate::verify::own_log(path)
}

/// `--raw --numstat -z --no-renames`: raw records carry status, numstat records carry counts and unquoted paths.
fn numstat(bytes: &[u8]) -> Vec<FileChange> {
    let mut records = bytes.split(|b| *b == 0).filter(|r| !r.is_empty());
    let mut statuses = BTreeMap::new();
    let mut files = vec![];
    while let Some(record) = records.next() {
        if record.starts_with(b":") {
            let Some(path) = records.next().and_then(|r| std::str::from_utf8(r).ok()) else { break };
            let status = match record.last() {
                Some(b'A') => "added",
                Some(b'D') => "deleted",
                _ => "modified",
            };
            statuses.insert(path.to_string(), status);
            continue;
        }
        let mut fields = record.splitn(3, |b| *b == b'\t');
        let (Some(added), Some(removed), Some(path)) = (fields.next(), fields.next(), fields.next()) else { continue };
        let Ok(path) = std::str::from_utf8(path) else { continue };
        if !safe(path) {
            continue;
        }
        let binary = added == b"-" || removed == b"-";
        let count = |s| std::str::from_utf8(s).ok().and_then(|s| s.trim().parse::<u64>().ok());
        let (added, removed) = if binary {
            (0, 0)
        } else {
            let (Some(a), Some(r)) = (count(added), count(removed)) else { continue };
            (a, r)
        };
        files.push(FileChange {
            path: path.to_string(),
            added,
            removed,
            binary,
            counts_known: !binary,
            too_large: false,
            status: statuses.get(path).unwrap_or(&"modified").to_string(),
        });
        if files.len() >= MAX_FILES {
            break;
        }
    }
    files
}

/// Only complete records become cursors; a cap never turns a partial listing into an empty one.
fn page(bytes: &[u8], cap: usize) -> (Vec<&[u8]>, usize) {
    let mut used = 0;
    let records = bytes
        .split_inclusive(|b| *b == 0)
        .filter(|r| r.last() == Some(&0))
        .take(cap)
        .map(|r| {
            used += r.len();
            &r[..r.len() - 1]
        })
        .collect();
    (records, used)
}

fn entry(record: &[u8], source: usize) -> Option<(String, Option<u64>)> {
    if source == 2 {
        return Some((std::str::from_utf8(record).ok()?.to_string(), None));
    }
    let (meta, path) = record.split_at(record.iter().position(|b| *b == b'\t')?);
    let fields: Vec<_> = std::str::from_utf8(meta).ok()?.split_whitespace().collect();
    if !matches!(*fields.first()?, "100644" | "100755") {
        return None;
    }
    if (source == 0 && (fields.len() != 3 || fields[2] != "0")) || (source == 1 && (fields.len() != 4 || fields[1] != "blob")) {
        return None;
    }
    let size = if source == 1 { Some(fields.get(3)?.parse().ok()?) } else { None };
    Some((std::str::from_utf8(&path[1..]).ok()?.to_string(), size))
}

fn exact_file(bytes: &[u8], path: &str, source: usize) -> bool {
    let prefix = format!("{path}/");
    bytes.split(|b| *b == 0).filter(|r| !r.is_empty()).all(|r| {
        let Some(name) = r.splitn(2, |b| *b == b'\t').nth(1) else { return false };
        !name.starts_with(prefix.as_bytes()) && (name != path.as_bytes() || entry(r, source).is_some_and(|(p, _)| p == path))
    })
}

fn plain_file(root: &Path, path: &str) -> bool {
    if !safe(path) || !plain_parents(root, path) {
        return false;
    }
    match std::fs::symlink_metadata(root.join(path)) {
        Ok(meta) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return false;
                }
            }
            meta.is_file() && !meta.file_type().is_symlink()
        }
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

fn disk_size(root: &Path, path: &str) -> Option<u64> {
    match std::fs::symlink_metadata(root.join(path)) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Some(meta.len()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(0),
        _ => None,
    }
}
/// A tracked path may have been replaced by a directory link; do not let git read through it either.
fn plain_parents(root: &Path, path: &str) -> bool {
    let mut full = root.to_path_buf();
    let parts: Vec<_> = path.split('/').collect();
    for part in &parts[..parts.len().saturating_sub(1)] {
        full.push(part);
        let meta = match std::fs::symlink_metadata(&full) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return true,
            Err(_) => return false,
        };
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return false;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return false;
            }
        }
    }
    true
}

fn listed(exec: &mut Request<'_>, copy: &WorkCopy, from: &str, cursor: Option<&str>) -> ChangeList {
    let root = Path::new(&copy.path);
    let (mut source, mut offset) = cursor
        .and_then(|s| s.split_once(':'))
        .and_then(|(s, o)| Some((s.parse::<usize>().ok()?, o.parse::<usize>().ok()?)))
        .filter(|(s, _)| *s <= 2)
        .unwrap_or((0, 0));
    let mut result = ChangeList::default();
    let mut remaining = MAX_FILES;
    let mut budget = COUNT_BYTES as u64;
    while source < 3 {
        let args = match source {
            0 => vec!["ls-files", "--stage", "-z", "--full-name"],
            1 => vec!["ls-tree", "-r", "-l", "-z", from],
            _ => vec!["ls-files", "--others", "--exclude-standard", "-z", "--full-name"],
        };
        let Some(bytes) = exec.git(root, &args, offset, LIST_BYTES) else {
            result.incomplete = true;
            result.next = Some(format!("{source}:{offset}"));
            break;
        };
        let (records, used) = page(&bytes[..bytes.len().min(LIST_BYTES)], remaining);
        let count = records.len();
        remaining -= count;
        let mut record_end = 0;
        let mut ends = BTreeMap::new();
        let names: Vec<_> = records
            .into_iter()
            .filter_map(|r| {
                record_end += r.len() + 1;
                let (path, size) = entry(r, source)?;
                if !safe(&path) {
                    return None;
                }
                ends.insert(path.clone(), record_end);
                Some((path, size))
            })
            .collect();
        let mut processed = 0;
        if source == 2 {
            for (path, _) in names {
                if Instant::now() >= exec.deadline {
                    result.incomplete = true;
                    break;
                }
                let limit = PATCH_BYTES.min(budget as usize);
                let Ok(bytes) = read(root, &path, limit) else {
                    processed = ends[&path];
                    continue;
                };
                budget = budget.saturating_sub(bytes.len() as u64);
                let binary = bytes.contains(&0) || (bytes.len() <= limit && std::str::from_utf8(&bytes).is_err());
                let counts_known = !binary && bytes.len() <= limit;
                let added = if counts_known { lines(&bytes) as u64 } else { 0 };
                processed = ends[&path];
                result.files.push(FileChange {
                    path,
                    added,
                    removed: 0,
                    binary,
                    counts_known,
                    too_large: !binary && bytes.len() > PATCH_BYTES,
                    status: "added".into(),
                });
            }
        } else {
            let mut next = 0;
            while next < names.len() {
                let start = next;
                let mut size = 0;
                while next < names.len() && size + 2 * names[next].0.len() + 4 <= PATH_ARGS {
                    size += 2 * names[next].0.len() + 4;
                    next += 1;
                }
                if next == start {
                    next += 1;
                    processed = ends[&names[start].0];
                    continue;
                }
                let batch = &names[start..next];
                // Tree metadata includes blob sizes without reading their contents, even for deleted files.
                let mut tree_args = vec!["ls-tree", "-r", "-l", "-z", from, "--"];
                tree_args.extend(batch.iter().map(|(p, _)| p.as_str()));
                let Some(tree) = exec.git(root, &tree_args, 0, LIST_BYTES) else {
                    result.incomplete = true;
                    break;
                };
                if tree.len() > LIST_BYTES {
                    result.incomplete = true;
                    break;
                }
                let mut index_args = vec!["ls-files", "--stage", "-z", "--full-name", "--"];
                index_args.extend(batch.iter().map(|(p, _)| p.as_str()));
                let Some(index) = exec.git(root, &index_args, 0, LIST_BYTES) else {
                    result.incomplete = true;
                    break;
                };
                if index.len() > LIST_BYTES {
                    result.incomplete = true;
                    break;
                }
                let base: BTreeMap<_, _> = tree.split(|b| *b == 0).filter_map(|r| entry(r, 1)).collect();
                let mut allowed = vec![];
                for (path, _) in batch {
                    if Instant::now() >= exec.deadline {
                        result.incomplete = true;
                        break;
                    }
                    if !plain_file(root, path) {
                        continue;
                    }
                    // A literal pathspec still selects descendants in either compared listing.
                    if !exact_file(&tree, path, 1) || !exact_file(&index, path, 0) {
                        continue;
                    }
                    let Some(disk) = disk_size(root, path) else {
                        result.incomplete = true;
                        break;
                    };
                    let old = base.get(path).copied().flatten().unwrap_or(0);
                    let cost = old.saturating_add(disk);
                    if old > FILE_BYTES || disk > FILE_BYTES || cost > budget {
                        if cache::unchanged(exec, root, path, &index, &tree) {
                            continue;
                        }
                        result.files.push(FileChange {
                            path: path.clone(),
                            added: 0,
                            removed: 0,
                            binary: false,
                            counts_known: false,
                            too_large: old > FILE_BYTES || disk > FILE_BYTES,
                            status: if disk == 0 { "deleted" } else { "modified" }.into(),
                        });
                    } else {
                        budget -= cost;
                        allowed.push(path.as_str());
                    }
                }
                if result.incomplete {
                    break;
                }
                if allowed.is_empty() {
                    processed = ends[&batch.last().unwrap().0];
                    continue;
                }
                // shortcut: numstat reads tracked paths for names and counts only, never displayed content; nothing reaches
                // the AI or leaves this PC. Racing it requires a process already running as the same user, which could read
                // the file itself, just as with the person's git status. Upgrade to pinned reads if the listing shows content.
                let mut args = vec!["diff", "--raw", "--numstat", "-z", "--no-renames", "--no-textconv", "--no-ext-diff", from, "--"];
                args.extend(allowed);
                let Some(stats) = exec.git(root, &args, 0, LIST_BYTES) else {
                    result.incomplete = true;
                    break;
                };
                if stats.len() > LIST_BYTES {
                    result.incomplete = true;
                }
                result.files.extend(numstat(&stats[..stats.len().min(LIST_BYTES)]));
                processed = ends[&batch.last().unwrap().0];
            }
        }
        if result.incomplete {
            result.next = Some(format!("{source}:{}", offset + processed));
            break;
        }
        offset += used;
        if used < bytes.len() || remaining == 0 {
            result.incomplete = true;
            result.next = Some(format!("{source}:{offset}"));
            break;
        }
        source += 1;
        offset = 0;
    }
    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    result.files.dedup_by(|a, b| a.path == b.path);
    result
}

pub fn list_page(exec: &mut dyn Executor, copy: &WorkCopy, cursor: Option<&str>) -> ChangeList {
    let mut request = Request { exec, deadline: Instant::now() + REQUEST_TIME };
    let Some(from) = base(&mut request, copy) else {
        return ChangeList { incomplete: true, next: Some(cursor.unwrap_or("0:0").into()), ..ChangeList::default() };
    };
    listed(&mut request, copy, &from, cursor)
}

pub fn list(exec: &mut dyn Executor, copy: &WorkCopy) -> Vec<FileChange> {
    list_page(exec, copy, None).files
}
fn lines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|b| **b == b'\n').count() + usize::from(!bytes.is_empty() && bytes.last() != Some(&b'\n'))
}

/// Reads only a plain file inside the copy, never links, device files, pipes or protected files.
fn read(root: &Path, path: &str, limit: usize) -> std::io::Result<Vec<u8>> {
    if !safe(path) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    #[cfg(any(windows, unix))]
    let file = faber_winproc::open_plain_file(root, Path::new(path))?;
    #[cfg(not(any(windows, unix)))]
    return Err(std::io::ErrorKind::Unsupported.into());
    if !file.metadata()?.is_file() {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let mut bytes = vec![];
    file.take(limit.saturating_add(1) as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Preserve Git's line-ending policy on the current snapshot without filters or rewriting the base blob.
fn snapshot_policy(request: &mut Request<'_>, root: &Path, path: &str, index: &[u8]) -> Option<(String, String)> {
    let config = request.git(root, &["config", "--default", "false", "--get", "core.autocrlf"], 0, 128)?;
    let config = String::from_utf8(config).ok()?.trim().to_ascii_lowercase();
    let autocrlf = match config.as_str() {
        "true" | "yes" | "on" | "1" => "true",
        "input" => "input",
        "false" | "no" | "off" | "0" | "" => "false",
        _ => return None,
    };
    let attrs = request.git(root, &["check-attr", "-z", "text", "eol", "--", path], 0, LIST_BYTES)?;
    let records: Vec<_> = attrs.split(|b| *b == 0).collect();
    if records.len() != 7 || records[6] != b"" {
        return None;
    }
    let mut text = "!text";
    let mut eol = "";
    for record in records[..6].as_chunks::<3>().0 {
        if record[0] != path.as_bytes() {
            return None;
        }
        match (record[1], record[2]) {
            (b"text", b"set") => text = "text",
            (b"text", b"unset") => text = "-text",
            (b"text", b"auto") => text = "text=auto",
            (b"eol", b"lf") => eol = " eol=lf",
            (b"eol", b"crlf") => eol = " eol=crlf",
            _ => {}
        }
    }
    // Git's automatic conversion preserves CRLF already present in the index, even with autocrlf enabled.
    if (text == "text=auto" || (text == "!text" && eol.is_empty() && autocrlf != "false"))
        && let Some(oid) = std::str::from_utf8(index).ok()?.split_whitespace().nth(1).filter(|oid| valid_oid(oid))
    {
        let indexed = request.git(root, &["cat-file", "blob", oid], 0, FILE_BYTES as usize)?;
        if indexed.len() > FILE_BYTES as usize {
            return None;
        }
        if indexed.windows(2).any(|bytes| bytes == b"\r\n") {
            text = "-text";
            eol = "";
        }
    }
    Some((format!("core.autocrlf={autocrlf}"), format!("before -text\nafter {text}{eol}\n")))
}

pub fn patch(exec: &mut dyn Executor, copy: &WorkCopy, path: &str) -> ChangePatch {
    if !safe(path) {
        return ChangePatch::failed();
    }
    let mut request = Request { exec, deadline: Instant::now() + REQUEST_TIME };
    let Some(from) = base(&mut request, copy) else { return ChangePatch::failed() };
    let root = Path::new(&copy.path);
    if !plain_file(root, path) {
        return ChangePatch::failed();
    }
    let Some(index) = request.git(root, &["ls-files", "--stage", "-z", "--full-name", "--", path], 0, LIST_BYTES) else {
        return ChangePatch::failed();
    };
    let Some(tree) = request.git(root, &["ls-tree", "-r", "-l", "-z", &from, "--", path], 0, LIST_BYTES) else {
        return ChangePatch::failed();
    };
    if index.len() > LIST_BYTES || tree.len() > LIST_BYTES {
        return ChangePatch::failed();
    }
    // Membership is exact on both sides: a file/directory transition must never select a subtree.
    let exact =
        |bytes: &[u8], source| bytes.split(|b| *b == 0).filter(|r| !r.is_empty()).all(|r| entry(r, source).is_some_and(|(p, _)| p == path));
    if !exact(&index, 0) || !exact(&tree, 1) {
        return ChangePatch::failed();
    }
    let tracked = !index.is_empty() || !tree.is_empty();
    if !tracked {
        let Some(others) =
            request.git(root, &["ls-files", "--others", "--exclude-standard", "-z", "--full-name", "--", path], 0, LIST_BYTES)
        else {
            return ChangePatch::failed();
        };
        if !others.split(|b| *b == 0).any(|p| p == path.as_bytes()) {
            return ChangePatch::failed();
        }
    }
    let old = tree.split(|b| *b == 0).filter_map(|r| entry(r, 1)).filter_map(|(_, size)| size).max().unwrap_or(0);
    if old > FILE_BYTES {
        return ChangePatch { too_large: true, ..ChangePatch::default() };
    }
    let limit = if tracked { FILE_BYTES as usize } else { PATCH_BYTES };
    // The same handle-checked open supplies every displayed working-tree byte, including tracked files.
    let current = match read(root, path, limit) {
        Ok(bytes) => Some(bytes),
        Err(error) if tracked && error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return ChangePatch::failed(),
    };
    let bytes = current.as_deref().unwrap_or_default();
    if bytes.len() > limit || (!tracked && lines(bytes) > PATCH_LINES) {
        return ChangePatch { too_large: true, ..ChangePatch::default() };
    }
    if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        return ChangePatch { binary: true, ..ChangePatch::default() };
    }
    let text = if tracked {
        let base = if tree.is_empty() {
            vec![]
        } else {
            let object = format!("{from}:{path}");
            let Some(bytes) = request.git(root, &["cat-file", "blob", &object], 0, FILE_BYTES as usize) else {
                return ChangePatch::failed();
            };
            bytes
        };
        if base.len() > FILE_BYTES as usize {
            return ChangePatch { too_large: true, ..ChangePatch::default() };
        }
        if base.contains(&0) || std::str::from_utf8(&base).is_err() {
            return ChangePatch { binary: true, ..ChangePatch::default() };
        }
        // RAII removes the private snapshots on success, error and timeout; Git never gets a working-tree path.
        let Ok(temp) = tempfile::Builder::new().prefix("faber-changes-").tempdir() else { return ChangePatch::failed() };
        let before = temp.path().join("before");
        let after = temp.path().join("after");
        if std::fs::write(&before, &base).is_err() || std::fs::write(&after, bytes).is_err() {
            return ChangePatch::failed();
        }
        let (autocrlf, attributes) = if bytes.contains(&b'\r') {
            let Some(policy) = snapshot_policy(&mut request, root, path, &index) else { return ChangePatch::failed() };
            policy
        } else {
            ("core.autocrlf=false".into(), "before -text\nafter -text\n".into())
        };
        let attributes_path = temp.path().join("attributes");
        if std::fs::write(&attributes_path, attributes).is_err() {
            return ChangePatch::failed();
        }
        let attributes = format!("core.attributesFile={}", crate::project::git::native_path(&attributes_path));
        let Some(diff) = request.git(
            temp.path(),
            &[
                "-c",
                &autocrlf,
                "-c",
                &attributes,
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "-U3",
                "--",
                "before",
                "after",
            ],
            0,
            PATCH_BYTES,
        ) else {
            return ChangePatch::failed();
        };
        if diff.len() > PATCH_BYTES || lines(&diff) > PATCH_LINES {
            return ChangePatch { too_large: true, ..ChangePatch::default() };
        }
        let diff = String::from_utf8_lossy(&diff);
        if let Some(start) = diff.find("@@ ") {
            let before = if tree.is_empty() { "/dev/null".into() } else { format!("a/{path}") };
            let after = if current.is_none() { "/dev/null".into() } else { format!("b/{path}") };
            format!("--- {before}\n+++ {after}\n{}", &diff[start..])
        } else {
            String::new()
        }
    } else {
        let count = lines(bytes);
        let text = std::str::from_utf8(bytes).unwrap_or_default();
        let mut patch = format!("--- /dev/null\n+++ b/{path}\n");
        if count > 0 {
            patch.push_str(&format!("@@ -0,0 +1,{count} @@\n"));
        }
        for line in text.split_inclusive('\n') {
            patch.push('+');
            patch.push_str(line);
            if !line.ends_with('\n') {
                patch.push_str("\n\\ No newline at end of file\n");
            }
        }
        patch
    };
    if Instant::now() >= request.deadline {
        return ChangePatch::failed();
    }
    if text.len() > PATCH_BYTES || text.lines().count() > PATCH_LINES {
        return ChangePatch { too_large: true, ..ChangePatch::default() };
    }
    ChangePatch { text, ..ChangePatch::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spaces_tabs_binary_and_raw_status_without_quoting_paths() {
        let stats = b":000000 100644 a b A\0new file.rs\0:100644 000000 a b D\0gone.rs\0\
                      3\t0\tnew file.rs\x002\t4\ta\tfile.rs\0-\t-\timage.png\x000\t5\tgone.rs\0";
        let files = numstat(stats);
        assert_eq!((files[0].path.as_str(), files[0].added, files[0].status.as_str()), ("new file.rs", 3, "added"));
        assert_eq!(files[1].path, "a\tfile.rs");
        assert!(files[2].binary);
        assert_eq!(files[3].status, "deleted");
    }

    #[test]
    fn refuses_traversal_git_and_credential_paths() {
        for path in ["../outside", "a/../../b", "/absolute", "C:/outside", "a\\b", ".git/config", ".codex/auth.json", ".env", "auth.json"] {
            assert!(!safe(path), "{path}");
        }
    }

    #[test]
    fn changes_metadata_accepts_only_exact_regular_stage_zero_files() {
        for record in [b"040000 tree a 1\tcache\0".as_slice(), b"120000 blob a 1\tlink\0", b"100644 tree a 1\twrong\0"] {
            assert!(entry(record, 1).is_none());
        }
        assert!(entry(b"100644 a 1\tconflict", 0).is_none());
        assert!(!exact_file(b"100644 blob a 1\tcache/auth.json\0", "cache", 1));
        assert!(exact_file(b"100644 blob a 1\tcache-old\0", "cache", 1));
    }
}
