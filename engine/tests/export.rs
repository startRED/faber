use crate::support;
use faber_engine::cli::Cli;
use faber_engine::codex::{CodexTurnState, parse_codex_notification};
use faber_engine::export::{Paths, Words, markdown};
use faber_engine::model::{Conversation, Message, Role, Status, Vendor};
use faber_engine::pieces::{detect, save_value};
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

fn words() -> Words {
    Words { you: "You".into(), auto: "Auto".into(), attachments: "Attachments".into(), files: "Files changed".into() }
}

fn conversation(text: String) -> Conversation {
    let mut conversation: Conversation = serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
    conversation.messages = vec![Message::new("answer".into(), Role::Assistant, text, 2)];
    conversation.piece_values.clear();
    conversation
}

fn fences(markdown: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut fence = None;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language))) => fence = Some((language.to_string(), String::new())),
            Event::Text(text) => {
                if let Some((_, source)) = &mut fence {
                    source.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(fence) = fence.take() {
                    result.push(fence);
                }
            }
            _ => {}
        }
    }
    result
}

#[tokio::test]
async fn conversation_export_keeps_all_piece_content_and_current_choices_after_reopening_for_both_clis() {
    for vendor in Vendor::ALL {
        let mut state = CodexTurnState::default();
        let events = support::fixture(vendor.name(), "visual-pieces")
            .into_iter()
            .filter(|entry| !entry.input)
            .flat_map(|entry| match vendor {
                Vendor::Claude => faber_engine::claude::parse_claude_message(&entry.msg),
                Vendor::Codex => parse_codex_notification(&entry.msg, &mut state),
            })
            .collect::<Vec<_>>();
        let mut initial = conversation(support::text_of(&events));
        let choices = [(3, json!(7)), (4, json!([true, false])), (5, json!(1)), (6, json!(1)), (7, json!(1))];
        for (block, value) in &choices {
            save_value(&mut initial, "answer", *block, value.clone()).unwrap();
        }
        let dir = support::DataFolder::new();
        Store::new(dir.path()).save_checked(&initial).unwrap();
        let engine = Engine::new(
            EngineOptions {
                data_dir: dir.path().into(),
                #[cfg(windows)]
                chat_dir: dir.chat_path().into(),
                clis: BTreeMap::<Vendor, Cli>::new(),
                spawn: support::fake(vec![]).spawn,
                windows: false,
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(120),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        let (_, exported) = engine.export_conversation(&initial.id, &words()).unwrap();
        let snapshots = fences(&exported)
            .into_iter()
            .filter(|(language, _)| language == "json")
            .map(|(_, source)| serde_json::from_str::<Value>(&source).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(snapshots.len(), 10, "{vendor:?}: {exported}");
        for (snapshot, piece) in snapshots.iter().zip(detect(&initial.messages[0], &initial.piece_values)) {
            assert_eq!(snapshot["piece"], serde_json::to_value(piece.data).unwrap());
            assert!(snapshot.get("source").is_none());
            assert!(snapshot.get("messageId").is_none());
        }
        for (block, choice) in choices {
            assert_eq!(snapshots[block]["value"], choice);
        }
        assert_eq!(snapshots[3]["result"], json!(110.0));
        for saved in &initial.piece_values {
            assert!(!exported.contains(&saved.source), "source digests stay internal");
        }
        let fallback = fences(&exported).into_iter().filter(|(language, _)| language == "faber-piece").collect::<Vec<_>>();
        assert_eq!(fallback.len(), 3, "unknown, invalid and unfinished source stays ordinary code");
        assert_eq!(serde_json::from_str::<Value>(&fallback[0].1).unwrap(), json!({"type":"unknown","title":"Fallback"}));
        assert_eq!(serde_json::from_str::<Value>(&fallback[1].1).unwrap(), json!({"type":"notice","title":"Broken","text":false}));
        assert!(fallback[2].1.contains("Still streaming"));
        assert_eq!(engine.open(&initial.id).unwrap().messages, initial.messages, "export does not edit the conversation");
        engine.shutdown();
    }
}

#[test]
fn conversation_export_piece_snapshots_use_validated_choices_and_redact_before_json_escaping() {
    let data = json!({"type":"form","title":"Choose <plan> `now`","options":["C:\\Users\\Erick\\private\\file.md","Other"]});
    let mut conversation = conversation(format!("```faber-piece\n{data}\n```"));
    save_value(&mut conversation, "answer", 0, json!(1)).unwrap();
    conversation.piece_values[0].value = json!(999);
    let exported = markdown(&conversation, &words(), &chrono::Utc, &Paths::new("", "C:/Users/Erick", true)).unwrap();
    let snapshots = fences(&exported);
    assert_eq!(snapshots.len(), 1);
    let snapshot: Value = serde_json::from_str(&snapshots[0].1).unwrap();
    assert_eq!(snapshot["value"], json!(0), "corrupt saved choice exports the validated default");
    assert_eq!(snapshot["piece"]["title"], data["title"]);
    assert_eq!(snapshot["piece"]["options"][0], json!("~\\private\\file.md"));
    assert!(!exported.contains("Erick"));
    conversation.messages[0].text = conversation.messages[0].text.replace("Other", "Changed");
    conversation.piece_values[0].value = json!(1);
    let exported = markdown(&conversation, &words(), &chrono::Utc, &Paths::new("", "", true)).unwrap();
    let snapshot: Value = serde_json::from_str(&fences(&exported)[0].1).unwrap();
    assert_eq!(snapshot["value"], json!(0), "a changed piece cannot export a stale choice");
}

#[test]
fn conversation_export_piece_snapshots_respect_the_utf16_final_answer_boundary() {
    let hidden = "```faber-piece\n{\"type\":\"checklist\",\"title\":\"Private work\",\"items\":[\"Hidden\"]}\n```";
    let prefix = format!("Work \u{1f9e9}\n\n{hidden}\n\n");
    let final_piece = "```faber-piece\n{\"type\":\"checklist\",\"title\":\"Final\",\"items\":[\"Read <this>\"]}\n```";
    let mut conversation = conversation(format!("{prefix}Final \u{1f9e9}\n\n{final_piece}\n\nAfter."));
    conversation.messages[0].activity_end = Some(prefix.encode_utf16().count());
    conversation.messages[0].status = Some(Status::Done);
    conversation.messages[0].tools =
        Some(vec![serde_json::from_value(json!({"id":"tool","name":"Read","input":"PRIVATE_METADATA"})).unwrap()]);
    save_value(&mut conversation, "answer", 0, json!([true])).unwrap();
    save_value(&mut conversation, "answer", 1, json!([true])).unwrap();
    let exported = markdown(&conversation, &words(), &chrono::Utc, &Paths::new("", "", false)).unwrap();
    assert!(!exported.contains("Private work"));
    assert!(!exported.contains("Hidden"));
    assert!(!exported.contains("PRIVATE_METADATA"));
    assert!(exported.contains("After."));
    let snapshots = fences(&exported);
    assert_eq!(snapshots.len(), 1);
    let snapshot: Value = serde_json::from_str(&snapshots[0].1).unwrap();
    assert_eq!(snapshot["piece"]["title"], json!("Final"));
    assert_eq!(snapshot["piece"]["items"], json!(["Read <this>"]));
    assert_eq!(snapshot["value"], json!([true]));
}
