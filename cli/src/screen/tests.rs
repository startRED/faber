//! The screen without a terminal: keys and events go into the state, and the layout is drawn on a test backend.

use super::app::{Account, App, Effect, Focus, Input, Leave, Mode};
use super::palette::Palette;
use super::view;
use crate::words::Words;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn state(language: &str) -> Value {
    json!({
        "conversations": [
            { "id": "c1", "title": "Older chat", "area": "chat", "updatedAt": 10 },
            { "id": "c2", "title": "Fix the parser", "area": "project", "projectId": "p1", "updatedAt": 20 },
            { "id": "c3", "title": "Pinned notes", "area": "chat", "updatedAt": 5, "pinnedAt": 5 },
            { "id": "c4", "title": "Gone", "area": "chat", "updatedAt": 30, "archived": true },
        ],
        "projects": [{ "id": "p1", "name": "Faber", "path": "E:/faber" }],
        "settings": { "language": language },
        "running": { "items": [] },
        "quotas": [
            { "vendor": "claude", "window": "five_hour", "used": 0.42, "resetsAt": 0 },
            { "vendor": "claude", "window": "seven_day", "used": 0.1, "resetsAt": 0 },
        ],
        "look": { "vars": {}, "motion": "full" },
    })
}

fn app_in(language: &str, with_state: bool) -> App {
    let mut app = App::new(Words::new(language), Palette::plain(), false, Instant::now());
    if with_state {
        app.apply_state(&state(language));
    }
    app
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Vec<Effect> {
    let key = KeyEvent { code, modifiers, kind: KeyEventKind::Press, state: KeyEventState::NONE };
    app.update(Input::Key(key), Instant::now())
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

fn draw(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| view::render(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height).map(|y| (0..width).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>().trim_end().to_string()).collect()
}

fn shows(lines: &[String], text: &str) -> bool {
    lines.iter().any(|l| l.contains(text))
}

fn running_item(conversation: &str, permissions: Value) -> Value {
    json!({ "conversationId": conversation, "turnId": "t1", "state": if permissions.as_array().is_some_and(|p| !p.is_empty()) { "needsYou" } else { "working" }, "permissions": permissions })
}

fn sign_in(app: &mut App, vendor: &str, account: &str) {
    app.update(Input::Account(json!({ "vendor": vendor, "installed": true, "account": account })), Instant::now());
}

// ---- layout ----

#[test]
fn at_80_by_24_the_list_is_hidden_and_a_new_person_is_told_what_to_do() {
    let mut app = app_in("en", true);
    sign_in(&mut app, "claude", "signedOut");
    sign_in(&mut app, "codex", "signedOut");
    let lines = draw(&mut app, 80, 24);
    assert_eq!(lines.len(), 24);
    assert!(!shows(&lines, "Conversations"), "{lines:#?}");
    assert!(shows(&lines, "No AI is connected yet."));
    assert!(shows(&lines, "faber sign-in claude") && shows(&lines, "faber sign-in codex"));
    assert!(shows(&lines, "Ask anything…"));
    assert!(shows(&lines, "Enter send"));
    assert!(shows(&lines, "Claude Code: not signed in"), "{}", lines[23]);
}

#[test]
fn at_120_by_40_the_list_sits_beside_the_thread() {
    let mut app = app_in("en", true);
    sign_in(&mut app, "claude", "signedIn");
    sign_in(&mut app, "codex", "signedOut");
    let lines = draw(&mut app, 120, 40);
    assert!(shows(&lines, "Conversations"));
    assert!(shows(&lines, "Fix the parser") && shows(&lines, "Older chat") && shows(&lines, "Pinned notes"));
    assert!(!shows(&lines, "Gone"), "archived conversations stay out of the list");
    // Pinned first, then the newest.
    let row = |text: &str| lines.iter().position(|l| l.contains(text)).unwrap();
    assert!(row("Pinned notes") < row("Fix the parser") && row("Fix the parser") < row("Older chat"));
    assert!(shows(&lines, "Ask anything. Your conversation starts here."));
    assert!(shows(&lines, "Claude Code ✓"));
    assert!(shows(&lines, "5-hour 42%"));
}

#[test]
fn an_empty_list_says_so() {
    let mut app = app_in("en", false);
    app.apply_state(&json!({ "conversations": [], "settings": { "language": "en" } }));
    let lines = draw(&mut app, 120, 30);
    assert!(shows(&lines, "No conversations yet.") && shows(&lines, "Write below to start one."));
}

#[test]
fn the_longest_language_fits_both_sizes() {
    for (width, height) in [(80, 24), (120, 40)] {
        let mut app = app_in("de", true);
        sign_in(&mut app, "claude", "signedOut");
        sign_in(&mut app, "codex", "signedOut");
        let lines = draw(&mut app, width, height);
        assert!(shows(&lines, "Noch keine KI verbunden."), "{lines:#?}");
        assert!(shows(&lines, "Frag, was du willst…"));
        assert!(shows(&lines, "Enter senden"));
        assert!(shows(&lines, "Claude Code: nicht angemeldet"), "{}", lines[usize::from(height) - 1]);
        if width >= 100 {
            assert!(shows(&lines, "Unterhaltungen"));
        }
    }
}

#[test]
fn a_window_that_is_too_small_says_so_instead_of_breaking() {
    let mut app = app_in("en", true);
    let lines = draw(&mut app, 30, 6);
    assert!(shows(&lines, "The window is too"));
}

#[test]
fn the_opened_thread_shows_the_person_the_steps_and_the_answer() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c2", &mut fx);
    assert!(matches!(fx.as_slice(), [Effect::Open(id)] if id == "c2"));
    let conversation = json!({ "id": "c2", "messages": [
        { "id": "m1", "role": "user", "text": "Fix the parser" },
        { "id": "m2", "role": "assistant", "status": "done", "vendor": "claude", "model": "sonnet", "text": "# Fixed\n\n- the loop", "tools": [{ "id": "t1", "name": "Read", "input": { "file_path": "src/p.rs" }, "ok": true }] },
    ] });
    app.update(Input::Opened("c2".into(), conversation), Instant::now());
    let lines = draw(&mut app, 120, 40);
    assert!(shows(&lines, "Faber › Fix the parser"), "the project's name leads the title: {lines:#?}");
    assert!(shows(&lines, "› Fix the parser"));
    assert!(shows(&lines, "✓ Read src/p.rs"));
    assert!(shows(&lines, "• the loop"));
    assert!(shows(&lines, "Claude Code · sonnet"));
}

// ---- keys ----

#[test]
fn enter_sends_and_alt_enter_makes_a_new_line() {
    let mut app = app_in("en", true);
    type_text(&mut app, "hello");
    assert!(press(&mut app, KeyCode::Enter, KeyModifiers::ALT).is_empty());
    type_text(&mut app, "world");
    assert_eq!(app.editor.text(), "hello\nworld");
    press(&mut app, KeyCode::Char('j'), KeyModifiers::CONTROL);
    assert_eq!(app.editor.text(), "hello\nworld\n");
    let fx = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let [Effect::Send(req)] = fx.as_slice() else { panic!("one send") };
    assert_eq!(req["text"], "hello\nworld");
    assert_eq!(req["conversationId"], Value::Null);
    assert_eq!(req["area"], "chat");
    assert_eq!(req["choice"], json!({ "vendor": "auto" }));
    assert!(app.editor.is_empty());
    // Up brings the last text back.
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(app.editor.text(), "hello\nworld");
}

#[test]
fn a_send_in_a_project_conversation_stays_in_its_project() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c2", &mut fx);
    type_text(&mut app, "go on");
    let fx = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let [Effect::Send(req)] = fx.as_slice() else { panic!("one send") };
    assert_eq!(
        (req["conversationId"].as_str(), req["area"].as_str(), req["projectId"].as_str()),
        (Some("c2"), Some("project"), Some("p1"))
    );
}

