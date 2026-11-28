//! A bounded, read-only measurement of the data folder's regular files.
use serde::Serialize;
use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 250_000;
const TIME_LIMIT: Duration = Duration::from_secs(3);
const PROJECT_HEADER_BYTES: u64 = 256 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    pub total_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub by_kind: StorageKinds,
    pub copies_by_project: BTreeMap<String, Option<u64>>,
    pub unassigned_copy_bytes: Option<u64>,
    pub partial: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageKinds {
    pub copies: Option<u64>,
    pub reports: Option<u64>,
    pub tools: Option<u64>,
    pub rewind: Option<u64>,
    pub helper_transcripts: Option<u64>,
    pub conversations: Option<u64>,
    pub rest: Option<u64>,
}

impl StorageKinds {
    fn with_bytes(bytes: Option<u64>) -> Self {
        Self { copies: bytes, reports: bytes, tools: bytes, rewind: bytes, helper_transcripts: bytes, conversations: bytes, rest: bytes }
    }

    fn total(&self) -> Option<u64> {
        [self.copies, self.reports, self.tools, self.rewind, self.helper_transcripts, self.conversations, self.rest]
            .into_iter()
            .try_fold(0u64, |total, bytes| total.checked_add(bytes?))
    }
}

pub fn measure(data: &Path, project_of: impl Fn(&str) -> Option<String>) -> StorageUsage {
    scan(data, project_of, Budget { left: MAX_ENTRIES, deadline: Instant::now() + TIME_LIMIT, partial: false })
}

struct Budget {
    left: usize,
    deadline: Instant,
    partial: bool,
}

impl Budget {
    fn next(&mut self, folder: &mut Folder) -> Option<io::Result<OsString>> {
        if self.left == 0 || Instant::now() >= self.deadline {
            self.partial = true;
            return None;
        }
        let entry = folder.next()?;
        self.left -= 1;
        Some(entry)
    }
}

fn add(total: &mut Option<u64>, bytes: Option<u64>) {
    *total = total.zip(bytes).and_then(|(total, bytes)| total.checked_add(bytes));
}

fn is_name(name: &OsStr, expected: &str) -> bool {
    if cfg!(windows) { name.eq_ignore_ascii_case(expected) } else { name == expected }
}

fn scan(data: &Path, project_of: impl Fn(&str) -> Option<String>, mut budget: Budget) -> StorageUsage {
    let mut usage = StorageUsage {
        total_bytes: None,
        free_bytes: None,
        by_kind: StorageKinds::with_bytes(None),
        copies_by_project: BTreeMap::new(),
        unassigned_copy_bytes: None,
        partial: false,
    };
    let Ok(mut root) = Folder::root(data) else {
        usage.partial = Instant::now() >= budget.deadline;
        return usage;
    };
    usage.free_bytes = root.available_space().ok();
    usage.by_kind = StorageKinds::with_bytes(Some(0));
    usage.unassigned_copy_bytes = Some(0);
    while let Some(entry) = budget.next(&mut root) {
        let Ok(name) = entry else {
            // An enumeration error without a name could belong to any kind.
            usage.by_kind = StorageKinds::with_bytes(None);
            usage.unassigned_copy_bytes = None;
            for bytes in usage.copies_by_project.values_mut() {
                *bytes = None;
            }
            break;
        };
        if is_name(&name, "copies") {
            add(
                &mut usage.by_kind.copies,
                copy_bytes(&root, &name, &project_of, &mut budget, &mut usage.copies_by_project, &mut usage.unassigned_copy_bytes),
            );
            continue;
        }
        let total = if is_name(&name, "reports") {
            &mut usage.by_kind.reports
        } else if is_name(&name, "tools") {
            &mut usage.by_kind.tools
        } else if is_name(&name, "rewind") {
            &mut usage.by_kind.rewind
        } else if is_name(&name, crate::store::TRANSCRIPTS_DIR) {
            &mut usage.by_kind.helper_transcripts
        } else if is_name(&name, crate::store::CONVERSATIONS_DIR) {
            &mut usage.by_kind.conversations
        } else {
            &mut usage.by_kind.rest
        };
        add(total, tree_bytes(&root, &name, &mut budget));
    }
    usage.total_bytes = usage.by_kind.total();
    usage.partial = budget.partial || Instant::now() >= budget.deadline;
    usage
}

enum Entry {
    File(u64),
    Directory,
    Skip,
}

