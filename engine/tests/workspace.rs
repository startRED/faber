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
    let saved = save_files(&dir.path().join("attachments"), &[pdf("report.pdf"), pdf("report.pdf"), pdf("..\\..\\evil:name?.txt")]);
    let names: Vec<String> = saved.iter().map(|p| p.strip_prefix(dir.path()).unwrap().to_string_lossy().replace('\\', "/")).collect();
    assert_eq!(names, ["attachments/report.pdf", "attachments/report-1.pdf", "attachments/evil_name_.txt"]);
    assert_eq!(new_files(dir.path(), since), []);
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
