//! What the tray says and when it notifies: plain functions of the running list, the window's focus and the words the
//! screen sent. Nothing here touches Tauri, so all of it is tested without a tray.

use faber_engine::model::{RunState, RunningItem, Settings};
use faber_engine::project::wire::MissionView;
use faber_engine::project::{DecisionKind, MissionState};
use serde::Deserialize;
use std::collections::HashSet;

/// A text in two forms, as the screen sends it: `one` for a count of 1 and `other` for any other count. That is what
/// `Intl.PluralRules` picks in all five languages for counts of 1 and more.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(super) struct Plural {
    one: String,
    other: String,
}

impl Plural {
    fn pick(&self, n: usize) -> &str {
        if n == 1 { &self.one } else { &self.other }
    }
}

/// The words the shell shows by itself, which the screen sends whenever the language changes. They stay here, so a
/// closed window still has them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ShellTexts {
    pub(super) open: String,
    pub(super) quit: String,
    /// The tooltip while something runs, with `{n}` for the count: "Faber: {n} running".
    running: Plural,
    /// Added to it while something needs the user, with `{n}`: ", {n} needs you".
    needs_you: Plural,
    /// The notification, with `{title}` for the conversation: "{title} needs you".
    notify: String,
}

impl Default for ShellTexts {
    /// English, until the screen has said which language it uses.
    fn default() -> Self {
        let same = |s: &str| Plural { one: s.into(), other: s.into() };
        ShellTexts {
            open: "Open Faber".into(),
            quit: "Quit Faber".into(),
            running: same("Faber: {n} running"),
            needs_you: Plural { one: ", {n} needs you".into(), other: ", {n} need you".into() },
            notify: "{title} needs you".into(),
        }
    }
}

fn fill(template: &str, name: &str, value: &str) -> String {
    template.replace(&format!("{{{name}}}"), value)
}

/// What the tray reports: every item of the running list, and those that wait for the user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Counts {
    running: usize,
    pub(super) needs_you: usize,
}

fn counts(items: &[RunningItem]) -> Counts {
    Counts { running: items.len(), needs_you: items.iter().filter(|i| i.state == RunState::NeedsYou).count() }
}

/// "Faber" when nothing runs. Otherwise the count, and the words "needs you" next to it when something does: amber
/// never stands for a number alone [tokens].
pub(super) fn tooltip(texts: &ShellTexts, c: Counts) -> String {
    if c.running == 0 {
        return "Faber".into();
    }
    let mut text = fill(texts.running.pick(c.running), "n", &c.running.to_string());
    if c.needs_you > 0 {
        text.push_str(&fill(texts.needs_you.pick(c.needs_you), "n", &c.needs_you.to_string()));
    }
    text
}

