//! Search uses the same recorded final answer as the drivers, saved as a real conversation file.
use crate::support;
use faber_engine::claude::parse_claude_message;
use faber_engine::model::{Area, Conversation, Message, Role, Status};
use faber_engine::store::Store;
use serde_json::json;

fn recorded(area: Area) -> Conversation {
    let entries = support::fixture("claude", "chat");
    let events: Vec<_> = entries.iter().filter(|e| !e.input).flat_map(|e| parse_claude_message(&e.msg)).collect();
    let mut answer = Message::new("answer".into(), Role::Assistant, support::text_of(&events), 2);
    answer.status = Some(Status::Done);
    serde_json::from_value(json!({
        "id": "recorded", "area": area, "title": "Physics", "createdAt": 1, "updatedAt": 2,
        "messages": [Message::new("question".into(), Role::User, "Why is the sky blue?".into(), 1), answer]
    }))
    .unwrap()
}

#[test]
fn search_finds_a_word_in_the_middle_of_a_recorded_answer_in_each_area() {
    for area in [Area::Chat, Area::Project] {
        let dir = tempfile::tempdir().unwrap();
        Store::new(dir.path()).save(&recorded(area));
        let store = Store::new(dir.path());
        let search = store.search();
        assert!(!search.query("Rayleigh", area).unwrap().ready);
        search.start().unwrap().recv().unwrap();
        let result = search.query("RAYLEIGH wavelengths", area).unwrap();
        assert!(result.ready);
        assert_eq!(result.hits.len(), 1);
        let hit = &result.hits[0];
        assert_eq!(hit.conversation_id, "recorded");
        assert_eq!(hit.message_index, 1);
        assert_eq!(hit.title, "Physics");
        assert_eq!(hit.area, area);
        assert!(hit.snippet.contains("wavelengths"));
        assert!(!hit.highlights.is_empty());
        assert!(search.query("Rayleigh missingword", area).unwrap().hits.is_empty());
        let other = if area == Area::Chat { Area::Project } else { Area::Chat };
        assert!(search.query("Rayleigh", other).unwrap().hits.is_empty());
    }
}

#[test]
fn search_finds_the_middle_of_a_long_answer_in_an_old_message() {
    for area in [Area::Chat, Area::Project] {
        let dir = tempfile::tempdir().unwrap();
        let mut c = recorded(area);
        c.messages[1].text = format!("{} {} {}", "earlier words ".repeat(5000), c.messages[1].text, "later words ".repeat(5000));
        for i in 2..200 {
            c.messages.push(Message::new(format!("newer-{i}"), Role::User, "A newer message. ".repeat(1000), i));
        }
        Store::new(dir.path()).save(&c);
        let store = Store::new(dir.path());
        let search = store.search();
        search.start().unwrap().recv().unwrap();
        let hits = search.query("RAYLEI,wavel", area).unwrap().hits;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].message_index, 1);
        assert!(hits[0].snippet.contains("wavelengths"));
        store.remove(&c.id);
        assert!(search.query("RAYLEI,wavel", area).unwrap().hits.is_empty());
    }
}

#[test]
fn search_updates_a_saved_answer_and_never_returns_a_removed_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let mut c = recorded(Area::Chat);
    store.save(&c);
    let search = store.search();
    search.start().unwrap().recv().unwrap();
    c.messages[1].text = "The corrected answer mentions café and açúcar.".into();
    c.title = "Renamed".into();
    store.save(&c);
    assert!(search.query("Rayleigh", Area::Chat).unwrap().hits.is_empty());
    let result = search.query("CAFE acucar", Area::Chat).unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].title, "Renamed");
    store.remove(&c.id);
    assert!(search.query("cafe", Area::Chat).unwrap().hits.is_empty());
    assert!(Store::new(dir.path()).list().is_empty());
}

#[test]
fn search_skips_tools_thinking_attachments_narration_and_corrupt_files() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let mut c = recorded(Area::Chat);
    c.messages[1].text = "hiddennarration. Final visible words.".into();
    c.messages[1].activity_end = Some("hiddennarration. ".encode_utf16().count());
    c.messages[1].thinking = Some("hiddenthinking".into());
    c.messages[1].attachments = Some(vec!["hiddenattachment.txt".into()]);
    let mut value = serde_json::to_value(c).unwrap();
    value["messages"][1]["tools"] = json!([{"id":"tool","name":"Read","input":{},"output":"hiddentool","status":"done"}]);
    let c = serde_json::from_value(value).unwrap();
    store.save(&c);
    std::fs::write(dir.path().join("conversations/broken.json"), "{").unwrap();
    let search = store.search();
    search.start().unwrap().recv().unwrap();
    for hidden in ["hiddennarration", "hiddenthinking", "hiddenattachment", "hiddentool"] {
        assert!(search.query(hidden, Area::Chat).unwrap().hits.is_empty(), "found {hidden}");
    }
    assert_eq!(search.query("visible words", Area::Chat).unwrap().hits.len(), 1);
    // Even an external deletion cannot leave a stale clickable result.
    std::fs::remove_file(dir.path().join("conversations/recorded.json")).unwrap();
    assert!(search.query("visible", Area::Chat).unwrap().hits.is_empty());
}
