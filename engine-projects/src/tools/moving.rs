//! Moves installed catalog tools from the main data folder to the chosen storage folder while nothing else runs: the host
//! calls this at start, before the engine exists, so no session, kit, install or storage change can meet a moving item.
//!
//! Each item goes through the same steps. Its folder is copied into `<Faber-storage>/move-staging/<id>`, every file hashed
//! as it is read, every file and folder flushed once written. The staged folder is renamed into its place under
//! `<Faber-storage>/tools`. The source is read again by content (never sizes or times), then the copy is read back where
//! it lies, without following any link: what is checked is what was published. The original is set aside in
//! `<data>/storage-move/old/<attempt>/`, the copy is checked to still be a plain folder, and only then is
//! `tools/locations/<id>.json` written, flushed with its folder: that record is the switch, and until it is on disk the old
//! folder is the truth. A journal names the destination and the items in flight, so [`recover`] at the next start decides
//! by each record as the disk has it: it puts back an original that was never switched and deletes its copy, or keeps the
//! copy to delete once a drive that was away comes back. Interpreter pins a runtime names are copied beside it and kept
//! where they were.
use super::{location, replacement, small_file, valid_id};
use crate::project::lease::real_path;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Faber's own folder for moves, inside the main data folder.
pub const FOLDER: &str = "storage-move";
/// The originals of moved tools, kept until the person removes them.
pub const OLD: &str = "old";
const JOURNAL: &str = "journal.json";
/// Copies a move could not delete because their drive was away, deleted at a later start that reaches it.
const LEFTOVERS: &str = "leftovers.json";
/// Kept originals being removed: renamed first, so a removal cut halfway is finished by the next one.
const REMOVING: &str = ".removing-";
const STAGING: &str = "move-staging";
const PAYLOADS: [&str; 5] = ["disabled", "plugin/skills", "plugins", "runtimes", "servers"];
/// Room kept free on the destination beyond the item being copied.
const MARGIN: u64 = 256 * 1024 * 1024;
const CHUNK: usize = 256 * 1024;
const RECORD_LIMIT: u64 = 1024 * 1024;

pub const CHANGED: &str = "reason.storage.moveChanged";
pub const IN_USE: &str = "reason.storage.moveInUse";
pub const FAILED: &str = "reason.storage.moveFailed";
pub const SPACE: &str = "error.storage.moveSpace";
pub const UNREACHABLE: &str = "error.storage.unreachable";

pub type Free = Box<dyn Fn(&Path) -> io::Result<u64> + Send + Sync>;
pub type Flush = Box<dyn Fn(&Path) -> io::Result<()> + Send + Sync>;

/// What the filesystem calls that tests replace: the destination's free space and the flush of a folder's entries.
pub struct Disk {
    pub free: Free,
    pub sync_dir: Flush,
}

impl Disk {
    pub fn real() -> Self {
        Self { free: Box::new(|path| faber_winproc::rewind::Root::open(path)?.available_space()), sync_dir: Box::new(location::sync_dir) }
    }
}

