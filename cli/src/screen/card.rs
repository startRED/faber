//! The cards of the AI: a question, a plan to approve, a permission. Each is answered on number keys, and the answer is the
//! command the window sends for the same card.

use crate::thread;
use serde_json::{Map, Value, json};

pub struct Choice {
    pub label: String,
    pub about: String,
}

pub struct Question {
    pub text: String,
    pub options: Vec<Choice>,
    pub multi: bool,
}

pub enum Kind {
    /// `AskUserQuestion`: one or more questions asked in turn, each with its options and a way to write one's own answer.
    Questions { items: Vec<Question>, at: usize, answers: Map<String, Value>, marked: Vec<usize> },
    /// `ExitPlanMode`: start the plan or keep planning.
    Plan { text: String, mode: String },
    /// Any other tool: allow once, always allow, refuse.
    Permission { step: String },
}

pub struct Card {
    /// The turn and the request: request ids repeat across turns.
    pub key: String,
    pub conversation: String,
    pub turn: String,
    pub request: String,
    pub kind: Kind,
    /// The choice Enter takes.
    pub cursor: usize,
    /// The person is writing their own answer to the question.
    pub own: bool,
}

/// What a key did to a card.
#[derive(Debug, PartialEq)]
pub enum Step {
    Stay,
    /// The card is answered: this command settles it.
    Send(&'static str, Value),
}

pub fn key_of(turn: &str, request: &str) -> String {
    format!("{turn}/{request}")
}

impl Card {
    /// From one entry of a running item's `permissions`.
    pub fn new(conversation: &str, turn: &str, permission: &Value) -> Card {
        let request = permission["requestId"].as_str().unwrap_or("").to_string();
        let input = &permission["input"];
        let tool = permission["tool"].as_str().unwrap_or("");
        let kind = match tool {
            "AskUserQuestion" if input["questions"].as_array().is_some_and(|q| !q.is_empty()) => Kind::Questions {
                items: input["questions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|q| Question {
                        text: q["question"].as_str().unwrap_or("").to_string(),
                        options: q["options"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|o| Choice {
                                label: o["label"].as_str().unwrap_or("").to_string(),
                                about: o["description"].as_str().unwrap_or("").to_string(),
                            })
                            .collect(),
                        multi: q["multiSelect"].as_bool() == Some(true),
                    })
                    .collect(),
                at: 0,
                answers: Map::new(),
                marked: Vec::new(),
            },
            "ExitPlanMode" => Kind::Plan {
                text: input["plan"].as_str().unwrap_or("").to_string(),
                mode: input["mode"].as_str().unwrap_or("ask").to_string(),
            },
            _ => Kind::Permission { step: thread::step_line(&json!({ "name": tool, "input": input })) },
        };
        let mut card = Card {
            key: key_of(turn, &request),
            conversation: conversation.into(),
            turn: turn.into(),
            request,
            kind,
            cursor: 0,
            own: false,
        };
        card.settle();
        card
    }

    /// A question with no options can only be answered in the person's words.
    fn settle(&mut self) {
        if let Kind::Questions { items, at, .. } = &self.kind {
            self.own = items.get(*at).is_some_and(|q| q.options.is_empty());
        }
    }

    /// The question being asked, when this card asks questions.
    pub fn question(&self) -> Option<(&Question, usize, usize)> {
        match &self.kind {
            Kind::Questions { items, at, .. } => items.get(*at).map(|q| (q, *at, items.len())),
            _ => None,
        }
    }

    pub fn marked(&self, n: usize) -> bool {
        matches!(&self.kind, Kind::Questions { marked, .. } if marked.contains(&n))
    }

    /// How many numbered choices the card has (a question's last one is "write my own").
    pub fn choices(&self) -> usize {
        match &self.kind {
            Kind::Questions { items, at, .. } => items.get(*at).map_or(0, |q| q.options.len() + usize::from(!q.options.is_empty())),
            Kind::Plan { .. } => 2,
            Kind::Permission { .. } => 3,
        }
    }

    pub fn is_multi(&self) -> bool {
        self.question().is_some_and(|(q, _, _)| q.multi)
    }

    pub fn move_cursor(&mut self, down: bool) {
        let last = self.choices().saturating_sub(1);
        self.cursor = if down { (self.cursor + 1).min(last) } else { self.cursor.saturating_sub(1) };
    }

    /// The number key of choice `n` (counted from 0).
    pub fn pick(&mut self, n: usize) -> Step {
        if n >= self.choices() {
            return Step::Stay;
        }
        let args = |extra: Value| {
            let mut args = json!({ "id": self.conversation, "turnId": self.turn, "requestId": self.request });
            if let (Some(args), Some(extra)) = (args.as_object_mut(), extra.as_object()) {
                args.extend(extra.clone());
            }
            args
        };
        match &self.kind {
            Kind::Permission { .. } => {
                let (allow, always) = match n {
                    0 => (true, false),
                    1 => (true, true),
                    _ => (false, false),
                };
                Step::Send("answer", args(json!({ "allow": allow, "always": always })))
            }
            Kind::Plan { mode, .. } => {
                let mode = if n == 0 { json!(mode) } else { Value::Null };
                Step::Send("approve_plan", args(json!({ "mode": mode })))
            }
            Kind::Questions { .. } => self.pick_question(n),
        }
    }

    fn pick_question(&mut self, n: usize) -> Step {
        let Kind::Questions { items, at, marked, .. } = &mut self.kind else { return Step::Stay };
        let Some(question) = items.get(*at) else { return Step::Stay };
        if n == question.options.len() {
            self.own = true;
            return Step::Stay;
        }
        if question.multi {
            match marked.iter().position(|m| *m == n) {
                Some(found) => {
                    marked.remove(found);
                }
                None => marked.push(n),
            }
            return Step::Stay;
        }
        let label = question.options[n].label.clone();
        self.record(label)
    }