/// A question the user has not answered: the conversation, the turn and the request together, because request ids
/// alone repeat (Codex numbers them from 0 in each server).
type Question = (String, String, String);

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Delivery {
    pub(super) toast: bool,
    pub(super) sound: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Attention {
    Normal,
    #[cfg(any(windows, test))]
    SoundOnly,
    Silent,
}

/// Windows enum values are pinned below against shellapi.h. Unknown states never permit a toast.
#[cfg(any(windows, test))]
pub(super) fn windows_attention(state: Option<i32>, full_screen: bool) -> Attention {
    match state {
        Some(1 | 6) => Attention::Silent, // QUNS_NOT_PRESENT, QUNS_QUIET_TIME (initial login/upgrade, not DND).
        Some(5) if !full_screen => Attention::Normal, // QUNS_ACCEPTS_NOTIFICATIONS.
        _ => Attention::SoundOnly,
    }
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub(super) struct Rect {
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) right: i32,
    pub(super) bottom: i32,
}

#[cfg(any(windows, test))]
pub(super) fn covers_monitor(window: Rect, monitor: Rect) -> bool {
    monitor.left < monitor.right
        && monitor.top < monitor.bottom
        && window.left <= monitor.left
        && window.top <= monitor.top
        && window.right >= monitor.right
        && window.bottom >= monitor.bottom
}

pub(super) fn delivery(focused: bool, attention: Attention, settings: &Settings) -> Delivery {
    let active = !focused && attention != Attention::Silent;
    Delivery {
        toast: active && attention == Attention::Normal && settings.notifications != Some(false),
        sound: active && settings.notification_sound != Some(false),
    }
}

/// What the tray remembers between lists and mission views, to ignore old ones and to notify once per question or decision.
#[derive(Default)]
pub(super) struct Watch {
    revision: Option<u64>,
    /// Every question seen since Faber started, answered or not: a key is notified once, ever (the turn id keeps a reused
    /// request id apart). ponytail: one small tuple per question, never pruned; prune by turn if a session ever asks millions.
    seen: HashSet<Question>,
    /// Every mission decision seen, across reloads/restarts: (mission_id, decision_id).
    seen_decisions: HashSet<(String, String)>,
    last_running_counts: Counts,
    last_mission_counts: Counts,
}

/// What a list changes: the numbers to show, and the title of the conversation of each question to notify.
#[derive(Debug, PartialEq)]
pub(super) struct Update {
    pub(super) counts: Counts,
    pub(super) notices: Vec<String>,
}

impl Watch {
    pub(super) fn with_seen_decisions(seen_decisions: HashSet<(String, String)>) -> Self {
        Watch { seen_decisions, ..Default::default() }
    }

    pub(super) fn seen_decisions(&self) -> &HashSet<(String, String)> {
        &self.seen_decisions
    }

    /// None for a list that is not newer than the last one taken in: it changes nothing. A question the user could
    /// already see (the window was focused when it came) is never notified later, and a question never notifies twice.
    pub(super) fn take(&mut self, revision: u64, items: &[RunningItem], window_focused: bool) -> Option<Update> {
        if self.revision.is_some_and(|last| revision <= last) {
            return None;
        }
        self.revision = Some(revision);
        let mut notices = Vec::new();
        for item in items {
            for p in &item.permissions {
                let key = (item.conversation_id.clone(), item.turn_id.clone(), p.request_id.clone());
                if self.seen.insert(key) && !window_focused {
                    notices.push(item.title.clone());
                }
            }
        }
        self.last_running_counts = counts(items);
        let combined_counts = Counts {
            running: self.last_running_counts.running + self.last_mission_counts.running,
            needs_you: self.last_running_counts.needs_you + self.last_mission_counts.needs_you,
        };
        Some(Update { counts: combined_counts, notices })
    }

    /// Evaluates a mission view: concrete choices and direction/taste/money/legal blockers become needs-you items
    /// and notify when not focused. Completion, commits, proofs, quota waits, promotions and technical decisions emit nothing.
    /// Stable decision identity deduplicates across reloads and restarts.
    pub(super) fn take_mission(&mut self, view: &MissionView, window_focused: bool) -> Option<Update> {
        let mut notices = Vec::new();
        let mut mission_needs = 0;
        for d in &view.decisions {
            if d.kind != DecisionKind::Technical {
                mission_needs += 1;
                let key = (view.mission_id.clone(), d.id.clone());
                if self.seen_decisions.insert(key) && !window_focused {
                    let title = if view.goal.is_empty() { "Mission".to_string() } else { view.goal.clone() };
                    notices.push(title);
                }
            }
        }
        if matches!(view.state, MissionState::Blocked | MissionState::Paused)
            && view.reason.as_ref().is_some_and(|r| r.starts_with("reason.grant."))
        {
            mission_needs += 1;
        }
        self.last_mission_counts =
            Counts { running: if matches!(view.state, MissionState::Running) { 1 } else { 0 }, needs_you: mission_needs };
        let combined_counts = Counts {
            running: self.last_running_counts.running + self.last_mission_counts.running,
            needs_you: self.last_running_counts.needs_you + self.last_mission_counts.needs_you,
        };
        Some(Update { counts: combined_counts, notices })
    }
}

/// What the tray picture and tooltip last showed: the counts and the side of the tray picture. A scale change redraws
/// even when the counts stay the same.
#[derive(Default)]
pub(super) struct Shown(Option<(Counts, u32)>);

impl Shown {
    /// True when `counts` at `side` differ from what was last shown; remembers them either way.
    pub(super) fn changed(&mut self, counts: Counts, side: u32) -> bool {
        self.0.replace((counts, side)) != Some((counts, side))
    }
}

/// The text of one notification; a conversation without a title yet is called by the app's name.
pub(super) fn notice_text(texts: &ShellTexts, title: &str) -> String {
    fill(&texts.notify, "title", if title.is_empty() { "Faber" } else { title })
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_engine::model::{Area, Permission};
    use serde_json::json;

    #[test]
    fn a_store_app_does_not_allow_a_toast() {
        assert_eq!(windows_attention(Some(7), false), Attention::SoundOnly);
    }

    #[test]
    fn locked_and_quiet_states_are_silent() {
        for state in [1, 6] {
            assert_eq!(
                delivery(false, windows_attention(Some(state), false), &Settings::default()),
                Delivery { toast: false, sound: false }
            );
        }
    }

    #[test]
    fn sound_only_alerts_work_with_notifications_disabled() {
        let settings = Settings { notifications: Some(false), ..Settings::default() };
        assert_eq!(delivery(false, Attention::SoundOnly, &settings), Delivery { toast: false, sound: true });
    }

    #[test]
    fn focused_full_screen_and_normal_delivery_respect_both_switches() {
        // (attention, toast switch, sound switch, expected toast, expected chime).
        use Attention::{Normal, Silent, SoundOnly};
        for (attention, notifications, notification_sound, toast, chime) in [
            (Normal, true, true, true, true),
            (Normal, true, false, true, false),
            (Normal, false, true, false, true),
            (Normal, false, false, false, false),
            (SoundOnly, true, true, false, true),
            (SoundOnly, true, false, false, false),
            (SoundOnly, false, true, false, true),
            (SoundOnly, false, false, false, false),
            (Silent, true, true, false, false),
            (Silent, true, false, false, false),
            (Silent, false, true, false, false),
            (Silent, false, false, false, false),
        ] {
            let settings =
                Settings { notifications: Some(notifications), notification_sound: Some(notification_sound), ..Settings::default() };
            assert_eq!(delivery(false, attention, &settings), Delivery { toast, sound: chime });
            assert_eq!(delivery(true, attention, &settings), Delivery { toast: false, sound: false });
        }
        let legacy: Settings = serde_json::from_value(json!({ "language": "en" })).unwrap();
        assert_eq!(delivery(false, Normal, &legacy), Delivery { toast: true, sound: true });
        assert_eq!(delivery(false, SoundOnly, &legacy), Delivery { toast: false, sound: true });
        assert_eq!(delivery(true, Normal, &legacy), Delivery { toast: false, sound: false });
    }

    #[test]
    fn windows_enum_values_allow_only_accepts_notifications_and_respect_quiet_time() {
        use Attention::{Normal, Silent, SoundOnly};
        for (state, expected) in [(1, Silent), (2, SoundOnly), (3, SoundOnly), (4, SoundOnly), (5, Normal), (6, Silent), (7, SoundOnly)] {
            assert_eq!(windows_attention(Some(state), false), expected, "shellapi.h state {state}");
        }
        for state in [None, Some(0), Some(-1), Some(8)] {
            assert_eq!(windows_attention(state, false), SoundOnly, "failed or unknown query {state:?}");
        }
        assert_eq!(windows_attention(Some(5), true), SoundOnly, "foreground borderless full screen overrides normal");
        assert_eq!(windows_attention(Some(1), true), Silent, "full screen does not override locking");
        assert_eq!(windows_attention(Some(6), true), Silent, "full screen does not override quiet time");
    }

    #[test]
    fn full_screen_geometry_uses_the_whole_monitor_and_handles_negative_origins() {
        let monitor = Rect { left: -2560, top: -1440, right: 0, bottom: 0 };
        assert!(covers_monitor(monitor, monitor), "borderless on another monitor");
        assert!(covers_monitor(Rect { left: -2568, top: -1448, right: 8, bottom: 8 }, monitor), "invisible resize borders");
        assert!(!covers_monitor(Rect { bottom: -48, ..monitor }, monitor), "maximized with a visible taskbar");
        assert!(!covers_monitor(Rect { left: -2559, ..monitor }, monitor), "one edge falls short");
        assert!(!covers_monitor(Rect { left: 0, top: 0, right: 2560, bottom: 1440 }, monitor), "same size on the wrong monitor");
        assert!(!covers_monitor(Rect::default(), monitor));
        assert!(!covers_monitor(monitor, Rect::default()), "invalid monitor bounds");
    }

    #[test]
    fn a_question_tool_notifies_once_but_finished_work_never_notifies() {
        let mut watch = Watch::default();
        let events = faber_engine::claude::parse_claude_message(&json!({
            "type": "control_request", "request_id": "ask-1",
            "request": { "subtype": "can_use_tool", "tool_name": "AskUserQuestion", "input": { "questions": [] } }
        }));
        let [faber_engine::events::EngineEvent::Permission { request_id, tool, input }] = events.as_slice() else {
            panic!("a user question must reach permissions")
        };
        let mut asking = item("a", "t1", "Choose a direction", &[]);
        asking.state = RunState::NeedsYou;
        asking.permissions = vec![Permission { request_id: request_id.clone(), tool: tool.clone(), input: input.clone() }];
        assert_eq!(watch.take(1, &[asking.clone()], false).unwrap().notices, ["Choose a direction"]);
        assert!(watch.take(2, &[asking], false).unwrap().notices.is_empty());
        assert!(watch.take(3, &[item("a", "t1", "Working", &[])], false).unwrap().notices.is_empty());
        assert!(watch.take(4, &[], false).unwrap().notices.is_empty(), "finishing work is not an attention request");
    }

    fn item(conversation: &str, turn: &str, title: &str, requests: &[&str]) -> RunningItem {
        RunningItem {
            conversation_id: conversation.into(),
            turn_id: turn.into(),
            title: title.into(),
            area: Area::Chat,
            project_id: None,
            vendor: None,
            model: None,
            state: if requests.is_empty() { RunState::Working } else { RunState::NeedsYou },
            since: 0,
            wait_until: None,
            permissions: requests.iter().map(|r| Permission { request_id: (*r).into(), tool: "Write".into(), input: json!({}) }).collect(),
        }
    }

    #[test]
    fn the_tooltip_has_the_words_for_each_case() {
        let texts = ShellTexts::default();
        let tip = |running, needs_you| tooltip(&texts, Counts { running, needs_you });
        assert_eq!(tip(0, 0), "Faber");
        assert_eq!(tip(1, 0), "Faber: 1 running");
        assert_eq!(tip(3, 0), "Faber: 3 running");
        assert_eq!(tip(3, 1), "Faber: 3 running, 1 needs you");
        assert_eq!(tip(3, 2), "Faber: 3 running, 2 need you");
        assert_eq!(tip(12, 11), "Faber: 12 running, 11 need you");
    }

    #[test]
    fn the_plural_is_one_for_1_and_other_for_the_rest() {
        let p = Plural { one: "a".into(), other: "b".into() };
        assert_eq!([p.pick(1), p.pick(2), p.pick(11)], ["a", "b", "b"]);
    }

    #[test]
    fn the_texts_arrive_as_the_screen_sends_them() {
        let texts: ShellTexts = serde_json::from_value(json!({
            "open": "Abrir", "quit": "Sair",
            "running": { "one": "Faber: {n} em andamento", "other": "Faber: {n} em andamento" },
            "needsYou": { "one": ", {n} precisa de você", "other": ", {n} precisam de você" },
            "notify": "{title} precisa de você"
        }))
        .unwrap();
        assert_eq!(tooltip(&texts, Counts { running: 2, needs_you: 2 }), "Faber: 2 em andamento, 2 precisam de você");
        assert_eq!(notice_text(&texts, "Relatório"), "Relatório precisa de você");
    }

    #[test]
    fn the_counts_are_the_running_items_and_those_that_wait() {
        let items = [item("a", "t1", "A", &[]), item("b", "t1", "B", &["0"]), item("c", "t1", "C", &["0", "1"])];
        assert_eq!(counts(&items), Counts { running: 3, needs_you: 2 });
        assert_eq!(counts(&[]), Counts::default());
    }

    #[test]
    fn a_notice_without_a_title_uses_the_name_of_the_app() {
        let texts = ShellTexts::default();
        assert_eq!(notice_text(&texts, "Fix the build"), "Fix the build needs you");
        assert_eq!(notice_text(&texts, ""), "Faber needs you");
    }

    #[test]
    fn a_title_that_looks_like_a_placeholder_stays_as_it_is() {
        let texts = ShellTexts::default();
        assert_eq!(notice_text(&texts, "Use {n} and {title}"), "Use {n} and {title} needs you");
    }

    #[test]
    fn a_question_notifies_once() {
        let mut w = Watch::default();
        let list = [item("a", "t1", "Fix the build", &["0"])];
        assert_eq!(w.take(1, &list, false).unwrap().notices, ["Fix the build"]);
        assert_eq!(w.take(2, &list, false).unwrap().notices, Vec::<String>::new(), "the same list again");
        let renamed = [item("a", "t1", "Fix the tests", &["0"])];
        assert_eq!(w.take(3, &renamed, false).unwrap().notices, Vec::<String>::new(), "a new title is not a new question");
    }

    #[test]
    fn a_question_that_leaves_the_list_and_comes_back_does_not_notify_again() {
        let mut w = Watch::default();
        let asking = [item("a", "t1", "A", &["0"])];
        assert_eq!(w.take(1, &asking, false).unwrap().notices.len(), 1);
        assert!(w.take(2, &[], false).unwrap().notices.is_empty(), "gone from the list");
        assert!(w.take(3, &asking, false).unwrap().notices.is_empty(), "the same key is back");
        let next = [item("a", "t1", "A", &["1"])];
        assert_eq!(w.take(4, &next, false).unwrap().notices.len(), 1, "a different request is a new question");
    }

    #[test]
    fn a_key_twice_in_one_list_notifies_once() {
        let mut w = Watch::default();
        let twice = [item("a", "t1", "A", &["0"]), item("a", "t1", "A", &["0"])];
        assert_eq!(w.take(1, &twice, false).unwrap().notices, ["A"]);
    }

    #[test]
    fn the_same_request_id_in_two_conversations_or_two_turns_is_two_questions() {
        let mut w = Watch::default();
        let two_conversations = [item("a", "t1", "A", &["0"]), item("b", "t1", "B", &["0"])];
        assert_eq!(w.take(1, &two_conversations, false).unwrap().notices, ["A", "B"]);
        let next_turn = [item("a", "t2", "A", &["0"]), item("b", "t1", "B", &["0"])];
        assert_eq!(w.take(2, &next_turn, false).unwrap().notices, ["A"], "only the new turn's question");
    }

    #[test]
    fn two_questions_of_one_conversation_notify_twice() {
        let mut w = Watch::default();
        assert_eq!(w.take(1, &[item("a", "t1", "A", &["0", "1"])], false).unwrap().notices, ["A", "A"]);
    }

    #[test]
    fn it_never_notifies_while_the_window_is_focused_and_does_not_notify_that_question_later() {
        let mut w = Watch::default();
        let list = [item("a", "t1", "A", &["0"])];
        let focused = w.take(1, &list, true).unwrap();
        assert!(focused.notices.is_empty());
        assert_eq!(focused.counts.needs_you, 1, "the number still shows");
        assert!(w.take(2, &list, false).unwrap().notices.is_empty(), "the user could see it when it came");
        let another = [item("a", "t1", "A", &["0", "1"])];
        assert_eq!(w.take(3, &another, false).unwrap().notices.len(), 1, "a new question does notify");
    }

    #[test]
    fn it_notifies_while_the_window_is_closed_which_is_not_focused() {
        let mut w = Watch::default();
        let closed = false;
        assert_eq!(w.take(1, &[item("a", "t1", "A", &["0"])], closed).unwrap().notices.len(), 1);
    }

    #[test]
    fn a_question_that_waited_at_launch_notifies_once() {
        let mut w = Watch::default();
        assert_eq!(w.take(0, &[item("a", "t1", "A", &["0"])], false).unwrap().notices.len(), 1, "the first list may be revision 0");
    }

    #[test]
    fn an_older_or_repeated_revision_changes_nothing() {
        let mut w = Watch::default();
        let asking = [item("a", "t1", "A", &["0"])];
        assert!(w.take(5, &[], false).is_some());
        assert_eq!(w.take(4, &asking, false), None, "older");
        assert_eq!(w.take(5, &asking, false), None, "the same revision");
        // Nothing of the ignored lists was kept: the question is new when a newer list brings it.
        assert_eq!(w.take(6, &asking, false).unwrap().notices.len(), 1);
    }

    #[test]
    fn the_picture_is_redrawn_when_the_counts_or_the_display_scale_change() {
        let mut shown = Shown::default();
        let one = Counts { running: 1, needs_you: 1 };
        assert!(shown.changed(one, 16), "first time");
        assert!(!shown.changed(one, 16), "nothing changed");
        assert!(shown.changed(one, 32), "same counts, new display scale");
        assert!(shown.changed(Counts::default(), 32), "new counts");
    }

    #[test]
    fn the_numbers_follow_the_list() {
        let mut w = Watch::default();
        let list = [item("a", "t1", "A", &[]), item("b", "t1", "B", &["0"])];
        assert_eq!(w.take(1, &list, false).unwrap().counts, Counts { running: 2, needs_you: 1 });
        assert_eq!(w.take(2, &[], false).unwrap().counts, Counts::default());
    }

    #[test]
    fn stable_mission_decision_deduplicates_across_restart() {
        let mut seen = HashSet::new();
        seen.insert(("m1".into(), "d1".into()));
        let mut w = Watch::with_seen_decisions(seen);
        let view: MissionView = serde_json::from_value(json!({
            "missionId": "m1",
            "revision": 1,
            "state": "running",
            "mode": "normal",
            "gitMode": "seguro",
            "goal": "Refactor parser",
            "tasks": [],
            "cost": { "complete": false },
            "pace": [],
            "decisions": [
                { "id": "d1", "question": "Use AGPL or MIT?", "kind": "legal", "options": ["AGPL", "MIT"] },
                { "id": "d2", "question": "Pick accent color", "kind": "taste", "options": ["Lime", "Blue"] }
            ]
        }))
        .unwrap();
        let update = w.take_mission(&view, false).unwrap();
        // d1 was seen before restart; only d2 notifies!
        assert_eq!(update.notices, ["Refactor parser"]);
        assert_eq!(update.counts.needs_you, 2);
        // A second view with the same decisions notifies nothing:
        let update2 = w.take_mission(&view, false).unwrap();
        assert!(update2.notices.is_empty());
    }

    #[test]
    fn focused_mission_decision_means_silence() {
        let mut w = Watch::default();
        let view: MissionView = serde_json::from_value(json!({
            "missionId": "m1",
            "revision": 1,
            "state": "running",
            "mode": "normal",
            "gitMode": "seguro",
            "goal": "Goal",
            "tasks": [],
            "cost": { "complete": false },
            "pace": [],
            "decisions": [
                { "id": "d1", "question": "Question", "kind": "direction", "options": [] }
            ]
        }))
        .unwrap();
        let update = w.take_mission(&view, true).unwrap();
        assert!(update.notices.is_empty(), "focused means silence");
        assert_eq!(update.counts.needs_you, 1, "badge still counts needs-you");
        // Coming back while unfocused later does not notify because it was seen while focused:
        let update2 = w.take_mission(&view, false).unwrap();
        assert!(update2.notices.is_empty(), "seen while focused never notifies later");
    }

    #[test]
    fn completion_proof_commit_quota_wait_and_promotion_emit_nothing() {
        let mut w = Watch::default();
        for state in ["done", "running", "planning"] {
            let view: MissionView = serde_json::from_value(json!({
                "missionId": "m1",
                "revision": 2,
                "state": state,
                "mode": "normal",
                "gitMode": "seguro",
                "goal": "Completed goal",
                "tasks": [{"id": "t1", "title": "Proof passed", "state": "done"}],
                "cost": { "complete": true },
                "pace": [],
                "decisions": []
            }))
            .unwrap();
            let update = w.take_mission(&view, false).unwrap();
            assert!(update.notices.is_empty());
        }
    }

    #[test]
    fn technical_decisions_emit_nothing() {
        let mut w = Watch::default();
        let view: MissionView = serde_json::from_value(json!({
            "missionId": "m1",
            "revision": 1,
            "state": "running",
            "mode": "normal",
            "gitMode": "seguro",
            "goal": "Fix bug",
            "tasks": [],
            "cost": { "complete": false },
            "pace": [],
            "decisions": [
                { "id": "d1", "question": "Use tokio or async-std?", "kind": "technical", "options": [] }
            ]
        }))
        .unwrap();
        let update = w.take_mission(&view, false).unwrap();
        assert!(update.notices.is_empty());
        assert_eq!(update.counts.needs_you, 0, "technical decision is not needs-you");
    }
}
