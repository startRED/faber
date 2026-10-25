use crate::support;
use faber_engine::cli::Cli;
use faber_engine::codex::{CodexTurnState, parse_codex_notification};
use faber_engine::model::{Area, Conversation, Message, Role, Vendor};
use faber_engine::pieces::{PieceData, detect, evaluate, save_value};
use faber_engine::rewind::{Mode, RedoRequest, RestoreRequest};
use faber_engine::router::Choice;
use faber_engine::store::Store;
use faber_engine::{BranchRequest, Engine, EngineOptions};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

fn fixture(vendor: Vendor) -> Message {
    let mut state = CodexTurnState::default();
    let events = support::fixture(vendor.name(), "visual-pieces")
        .into_iter()
        .filter(|entry| !entry.input)
        .flat_map(|entry| match vendor {
            Vendor::Claude => faber_engine::claude::parse_claude_message(&entry.msg),
            Vendor::Codex => parse_codex_notification(&entry.msg, &mut state),
        })
        .collect::<Vec<_>>();
    let mut message = Message::new("answer".into(), Role::Assistant, support::text_of(&events), 2);
    message.vendor = Some(vendor);
    message
}

fn conversation(message: Message, area: Area) -> Conversation {
    let mut conversation: Conversation = serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
    conversation.id = "pieces".into();
    conversation.area = area;
    conversation.messages = vec![message];
    conversation.piece_values.clear();
    conversation
}

fn engine(dir: &crate::support::DataFolder) -> Engine {
    engine_with_clis(dir, BTreeMap::new())
}

fn engine_with_clis(dir: &crate::support::DataFolder, clis: BTreeMap<Vendor, Cli>) -> Engine {
    Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis,
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
    .unwrap()
}

#[tokio::test]
async fn branches_edits_and_retries_keep_only_retained_piece_choices_after_reopening() {
    for vendor in Vendor::ALL {
        let dir = support::DataFolder::new();
        let mut initial = conversation(fixture(vendor), Area::Chat);
        initial.working_folder = None;
        initial.messages.insert(0, Message::new("first".into(), Role::User, "First question".into(), 1));
        initial.messages.push(Message::new("later".into(), Role::User, "Later question".into(), 3));
        let mut later = fixture(vendor);
        later.id = "later-answer".into();
        initial.messages.push(later);
        save_value(&mut initial, "answer", 4, json!([true, false])).unwrap();
        save_value(&mut initial, "later-answer", 4, json!([false, true])).unwrap();
        Store::new(dir.path()).save_checked(&initial).unwrap();
        let app = engine_with_clis(&dir, BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]));
        let here = app.branch_here("pieces", "answer").unwrap();
        let full = app.branch_here("pieces", "later-answer").unwrap();
        let mut branches = vec![here];
        for text in ["Later question", "Edited later question"] {
            branches.push(
                app.branch(BranchRequest {
                    conversation_id: "pieces".into(),
                    message_id: "later".into(),
                    text: text.into(),
                    choice: Choice { vendor: vendor.name().into(), model: None, effort: None },
                })
                .unwrap(),
            );
        }
        let verify = |app: &Engine| {
            for id in &branches {
                let branch = app.open(id).unwrap();
                assert_eq!(branch.piece_values.len(), 1, "{vendor:?}: {id}");
                assert_eq!(app.visual_pieces(id, "answer")[4].value, json!([true, false]));
                assert!(app.visual_pieces(id, "later-answer").is_empty());
            }
            assert_eq!(app.open(&full).unwrap().piece_values, initial.piece_values);
            assert_eq!(app.open("pieces").unwrap().piece_values, initial.piece_values, "the original stays unchanged");
        };
        verify(&app);
        app.shutdown();
        let reopened = engine(&dir);
        verify(&reopened);
        reopened.shutdown();
    }
}

