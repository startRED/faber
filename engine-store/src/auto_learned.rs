//! What Auto learns on this PC from what the person redoes and keeps: a bounded nudge, per kind of task, one rung up when a kind
//! of answer is often redone and back down once its answers are kept. Plain data with explicit times, so it is testable without
//! a clock; the engine keeps one and writes it to `auto-learned.json`, which is never sent anywhere.
//!
//! Only two things say anything about a pick. A retry of an Auto answer (the same request asked again) says the answer was not
//! good enough, and the person taking the step up Faber offered says it twice. A message sent after an answer says the answer
//! was kept. Nothing else counts: an edited request is a new request, a stopped answer may only have been slow or off course, and
//! a failed check is Faber's own reading that the person has not judged (it is offered as a step up instead).

use crate::route::KINDS;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Redo points among a kind's last answers that lift it: three, and at least half of what was marked.
pub const UP_AT: usize = 3;
/// Answers marked per kind; older ones fall out.
pub const WINDOW: usize = 10;
/// Kept answers in a row, with no redo between them, that bring a lifted kind back down.
pub const DOWN_AT: usize = 8;
/// A kind that has not been marked for this long starts again: what it learned is too old to say anything.
pub const FORGET_AFTER: u64 = 60 * 86_400;

/// What the person did with one Auto answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// The same request asked again.
    Redo,
    /// The same request asked again on the step up Faber offered: a redo that Faber's own checks agreed with. Counts twice.
    StepUp,
    /// A message sent after the answer, in the same conversation.
    Kept,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mark {
    Redo,
    Kept,
}

/// What is known about one kind of task.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Kind {
    marks: Vec<Mark>,
    lifted: bool,
    /// Unix seconds of the last mark.
    at: u64,
}

/// The nudges learned, by task kind (`route::KINDS`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Learned {
    kinds: BTreeMap<String, Kind>,
}