/// Where a move is, for its progress and for the checks that interrupt it at each step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step<'a> {
    /// An item starts: its number from 1, how many there are, and its size.
    Item { id: &'a str, number: usize, total: usize, bytes: u64 },
    /// Bytes of the current item copied so far.
    Copied(u64),
    /// A file of the current item was read again, to check the source or the copy.
    Checking,
    /// The copy is complete in staging, not yet published.
    Staged(&'a str),
    /// The copy is in its place and matches; the source is not read again yet.
    Published(&'a str),
    /// The original is set aside; the location record still names the old folder.
    Retired(&'a str),
    /// The location record names the new folder.
    Switched(&'a str),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Left {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub moved: Vec<String>,
    pub bytes: u64,
    /// Items that stayed where they were, with why.
    pub left: Vec<Left>,
    /// Why the move ended before its last item, if it did; the items after it stayed too.
    pub stopped: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub id: String,
    /// The item's folder under a tools root, with `/` between parts (`runtimes/<id>`).
    pub folder: String,
    pub bytes: u64,
}

#[derive(Serialize, Deserialize)]
struct Journal {
    attempt: String,
    /// The destination's `tools` folder.
    target: PathBuf,
    /// Items this attempt may have published or set aside, the last one possibly unfinished.
    items: Vec<Moving>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Moving {
    id: String,
    folder: String,
    /// The copy was renamed into its place: from then on, and only then, a copy there is this move's own to delete.
    #[serde(default)]
    published: bool,
}

/// An unswitched copy under `target` (a `tools` folder) that could not be deleted when its move ended.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Leftover {
    id: String,
    target: PathBuf,
    folder: String,
}

/// What the location record of an item says as the disk has it: no record (the main data folder), a folder, or a record
/// that cannot be read. Recovery decides by this, never by whether the folder it names can be reached right now.
#[derive(PartialEq)]
enum Record {
    Absent,
    Names(PathBuf),
    Unclear,
}

fn recorded(tools: &Path, id: &str) -> Record {
    let path = location::record(tools, id);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Record::Absent,
        Ok(meta) if meta.is_file() && !linked(&meta) => {
            small_file(&path).and_then(|bytes| serde_json::from_slice(&bytes).ok()).map_or(Record::Unclear, Record::Names)
        }
        _ => Record::Unclear,
    }
}

/// One tree as read: each path under its root (`/` between parts, "" for the root) and what it is.
type Manifest = BTreeMap<String, Node>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Dir,
    File {
        len: u64,
        sha256: [u8; 32],
        mode: u32,
    },
    /// A uv interpreter link inside a runtime's `.venv` (Unix only), kept as its text.
    Link(PathBuf),
}

/// How one item ended: a crash simulated by a check stops everything as a real one would, with nothing undone.
enum Fail {
    Item(&'static str),
    Stop(&'static str),
    Crash(io::Error),
}

fn under(root: &Path, relative: &str) -> PathBuf {
    relative.split('/').filter(|part| !part.is_empty()).fold(root.to_path_buf(), |path, part| path.join(part))
}

fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0 || meta.file_type().is_symlink()
    }
    #[cfg(not(windows))]
    meta.file_type().is_symlink()
}

fn exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn remove_tree(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        // A link is removed itself, never what it points to.
        Ok(meta) if linked(&meta) || !meta.is_dir() => fs::remove_file(path).or_else(|_| fs::remove_dir(path)),
        Ok(_) => fs::remove_dir_all(path),
    }
}

/// Creates `path` and any missing parent, flushing the parent of each new folder, so a power cut cannot lose a folder a
/// later step relies on (on Unix a new entry is durable only once its parent is flushed).
fn make_dirs(path: &Path, disk: &Disk) -> io::Result<()> {
    let parent = path.parent();
    if exists(path)? {
        // Made by an earlier call whose flush may have failed: flushed again, so no step relies on an entry not on disk.
        return parent.map_or(Ok(()), |parent| (disk.sync_dir)(parent));
    }
    let parent = parent.ok_or_else(invalid)?;
    make_dirs(parent, disk)?;
    match fs::create_dir(path) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
        Ok(()) => (disk.sync_dir)(parent),
    }
}

fn mode(meta: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0
    }
}

/// What one entry of a tree is, refusing anything but plain folders, plain files and a runtime's interpreter links.
fn node_kind(tools: &Path, path: &Path) -> io::Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    if linked(&meta) {
        return match replacement::runtime_link(tools, path) {
            Ok(Some(_)) => Ok(meta),
            _ => Err(invalid()),
        };
    }
    if meta.is_dir() || meta.is_file() { Ok(meta) } else { Err(invalid()) }
}