#[tokio::test]
async fn rewind_then_edit_an_earlier_piece_and_redo_recovers_the_saved_choices_after_reopening() {
    for vendor in Vendor::ALL {
        let dir = support::DataFolder::new();
        let mut initial = conversation(fixture(vendor), Area::Chat);
        initial.id = "3611dcb7-6387-47c6-8b25-1b6f20903171".into();
        initial.working_folder = None;
        initial.messages.push(Message::new("cut".into(), Role::User, "Later question".into(), 3));
        let mut later = fixture(vendor);
        later.id = "later-answer".into();
        initial.messages.push(later);
        save_value(&mut initial, "answer", 4, json!([true, false])).unwrap();
        save_value(&mut initial, "later-answer", 4, json!([false, true])).unwrap();
        Store::new(dir.path()).save_checked(&initial).unwrap();
        let app = engine(&dir);
        let restored = app
            .rewind_restore(RestoreRequest {
                conversation_id: initial.id.clone(),
                message_id: "cut".into(),
                mode: Mode::Conversation,
                saved_versions: vec![],
            })
            .unwrap();
        assert!(!restored.recovery);
        assert_eq!(app.open(&initial.id).unwrap().piece_values.len(), 1, "the shortened conversation keeps only its visible choices");
        app.save_piece_value(&initial.id, "answer", 4, json!([false, true])).unwrap();
        app.shutdown();
        let reopened = engine(&dir);
        assert_eq!(reopened.visual_pieces(&initial.id, "answer")[4].value, json!([false, true]));
        assert_eq!(reopened.rewind_redos(&initial.id).unwrap()[0].id, restored.id);
        let redone =
            reopened.rewind_redo(RedoRequest { conversation_id: initial.id.clone(), id: restored.id, saved_versions: vec![] }).unwrap();
        assert!(!redone.recovery);
        assert_eq!(
            reopened.open(&initial.id).unwrap().piece_values,
            initial.piece_values,
            "Redo restores the entire pre-rewind choice snapshot"
        );
        reopened.shutdown();
        let again = engine(&dir);
        assert_eq!(again.open(&initial.id).unwrap().piece_values, initial.piece_values);
        assert_eq!(again.visual_pieces(&initial.id, "later-answer")[4].value, json!([false, true]));
        again.shutdown();
    }
}

