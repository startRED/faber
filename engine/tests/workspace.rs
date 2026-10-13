use faber_engine::model::FileInfo;
use faber_engine::workspace::{Attachment, inside_folder, new_files, save_files};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

#[test]
fn attachments_get_safe_unique_names_and_are_not_reported_as_created_by_the_answer() {
    let dir = tempfile::tempdir().unwrap();
    let since = now();
    let pdf = |name: &str| Attachment { name: name.into(), base64: "JVBERi0xLjQ=".into() };
    let saved =
        save_files(dir.path(), &dir.path().join("attachments"), &[pdf("report.pdf"), pdf("report.pdf"), pdf("..\\..\\evil:name?.txt")]);
    let names: Vec<String> = saved.iter().map(|p| p.strip_prefix(dir.path()).unwrap().to_string_lossy().replace('\\', "/")).collect();
    assert_eq!(names, ["attachments/report.pdf", "attachments/report-1.pdf", "attachments/evil_name_.txt"]);
    assert_eq!(new_files(dir.path(), since), []);
}

#[test]
fn attachments_accept_a_trusted_tilde_root_without_relaxing_file_boundaries() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("working~1");
    std::fs::create_dir(&root).unwrap();
    let file = || Attachment { name: "notes.pdf".into(), base64: "JVBERi0xLjQ=".into() };
    let saved = save_files(&root, &root.join("attachments"), &[file(), file()]);
    assert_eq!(saved, [root.join("attachments/notes.pdf"), root.join("attachments/notes-1.pdf")]);
    assert!(saved.iter().all(|path| path.is_file()));
    for dir in [parent.path().join("outside"), root.join(".git"), root.join("../outside")] {
        assert!(save_files(&root, &dir, &[file()]).is_empty());
    }
    assert!(!parent.path().join("outside").exists());
    assert!(!root.join(".git").exists());
}

#[test]
fn only_files_changed_since_the_turn_started_are_listed_with_forward_slashes() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.txt");
    std::fs::write(&old, "old").unwrap();
    let past = SystemTime::now() - Duration::from_secs(60);
    std::fs::File::options().write(true).open(&old).unwrap().set_modified(past).unwrap();
    let since = now();
    std::fs::create_dir(dir.path().join("out")).unwrap();
    std::fs::write(dir.path().join("out").join("chart.png"), "png").unwrap();
    assert_eq!(new_files(dir.path(), since), [FileInfo { path: "out/chart.png".into(), size: 3 }]);
}

#[test]
fn paths_outside_the_workspace_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(inside_folder(dir.path(), "a.txt"), Some(dir.path().join("a.txt")));
    assert_eq!(inside_folder(dir.path(), "../secret.txt"), None);
    assert_eq!(inside_folder(dir.path(), &std::env::temp_dir().join("elsewhere.txt").to_string_lossy()), None);
    assert_eq!(inside_folder(dir.path(), "."), None);
}

#[test]
fn r107_host_file_discovery_and_exports_do_not_expose_credentials_or_git_metadata() {
    let root = tempfile::tempdir().unwrap();
    for name in [".npmrc", "updater.key", ".env", ".git/config", ".faber/tmp/cache", "answer.txt"] {
        let path = root.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "synthetic").unwrap();
    }
    assert_eq!(new_files(root.path(), 0), [FileInfo { path: "answer.txt".into(), size: 9 }]);
    for name in [".npmrc", "updater.key", ".env"] {
        assert_eq!(inside_folder(root.path(), name), None, "{name}");
    }
}

#[test]
fn r107_host_discovery_and_exports_refuse_external_links() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "synthetic").unwrap();
    #[cfg(windows)]
    junction::create(outside.path(), root.path().join("link")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
    assert!(new_files(root.path(), 0).is_empty());
    assert_eq!(inside_folder(root.path(), "link/secret.txt"), None);
}

#[test]
fn r107_attachments_cannot_write_through_a_folder_link_outside_the_working_root() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    junction::create(outside.path(), root.path().join("attachments")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), root.path().join("attachments")).unwrap();
    let saved =
        save_files(root.path(), &root.path().join("attachments"), &[Attachment { name: "output.txt".into(), base64: "b2s=".into() }]);
    assert!(saved.is_empty());
    assert!(!outside.path().join("output.txt").exists());
}