fn sorted(dir: &Path) -> io::Result<Vec<std::ffi::OsString>> {
    let mut names = fs::read_dir(dir)?.map(|entry| entry.map(|entry| entry.file_name())).collect::<io::Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

fn name(name: &std::ffi::OsStr) -> io::Result<&str> {
    // Catalog folders hold portable names only; anything else is not Faber's and is never moved.
    name.to_str().filter(|name| !name.is_empty() && !name.contains(['/', '\\'])).ok_or_else(invalid)
}

fn hash_file(
    path: &Path,
    buffer: &mut [u8],
    mut written: Option<&mut File>,
    progress: &mut dyn FnMut(u64) -> io::Result<()>,
) -> io::Result<(u64, [u8; 32])> {
    let parent = path.parent().ok_or_else(invalid)?;
    let leaf = path.file_name().ok_or_else(invalid)?;
    let mut input = faber_winproc::open_catalog_file(parent, Path::new(leaf))?;
    let mut digest = Sha256::new();
    let mut len = 0u64;
    loop {
        let count = input.read(buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        if let Some(output) = written.as_deref_mut() {
            output.write_all(&buffer[..count])?;
        }
        len += count as u64;
        progress(count as u64)?;
    }
    Ok((len, digest.finalize().into()))
}

/// Reads a tree whole, hashing every file, following no link; `tick` hears each chunk read.
fn read_tree(tools: &Path, root: &Path, tick: &mut dyn FnMut(u64) -> io::Result<()>) -> io::Result<Manifest> {
    let mut manifest = Manifest::new();
    let mut buffer = vec![0; CHUNK];
    read_node(tools, root, String::new(), &mut manifest, &mut buffer, tick)?;
    Ok(manifest)
}

fn read_node(
    tools: &Path,
    path: &Path,
    relative: String,
    manifest: &mut Manifest,
    buffer: &mut [u8],
    tick: &mut dyn FnMut(u64) -> io::Result<()>,
) -> io::Result<()> {
    let meta = node_kind(tools, path)?;
    if linked(&meta) {
        manifest.insert(relative, Node::Link(fs::read_link(path)?));
    } else if meta.is_dir() {
        manifest.insert(relative.clone(), Node::Dir);
        for entry in sorted(path)? {
            let child = name(&entry)?;
            let next = if relative.is_empty() { child.to_owned() } else { format!("{relative}/{child}") };
            read_node(tools, &path.join(child), next, manifest, buffer, tick)?;
        }
    } else {
        let (len, sha256) = hash_file(path, buffer, None, tick)?;
        manifest.insert(relative, Node::File { len, sha256, mode: mode(&meta) });
    }
    Ok(())
}

/// Copies a tree into `to` (which must not exist), returning what was read from the source as it was copied. Every file
/// and every folder is flushed, so the copy is on disk before anything is switched to it.
fn copy_tree(tools: &Path, from: &Path, to: &Path, disk: &Disk, progress: &mut dyn FnMut(u64) -> io::Result<()>) -> io::Result<Manifest> {
    let mut manifest = Manifest::new();
    let mut buffer = vec![0; CHUNK];
    copy_node(tools, (from, to), disk, String::new(), &mut manifest, &mut buffer, progress)?;
    Ok(manifest)
}

fn copy_node(
    tools: &Path,
    (from, to): (&Path, &Path),
    disk: &Disk,
    relative: String,
    manifest: &mut Manifest,
    buffer: &mut [u8],
    progress: &mut dyn FnMut(u64) -> io::Result<()>,
) -> io::Result<()> {
    let meta = node_kind(tools, from)?;
    if linked(&meta) {
        #[cfg(unix)]
        {
            let target = fs::read_link(from)?;
            std::os::unix::fs::symlink(&target, to)?;
            manifest.insert(relative, Node::Link(target));
        }
        #[cfg(not(unix))]
        return Err(invalid());
    } else if meta.is_dir() {
        fs::create_dir(to)?;
        manifest.insert(relative.clone(), Node::Dir);
        for entry in sorted(from)? {
            let child = name(&entry)?;
            let next = if relative.is_empty() { child.to_owned() } else { format!("{relative}/{child}") };
            copy_node(tools, (&from.join(child), &to.join(child)), disk, next, manifest, buffer, progress)?;
        }
        #[cfg(unix)]
        fs::set_permissions(to, meta.permissions())?;
        (disk.sync_dir)(to)?;
    } else {
        let mut output = File::create_new(to)?;
        let (len, sha256) = hash_file(from, buffer, Some(&mut output), progress)?;
        #[cfg(unix)]
        output.set_permissions(meta.permissions())?;
        // Python trusts a runtime's bytecode only while each source keeps the time it was compiled against.
        output.set_modified(meta.modified()?)?;
        output.sync_all()?;
        manifest.insert(relative, Node::File { len, sha256, mode: mode(&meta) });
    }
    Ok(())
}

fn tree_bytes(tools: &Path, path: &Path) -> io::Result<u64> {
    let meta = node_kind(tools, path)?;
    if linked(&meta) || !meta.is_dir() {
        return Ok(if meta.is_file() { meta.len() } else { 0 });
    }
    sorted(path)?.iter().try_fold(0u64, |sum, entry| Ok(sum.saturating_add(tree_bytes(tools, &path.join(name(entry)?))?)))
}

/// The `tools` folder and the folders a move writes in must be plain folders, reached through no link.
fn plain_root(tools: &Path) -> io::Result<()> {
    for folder in std::iter::once(String::new())
        .chain(["plugin", "plugin/skills", "plugins", "runtimes", "servers", "disabled", "runtime-pins", "locations"].map(String::from))
    {
        let path = under(tools, &folder);
        real_path(&path).map_err(|_| invalid())?;
        if exists(&path)? {
            replacement::plain(&path, true).map_err(|_| invalid())?;
        }
    }
    Ok(())
}

fn ledger(tools: &Path) -> io::Result<BTreeMap<String, String>> {
    let path = tools.join("installed.json");
    if !exists(&path)? {
        return Ok(BTreeMap::new());
    }
    let bytes = small_file(&path).ok_or_else(invalid)?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}

/// A small JSON file written whole and flushed, its folder flushed after the rename: once this returns it survives a crash.
fn write_durable(path: &Path, bytes: &[u8], disk: &Disk) -> io::Result<()> {
    let parent = path.parent().ok_or_else(invalid)?;
    make_dirs(parent, disk)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|failure| failure.error)?;
    (disk.sync_dir)(parent)
}

fn remove_durable(path: &Path, disk: &Disk) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(()) => (disk.sync_dir)(path.parent().ok_or_else(invalid)?),
    }
}

