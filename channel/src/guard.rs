//! Keeps an AI inside Faber from starting turns without end through the command line. Faber marks every CLI it starts
//! (`FABER_AGENT_DEPTH=1`), so a client that says it is an AI is one Faber runs. Such a client may drive Faber like a person,
//! with two limits: at most `MAX_RUNNING` of the turns it started run at once, and at most `MAX_PER_HOUR` start in an hour.
//! A chain of AIs that each wait for the next stops at the first limit; one that hands on and ends stops at the second.
//! A conversation cannot carry its own depth, because one Codex process serves every conversation of an area.

use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const MAX_RUNNING: usize = 2;
pub const MAX_PER_HOUR: usize = 20;
const HOUR: Duration = Duration::from_secs(3600);

#[derive(Default)]
struct Seen {
    /// Turns admitted whose answer has not come yet: they count as running, so requests sent together cannot pass the limit.
    admitted: usize,
    /// The conversations an AI client started a turn in.
    started: HashSet<String>,
    /// When each of those turns started, within the last hour.
    starts: VecDeque<Instant>,
    /// The conversations running now, from the latest `running` event.
    running: HashSet<String>,
    /// How many `running` events came so far.
    snapshots: u64,
    /// While turns wait for their answer: the `running` event each conversation last came into, and the one it last left.
    joined: HashMap<String, u64>,
    left: HashMap<String, u64>,
}

/// An admitted turn, handed back when its answer comes.
pub struct Ticket {
    /// The `running` events seen when it was admitted.
    seen: u64,
    /// When it was admitted: the entry it put in the hour's list.
    at: Instant,
}

#[derive(Default)]
pub struct Guard {
    seen: Mutex<Seen>,
}