#[test]
fn ctrl_c_stops_an_answer_and_a_second_one_leaves() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    app.update(Input::Faber(json!({ "type": "running", "items": [running_item("c1", json!([]))] })), Instant::now());
    let fx = press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(matches!(fx.as_slice(), [Effect::Call { cmd: "interrupt", .. }]));
    assert!(app.leave.is_none() && app.toast.as_deref().is_some_and(|t| t.starts_with("Stopped.")));
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(app.leave, Some(Leave::Done));
}

#[test]
fn ctrl_c_with_nothing_running_leaves_but_first_clears_a_draft() {
    let mut app = app_in("en", true);
    type_text(&mut app, "draft");
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(app.leave.is_none() && app.editor.is_empty());
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(app.leave, Some(Leave::Done));
}

#[test]
fn the_second_ctrl_c_after_two_seconds_only_stops_again() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    app.update(Input::Faber(json!({ "type": "running", "items": [running_item("c1", json!([]))] })), Instant::now());
    let start = Instant::now();
    let ctrl_c =
        KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, kind: KeyEventKind::Press, state: KeyEventState::NONE };
    app.update(Input::Key(ctrl_c), start);
    let fx = app.update(Input::Key(ctrl_c), start + Duration::from_secs(3));
    assert!(app.leave.is_none());
    assert!(matches!(fx.as_slice(), [Effect::Call { cmd: "interrupt", .. }]));
}