fn copy_bytes(
    root: &Folder,
    name: &OsStr,
    project_of: &impl Fn(&str) -> Option<String>,
    budget: &mut Budget,
    projects: &mut BTreeMap<String, Option<u64>>,
    unassigned: &mut Option<u64>,
) -> Option<u64> {
    let folder = match root.entry(name) {
        Ok(Entry::Directory) => root.child(name),
        Ok(Entry::Skip) => return Some(0),
        Ok(Entry::File(bytes)) => {
            add(unassigned, Some(bytes));
            return Some(bytes);
        }
        Err(error) => Err(error),
    };
    let Ok(mut folder) = folder else {
        *unassigned = None;
        return None;
    };
    let mut conversations = None;
    let mut total = Some(0);
    while let Some(entry) = budget.next(&mut folder) {
        let Ok(id) = entry else {
            // The unseen copy may belong to any project.
            for bytes in projects.values_mut() {
                *bytes = None;
            }
            *unassigned = None;
            return None;
        };
        let entry = folder.entry(&id);
        if matches!(entry, Ok(Entry::Skip)) {
            continue;
        }
        let bytes = entry.ok().and_then(|entry| entry_bytes(&folder, &id, entry, budget));
        add(&mut total, bytes);
        let project = id.to_str().and_then(|id| {
            project_of(id).or_else(|| {
                if Instant::now() >= budget.deadline
                    || id.is_empty()
                    || id.len() > 64
                    || !id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                {
                    return None;
                }
                let folder = conversations.get_or_insert_with(|| root.child(OsStr::new(crate::store::CONVERSATIONS_DIR)).ok());
                let file = format!("{id}.json");
                let record = folder.as_ref()?.read_file(OsStr::new(&file)).ok()?;
                project_header(record, id)
            })
        });
        let bucket = match project {
            Some(project) => projects.entry(project).or_insert(Some(0)),
            None => &mut *unassigned,
        };
        add(bucket, bytes);
    }
    total
}

fn project_header(record: impl Read, id: &str) -> Option<String> {
    struct Header<'a> {
        id: &'a str,
        project: &'a mut Option<String>,
    }

    impl<'de> Visitor<'de> for Header<'_> {
        type Value = ();

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a conversation record")
        }

        fn visit_map<M: MapAccess<'de>>(self, mut fields: M) -> Result<(), M::Error> {
            let mut matches = false;
            let mut project = None;
            while let Some(key) = fields.next_key::<String>()? {
                match key.as_str() {
                    "id" => matches = fields.next_value::<String>()? == self.id,
                    "projectId" => project = fields.next_value::<Option<String>>()?,
                    "messages" => break,
                    _ => {
                        fields.next_value::<IgnoredAny>()?;
                    }
                }
                if matches && project.is_some() {
                    *self.project = project;
                    break;
                }
            }
            Ok(())
        }
    }

    // The header is written before the messages. The parser can stop there even when the record itself is large.
    let mut project = None;
    let mut json = serde_json::Deserializer::from_reader(io::BufReader::new(record.take(PROJECT_HEADER_BYTES)));
    // Returning before the end of the object can leave trailing JSON, which this prefix lookup does not need.
    let _ = json.deserialize_map(Header { id, project: &mut project });
    project
}

fn tree_bytes(parent: &Folder, name: &OsStr, budget: &mut Budget) -> Option<u64> {
    entry_bytes(parent, name, parent.entry(name).ok()?, budget)
}

fn entry_bytes(parent: &Folder, name: &OsStr, entry: Entry, budget: &mut Budget) -> Option<u64> {
    let mut stack = match entry {
        Entry::File(bytes) => return Some(bytes),
        Entry::Skip => return Some(0),
        Entry::Directory => vec![parent.child(name).ok()?],
    };
    let mut bytes = 0u64;
    while let Some(folder) = stack.last_mut() {
        let Some(entry) = budget.next(folder) else {
            if budget.partial {
                break;
            }
            stack.pop();
            continue;
        };
        let name = entry.ok()?;
        match folder.entry(&name).ok()? {
            Entry::Skip => {}
            Entry::File(len) => bytes = bytes.checked_add(len)?,
            Entry::Directory => {
                let child = folder.child(&name).ok()?;
                stack.push(child);
            }
        }
    }
    Some(bytes)
}

#[cfg(windows)]
struct Folder {
    path: std::path::PathBuf,
    entries: fs::ReadDir,
    _held: fs::File,
    _root_ancestors: Option<faber_winproc::MemoryDirectory>,
}

#[cfg(windows)]
impl Folder {
    fn root(path: &Path) -> io::Result<Self> {
        let held = faber_winproc::MemoryDirectory::open(path, Path::new(""))?;
        let mut folder = Self::open(held.resolved_path()?)?;
        folder._root_ancestors = Some(held);
        Ok(folder)
    }