fn read_journal(data: &Path) -> io::Result<Option<Journal>> {
    let path = data.join(FOLDER).join(JOURNAL);
    if !exists(&path)? {
        return Ok(None);
    }
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || meta.len() > RECORD_LIMIT {
        return Err(invalid());
    }
    serde_json::from_slice(&fs::read(path)?).map(Some).map_err(|_| invalid())
}

/// Whether `folder` is where a tool `id` can live (`runtimes/<id>`...): what Faber's own files name is checked before
/// anything is deleted by it.
fn item_folder(id: &str, folder: &str) -> bool {
    valid_id(id) && PAYLOADS.iter().any(|parent| folder.strip_prefix(parent).and_then(|rest| rest.strip_prefix('/')) == Some(id))
}

fn pins(tools: &Path, runtime: &Path) -> io::Result<Vec<String>> {
    if !exists(&runtime.join("interpreter.json"))? {
        return Ok(Vec::new());
    }
    let mut digest = String::new();
    faber_winproc::open_catalog_file(runtime, Path::new("interpreter.json"))?.take(65).read_to_string(&mut digest)?;
    if !super::runtime::pin(&digest) {
        return Err(invalid());
    }
    let (_, hash) = hash_file(&tools.join("runtime-pins").join(&digest), &mut vec![0; CHUNK], None, &mut |_| Ok(()))?;
    if crate::hex(&hash) != digest {
        return Err(invalid());
    }
    Ok(vec![digest])
}

/// Installed tools that live in the main data folder and can move: each has exactly one folder and no update half done.
/// Items whose folder already exists at the destination stay out of the plan, so nothing a move did not make is ever
/// deleted by it.
pub fn plan(data: &Path, target: Option<&Path>) -> io::Result<Vec<Planned>> {
    let tools = data.join("tools");
    real_path(&tools).map_err(|_| invalid())?;
    if !exists(&tools)? {
        return Ok(Vec::new());
    }
    plain_root(&tools)?;
    let mut planned = Vec::new();
    for id in ledger(&tools)?.into_keys() {
        if !valid_id(&id) || location::source_root(&tools, &id) != tools || exists(&tools.join("replacements").join(&id))? {
            continue;
        }
        let mut found = Vec::new();
        for parent in PAYLOADS {
            let folder = format!("{parent}/{id}");
            if exists(&under(&tools, &folder))? {
                found.push(folder);
            }
        }
        let [folder] = found.as_slice() else { continue };
        if let Some(target) = target
            && exists(&under(&target.join("tools"), folder))?
        {
            continue;
        }
        // A tree that cannot be read whole, or holds a link of its own, is not Faber's to move.
        let Ok(bytes) = tree_bytes(&tools, &under(&tools, folder)) else { continue };
        planned.push(Planned { id, folder: folder.clone(), bytes });
    }
    Ok(planned)
}

