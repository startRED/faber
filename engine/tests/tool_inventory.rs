use crate::support;
use faber_engine::model::Vendor;
use faber_engine::tool_inventory::{List, inventory, read_with};
use serde_json::json;
use std::path::Path;

fn skill(root: &Path, relative: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(&path).unwrap();
    // Discovery must not read skill bodies or execute any code they contain.
    std::fs::write(path.join("SKILL.md"), [0xff, 0xfe]).unwrap();
}

fn names(list: &List, source: &str) -> Vec<String> {
    list.rows.iter().filter(|r| r.source == source).map(|r| r.name.clone()).collect()
}

#[test]
fn tool_inventory_lists_each_vendor_and_source_without_reading_skill_bodies() {
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let project = support::work_folder();
    skill(home.path(), ".claude/skills/personal-writing");
    skill(home.path(), ".agents/skills/personal-review");
    skill(home.path(), ".codex/skills/legacy-review");
    skill(project.path(), ".claude/skills/project-writing");
    skill(project.path(), ".agents/skills/team/project-review");
    std::fs::write(home.path().join(".claude/credentials.json"), "must never leave this file").unwrap();
    std::fs::write(home.path().join(".codex/auth.json"), "must never leave this file").unwrap();

    let lists = inventory(data.path(), Some(home.path()), Some(project.path()));
    assert_eq!(lists.len(), 2, "discovery works even before connecting either CLI");
    assert_eq!(lists[0].vendor, Vendor::Claude);
    assert_eq!(names(&lists[0], "user"), ["personal-writing"]);
    assert_eq!(names(&lists[0], "project"), ["project-writing"]);
    assert_eq!(names(&lists[1], "user"), ["legacy-review", "personal-review"]);
    assert_eq!(names(&lists[1], "project"), ["project-review"]);
    assert!(lists.iter().flat_map(|l| &l.rows).all(|r| r.catalog_id.is_none() && r.enabled.is_none()));
    assert!(lists.iter().flat_map(|l| &l.rows).all(|r| !r.location.starts_with(r"\\?\")), "display paths omit Windows API prefixes");
    assert!(!serde_json::to_string(&lists).unwrap().contains("must never leave"));
    assert!(
        lists[1].issues.iter().any(|i| i.kind == "plugin" && i.reason == "unsupported"),
        "cached folders cannot prove a plugin is loaded"
    );
}

#[test]
fn tool_inventory_plugins_use_only_installation_metadata_for_the_chosen_project() {
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let project = support::work_folder();
    let other = support::work_folder();
    let plugins = home.path().join(".claude/plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    let manifest = json!({"version":2,"plugins":{
        "personal@source":[{"scope":"user","installPath":"must never follow this path","secret":"must never expose"}],
        "team@source":[{"scope":"project","projectPath":project.path()}],
        "local@source":[{"scope":"local","projectPath":project.path()}],
        "untrusted@source":[{"scope":"project","projectPath":"\\\\untrusted.example\\private"}],
        "other@source":[{"scope":"project","projectPath":other.path()}]
    }});
    std::fs::write(plugins.join("installed_plugins.json"), manifest.to_string()).unwrap();
    let lists = inventory(data.path(), Some(home.path()), Some(project.path()));
    assert_eq!(names(&lists[0], "user"), ["personal@source"]);
    assert_eq!(names(&lists[0], "project"), ["local@source", "team@source"]);
    assert!(lists[0].rows.iter().all(|r| r.kind == "plugin" && r.enabled.is_none() && r.catalog_id.is_none()));
    assert!(lists[0].rows.iter().all(|r| r.location == "pluginRecord"), "a plugin record is not its installed location");
    assert!(!serde_json::to_string(&lists).unwrap().contains("must never"));
    let personal = inventory(data.path(), Some(home.path()), None);
    assert!(names(&personal[0], "project").is_empty());
}

#[test]
fn tool_inventory_lists_personal_link_names_without_requiring_or_inspecting_the_target() {
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let skills = home.path().join(".agents/skills");
    std::fs::create_dir_all(&skills).unwrap();
    #[cfg(windows)]
    junction::create(outside.path(), skills.join("linked-personal-skill")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), skills.join("linked-personal-skill")).unwrap();
    // A missing target cannot supply a SKILL.md or contents for the inventory to inspect.
    drop(outside);
    let lists = inventory(data.path(), Some(home.path()), None);
    assert_eq!(names(&lists[1], "user"), ["linked-personal-skill"]);
    let row = &lists[1].rows[0];
    assert_eq!(row.linked, Some(true));
    assert!(row.location.ends_with("linked-personal-skill"));
    assert_eq!(row.enabled, None, "a link name is not a confirmed enabled skill");
    assert_eq!(row.catalog_id, None, "Faber never manages personal links");
}