    fn open(path: std::path::PathBuf) -> io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        // Hold the directory without write/delete sharing so no rename or reparse change can redirect read_dir.
        let held = fs::OpenOptions::new().read(true).share_mode(1).custom_flags(0x0020_0000 | 0x0200_0000).open(&path)?;
        if !matches!(classify(&held.metadata()?), Entry::Directory) {
            return Err(io::ErrorKind::NotADirectory.into());
        }
        Ok(Self { entries: fs::read_dir(&path)?, path, _held: held, _root_ancestors: None })
    }

    fn available_space(&self) -> io::Result<u64> {
        faber_winproc::rewind::Root::open(&self.path)?.available_space()
    }

    fn next(&mut self) -> Option<io::Result<OsString>> {
        self.entries.next().map(|entry| entry.map(|entry| entry.file_name()))
    }

    fn child(&self, name: &OsStr) -> io::Result<Self> {
        Self::open(self.path.join(name))
    }

    fn entry(&self, name: &OsStr) -> io::Result<Entry> {
        fs::symlink_metadata(self.path.join(name)).map(|meta| classify(&meta))
    }

    fn read_file(&self, name: &OsStr) -> io::Result<fs::File> {
        use std::os::windows::fs::OpenOptionsExt;
        let file = fs::OpenOptions::new().read(true).custom_flags(0x0020_0000).open(self.path.join(name))?;
        if !matches!(classify(&file.metadata()?), Entry::File(_)) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(file)
    }
}

#[cfg(windows)]
fn classify(meta: &fs::Metadata) -> Entry {
    use std::os::windows::fs::MetadataExt;
    if meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0 {
        Entry::Skip
    } else if meta.is_dir() {
        Entry::Directory
    } else if meta.is_file() {
        Entry::File(meta.len())
    } else {
        Entry::Skip
    }
}

#[cfg(unix)]
struct Folder {
    handle: std::os::fd::OwnedFd,
    entries: rustix::fs::Dir,
}

#[cfg(unix)]
impl Folder {
    const FLAGS: rustix::fs::OFlags = rustix::fs::OFlags::RDONLY
        .union(rustix::fs::OFlags::DIRECTORY)
        .union(rustix::fs::OFlags::NOFOLLOW)
        .union(rustix::fs::OFlags::CLOEXEC);

    fn root(path: &Path) -> io::Result<Self> {
        if !fs::symlink_metadata(path)?.is_dir() {
            return Err(io::ErrorKind::NotADirectory.into());
        }
        Self::open(rustix::fs::open(path, Self::FLAGS, rustix::fs::Mode::empty())?)
    }

    fn open(handle: std::os::fd::OwnedFd) -> io::Result<Self> {
        let entries = rustix::fs::Dir::read_from(&handle)?;
        Ok(Self { handle, entries })
    }

    fn available_space(&self) -> io::Result<u64> {
        let stat = rustix::fs::fstatvfs(&self.handle)?;
        Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
    }

    fn next(&mut self) -> Option<io::Result<OsString>> {
        use std::os::unix::ffi::OsStrExt;
        loop {
            let entry = match self.entries.next()? {
                Ok(entry) => entry,
                Err(error) => return Some(Err(error.into())),
            };
            let name = entry.file_name().to_bytes();
            if name != b"." && name != b".." {
                return Some(Ok(OsStr::from_bytes(name).to_owned()));
            }
        }
    }

    fn child(&self, name: &OsStr) -> io::Result<Self> {
        Self::open(rustix::fs::openat(&self.handle, name, Self::FLAGS, rustix::fs::Mode::empty())?)
    }

    fn entry(&self, name: &OsStr) -> io::Result<Entry> {
        use rustix::fs::{AtFlags, FileType, statat};
        let meta = statat(&self.handle, name, AtFlags::SYMLINK_NOFOLLOW)?;
        Ok(match FileType::from_raw_mode(meta.st_mode) {
            FileType::RegularFile => Entry::File(meta.st_size.try_into().map_err(io::Error::other)?),
            FileType::Directory => Entry::Directory,
            _ => Entry::Skip,
        })
    }

    fn read_file(&self, name: &OsStr) -> io::Result<fs::File> {
        use rustix::fs::{Mode, OFlags, openat};
        let handle = openat(&self.handle, name, OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK, Mode::empty())?;
        let file = fs::File::from(handle);
        if !file.metadata()?.is_file() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(file)
    }
}

pub const CHOICE_FILE: &str = "Faber-storage.json";

/// The profile records only absolute locations in the app's own directory namespace.
pub fn external_root_name(root: &Path) -> bool {
    use std::path::Component;

    let shaped = root.is_absolute()
        && root.file_name().is_some_and(|name| name == "Faber-storage")
        && !root.components().any(|part| matches!(part, Component::ParentDir));
    #[cfg(windows)]
    {
        use std::path::Prefix;
        shaped
            && matches!(root.components().next(), Some(Component::Prefix(prefix))
                if matches!(prefix.kind(), Prefix::Disk(letter) | Prefix::VerbatimDisk(letter)
                    if !faber_winproc::is_remote_drive(letter as char)))
    }
    #[cfg(not(windows))]
    shaped
}