#[test]
fn slash_opens_the_commands_and_enter_runs_the_one_under_the_cursor() {
    let mut app = app_in("en", true);
    type_text(&mut app, "/");
    assert_eq!(app.command_matches().len(), 8);
    let lines = draw(&mut app, 80, 24);
    assert!(shows(&lines, "/rename") && shows(&lines, "Rename this conversation"), "{lines:#?}");
    type_text(&mut app, "q");
    assert_eq!(app.command_matches(), ["quota", "quit"]);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.leave, Some(Leave::Done));
    assert!(app.editor.is_empty());
}

#[test]
fn commands_about_a_conversation_need_one_and_then_call_faber() {
    let mut app = app_in("en", true);
    type_text(&mut app, "/pin");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.toast.as_deref(), Some("Open a conversation first."));
    let mut fx = Vec::new();
    app.open_conversation("c3", &mut fx);
    type_text(&mut app, "/pin");
    let fx = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let [Effect::Call { cmd: "pin", args, refresh: true }] = fx.as_slice() else { panic!("a pin") };
    assert_eq!(*args, json!({ "id": "c3", "on": false }), "c3 is pinned already");
    type_text(&mut app, "/rename");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.mode, Mode::Rename);
    type_text(&mut app, "Better name");
    let fx = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let [Effect::Call { cmd: "rename", args, .. }] = fx.as_slice() else { panic!("a rename") };
    assert_eq!(*args, json!({ "id": "c3", "title": "Better name" }));
    assert_eq!(app.mode, Mode::Normal);
    type_text(&mut app, "/archive");
    let fx = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(fx.as_slice(), [Effect::Call { cmd: "archive", .. }]));
    assert!(app.active.is_none(), "an archived conversation is left");
}

#[test]
fn a_path_that_starts_with_a_slash_is_a_message_not_a_command() {
    let mut app = app_in("en", true);
    type_text(&mut app, "/usr/bin is where");
    let fx = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(fx.as_slice(), [Effect::Send(_)]));
}

#[test]
fn tab_moves_the_focus_and_a_letter_in_the_list_starts_a_message() {
    let mut app = app_in("en", true);
    app.size = (120, 40);
    assert_eq!(app.focus, Focus::Box);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.focus, Focus::List);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.focus, Focus::Thread);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.focus, Focus::Box);
    press(&mut app, KeyCode::BackTab, KeyModifiers::NONE);
    press(&mut app, KeyCode::BackTab, KeyModifiers::NONE);
    assert_eq!(app.focus, Focus::List);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(app.list_cursor, 1);
    let fx = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(fx.as_slice(), [Effect::Open(id)] if id == "c3"), "the pinned one is first");
    assert_eq!(app.focus, Focus::Box);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.focus, Focus::Thread);
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!((app.focus, app.editor.text()), (Focus::Box, "x"));
}

