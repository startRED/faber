//! Bounded Git evidence for a lead recovering an interrupted helper's working copy.
use super::{Duration, Executor, Instant, Path, Request, Serialize, WorkCopy, safe};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

const TIME: Duration = Duration::from_secs(3);
const STATUS_BYTES: usize = 32 * 1024;
const CONFLICT_LIMIT: usize = 20;
const CONFLICT_BYTES: usize = 2048;
const PATH_BYTES: usize = 512;
const GIT_PATH_BYTES: usize = 8192;

#[derive(Serialize, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum State {
    Clean,
    Changed,
    #[default]
    Unknown,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    state: State,
    merge_in_progress: Option<bool>,
    rebase_in_progress: Option<bool>,
    mail_apply_in_progress: Option<bool>,
    cherry_pick_head: Option<bool>,
    revert_head: Option<bool>,
    conflicts: Vec<String>,
    conflicts_incomplete: bool,
    scope: String,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            state: State::Unknown,
            merge_in_progress: None,
            rebase_in_progress: None,
            mail_apply_in_progress: None,
            cherry_pick_head: None,
            revert_head: None,
            conflicts: Vec::new(),
            conflicts_incomplete: true,
            scope: String::new(),
        }
    }
}

pub fn snapshot(exec: &mut dyn Executor, copy: &WorkCopy) -> Snapshot {
    if copy.sub.len() > PATH_BYTES || (!copy.sub.is_empty() && !safe(&copy.sub)) {
        return Snapshot::default();
    }
    let root = Path::new(&copy.path);
    let source = Path::new(&copy.root);
    if !root.is_absolute() || !source.is_absolute() {
        return Snapshot::default();
    }
    let mut request = Request { exec, deadline: Instant::now() + TIME };
    let Some(dir) = linked_metadata(&mut request, root, source) else { return Snapshot::default() };
    let (rebase_in_progress, mail_apply_in_progress) = applying(&dir);
    let mut read = Snapshot {
        merge_in_progress: present(&dir.join("MERGE_HEAD"), false),
        rebase_in_progress,
        mail_apply_in_progress,
        cherry_pick_head: present(&dir.join("CHERRY_PICK_HEAD"), false),
        revert_head: present(&dir.join("REVERT_HEAD"), false),
        scope: copy.sub.clone(),
        ..Snapshot::default()
    };
    // Bind the status read to the verified metadata even if the worktree's pointer changes between reads.
    let git_dir = format!("--git-dir={}", crate::project::git::native_path(&dir));
    let mut args = vec![
        git_dir.as_str(),
        "status",
        "--porcelain=v1",
        "-z",
        "--no-renames",
        "--untracked-files=normal",
        "--ignore-submodules=dirty",
        "--",
    ];
    if !copy.sub.is_empty() {
        // The runner uses literal paths, and this command runs at the repository root.
        args.push(&copy.sub);
    }
    if let Some(bytes) = request.git(root, &args, 0, STATUS_BYTES) {
        read.status(&bytes);
    }
    read
}

