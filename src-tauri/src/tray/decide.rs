//! What the tray says and when it notifies: plain functions of the running list, the window's focus and the words the
//! screen sent. Nothing here touches Tauri, so all of it is tested without a tray.

use faber_engine::model::{RunState, RunningItem};
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

/// What the tray remembers between lists, to ignore old ones and to notify once per question.
#[derive(Default)]
pub(super) struct Watch {
    revision: Option<u64>,
    /// Every question seen since Faber started, answered or not: a key is notified once, ever (the turn id keeps a reused
    /// request id apart). ponytail: one small tuple per question, never pruned; prune by turn if a session ever asks millions.
    seen: HashSet<Question>,
}

/// What a list changes: the numbers to show, and the title of the conversation of each question to notify.
#[derive(Debug, PartialEq)]
pub(super) struct Update {
    pub(super) counts: Counts,
    pub(super) notices: Vec<String>,
}

impl Watch {
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
        Some(Update { counts: counts(items), notices })
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
}