/// What an interrupted move left, put right before anything else opens the store: every item it did not switch is back
/// in its old folder and its copy is gone; switched items stay moved. Returns None when no move was interrupted.
pub fn recover(data: &Path, disk: &Disk) -> io::Result<Option<Outcome>> {
    let tools = data.join("tools");
    let locations = tools.join("locations");
    // Nothing is put back through a link: the store's own folders must be plain, as the store itself requires.
    if exists(&tools)? {
        plain_root(&tools)?;
    }
    // Every decision below rests on the location records as the disk keeps them, not as a cache a power cut could undo.
    if exists(&locations)? {
        (disk.sync_dir)(&locations)?;
    }
    let found = read_leftovers(data);
    let mut leftovers = Vec::new();
    // A copy that cannot be deleted yet only costs room: it never holds back the recovery below.
    for left in &found {
        if !settle(&tools, left, disk).unwrap_or(false) {
            leftovers.push(left.clone());
        }
    }
    let journal_path = data.join(FOLDER).join(JOURNAL);
    let journal = match read_journal(data) {
        Ok(None) => {
            if leftovers != found {
                save_leftovers(data, &leftovers, disk)?;
            }
            return Ok(None);
        }
        Ok(Some(journal)) if attempt_name(&journal.attempt) => journal,
        // Without its journal nothing can say which copy was this move's own: originals that were set aside still go
        // back, from every attempt, as long as their tool is installed and has no location record.
        _ => Journal { attempt: String::new(), target: PathBuf::new(), items: Vec::new() },
    };
    let installed = ledger(&tools)?;
    let old = data.join(FOLDER).join(OLD);
    let attempts: Vec<String> = if journal.attempt.is_empty() {
        let names = if exists(&old)? { sorted(&old)? } else { Vec::new() };
        names.iter().filter_map(|attempt| attempt.to_str().filter(|attempt| attempt_name(attempt)).map(String::from)).collect()
    } else {
        vec![journal.attempt.clone()]
    };
    for attempt in &attempts {
        let set_aside = old.join(attempt);
        for parent in PAYLOADS {
            let folder = under(&set_aside, parent);
            if !exists(&folder)? {
                continue;
            }
            for id in sorted(&folder)? {
                let id = name(&id)?;
                if valid_id(id) && installed.contains_key(id) && recorded(&tools, id) == Record::Absent {
                    put_back(&folder.join(id), &under(&tools, &format!("{parent}/{id}")), disk)?;
                }
            }
        }
    }
    let mut outcome = Outcome { stopped: Some("error.storage.moveInterrupted".into()), ..Outcome::default() };
    if journal.target.file_name().is_some_and(|name| name == "tools") && journal.target.is_absolute() {
        let reachable = usable(&tools, &journal.target);
        for item in journal.items.iter().filter(|item| item_folder(&item.id, &item.folder)) {
            let copy = under(&journal.target, &item.folder);
            match recorded(&tools, &item.id) {
                Record::Names(root) if root == journal.target => {
                    if !reachable || exists(&copy)? {
                        outcome.moved.push(item.id.clone());
                        continue;
                    }
                    // The switch reached the disk and its copy did not: the original goes back and the record goes.
                    let original = under(&tools, &item.folder);
                    if !exists(&original)? {
                        put_back(&under(&old.join(&journal.attempt), &item.folder), &original, disk)?;
                    }
                    if exists(&original)? {
                        remove_durable(&location::record(&tools, &item.id), disk)?;
                    }
                }
                // Never switched, and published by this attempt after finding the place empty: this move's own copy.
                Record::Absent if item.published && reachable => {
                    // The original's way back is flushed first (a rollback's own flush may have failed), then its copy goes.
                    let original = under(&tools, &item.folder);
                    if !exists(&original)? {
                        continue;
                    }
                    (disk.sync_dir)(original.parent().ok_or_else(invalid)?)?;
                    delete_copy(&copy, disk)?
                }
                Record::Absent if item.published => {
                    leftovers.push(Leftover { id: item.id.clone(), target: journal.target.clone(), folder: item.folder.clone() })
                }
                // A record that cannot be read, or names another folder: nothing is deleted on a guess.
                _ => {}
            }
        }
        if reachable && let Some(root) = journal.target.parent() {
            remove_tree(&root.join(STAGING))?;
        }
    }
    if leftovers != found {
        save_leftovers(data, &leftovers, disk)?;
    }
    remove_durable(&journal_path, disk)?;
    Ok(Some(outcome))
}