fn path_text(bytes: &[u8]) -> Option<&str> {
    if bytes.len() > GIT_PATH_BYTES {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?.trim_end_matches(['\r', '\n']);
    (!text.is_empty() && !text.contains(['\0', '\r', '\n'])).then_some(text)
}

fn path_file(folder: &Path, name: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    faber_winproc::open_plain_file(folder, Path::new(name)).ok()?.take((GIT_PATH_BYTES + 1) as u64).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn lexical_path(path: &Path) -> Option<PathBuf> {
    let spelling = crate::project::git::native_path(path);
    let mut normalized = PathBuf::new();
    for part in Path::new(&spelling).components() {
        if part == std::path::Component::ParentDir {
            if !normalized.pop() {
                return None;
            }
        } else {
            normalized.push(part);
        }
    }
    normalized.is_absolute().then_some(normalized)
}

fn metadata_location(base: &Path, text: &str) -> Option<PathBuf> {
    use std::path::Component::{Normal, ParentDir};
    let path = Path::new(text);
    // Git's relative pointers walk up first. Reject a later parent step, which could cross a directory link.
    if path.components().skip_while(|part| !matches!(part, Normal(_))).any(|part| part == ParentDir) {
        return None;
    }
    lexical_path(&base.join(path))
}

fn linked_metadata(request: &mut Request<'_>, root: &Path, source: &Path) -> Option<PathBuf> {
    let source = crate::permissions::checked_folder(source)?;
    let common = request.git(&source, &["rev-parse", "--path-format=absolute", "--git-common-dir"], 0, GIT_PATH_BYTES)?;
    let common = Path::new(path_text(&common)?);
    if !common.is_absolute() {
        return None;
    }
    let common = fs::canonicalize(crate::permissions::checked_folder(common)?).ok()?;
    let pointer = root.join(".git");
    let target = path_file(root, ".git")?;
    let real_root = lexical_path(&fs::canonicalize(root).ok()?)?;
    let descriptor = metadata_location(&real_root, path_text(&target)?.strip_prefix("gitdir: ")?)?;
    let registry = common.join("worktrees");
    // Check the pointer against the known local spelling before touching its target (which could be a network path).
    if descriptor.parent()? != lexical_path(&registry)? {
        return None;
    }
    let target = crate::project::lease::real_path(&registry.join(descriptor.file_name()?)).ok()?;
    if !target.is_dir() {
        return None;
    }
    let shared = path_file(&target, "commondir")?;
    let back = path_file(&target, "gitdir")?;
    let back = metadata_location(&target, path_text(&back)?)?;
    let points_back = back == lexical_path(&pointer)? || back == real_root.join(".git");
    (points_back && metadata_location(&target, path_text(&shared)?)? == lexical_path(&common)?).then_some(target)
}

fn present(path: &Path, directory: bool) -> Option<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if if directory { meta.is_dir() } else { meta.is_file() } => Some(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
        _ => None,
    }
}

fn applying(dir: &Path) -> (Option<bool>, Option<bool>) {
    let apply = dir.join("rebase-apply");
    match present(&apply, true) {
        Some(true) => {
            let mail = present(&apply.join("applying"), false);
            (mail.map(|mail| !mail), mail)
        }
        Some(false) => (present(&dir.join("rebase-merge"), true), Some(false)),
        None => (None, None),
    }
}

impl Snapshot {
    fn status(&mut self, bytes: &[u8]) {
        let complete = bytes.len() <= STATUS_BYTES && (bytes.is_empty() || bytes.last() == Some(&0));
        let mut valid = true;
        let mut omitted = false;
        let mut budget = CONFLICT_BYTES - 1;
        for row in bytes.split_inclusive(|byte| *byte == 0) {
            let Some(row) = row.strip_suffix(&[0]) else { break };
            let [x, y, b' ', name @ ..] = row else {
                valid = false;
                continue;
            };
            if name.is_empty() || [x, y].iter().any(|code| !b" MADTU?!".contains(code)) {
                valid = false;
                continue;
            }
            if !matches!([*x, *y], [b'D', b'D'] | [b'A', b'U'] | [b'U', b'D'] | [b'U', b'A'] | [b'D', b'U'] | [b'A', b'A'] | [b'U', b'U']) {
                continue;
            }
            // Names are returned only when the whole path fits, so there is no misleading partial filename.
            if self.conflicts.len() < CONFLICT_LIMIT
                && name.len() <= PATH_BYTES
                && let Ok(path) = std::str::from_utf8(name)
                && safe(path)
                && let Ok(encoded) = serde_json::to_string(path)
                && encoded.len() < budget
            {
                budget -= encoded.len() + 1;
                self.conflicts.push(path.into());
            } else {
                omitted = true;
            }
        }
        if complete && valid {
            self.state = if bytes.is_empty() { State::Clean } else { State::Changed };
        }
        self.conflicts_incomplete = !complete || !valid || omitted || !self.scope.is_empty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::git::{GitError, Invocation, Output};

    struct Git<'a> {
        common: &'a Path,
        status_reads: usize,
        status: Result<&'a [u8], GitError>,
        deadline: Option<Instant>,
    }

    impl Executor for Git<'_> {
        fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
            let deadline = call.deadline.expect("every Git read has a deadline");
            assert_eq!(*self.deadline.get_or_insert(deadline), deadline);
            let words: Vec<_> = call.args.iter().map(String::as_str).collect();
            let stdout = if words.ends_with(&["--path-format=absolute", "--git-common-dir"]) {
                format!("{}\n", self.common.display()).into_bytes()
            } else {
                self.status_reads += 1;
                assert!(words.iter().any(|word| word.starts_with("--git-dir=")));
                assert!(words.contains(&"--no-renames"));
                assert!(words.ends_with(&["--", "app[1]"]));
                self.status?.to_vec()
            };
            Ok(Output { success: true, stdout })
        }
    }

    #[test]
    fn snapshots_are_bounded_and_use_the_linked_worktrees_own_markers() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Worktree");
        let source = root.join("source");
        let common = source.join(".git");
        let metadata = common.join("worktrees").join("copy");
        fs::create_dir_all(&metadata).unwrap();
        let pointer = root.join(".git");
        fs::write(&pointer, "gitdir: source/.git/worktrees/copy\n").unwrap();
        fs::write(metadata.join("gitdir"), "../../../../.git\n").unwrap();
        fs::write(metadata.join("commondir"), "../..\n").unwrap();
        let copy = WorkCopy {
            path: root.display().to_string(),
            branch: "faber/test".into(),
            root: source.display().to_string(),
            sub: "app[1]".into(),
            base: None,
        };
        let fixture = |status| Git { common: &common, status, deadline: None, status_reads: 0 };
        let run = |status| snapshot(&mut fixture(status), &copy);
        let clean = run(Ok(b""));
        assert_eq!(clean.state, State::Clean);
        assert_eq!((clean.merge_in_progress, clean.rebase_in_progress), (Some(false), Some(false)));
        #[cfg(windows)]
        {
            let alias = dir.path().join("worktree");
            if alias.is_dir() {
                let aliased = WorkCopy { path: alias.display().to_string(), ..copy.clone() };
                assert_eq!(snapshot(&mut fixture(Ok(b"")), &aliased), clean);
            }
        }
        std::fs::write(metadata.join("MERGE_HEAD"), "").unwrap();
        std::fs::create_dir(metadata.join("rebase-merge")).unwrap();
        let merged = run(Ok("UU app[1]/ação.txt\0?? app[1]/new\0".as_bytes()));
        assert_eq!(merged.state, State::Changed);
        assert_eq!(merged.conflicts, ["app[1]/ação.txt"]);
        assert_eq!((merged.merge_in_progress, merged.rebase_in_progress), (Some(true), Some(true)));
        assert_eq!(merged.scope, "app[1]");
        assert!(merged.conflicts_incomplete, "a subproject scan cannot claim all worktree conflicts");
        let large = [b"UU app[1]/seen\0".as_slice(), &vec![b'x'; STATUS_BYTES]].concat();
        let cut = run(Ok(&large));
        assert_eq!(cut.state, State::Unknown);
        assert_eq!(cut.conflicts, ["app[1]/seen"]);
        assert!(cut.conflicts_incomplete);
        assert_eq!(run(Err(GitError::Command)).state, State::Unknown);
        fs::write(metadata.join("gitdir"), "not this worktree\n").unwrap();
        let mut foreign = fixture(Ok(b"UU hidden\0"));
        assert_eq!(snapshot(&mut foreign, &copy), Snapshot::default());
        assert_eq!(foreign.status_reads, 0, "a broken link cannot expose Git status");
        let unregistered = dir.path().join("unregistered");
        fs::create_dir(&unregistered).unwrap();
        fs::write(&pointer, format!("gitdir: {}\n", unregistered.display())).unwrap();
        let mut foreign = fixture(Ok(b"UU hidden\0"));
        assert_eq!(snapshot(&mut foreign, &copy), Snapshot::default());
        assert_eq!(foreign.status_reads, 0, "an unregistered metadata directory is never queried");
    }

    #[test]
    fn mail_apply_is_distinct_from_rebase() {
        let dir = tempfile::tempdir().unwrap();
        let apply = dir.path().join("rebase-apply");
        std::fs::create_dir(&apply).unwrap();
        assert_eq!(applying(dir.path()), (Some(true), Some(false)));
        std::fs::write(apply.join("applying"), "").unwrap();
        assert_eq!(applying(dir.path()), (Some(false), Some(true)));
    }

    #[test]
    fn escaped_filenames_fit_the_result_budget_without_partial_names() {
        let name = "\n".repeat(PATH_BYTES - 10);
        let rows = format!("UU {name}\0").repeat(CONFLICT_LIMIT);
        let mut snapshot = Snapshot::default();
        snapshot.status(rows.as_bytes());
        assert_eq!(snapshot.state, State::Changed);
        assert!(snapshot.conflicts_incomplete);
        assert_eq!(snapshot.conflicts, [name.clone(), name]);
        assert!(serde_json::to_string(&snapshot.conflicts).unwrap().len() <= CONFLICT_BYTES);
    }
}