#[test]
fn ctrl_n_starts_a_new_conversation_and_ctrl_b_toggles_the_list() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    press(&mut app, KeyCode::Char('n'), KeyModifiers::CONTROL);
    assert!(app.active.is_none() && app.thread.is_none());
    assert!(!app.show_sidebar());
    press(&mut app, KeyCode::Char('b'), KeyModifiers::CONTROL);
    assert!(app.show_sidebar() && app.focus == Focus::List);
    let lines = draw(&mut app, 80, 24);
    assert!(shows(&lines, "Conversations"));
    press(&mut app, KeyCode::Char('b'), KeyModifiers::CONTROL);
    assert!(!app.show_sidebar() && app.focus == Focus::Box);
}

#[test]
fn the_thread_scrolls_and_holds_its_place_while_the_answer_grows() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    let long: String = (0..60).map(|n| format!("line {n}\n\n")).collect();
    app.update(
        Input::Opened("c1".into(), json!({ "messages": [{ "id": "m1", "role": "assistant", "status": "streaming", "text": long }] })),
        Instant::now(),
    );
    let lines = draw(&mut app, 80, 24);
    assert!(shows(&lines, "line 59") && !shows(&lines, "line 0"));
    press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
    let lines = draw(&mut app, 80, 24);
    assert!(!shows(&lines, "line 59") && shows(&lines, "Jump to latest: End"));
    let top = lines.iter().find(|l| l.contains("line ")).cloned();
    let grown = format!("{long}more\n\nmore\n\nmore\n\n");
    app.update(Input::Faber(json!({ "type": "message", "conversationId": "c1", "message": { "id": "m1", "role": "assistant", "status": "streaming", "text": grown } })), Instant::now());
    let lines = draw(&mut app, 80, 24);
    assert_eq!(lines.iter().find(|l| l.contains("line ")).cloned(), top, "the reader's place did not move");
    press(&mut app, KeyCode::End, KeyModifiers::CONTROL);
    assert_eq!(app.scroll, 0);
    press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.focus, Focus::Thread);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    assert_eq!(app.scroll, 0);
}

// ---- events ----

#[test]
fn a_streaming_reply_is_put_back_together_and_an_unknown_one_opens_the_conversation_again() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    app.update(Input::Opened("c1".into(), json!({ "messages": [] })), Instant::now());
    let event = |message: Value| Input::Faber(json!({ "type": "message", "conversationId": "c1", "message": message }));
    app.update(event(json!({ "id": "m1", "role": "assistant", "status": "streaming", "text": "a", "tools": [{ "id": "t1", "name": "Read", "input": {} }] })), Instant::now());
    app.update(event(json!({ "id": "m1", "role": "assistant", "status": "streaming", "text": "ab", "toolsFrom": 0, "tools": [{ "id": "t1", "same": true }] })), Instant::now());
    let thread = app.thread.as_ref().unwrap();
    assert_eq!(thread.get("m1").unwrap()["text"], "ab");
    assert_eq!(thread.get("m1").unwrap()["tools"][0]["name"], "Read");
    let fx = app.update(event(json!({ "id": "m9", "role": "assistant", "toolsFrom": 1, "tools": [] })), Instant::now());
    assert!(fx.iter().any(|e| matches!(e, Effect::Open(id) if id == "c1")));
}

#[test]
fn a_finished_reply_or_an_unknown_conversation_reads_the_state_once() {
    let mut app = app_in("en", true);
    let done =
        json!({ "type": "message", "conversationId": "c1", "message": { "id": "m", "role": "assistant", "status": "done", "text": "x" } });
    let fx = app.update(Input::Faber(done.clone()), Instant::now());
    assert!(matches!(fx.as_slice(), [Effect::State]));
    let fx = app.update(Input::Faber(done), Instant::now());
    assert!(fx.is_empty(), "a read is under way already");
    // The read that was asked for while the first was under way follows it.
    let fx = app.update(Input::State(state("en")), Instant::now());
    assert!(matches!(fx.as_slice(), [Effect::State]));
    app.update(Input::State(state("en")), Instant::now());
    let fx = app.update(Input::Faber(json!({ "type": "list" })), Instant::now());
    assert!(matches!(fx.as_slice(), [Effect::State]));
}