impl Guard {
    /// A guard that follows what runs through `events` (the backend's own events).
    pub fn watching(mut events: tokio::sync::broadcast::Receiver<Value>) -> Arc<Guard> {
        let guard = Arc::new(Guard::default());
        let watcher = Arc::downgrade(&guard);
        tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // A dropped `running` event could be the one that said a turn ended: forget what was thought to run, so
                        // a place is never held for good (the hour's limit still holds).
                        if let Some(guard) = watcher.upgrade() {
                            guard.lock().running.clear();
                        }
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                };
                let Some(guard) = watcher.upgrade() else { return };
                if event["type"] == "running" {
                    guard.running(&event);
                }
            }
        });
        guard
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Seen> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn running(&self, event: &Value) {
        let running: HashSet<String> =
            event["items"].as_array().into_iter().flatten().filter_map(|i| i["conversationId"].as_str().map(String::from)).collect();
        let mut seen = self.lock();
        seen.snapshots += 1;
        let now = seen.snapshots;
        if seen.admitted > 0 {
            let came: Vec<String> = running.difference(&seen.running).cloned().collect();
            let went: Vec<String> = seen.running.difference(&running).cloned().collect();
            for id in came {
                seen.joined.insert(id, now);
            }
            for id in went {
                seen.left.insert(id, now);
            }
        }
        seen.running = running;
    }

    /// Whether an AI client may start one more turn now; the refusal's key when not.
    pub fn admit(&self, now: Instant) -> Result<Ticket, &'static str> {
        let mut seen = self.lock();
        while seen.starts.front().is_some_and(|at| now.duration_since(*at) >= HOUR) {
            seen.starts.pop_front();
        }
        let running = seen.started.intersection(&seen.running).count() + seen.admitted;
        if running >= MAX_RUNNING || seen.starts.len() >= MAX_PER_HOUR {
            return Err("error.channel.agentTurns");
        }
        seen.starts.push_back(now);
        seen.admitted += 1;
        Ok(Ticket { seen: seen.snapshots, at: now })
    }

    /// A request refused before it could start anything (`error.reach.*`, bad arguments) gives back its place in the hour,
    /// so an AI asking for what it may not do cannot use up the turns every AI shares.
    pub fn refund(&self, ticket: &Ticket) {
        let mut seen = self.lock();
        if let Some(place) = seen.starts.iter().position(|at| *at == ticket.at) {
            seen.starts.remove(place);
        }
    }

    /// An admitted turn was answered. When it started (`answer` is its reply), notes the conversation it went to, from its
    /// answer or its arguments, and counts it as running at once: the next `running` event may come after the next request.
    /// Only a conversation seen coming in and then leaving since the turn was admitted is known to have ended already.
    pub fn answered(&self, ticket: Ticket, args: &Value, answer: Option<&Value>) {
        let mut seen = self.lock();
        seen.admitted = seen.admitted.saturating_sub(1);
        let found = answer.and_then(|answer| {
            [
                answer.as_str(),
                answer["id"].as_str(),
                args["req"]["conversationId"].as_str(),
                args["conversationId"].as_str(),
                args["id"].as_str(),
            ]
            .into_iter()
            .flatten()
            .next()
        });
        if let Some(id) = found {
            seen.started.insert(id.to_string());
            let ended = matches!((seen.joined.get(id), seen.left.get(id)), (Some(&came), Some(&went)) if came > ticket.seen && went > came);
            if !ended {
                seen.running.insert(id.to_string());
            }
        }
        // Nothing waits any more, answered or refused: what came and left so far matters to no turn.
        if seen.admitted == 0 {
            seen.joined.clear();
            seen.left.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_ai_runs_two_turns_at_once_and_a_third_waits_until_one_ends() {
        let guard = Guard::default();
        let now = Instant::now();
        let ticket = guard.admit(now).unwrap();
        guard.answered(ticket, &json!({ "req": {} }), Some(&json!("c1")));
        let ticket = guard.admit(now).unwrap();
        guard.answered(ticket, &json!({ "id": "c2" }), Some(&json!(null)));
        assert_eq!(guard.admit(now).err(), Some("error.channel.agentTurns"));
        guard.running(&json!({ "items": [{ "conversationId": "c2" }, { "conversationId": "person" }] }));
        assert!(guard.admit(now).is_ok(), "c1 ended, and the person's own turn does not count");
    }

    #[test]
    fn turns_sent_together_count_before_their_answers_and_a_refused_one_frees_its_place() {
        let guard = Guard::default();
        let now = Instant::now();
        let ticket = guard.admit(now).unwrap();
        guard.admit(now).unwrap();
        assert_eq!(guard.admit(now).err(), Some("error.channel.agentTurns"), "two still waiting for their answer");
        guard.answered(ticket, &json!({}), None);
        assert!(guard.admit(now).is_ok(), "the refused one no longer counts");
    }

    #[test]
    fn a_refused_request_gives_back_its_place_in_the_hour() {
        let guard = Guard::default();
        let start = Instant::now();
        for _ in 0..MAX_PER_HOUR * 2 {
            let ticket = guard.admit(start).unwrap();
            guard.refund(&ticket);
            guard.answered(ticket, &json!({}), None);
        }
        assert!(guard.admit(start).is_ok(), "refused requests used up nothing");
    }

    #[test]
    fn at_most_twenty_turns_start_in_an_hour() {
        let guard = Guard::default();
        let start = Instant::now();
        for _ in 0..MAX_PER_HOUR {
            let ticket = guard.admit(start).unwrap();
            guard.answered(ticket, &json!({}), None);
        }
        assert!(guard.admit(start + Duration::from_secs(60)).is_err());
        assert!(guard.admit(start + HOUR).is_ok(), "the hour has passed");
    }

    #[test]
    fn a_turn_that_ended_before_its_answer_came_does_not_count_as_running() {
        let guard = Guard::default();
        let now = Instant::now();
        let ticket = guard.admit(now).unwrap();
        guard.running(&json!({ "items": [{ "conversationId": "c1" }] }));
        guard.running(&json!({ "items": [] }));
        guard.answered(ticket, &json!({}), Some(&json!("c1")));
        let ticket = guard.admit(now).unwrap();
        guard.answered(ticket, &json!({}), Some(&json!("c2")));
        assert!(guard.admit(now).is_ok(), "c1 ended before its answer, only c2 runs");
    }

    #[test]
    fn a_running_list_from_before_the_turn_began_does_not_free_its_place() {
        let guard = Guard::default();
        let now = Instant::now();
        let ticket = guard.admit(now).unwrap();
        guard.running(&json!({ "items": [{ "conversationId": "person" }] }));
        guard.answered(ticket, &json!({}), Some(&json!("c1")));
        let ticket = guard.admit(now).unwrap();
        guard.answered(ticket, &json!({}), Some(&json!("c2")));
        assert_eq!(guard.admit(now).err(), Some("error.channel.agentTurns"), "c1 and c2 still run");
    }
}