    /// Enter: confirms a multiple choice, or takes the choice under the cursor.
    pub fn confirm(&mut self) -> Step {
        if let Kind::Questions { items, at, marked, .. } = &self.kind
            && items.get(*at).is_some_and(|q| q.multi)
        {
            if marked.is_empty() {
                return Step::Stay;
            }
            let mut picked = marked.clone();
            picked.sort_unstable();
            let labels: Vec<&str> = picked.iter().filter_map(|n| items[*at].options.get(*n)).map(|o| o.label.as_str()).collect();
            let answer = labels.join(", ");
            return self.record(answer);
        }
        self.pick(self.cursor)
    }

    /// The person's own words for the question being asked.
    pub fn own_answer(&mut self, text: &str) -> Step {
        let text = text.trim();
        if text.is_empty() {
            return Step::Stay;
        }
        self.own = false;
        self.record(text.to_string())
    }

    /// Leaves the "write my own" mode without answering.
    pub fn cancel_own(&mut self) {
        self.own = false;
        self.settle();
    }

    /// Keeps the answer to the question being asked and moves to the next; the last one settles the card.
    fn record(&mut self, answer: String) -> Step {
        let Kind::Questions { items, at, answers, marked } = &mut self.kind else { return Step::Stay };
        if let Some(question) = items.get(*at) {
            answers.insert(question.text.clone(), Value::String(answer));
        }
        *at += 1;
        marked.clear();
        self.cursor = 0;
        if *at >= items.len() {
            let answers = Value::Object(answers.clone());
            return Step::Send(
                "answer_question",
                json!({ "id": self.conversation, "turnId": self.turn, "requestId": self.request, "answers": answers }),
            );
        }
        self.settle();
        Step::Stay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn questions() -> Value {
        json!({ "requestId": "r1", "tool": "AskUserQuestion", "input": { "questions": [
            { "question": "Which?", "options": [{ "label": "A", "description": "first" }, { "label": "B" }] },
            { "question": "Which ones?", "multiSelect": true, "options": [{ "label": "X" }, { "label": "Y" }, { "label": "Z" }] },
        ] } })
    }

    #[test]
    fn questions_are_answered_in_turn_with_their_labels() {
        let mut card = Card::new("c1", "t1", &questions());
        assert_eq!((card.choices(), card.key.as_str()), (3, "t1/r1"));
        assert_eq!(card.pick(1), Step::Stay);
        assert_eq!(card.choices(), 4);
        assert_eq!(card.confirm(), Step::Stay, "nothing marked yet");
        card.pick(0);
        card.pick(2);
        card.pick(0);
        let step = card.confirm();
        assert_eq!(
            step,
            Step::Send(
                "answer_question",
                json!({ "id": "c1", "turnId": "t1", "requestId": "r1", "answers": { "Which?": "B", "Which ones?": "Z" } })
            )
        );
    }

    #[test]
    fn writing_ones_own_answer_takes_the_last_number() {
        let mut card = Card::new("c1", "t1", &questions());
        assert_eq!(card.pick(2), Step::Stay);
        assert!(card.own);
        assert_eq!(card.own_answer("  "), Step::Stay);
        assert_eq!(card.own_answer(" only Fridays "), Step::Stay);
        assert!(!card.own);
        assert_eq!(card.question().map(|(_, at, total)| (at, total)), Some((1, 2)));
        card.pick(1);
        let Step::Send(cmd, args) = card.confirm() else { panic!("the last answer settles the card") };
        assert_eq!(cmd, "answer_question");
        assert_eq!(args["answers"], json!({ "Which?": "only Fridays", "Which ones?": "Y" }));
    }

    #[test]
    fn a_question_without_options_is_answered_in_words_only() {
        let card = Card::new(
            "c1",
            "t1",
            &json!({ "requestId": "r", "tool": "AskUserQuestion", "input": { "questions": [{ "question": "Name?" }] } }),
        );
        assert!(card.own);
        assert_eq!(card.choices(), 0);
    }

    #[test]
    fn a_plan_is_started_in_its_mode_or_kept_open() {
        let permission = json!({ "requestId": "r2", "tool": "ExitPlanMode", "input": { "plan": "1. do", "mode": "acceptEdits" } });
        let mut card = Card::new("c1", "t1", &permission);
        assert_eq!(
            card.pick(0),
            Step::Send("approve_plan", json!({ "id": "c1", "turnId": "t1", "requestId": "r2", "mode": "acceptEdits" }))
        );
        assert_eq!(card.pick(1), Step::Send("approve_plan", json!({ "id": "c1", "turnId": "t1", "requestId": "r2", "mode": null })));
        assert_eq!(card.pick(2), Step::Stay);
    }

    #[test]
    fn a_permission_allows_once_always_or_refuses() {
        let permission = json!({ "requestId": "r3", "tool": "Bash", "input": { "command": "cargo test" } });
        let mut card = Card::new("c1", "t1", &permission);
        let Kind::Permission { step } = &card.kind else { panic!("a permission") };
        assert_eq!(step, "Bash cargo test");
        let send = |allow, always| {
            Step::Send("answer", json!({ "id": "c1", "turnId": "t1", "requestId": "r3", "allow": allow, "always": always }))
        };
        assert_eq!(card.pick(0), send(true, false));
        assert_eq!(card.pick(1), send(true, true));
        assert_eq!(card.pick(2), send(false, false));
        card.move_cursor(true);
        card.move_cursor(true);
        card.move_cursor(true);
        assert_eq!(card.confirm(), send(false, false), "Enter takes the choice under the cursor, and the cursor stops at the last");
    }
}