#[test]
fn the_language_of_settings_changes_the_words() {
    let mut app = app_in("en", true);
    assert_eq!(app.t("cli.screen.list.title"), "Conversations");
    app.update(Input::State(state("pt-BR")), Instant::now());
    assert_eq!(app.t("cli.screen.list.title"), "Conversas");
}

#[test]
fn the_last_vendor_that_answered_decides_which_quota_the_bar_shows() {
    let mut app = app_in("en", true);
    app.update(
        Input::Faber(json!({ "type": "quota", "vendor": "codex", "window": "five_hour", "used": 0.95, "resetsAt": 0 })),
        Instant::now(),
    );
    sign_in(&mut app, "claude", "signedOut");
    sign_in(&mut app, "codex", "signedIn");
    let lines = draw(&mut app, 120, 30);
    assert!(shows(&lines, "5-hour 95%"), "the first signed-in vendor: {}", lines[29]);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    app.update(
        Input::Opened(
            "c1".into(),
            json!({ "messages": [{ "id": "m", "role": "assistant", "status": "done", "vendor": "claude", "text": "x" }] }),
        ),
        Instant::now(),
    );
    let lines = draw(&mut app, 120, 30);
    assert!(shows(&lines, "5-hour 42%"), "{}", lines[29]);
}

// ---- cards ----

fn question_card() -> Value {
    json!({ "requestId": "r1", "tool": "AskUserQuestion", "input": { "questions": [
        { "question": "Which database?", "options": [{ "label": "SQLite", "description": "one file" }, { "label": "Postgres" }] }
    ] } })
}

#[test]
fn a_question_shows_as_a_card_and_a_number_key_answers_it() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    app.update(Input::Faber(json!({ "type": "running", "items": [running_item("c1", json!([question_card()]))] })), Instant::now());
    let lines = draw(&mut app, 100, 30);
    assert!(
        shows(&lines, "Which database?")
            && shows(&lines, "1 SQLite")
            && shows(&lines, "2 Postgres")
            && shows(&lines, "3 Write my own answer"),
        "{lines:#?}"
    );
    assert!(shows(&lines, "Press 1-3 to choose · Esc decides later"));
    assert!(shows(&lines, "1 needs you"));
    let fx = press(&mut app, KeyCode::Char('2'), KeyModifiers::NONE);
    let [Effect::Call { cmd: "answer_question", args, refresh: false }] = fx.as_slice() else { panic!("an answer") };
    assert_eq!(*args, json!({ "id": "c1", "turnId": "t1", "requestId": "r1", "answers": { "Which database?": "Postgres" } }));
    assert!(app.card.is_none(), "an answered card goes while Faber still lists it");
    // Faber lists it again in the next update: it is not asked twice.
    app.update(Input::Faber(json!({ "type": "running", "items": [running_item("c1", json!([question_card()]))] })), Instant::now());
    assert!(app.card.is_none());
}

#[test]
fn digits_type_when_the_box_has_text_and_esc_puts_a_card_off_until_ctrl_k() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    app.update(Input::Faber(json!({ "type": "running", "items": [running_item("c1", json!([question_card()]))] })), Instant::now());
    type_text(&mut app, "ab");
    press(&mut app, KeyCode::Char('1'), KeyModifiers::NONE);
    assert_eq!(app.editor.text(), "ab1");
    app.editor.clear();
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.card.is_none() && app.card_deferred());
    let lines = draw(&mut app, 100, 30);
    assert!(shows(&lines, "Press Ctrl+K to answer."));
    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(app.card.is_some());
}