/// The attempt names this module makes (a UUID's 32 hex digits): a journal naming anything else is not trusted.
fn attempt_name(name: &str) -> bool {
    name.len() == 32 && name.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The destination is this profile's own folder and no folder a copy lies in is a link: only then is anything deleted there.
fn usable(tools: &Path, target: &Path) -> bool {
    location::external(tools, target) && plain_root(target).is_ok()
}

/// Deletes a copy this move made and flushes its folder, so it cannot come back after a power cut.
fn delete_copy(copy: &Path, disk: &Disk) -> io::Result<()> {
    remove_tree(copy)?;
    match copy.parent() {
        Some(parent) if exists(parent)? => (disk.sync_dir)(parent),
        _ => Ok(()),
    }
}

/// An original set aside goes back to `original` unless something is already there.
fn put_back(aside: &Path, original: &Path, disk: &Disk) -> io::Result<()> {
    if !exists(aside)? || exists(original)? {
        return Ok(());
    }
    let parent = original.parent().ok_or_else(invalid)?;
    make_dirs(parent, disk)?;
    super::retry_busy(|| fs::rename(aside, original))?;
    (disk.sync_dir)(parent)
}

fn read_leftovers(data: &Path) -> Vec<Leftover> {
    let path = data.join(FOLDER).join(LEFTOVERS);
    // A list that cannot be read only costs the room its copies take: nothing else depends on it.
    small_file(&path).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn save_leftovers(data: &Path, leftovers: &[Leftover], disk: &Disk) -> io::Result<()> {
    let path = data.join(FOLDER).join(LEFTOVERS);
    if leftovers.is_empty() {
        return remove_durable(&path, disk);
    }
    write_durable(&path, &serde_json::to_vec(leftovers).map_err(io::Error::other)?, disk)
}

/// Deletes one leftover copy once its drive is back. Returns whether it is settled: deleted, or found to be the tool's
/// live folder since (a later install there). One whose drive is still away, or whose record cannot be read, waits.
fn settle(tools: &Path, left: &Leftover, disk: &Disk) -> io::Result<bool> {
    if !item_folder(&left.id, &left.folder) || left.target.file_name().is_none_or(|name| name != "tools") || !left.target.is_absolute() {
        return Ok(true);
    }
    if !usable(tools, &left.target) {
        return Ok(false);
    }
    match recorded(tools, &left.id) {
        Record::Names(root) if root == left.target => Ok(true),
        Record::Unclear => Ok(false),
        _ => {
            delete_copy(&under(&left.target, &left.folder), disk)?;
            if let Some(root) = left.target.parent() {
                remove_tree(&root.join(STAGING))?;
            }
            Ok(true)
        }
    }
}

/// Moves every planned item to `target` (a `Faber-storage` folder the caller checked is the chosen one), one at a time.
/// `step` hears each step; an error from it ends the run at once with nothing undone, as a crash would.
pub fn run(data: &Path, target: &Path, disk: &Disk, step: &mut dyn FnMut(Step<'_>) -> io::Result<()>) -> io::Result<Outcome> {
    let tools = data.join("tools");
    let to = target.join("tools");
    if real_path(&to).is_err() || !location::external(&tools, &to) {
        return Ok(Outcome { stopped: Some(UNREACHABLE.into()), ..Outcome::default() });
    }
    // Staging left by an earlier attempt goes first, so it never counts against the room this one needs.
    remove_tree(&target.join(STAGING))?;
    let planned = plan(data, Some(target))?;
    let mut outcome = Outcome::default();
    if planned.is_empty() {
        return Ok(outcome);
    }
    make_dirs(&to, disk)?;
    plain_root(&to)?;
    let mut journal = Journal { attempt: uuid::Uuid::new_v4().simple().to_string(), target: to.clone(), items: Vec::new() };
    let journal_path = data.join(FOLDER).join(JOURNAL);
    let total = planned.len();
    for (index, item) in planned.iter().enumerate() {
        match (disk.free)(target) {
            Ok(free) if free >= item.bytes.saturating_add(MARGIN) => {}
            Ok(_) => {
                outcome.stopped = Some(SPACE.into());
                break;
            }
            Err(_) => {
                outcome.stopped = Some(UNREACHABLE.into());
                break;
            }
        }
        journal.items.push(Moving { id: item.id.clone(), folder: item.folder.clone(), published: false });
        write_durable(&journal_path, &serde_json::to_vec(&journal).map_err(io::Error::other)?, disk)?;
        step(Step::Item { id: &item.id, number: index + 1, total, bytes: item.bytes })?;
        let attempt = journal.attempt.clone();
        let mut published = || {
            if let Some(last) = journal.items.last_mut() {
                last.published = true;
            }
            write_durable(&journal_path, &serde_json::to_vec(&journal).map_err(io::Error::other)?, disk)
        };
        match move_one(&tools, target, &attempt, item, disk, (&mut *step, &mut published)) {
            Ok(()) => {
                outcome.moved.push(item.id.clone());
                outcome.bytes += item.bytes;
            }
            Err(Fail::Crash(error)) => return Err(error),
            Err(Fail::Item(reason)) => outcome.left.push(Left { id: item.id.clone(), reason: reason.into() }),
            Err(Fail::Stop(reason)) => {
                outcome.left.push(Left { id: item.id.clone(), reason: FAILED.into() });
                outcome.stopped = Some(reason.into());
                break;
            }
        }
    }
    // The same steps as after a crash finish the run: whatever an item could not undo by itself (an original it could not
    // put back, a copy on a drive that went away) is put right now, and the journal goes only once that is done.
    recover(data, disk)?;
    Ok(outcome)
}

/// What hears an item's steps, and what marks it published in the journal.
type Hooks<'a> = (&'a mut dyn FnMut(Step<'_>) -> io::Result<()>, &'a mut dyn FnMut() -> io::Result<()>);

fn move_one(
    tools: &Path,
    target: &Path,
    attempt: &str,
    item: &Planned,
    disk: &Disk,
    (step, published_mark): Hooks<'_>,
) -> Result<(), Fail> {
    let to = target.join("tools");
    let source = under(tools, &item.folder);
    let staged = target.join(STAGING).join(&item.id);
    let published = under(&to, &item.folder);
    let set_aside = under(&tools.parent().ok_or(Fail::Item(FAILED))?.join(FOLDER).join(OLD).join(attempt), &item.folder);
    // Any failure before the switch leaves the original where it was and removes what this item made.
    let undo = |reason: &'static str, made: bool| {
        let _ = remove_tree(&staged);
        if made && remove_tree(&published).is_err() {
            return Fail::Stop(UNREACHABLE);
        }
        if target.is_dir() { Fail::Item(reason) } else { Fail::Stop(UNREACHABLE) }
    };
    let pins = pins(tools, &source).map_err(|_| Fail::Item(FAILED))?;
    make_dirs(&target.join(STAGING), disk).map_err(|_| undo(FAILED, false))?;
    let (mut copied, mut crashed) = (0u64, false);
    let expected = copy_tree(tools, &source, &staged, disk, &mut |bytes| {
        copied += bytes;
        step(Step::Copied(copied)).inspect_err(|_| crashed = true)
    });
    let expected = match expected {
        Ok(expected) => expected,
        // A check that interrupts the copy is a crash: nothing is undone.
        Err(error) if crashed => return Err(Fail::Crash(error)),
        Err(_) => return Err(undo(FAILED, false)),
    };
    step(Step::Staged(&item.id)).map_err(Fail::Crash)?;
    // Interpreter pins are named by their own hash: one already there with the right bytes is the same pin.
    for digest in &pins {
        let pin = to.join("runtime-pins").join(digest);
        let ok = match exists(&pin) {
            Ok(true) => hash_file(&pin, &mut vec![0; CHUNK], None, &mut |_| Ok(())).is_ok_and(|(_, hash)| crate::hex(&hash) == *digest),
            Ok(false) => copy_pin(tools, &pin, digest, disk).is_ok(),
            Err(_) => false,
        };
        if !ok {
            return Err(undo(FAILED, false));
        }
    }
    let parent = published.parent().ok_or(Fail::Item(FAILED))?;
    if make_dirs(parent, disk).is_err() || plain_root(&to).is_err() {
        return Err(undo(FAILED, false));
    }
    // Something that took the place meanwhile, or made the rename fail, is never deleted: the journal does not own it.
    if exists(&published).unwrap_or(true) || super::retry_busy(|| fs::rename(&staged, &published)).is_err() {
        return Err(undo(FAILED, false));
    }
    if (disk.sync_dir)(parent).is_err() || published_mark().is_err() {
        return Err(undo(FAILED, true));
    }
    step(Step::Published(&item.id)).map_err(Fail::Crash)?;
    // The source again, by content: an edit of the same size with its time put back is still a change.
    match read_tree(tools, &source, &mut |_| {
        let _ = step(Step::Checking);
        Ok(())
    }) {
        Ok(now) if now == expected => {}
        _ => return Err(undo(CHANGED, true)),
    }
    // Then the copy where it now lies, as the last read before the switch: a folder swapped for a link after staging is
    // refused here, never followed.
    let read_back = read_tree(&to, &published, &mut |_| {
        let _ = step(Step::Checking);
        Ok(())
    });
    if real_path(&published).is_err() || read_back.ok().as_ref() != Some(&expected) {
        return Err(undo(FAILED, true));
    }
    let aside_parent = set_aside.parent().ok_or(Fail::Item(FAILED))?;
    if make_dirs(aside_parent, disk).is_err() || real_path(&set_aside).is_err() {
        return Err(undo(FAILED, true));
    }
    // A program holding one of its files open keeps the original from being set aside on Windows.
    if super::retry_busy(|| fs::rename(&source, &set_aside)).is_err() {
        return Err(undo(IN_USE, true));
    }
    let restore = |reason: &'static str| {
        let back = super::retry_busy(|| fs::rename(&set_aside, &source)).is_ok();
        if !back || source.parent().is_none_or(|parent| (disk.sync_dir)(parent).is_err()) {
            // The journal keeps both; the next start decides by the record before the store opens.
            return Fail::Stop(FAILED);
        }
        undo(reason, true)
    };
    if (disk.sync_dir)(aside_parent).is_err() || source.parent().is_none_or(|parent| (disk.sync_dir)(parent).is_err()) {
        return Err(restore(FAILED));
    }
    step(Step::Retired(&item.id)).map_err(Fail::Crash)?;
    // The folder switched to is the one read back: one swapped for a link since then is refused, not recorded.
    let plain = real_path(&published).is_ok() && fs::symlink_metadata(&published).is_ok_and(|meta| meta.is_dir() && !linked(&meta));
    let locations = tools.join("locations");
    if !plain || make_dirs(&locations, disk).is_err() {
        return Err(restore(FAILED));
    }
    if location::switch(tools, &item.id, &to, &|path| (disk.sync_dir)(path)).is_err() {
        // Whether or not the record reached the disk, it is taken back and the original is the truth again. When its
        // removal cannot be made durable either, nothing more is touched: the journal lets recovery decide by the record.
        match fs::remove_file(location::record(tools, &item.id)) {
            Ok(()) if (disk.sync_dir)(&locations).is_ok() => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            _ => return Err(Fail::Stop(FAILED)),
        }
        return Err(restore(FAILED));
    }
    step(Step::Switched(&item.id)).map_err(Fail::Crash)?;
    Ok(())
}

fn copy_pin(tools: &Path, pin: &Path, digest: &str, disk: &Disk) -> io::Result<()> {
    let parent = pin.parent().ok_or_else(invalid)?;
    make_dirs(parent, disk)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let (_, hash) = hash_file(&tools.join("runtime-pins").join(digest), &mut vec![0; CHUNK], Some(temp.as_file_mut()), &mut |_| Ok(()))?;
    if crate::hex(&hash) != digest {
        return Err(invalid());
    }
    temp.as_file().sync_all()?;
    temp.persist_noclobber(pin).map_err(|failure| failure.error)?;
    (disk.sync_dir)(parent)
}

/// How much the originals kept from earlier moves take, for Storage.
/// A removal cut halfway counts too, so Storage keeps offering to finish it.
pub fn old_bytes(data: &Path) -> Option<u64> {
    let folder = data.join(FOLDER);
    let names = match exists(&folder) {
        Ok(false) => return Some(0),
        Ok(true) => sorted(&folder).ok()?,
        Err(_) => return None,
    };
    names
        .iter()
        .filter(|name| name.to_str().is_some_and(|name| name == OLD || name.starts_with(REMOVING)))
        .try_fold(0u64, |sum, name| old_tree_bytes(&folder.join(name)).ok().map(|bytes| sum.saturating_add(bytes)))
}

fn old_tree_bytes(path: &Path) -> io::Result<u64> {
    let meta = fs::symlink_metadata(path)?;
    if linked(&meta) || !meta.is_dir() {
        return Ok(if meta.is_file() { meta.len() } else { 0 });
    }
    fs::read_dir(path)?.try_fold(0u64, |sum, entry| Ok(sum.saturating_add(old_tree_bytes(&entry?.path())?)))
}

/// The person's "Remove old copies": deletes the originals earlier moves set aside. Nothing resolves a tool to them (their
/// tools were switched before the engine started), so no session can hold them and no lock is needed. Refused while a move
/// is unfinished, since its originals may still be needed. The folder is renamed first, so a removal cut halfway never
/// leaves part of it looking kept; the next removal finishes it.
pub fn remove_old(data: &Path) -> io::Result<()> {
    let folder = data.join(FOLDER);
    real_path(&folder).map_err(|_| invalid())?;
    if !exists(&folder)? {
        return Ok(());
    }
    if exists(&folder.join(JOURNAL))? {
        return Err(io::ErrorKind::ResourceBusy.into());
    }
    let old = folder.join(OLD);
    if exists(&old)? {
        replacement::plain(&old, true).map_err(|_| invalid())?;
        super::retry_busy(|| fs::rename(&old, folder.join(format!("{REMOVING}{}", uuid::Uuid::new_v4().simple()))))?;
    }
    let mut result = Ok(());
    for entry in sorted(&folder)? {
        if entry.to_str().is_some_and(|name| name.starts_with(REMOVING))
            && let Err(error) = remove_tree(&folder.join(&entry))
        {
            result = Err(error);
        }
    }
    result
}

#[cfg(test)]
#[path = "moving_tests.rs"]
mod tests;