#[test]
fn both_cli_fixtures_detect_all_ten_pieces_and_preserve_invalid_source_as_code() {
    for vendor in Vendor::ALL {
        let mut message = fixture(vendor);
        let pieces = detect(&message, &[]);
        assert_eq!(pieces.len(), 10, "{vendor:?}");
        assert_eq!(pieces.iter().map(|piece| piece.block).collect::<Vec<_>>(), (0..10).collect::<Vec<_>>());
        let kinds =
            pieces.iter().map(|piece| serde_json::to_value(&piece.data).unwrap()["type"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(kinds, ["number", "table", "chart", "calculation", "checklist", "tabs", "steps", "form", "notice", "timeline"]);
        let units = message.text.encode_utf16().collect::<Vec<_>>();
        let mut fallback = message.text.clone();
        for piece in pieces.iter().rev() {
            let source = String::from_utf16(&units[piece.start..piece.end]).unwrap();
            assert!(source.starts_with("```faber-piece\n{"));
            assert!(source.ends_with("```"));
            fallback = fallback.replacen(&source, "", 1);
        }
        assert!(fallback.contains("\"type\":\"unknown\""));
        assert!(fallback.contains("\"text\":false"));
        assert!(fallback.contains("Still streaming"));
        assert_eq!(pieces[3].value, json!(2.0));
        assert_eq!(pieces[4].value, json!([false, false]));
        message.role = Role::User;
        assert!(detect(&message, &[]).is_empty(), "the person's quoted source stays code");
    }
}

#[test]
fn piece_schema_rejects_unknown_keys_wrong_types_oversized_values_and_unsafe_calculations() {
    let valid = json!({"type":"notice","title":"Title","text":"Body"});
    assert!(PieceData::parse(&valid.to_string()).is_some());
    for source in [
        r#"{"type":"notice","title":"Title","text":"Body","onclick":"bad"}"#.to_string(),
        r#"{"type":"notice","title":"Title","title":"Again","text":"Body"}"#.to_string(),
        r#"{"type":"unknown","title":"Title"}"#.to_string(),
        r#"{"type":"notice","text":"Body"}"#.to_string(),
        json!({"type":"notice","title":"Title","text":"x".repeat(501)}).to_string(),
        json!({"type":"notice","title":"x".repeat(121),"text":"Body"}).to_string(),
        json!({"type":"notice","title":"Title","text":"\u{0000}"}).to_string(),
        json!({"type":"checklist","title":"Title","items":vec!["x";13]}).to_string(),
        json!({"type":"checklist","title":"Title","items":[]}).to_string(),
        json!({"type":"table","title":"Title","columns":["x"],"rows":[["a","b"]]}).to_string(),
        json!({"type":"table","title":"Title","columns":vec!["x";7],"rows":[["a"]]}).to_string(),
        json!({"type":"chart","title":"Title","items":[{"label":"x","value":1e13}]}).to_string(),
        format!("{}{}", valid, " ".repeat(8192)),
    ] {
        assert!(PieceData::parse(&source).is_none(), "accepted invalid source: {source}");
    }
    let calculation = json!({"type":"calculation","title":"Cost","label":"Hours","min":0,"max":10,"step":1,"value":2,"expression":"x*15"});
    for (key, value) in [
        ("min", json!(-1e7)),
        ("max", json!(0)),
        ("step", json!(0)),
        ("step", json!(0.00001)),
        ("value", json!(2.5)),
        ("value", json!(11)),
        ("expression", json!("eval(x)")),
        ("expression", json!("1/0")),
    ] {
        let mut invalid = calculation.clone();
        invalid[key] = value;
        assert!(PieceData::parse(&invalid.to_string()).is_none(), "{invalid}");
    }
}

#[test]
fn piece_fences_need_a_closer_keep_stable_ordinals_and_bound_the_number_of_components() {
    let valid = "```faber-piece\n{\"type\":\"notice\",\"title\":\"Title\",\"text\":\"Body\"}\n```";
    let mut message =
        Message::new("m".into(), Role::Assistant, format!("```text\nnormal\n```\n\n```faber-piece\nbroken\n```\n\n{valid}"), 0);
    let pieces = detect(&message, &[]);
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].block, 1, "only piece fences count; invalid ones retain their ordinal");
    for source in [
        valid.trim_end_matches('`').to_string(),
        valid.replacen("```faber", "````faber", 1),
        valid.replace("```faber-piece", "```faber-piece extra"),
    ] {
        message.text = source;
        assert!(detect(&message, &[]).is_empty());
    }
    message.text = std::iter::repeat_n(valid, 30).collect::<Vec<_>>().join("\n\n");
    assert_eq!(detect(&message, &[]).len(), 12);
    message.text = format!("```faber-piece\n{}{}\n```", " ".repeat(8193), r#"{"type":"notice","title":"Title","text":"Body"}"#);
    assert!(detect(&message, &[]).is_empty());
}

#[test]
fn pieces_in_lists_and_quotes_stay_code_without_splitting_the_markdown_structure() {
    let root = "```faber-piece\n{\"type\":\"notice\",\"title\":\"Root\",\"text\":\"Body\"}\n```";
    for nested in [
        "- Before\n\n  ```faber-piece\n  {\"type\":\"notice\",\"title\":\"T\",\"text\":\"Body\"}\n  ```\n\n  After\n- Next",
        "> Before\n>\n> ```faber-piece\n> {\"type\":\"notice\",\"title\":\"T\",\"text\":\"Body\"}\n> ```\n>\n> After",
    ] {
        let message = Message::new("m".into(), Role::Assistant, format!("{nested}\n\n{root}"), 0);
        let pieces = detect(&message, &[]);
        assert_eq!(pieces.len(), 1, "nested source stays in its Markdown list or quote");
        assert_eq!(pieces[0].block, 1, "nested piece fences still count toward stable ordinals");
        assert_eq!(pieces[0].start, format!("{nested}\n\n").encode_utf16().count());
        let units = message.text.encode_utf16().collect::<Vec<_>>();
        assert_eq!(String::from_utf16(&units[pieces[0].start..pieces[0].end]).unwrap(), root);
        assert_eq!(message.text, format!("{nested}\n\n{root}"));
    }
}

#[test]
fn arithmetic_has_precedence_decimals_unary_signs_and_explicit_resource_limits() {
    for (expression, x, result) in
        [("x*15+5", 2.0, 35.0), ("-(x + 2) / .5", 3.0, -10.0), ("2+3*4", 0.0, 14.0), ("(2+3)*4", 0.0, 20.0), ("+x--2", 2.0, 4.0)]
    {
        assert_eq!(evaluate(expression, x), Some(result), "{expression}");
    }
    for expression in [
        "",
        "1/0",
        "x.constructor",
        "Math.sin(x)",
        "x=2",
        "1e3",
        "1..2",
        "x;1",
        "2x",
        "1e12*100",
        "999999999999*100",
        "1 +",
        "(x",
        "x)",
        "NaN",
    ] {
        assert_eq!(evaluate(expression, 2.0), None, "{expression}");
    }
    assert_eq!(evaluate(&format!("{}x{}", "(".repeat(17), ")".repeat(17)), 1.0), None);
    assert_eq!(evaluate(&"1+".repeat(64), 0.0), None);
    assert_eq!(evaluate("x", 1e7), None);
    assert_eq!(evaluate("x", f64::NAN), None);
    assert_eq!(evaluate(&" ".repeat(129), 0.0), None);
}

#[tokio::test]
async fn both_cli_piece_choices_survive_engine_reopening_in_both_areas() {
    for vendor in Vendor::ALL {
        for area in [Area::Chat, Area::Project] {
            let dir = support::DataFolder::new();
            let initial = conversation(fixture(vendor), area);
            Store::new(dir.path()).save_checked(&initial).unwrap();
            let first = engine(&dir);
            let before = first.visual_pieces("pieces", "answer");
            assert_eq!(before.len(), 10);
            let choices = [(3, json!(7)), (4, json!([true, false])), (5, json!(1)), (6, json!(1)), (7, json!(1))];
            for (block, value) in &choices {
                first.save_piece_value("pieces", "answer", *block, value.clone()).unwrap();
            }
            for (block, value) in [
                (3, json!(2.5)),
                (3, json!(100)),
                (4, json!([true])),
                (5, json!(2)),
                (6, json!(-1)),
                (7, json!("send")),
                (8, Value::Null),
                (99, json!(0)),
            ] {
                assert!(first.save_piece_value("pieces", "answer", block, value).is_err());
            }
            assert!(first.visual_pieces("missing", "answer").is_empty());
            assert!(first.save_piece_value("pieces", "missing", 0, json!(0)).is_err());
            let expected = first.visual_pieces("pieces", "answer");
            assert_eq!(first.open("pieces").unwrap().messages, initial.messages, "a form choice never submits or changes source");
            first.shutdown();
            let reopened = engine(&dir);
            assert_eq!(reopened.visual_pieces("pieces", "answer"), expected, "{vendor:?} {area:?}");
            for (block, value) in choices {
                assert_eq!(expected[block].value, value);
            }
            reopened.shutdown();
        }
    }
}

#[tokio::test]
async fn moving_chat_to_a_project_keeps_both_clis_piece_choices_and_allows_saving_after_reopening() {
    for vendor in Vendor::ALL {
        let dir = support::DataFolder::new();
        let folder = support::work_folder();
        let mut initial = conversation(fixture(vendor), Area::Chat);
        initial.working_folder = None;
        Store::new(dir.path()).save_checked(&initial).unwrap();
        let app = engine(&dir);
        let project = app.add_project(&folder.path().to_string_lossy()).unwrap();
        for (block, value) in [(3, json!(7)), (4, json!([true, false])), (5, json!(1)), (6, json!(1)), (7, json!(1))] {
            app.save_piece_value("pieces", "answer", block, value).unwrap();
        }
        let before = app.open("pieces").unwrap();
        let expected = app.visual_pieces("pieces", "answer");
        let moved = app.move_to_project("pieces", &project.id).unwrap();
        assert_eq!(moved.area, Area::Project);
        assert_eq!(moved.project_id, Some(project.id.clone()));
        assert_eq!(moved.messages, before.messages, "{vendor:?}: moving keeps the piece source");
        assert_eq!(moved.piece_values.len(), 5);
        assert_eq!(moved.piece_values, before.piece_values, "{vendor:?}: moving keeps every choice and its source identity");
        assert_eq!(app.visual_pieces("pieces", "answer"), expected);
        app.shutdown();

        let reopened = engine(&dir);
        let record = reopened.open("pieces").unwrap();
        assert_eq!(record.area, Area::Project);
        assert_eq!(record.project_id, moved.project_id);
        assert_eq!(record.piece_values, before.piece_values, "{vendor:?}: moved choices survive reopening");
        assert_eq!(reopened.visual_pieces("pieces", "answer"), expected);
        reopened.save_piece_value("pieces", "answer", 4, json!([false, true])).unwrap();
        let edited = reopened.open("pieces").unwrap();
        let edited_pieces = reopened.visual_pieces("pieces", "answer");
        assert_eq!(edited_pieces[4].value, json!([false, true]));
        assert_eq!(edited.messages, before.messages);
        reopened.shutdown();

        let again = engine(&dir);
        let record = again.open("pieces").unwrap();
        assert_eq!(record.area, Area::Project);
        assert_eq!(record.project_id, moved.project_id);
        assert_eq!(record.piece_values, edited.piece_values, "{vendor:?}: choices remain writable in the project");
        assert_eq!(again.visual_pieces("pieces", "answer"), edited_pieces);
        again.shutdown();
    }
}

#[tokio::test]
async fn a_failed_piece_save_reports_the_disk_failure_and_rolls_back_the_live_value() {
    let dir = support::DataFolder::new();
    let initial = conversation(fixture(Vendor::Claude), Area::Chat);
    Store::new(dir.path()).save_checked(&initial).unwrap();
    let app = engine(&dir);
    let before = app.visual_pieces("pieces", "answer");
    let path = dir.path().join("conversations/pieces.json");
    let backup = dir.path().join("conversations/prior.json");
    std::fs::rename(&path, &backup).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(app.save_piece_value("pieces", "answer", 4, json!([true, true])).unwrap_err(), "error.store.write");
    assert_eq!(app.visual_pieces("pieces", "answer"), before);
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(&backup, &path).unwrap();
    app.shutdown();
    let reopened = engine(&dir);
    assert_eq!(reopened.visual_pieces("pieces", "answer"), before);
    reopened.shutdown();
}

#[test]
fn edited_piece_source_cannot_inherit_saved_values_and_corrupt_values_are_ignored() {
    let mut conversation = conversation(fixture(Vendor::Claude), Area::Chat);
    save_value(&mut conversation, "answer", 4, json!([true, false])).unwrap();
    assert_eq!(detect(&conversation.messages[0], &conversation.piece_values)[4].value, json!([true, false]));
    conversation.piece_values[0].value = json!("corrupt");
    assert_eq!(detect(&conversation.messages[0], &conversation.piece_values)[4].value, json!([false, false]));
    conversation.piece_values[0].value = json!([true, false]);
    conversation.messages[0].text = conversation.messages[0].text.replace("Keys", "Wallet");
    assert_eq!(detect(&conversation.messages[0], &conversation.piece_values)[4].value, json!([false, false]));
    save_value(&mut conversation, "answer", 4, json!([false, true])).unwrap();
    assert_eq!(conversation.piece_values.len(), 1, "replaced source leaves no stale choice");
}
