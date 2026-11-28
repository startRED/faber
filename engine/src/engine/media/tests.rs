use super::*;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nimage bytes";

#[tokio::test]
async fn protocol_reads_apply_live_settings_and_refuse_released_or_old_web_tokens() {
    use crate::model::{Settings, WebSearch};
    let dir = crate::workspace::TestRoot::new();
    let (engine, _) = crate::engine::answer::tests::engine(&dir);
    let lease = engine.media_enter("visible");
    let insert = || {
        engine.with(|core| {
            let url = Arc::new(reqwest::Url::parse("https://127.0.0.1/picture").unwrap());
            core.media.insert("visible", Grant::Web(url)).unwrap().simple().to_string()
        })
    };
    let first = insert();
    let second = insert();
    assert_ne!(first, second);
    for (web, expected) in
        [(WebSearch::Off, "error.media.webOff"), (WebSearch::Cached, "error.media.webCached"), (WebSearch::Live, "error.media.webPrivate")]
    {
        engine.save_settings(Settings { web_search: Some(web), ..Settings::default() }).unwrap();
        assert_eq!(engine.media_read_route(&second, None).await.unwrap_err().key, expected);
    }
    engine.media_release(&lease, &first);
    assert_eq!(engine.media_read_route(&first, None).await.unwrap_err().key, CLOSED);
    engine.media_leave(&lease);
    assert_eq!(engine.media_read_route(&second, None).await.unwrap_err().key, CLOSED);
}

fn pin(path: &Path) -> Root {
    crate::visual::pin(&path.canonicalize().unwrap()).unwrap()
}

#[test]
fn ranges_return_exact_bytes_and_cap_open_ended_reads() {
    let folder = tempfile::tempdir().unwrap();
    let mut bytes = PNG.to_vec();
    bytes.resize(2_200_000, 7);
    std::fs::write(folder.path().join("picture.png"), &bytes).unwrap();
    let root = pin(folder.path());
    let chunk = read(&root, "picture.png", Some("bytes=2-4")).unwrap();
    assert_eq!(chunk.bytes, &bytes[2..5]);
    assert_eq!(chunk.content_range.as_deref(), Some("bytes 2-4/2200000"));
    assert_eq!(read(&root, "picture.png", Some("bytes=-3")).unwrap().bytes, [7; 3]);
    assert_eq!(read(&root, "picture.png", Some("bytes=8-")).unwrap().bytes.len(), 1_048_576);
    for range in ["bytes=2200000-", "bytes=4-3", "bytes=1-2,3-4", "bytes=-0"] {
        assert_eq!(read(&root, "picture.png", Some(range)).unwrap_err().key, "error.media.range");
    }
}

#[test]
fn paths_and_kinds_have_to_match_the_pinned_reader() {
    let folder = tempfile::tempdir().unwrap();
    let root = pin(folder.path());
    for path in ["../picture.png", "/picture.png", ".CoDeX/picture.png", "folder/../picture.png", "C:/picture.png"] {
        assert_eq!(read(&root, path, None).unwrap_err().key, "error.media.path");
    }
    std::fs::write(folder.path().join("wrong.png"), "<html>wrong bytes</html>").unwrap();
    assert_eq!(read(&root, "wrong.png", None).unwrap_err().key, "error.media.gone");
}

#[test]
fn a_link_swapped_in_after_first_load_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let leaf = folder.path().join("picture.png");
    std::fs::write(&leaf, PNG).unwrap();
    std::fs::write(outside.path().join("other.png"), PNG).unwrap();
    let root = pin(folder.path());
    assert_eq!(read(&root, "picture.png", None).unwrap().bytes, PNG);
    std::fs::remove_file(leaf).unwrap();
    std::fs::hard_link(outside.path().join("other.png"), folder.path().join("picture.png")).unwrap();
    assert_eq!(read(&root, "picture.png", None).unwrap_err().key, "error.media.gone");
}

#[test]
fn a_new_view_and_restart_never_accept_the_previous_grant() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("picture.png"), PNG).unwrap();
    let mut media = Media::default();
    let view = media.enter("first", pin(folder.path()));
    let grant = media.grant("first", "picture.png").unwrap();
    assert_eq!(media.source(grant).unwrap().1.as_ref(), "picture.png");
    media.leave(view);
    assert!(media.source(grant).is_none());
    let view2 = media.enter("first", pin(folder.path()));
    media.leave(view);
    let grant2 = media.grant("first", "picture.png").unwrap();
    assert!(media.source(grant).is_none());
    assert!(media.source(grant2).is_some());
    assert!(media.grant("other", "picture.png").is_err());
    let mut restarted = Media::default();
    restarted.enter("first", pin(folder.path()));
    assert!(restarted.source(grant2).is_none());
    assert!(restarted.grant("first", "picture.png").is_ok());
    media.leave(view2);
}

#[test]
fn a_new_load_gets_a_fresh_url_and_can_release_it_independently() {
    let temp = tempfile::tempdir().unwrap();
    let mut media = Media::default();
    let lease = media.enter("local", pin(temp.path()));
    let first = media.grant("local", "picture.png").unwrap();
    let next = media.grant("local", "picture.png").unwrap();
    assert_ne!(first, next);
    assert!(media.source(first).is_some());
    media.release(lease, first);
    assert!(media.source(first).is_none());
    assert!(media.source(next).is_some());
}

#[test]
fn a_grant_holds_no_folder_handle_and_cannot_follow_a_replacement_folder() {
    let temp = tempfile::tempdir().unwrap();
    let active = temp.path().join("active");
    std::fs::create_dir(&active).unwrap();
    let folder = Folder::snapshot(&pin(&active)).unwrap();
    std::fs::rename(&active, temp.path().join("old")).unwrap();
    std::fs::create_dir(&active).unwrap();
    assert!(folder.open().is_none());
}