#[test]
fn writing_ones_own_answer_goes_through_the_box() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    app.update(Input::Faber(json!({ "type": "running", "items": [running_item("c1", json!([question_card()]))] })), Instant::now());
    press(&mut app, KeyCode::Char('3'), KeyModifiers::NONE);
    assert_eq!(app.mode, Mode::Answer);
    let lines = draw(&mut app, 100, 30);
    assert!(shows(&lines, "Write your answer…"));
    type_text(&mut app, "DuckDB");
    let fx = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let [Effect::Call { cmd: "answer_question", args, .. }] = fx.as_slice() else { panic!("an answer") };
    assert_eq!(args["answers"], json!({ "Which database?": "DuckDB" }));
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn a_permission_and_a_plan_are_answered_on_numbers_too() {
    let mut app = app_in("en", true);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    let permission = json!({ "requestId": "r7", "tool": "Bash", "input": { "command": "cargo test" } });
    app.update(Input::Faber(json!({ "type": "running", "items": [running_item("c1", json!([permission]))] })), Instant::now());
    let lines = draw(&mut app, 100, 30);
    assert!(shows(&lines, "The AI wants to run: Bash cargo test") && shows(&lines, "2 Always allow"), "{lines:#?}");
    let fx = press(&mut app, KeyCode::Char('2'), KeyModifiers::NONE);
    assert!(matches!(fx.as_slice(), [Effect::Call { cmd: "answer", args, .. }] if args["allow"] == true && args["always"] == true));
    let plan = json!({ "requestId": "r8", "tool": "ExitPlanMode", "input": { "plan": "1. Do it", "mode": "acceptEdits" } });
    app.update(Input::Faber(json!({ "type": "running", "items": [running_item("c1", json!([plan]))] })), Instant::now());
    let fx = press(&mut app, KeyCode::Char('1'), KeyModifiers::NONE);
    assert!(matches!(fx.as_slice(), [Effect::Call { cmd: "approve_plan", args, .. }] if args["mode"] == "acceptEdits"));
}

#[test]
fn a_card_of_another_conversation_waits_in_the_list() {
    let mut app = app_in("en", true);
    app.size = (120, 30);
    let mut fx = Vec::new();
    app.open_conversation("c1", &mut fx);
    app.update(Input::Faber(json!({ "type": "running", "items": [running_item("c2", json!([question_card()]))] })), Instant::now());
    assert!(app.card.is_none() && app.needs_you("c2"));
    let lines = draw(&mut app, 120, 30);
    assert!(lines.iter().any(|l| l.contains("! Fix the parser")), "{lines:#?}");
    assert!(shows(&lines, "1 answer running") && shows(&lines, "1 needs you"));
}

// ---- motion ----

#[test]
fn the_working_mark_turns_only_while_something_runs_and_stands_still_for_reduced_motion() {
    let mut app = app_in("en", true);
    assert!(!app.animating());
    app.update(Input::Faber(json!({ "type": "running", "items": [running_item("c1", json!([]))] })), Instant::now());
    assert!(app.animating());
    let before = view::mark(&app);
    app.update(Input::Tick, Instant::now());
    assert_ne!(before, view::mark(&app));
    let mut still = App::new(Words::new("en"), Palette::plain(), true, Instant::now());
    still.apply_state(&state("en"));
    still.update(Input::Faber(json!({ "type": "running", "items": [running_item("c1", json!([]))] })), Instant::now());
    assert!(!still.animating(), "no timer for a still mark");
    let (a, b) = (view::mark(&still), {
        still.update(Input::Tick, Instant::now());
        view::mark(&still)
    });
    assert_eq!(a, b);
}

#[test]
fn faber_closing_ends_the_screen() {
    let mut app = app_in("en", true);
    app.update(Input::Lost, Instant::now());
    assert_eq!(app.leave, Some(Leave::Gone));
}

#[test]
fn a_refusal_shows_in_words_in_the_status_bar() {
    let mut app = app_in("en", true);
    app.update(Input::Failed("error.noSubscription".into()), Instant::now());
    let lines = draw(&mut app, 80, 24);
    assert!(shows(&lines[23..], &app.t("error.noSubscription")), "{}", lines[23]);
}

#[test]
fn accounts_are_read_from_the_checks() {
    let mut app = app_in("en", true);
    sign_in(&mut app, "claude", "SIGNED_IN");
    app.update(Input::Account(json!({ "vendor": "codex", "installed": false })), Instant::now());
    assert_eq!(app.accounts, [Account::SignedIn, Account::Missing]);
}