impl Learned {
    /// A file that was edited by hand or came from another version keeps only what it can still mean; a stamp in the future (a
    /// clock set back, a hand edit) counts as `now`, so the 60 days of forgetting still run out.
    pub fn sanitized(mut self, now: u64) -> Self {
        self.kinds.retain(|name, _| KINDS.contains(&name.as_str()));
        for kind in self.kinds.values_mut() {
            let extra = kind.marks.len().saturating_sub(WINDOW);
            kind.marks.drain(..extra);
            kind.at = kind.at.min(now);
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// Everything forgotten.
    pub fn forget(&mut self) {
        self.kinds.clear();
    }

    /// True while this kind of task starts one rung higher than the table alone would start it.
    pub fn lifted(&self, kind: &str, now: u64) -> bool {
        self.kinds.get(kind).is_some_and(|k| k.lifted && now.saturating_sub(k.at) <= FORGET_AFTER)
    }

    /// Records what the person did with an answer of this kind. An unknown kind says nothing about any of ours.
    pub fn note(&mut self, kind: &str, signal: Signal, now: u64) {
        if !KINDS.contains(&kind) {
            return;
        }
        let k = self.kinds.entry(kind.into()).or_default();
        if now.saturating_sub(k.at) > FORGET_AFTER {
            *k = Kind::default();
        }
        k.at = now;
        k.marks.extend(match signal {
            Signal::Redo => vec![Mark::Redo],
            Signal::StepUp => vec![Mark::Redo; 2],
            Signal::Kept => vec![Mark::Kept],
        });
        let extra = k.marks.len().saturating_sub(WINDOW);
        k.marks.drain(..extra);
        let redos = k.marks.iter().filter(|m| **m == Mark::Redo).count();
        if !k.lifted {
            if redos >= UP_AT && redos * 2 >= k.marks.len() {
                // The evidence from here on is about the new pick.
                (k.lifted, k.marks) = (true, vec![]);
            }
        } else if k.marks.len() >= DOWN_AT && k.marks[k.marks.len() - DOWN_AT..].iter().all(|m| *m == Mark::Kept) {
            (k.lifted, k.marks) = (false, vec![]);
        }
    }
}

/// The kind an Auto answer was routed as, from its reason key (`reason.route.<kind>.<level>`); None for any other answer.
pub fn kind_of(reason: &str) -> Option<&str> {
    let kind = reason.strip_prefix("reason.route.")?.split('.').next()?;
    KINDS.contains(&kind).then_some(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: u64 = 1_000_000;

    #[test]
    fn the_kind_is_read_from_the_reason_key_and_nothing_else_counts() {
        assert_eq!(kind_of("reason.route.code.hard"), Some("code"));
        assert_eq!(kind_of("reason.route.research.easy"), Some("research"));
        for other in ["reason.chosen", "reason.autoDefault", "reason.route.poetry.easy", "reason.route.", ""] {
            assert_eq!(kind_of(other), None, "{other}");
        }
    }

    #[test]
    fn a_kind_is_lifted_after_three_redo_points_that_are_half_of_what_was_marked() {
        let mut l = Learned::default();
        l.note("code", Signal::Redo, T);
        l.note("code", Signal::Redo, T);
        assert!(!l.lifted("code", T), "two redos are not a habit");
        l.note("code", Signal::Redo, T);
        assert!(l.lifted("code", T));
        assert!(!l.lifted("writing", T), "another kind is untouched");
    }

    #[test]
    fn taking_the_step_up_counts_twice() {
        let mut l = Learned::default();
        l.note("math", Signal::StepUp, T);
        assert!(!l.lifted("math", T));
        l.note("math", Signal::Redo, T);
        assert!(l.lifted("math", T));
    }

    #[test]
    fn redos_among_many_kept_answers_are_not_a_habit() {
        let mut l = Learned::default();
        for _ in 0..3 {
            l.note("writing", Signal::Redo, T);
            if l.lifted("writing", T) {
                break;
            }
            for _ in 0..3 {
                l.note("writing", Signal::Kept, T);
            }
        }
        assert!(!l.lifted("writing", T), "a redo in every four answers stays under half");
        // Old redos fall out of the window as new answers are kept.
        for _ in 0..WINDOW {
            l.note("writing", Signal::Kept, T);
        }
        l.note("writing", Signal::Redo, T);
        l.note("writing", Signal::Redo, T);
        assert!(!l.lifted("writing", T));
    }

    #[test]
    fn a_lift_is_one_rung_and_goes_back_down_after_eight_kept_answers_in_a_row() {
        let mut l = Learned::default();
        for _ in 0..UP_AT {
            l.note("code", Signal::Redo, T);
        }
        assert!(l.lifted("code", T));
        // More redos change nothing: the bound is one rung.
        for _ in 0..5 {
            l.note("code", Signal::Redo, T);
        }
        assert!(l.lifted("code", T));
        // Seven kept answers are not enough, and a redo between them starts the count again.
        for _ in 0..DOWN_AT - 1 {
            l.note("code", Signal::Kept, T);
        }
        assert!(l.lifted("code", T));
        l.note("code", Signal::Redo, T);
        for _ in 0..DOWN_AT - 1 {
            l.note("code", Signal::Kept, T);
        }
        assert!(l.lifted("code", T));
        l.note("code", Signal::Kept, T);
        assert!(!l.lifted("code", T), "eight kept in a row bring it back");
        // And it starts from nothing: one redo does not lift it again.
        l.note("code", Signal::Redo, T);
        assert!(!l.lifted("code", T));
    }

    #[test]
    fn a_kind_not_marked_for_sixty_days_is_forgotten() {
        let mut l = Learned::default();
        for _ in 0..UP_AT {
            l.note("code", Signal::Redo, T);
        }
        assert!(l.lifted("code", T + FORGET_AFTER));
        assert!(!l.lifted("code", T + FORGET_AFTER + 1), "stale at read time");
        l.note("code", Signal::Redo, T + FORGET_AFTER + 1);
        assert!(!l.lifted("code", T + FORGET_AFTER + 1), "and the old redos do not count toward a new lift");
    }

    #[test]
    fn an_unknown_kind_is_never_stored() {
        let mut l = Learned::default();
        l.note("poetry", Signal::Redo, T);
        assert!(l.is_empty());
    }

    #[test]
    fn forgetting_clears_everything() {
        let mut l = Learned::default();
        for _ in 0..UP_AT {
            l.note("code", Signal::Redo, T);
        }
        l.forget();
        assert!(l.is_empty() && !l.lifted("code", T));
    }

    #[test]
    fn a_file_with_unknown_kinds_or_too_many_marks_is_cut_down_to_what_can_mean_something() {
        let marks = vec![Mark::Kept; 40];
        let l = Learned {
            kinds: BTreeMap::from([
                ("code".into(), Kind { marks, lifted: false, at: T + 500 }),
                ("poetry".into(), Kind { marks: vec![Mark::Redo], lifted: true, at: T }),
            ]),
        }
        .sanitized(T);
        assert_eq!(l.kinds.keys().collect::<Vec<_>>(), ["code"]);
        assert_eq!(l.kinds["code"].marks.len(), WINDOW);
        assert_eq!(l.kinds["code"].at, T, "a stamp from the future is read as now");
    }
}