/// A dedicated external folder belongs to this profile only when its marker matches.
pub fn owns_external_root(data: &Path, root: &Path) -> bool {
    use std::io::Read;

    let marker = root.join(".faber-owner");
    let Some(expected) = data.to_str() else { return false };
    if !fs::symlink_metadata(&marker).is_ok_and(|meta| meta.is_file() && meta.len() == expected.len() as u64) {
        return false;
    }
    let mut bytes = Vec::new();
    fs::File::open(marker).and_then(|file| file.take(expected.len() as u64 + 1).read_to_end(&mut bytes)).is_ok()
        && bytes == expected.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_measures_kinds_and_projects_without_following_links() {
        let data = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        for (file, bytes) in [
            ("copies/c1/file", 3),
            ("copies/c2/file", 5),
            ("copies/c3/nested/file", 7),
            ("copies/orphan/file", 11),
            ("reports/report", 13),
            ("tools/tool", 17),
            ("rewind/point", 19),
            ("child-transcripts/page", 23),
            ("conversations/c1.json", 29),
            ("rest", 31),
        ] {
            let path = data.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, vec![0; bytes]).unwrap();
        }
        let record = br#"{"id":"c3","projectId":"second","messages":[{}]}"#;
        fs::write(data.path().join("conversations/c3.json"), record).unwrap();
        fs::write(outside.path().join("target"), [0; 100]).unwrap();
        let link = data.path().join("copies/c1/outside");
        #[cfg(unix)]
        let linked = {
            std::os::unix::fs::symlink(outside.path(), &link).unwrap();
            true
        };
        #[cfg(windows)]
        // The rest of the test still runs on Windows volumes that cannot create a junction.
        let linked = junction::create(outside.path(), &link).is_ok();

        let usage = measure(data.path(), |id| match id {
            "c1" | "c2" => Some("first".into()),
            _ => None,
        });
        if linked {
            #[cfg(windows)]
            junction::delete(link).unwrap();
        }
        assert_eq!(usage.by_kind.copies, Some(26));
        assert_eq!(usage.by_kind.reports, Some(13));
        assert_eq!(usage.by_kind.tools, Some(17));
        assert_eq!(usage.by_kind.rewind, Some(19));
        assert_eq!(usage.by_kind.helper_transcripts, Some(23));
        assert_eq!(usage.by_kind.conversations, Some(29 + record.len() as u64));
        assert_eq!(usage.by_kind.rest, Some(31));
        assert_eq!(usage.total_bytes, Some(158 + record.len() as u64));
        assert!(usage.free_bytes.is_some());
        assert_eq!(usage.copies_by_project, BTreeMap::from([("first".into(), Some(8)), ("second".into(), Some(7))]));
        assert_eq!(usage.unassigned_copy_bytes, Some(11));
        assert!(!usage.partial);
    }

    #[test]
    fn storage_returns_partial_at_either_limit() {
        let data = tempfile::tempdir().unwrap();
        fs::write(data.path().join("file"), [1, 2, 3]).unwrap();
        fs::write(data.path().join("another"), [4, 5, 6]).unwrap();
        let measured = scan(data.path(), |_| None, Budget { left: 1, deadline: Instant::now() + TIME_LIMIT, partial: false });
        assert!(measured.partial);
        assert_eq!(measured.total_bytes, Some(3));
        let timed_out = scan(data.path(), |_| None, Budget { left: MAX_ENTRIES, deadline: Instant::now(), partial: false });
        assert!(timed_out.partial);
        assert_eq!(timed_out.total_bytes, Some(0));
    }

    #[test]
    fn storage_marks_an_unreadable_kind_unknown() {
        let data = tempfile::tempdir().unwrap();
        let blocked = data.path().join("tools/blocked");
        fs::create_dir_all(&blocked).unwrap();
        fs::write(blocked.join("file"), [0; 8]).unwrap();
        fs::write(data.path().join("settings.json"), [0; 3]).unwrap();

        #[cfg(unix)]
        let accessible = {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
            fs::read_dir(&blocked).is_ok()
        };
        #[cfg(windows)]
        let (_hold, accessible) = {
            use std::os::windows::fs::OpenOptionsExt;
            let hold = fs::OpenOptions::new().read(true).share_mode(0).custom_flags(0x0200_0000).open(&blocked).unwrap();
            (hold, false)
        };
        let measured = measure(data.path(), |_| None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
        }

        // A privileged Unix runner may still have access after chmod.
        if !accessible {
            assert_eq!(measured.by_kind.tools, None);
            assert_eq!(measured.total_bytes, None);
        }
        assert_eq!(measured.by_kind.rest, Some(3));
    }
}