#[tokio::test]
async fn tool_inventory_times_out_a_blocked_reader_and_retries_do_not_add_workers() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};
    let calls = Arc::new(AtomicUsize::new(0));
    let (release, blocked) = std::sync::mpsc::channel::<()>();
    let (entered, started) = tokio::sync::oneshot::channel();
    let reads = calls.clone();
    let start = Instant::now();
    let reading = tokio::spawn(read_with(true, move || {
        reads.fetch_add(1, Ordering::SeqCst);
        let _ = entered.send(());
        let _ = blocked.recv_timeout(Duration::from_secs(10));
        Ok(Vec::new())
    }));
    started.await.unwrap();
    let lists = reading.await.unwrap().unwrap();
    assert!(start.elapsed() < Duration::from_secs(4), "the two-second deadline does not wait for the blocked disk");
    assert_eq!(lists.len(), 2);
    assert!(lists.iter().all(|l| l.rows.is_empty() && l.issues.len() == 4 && l.issues.iter().all(|i| i.reason == "unavailable")));
    for _ in 0..3 {
        let reads = calls.clone();
        let retry = read_with(false, move || {
            reads.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        })
        .await
        .unwrap();
        assert!(retry.iter().all(|l| l.rows.is_empty() && l.issues.len() == 2));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the timed-out worker retains the only scan permit");
    release.send(()).unwrap();
}

#[test]
fn tool_inventory_catalog_rows_keep_ownership_and_on_off_without_scanning_the_store() {
    let data = tempfile::tempdir().unwrap();
    let item = faber_engine::tools::catalog().iter().find(|item| item.server.is_none()).unwrap();
    skill(data.path(), &format!("tools/plugin/skills/{}", item.id));
    std::fs::write(data.path().join("tools/installed.json"), json!({(item.id.clone()):item.commit}).to_string()).unwrap();
    let lists = inventory(data.path(), None, None);
    for list in &lists {
        let row = list.rows.iter().find(|r| r.catalog_id.as_deref() == Some(item.id.as_str())).unwrap();
        assert_eq!(row.source, "catalog");
        assert_eq!(row.enabled, Some(true));
    }
    std::fs::create_dir_all(data.path().join("tools/disabled")).unwrap();
    std::fs::rename(data.path().join(format!("tools/plugin/skills/{}", item.id)), data.path().join(format!("tools/disabled/{}", item.id)))
        .unwrap();
    assert!(inventory(data.path(), None, None).iter().all(|list| list.rows[0].enabled == Some(false)));
}

#[test]
fn tool_inventory_bounds_scans_and_marks_invalid_or_oversized_manifests() {
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let plugins = home.path().join(".claude/plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    for text in ["broken json".to_string(), " ".repeat(1024 * 1024 + 1)] {
        std::fs::write(plugins.join("installed_plugins.json"), text).unwrap();
        let lists = inventory(data.path(), Some(home.path()), None);
        assert!(lists[0].issues.iter().any(|i| i.kind == "plugin" && i.reason == "unreadable"));
    }
    for index in 0..257 {
        skill(home.path(), &format!(".agents/skills/s{index:03}"));
    }
    let lists = inventory(data.path(), Some(home.path()), None);
    assert!(lists[1].rows.len() <= 256);
    assert!(lists[1].issues.iter().any(|i| i.kind == "skill" && i.reason == "limited"));
}

#[test]
fn tool_inventory_never_follows_skill_or_manifest_links() {
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let project = support::work_folder();
    let outside = tempfile::tempdir().unwrap();
    skill(outside.path(), "private");
    std::fs::create_dir_all(project.path().join(".claude")).unwrap();
    #[cfg(windows)]
    junction::create(outside.path(), project.path().join(".claude/skills")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), project.path().join(".claude/skills")).unwrap();
    let lists = inventory(data.path(), Some(home.path()), Some(project.path()));
    assert!(names(&lists[0], "project").is_empty());
    assert!(lists[0].issues.iter().any(|i| i.source == "project" && i.reason == "unreadable"));
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::fs::write(outside.path().join("installed_plugins.json"), r#"{"version":2,"plugins":{"private@source":[{"scope":"user"}]}}"#)
        .unwrap();
    #[cfg(windows)]
    junction::create(outside.path(), home.path().join(".claude/plugins")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), home.path().join(".claude/plugins")).unwrap();
    let lists = inventory(data.path(), Some(home.path()), Some(project.path()));
    assert!(names(&lists[0], "user").is_empty());
    assert!(lists[0].issues.iter().any(|i| i.source == "user" && i.kind == "plugin" && i.reason == "unreadable"));
    // A home chosen as a project is still not a project skill source.
    let lists = inventory(data.path(), Some(home.path()), Some(home.path()));
    assert!(lists.iter().all(|l| names(l, "project").is_empty()));
}
