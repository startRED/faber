//! Keeps an AI inside Faber from starting turns without end through the command line. Faber marks every CLI it starts
//! (`FABER_AGENT_DEPTH=1`), so a client that says it is an AI is one Faber runs. Such a client may drive Faber like a person,
//! with two limits: at most `MAX_RUNNING` of the turns it started run at once, and at most `MAX_PER_HOUR` start in an hour.
//! A chain of AIs that each wait for the next stops at the first limit; one that hands on and ends stops at the second.
//! A conversation cannot carry its own depth, because one Codex process serves every conversation of an area.

use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const MAX_RUNNING: usize = 2;
pub const MAX_PER_HOUR: usize = 20;
const HOUR: Duration = Duration::from_secs(3600);

#[derive(Default)]
struct Seen {
    /// The conversations an AI client started a turn in.
    started: HashSet<String>,
    /// When each of those turns started, within the last hour.
    starts: VecDeque<Instant>,
    /// The conversations running now, from the latest `running` event.
    running: HashSet<String>,
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
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
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
        let running =
            event["items"].as_array().into_iter().flatten().filter_map(|i| i["conversationId"].as_str().map(String::from)).collect();
        self.lock().running = running;
    }

    /// Whether an AI client may start one more turn now; the refusal's key when not.
    pub fn admit(&self, now: Instant) -> Result<(), &'static str> {
        let mut seen = self.lock();
        while seen.starts.front().is_some_and(|at| now.duration_since(*at) >= HOUR) {
            seen.starts.pop_front();
        }
        let running = seen.started.intersection(&seen.running).count();
        if running >= MAX_RUNNING || seen.starts.len() >= MAX_PER_HOUR {
            return Err("error.channel.agentTurns");
        }
        seen.starts.push_back(now);
        Ok(())
    }

    /// Notes the conversation an admitted turn went to, from its arguments or its answer, and counts it as running at once:
    /// the next `running` event may come after the next request.
    pub fn started(&self, args: &Value, answer: &Value) {
        let found = [
            answer.as_str(),
            answer["id"].as_str(),
            args["req"]["conversationId"].as_str(),
            args["conversationId"].as_str(),
            args["id"].as_str(),
        ]
        .into_iter()
        .flatten()
        .next();
        if let Some(id) = found {
            let mut seen = self.lock();
            seen.started.insert(id.to_string());
            seen.running.insert(id.to_string());
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
        guard.admit(now).unwrap();
        guard.started(&json!({ "req": {} }), &json!("c1"));
        guard.admit(now).unwrap();
        guard.started(&json!({ "id": "c2" }), &json!(null));
        assert_eq!(guard.admit(now), Err("error.channel.agentTurns"));
        guard.running(&json!({ "items": [{ "conversationId": "c2" }, { "conversationId": "person" }] }));
        assert!(guard.admit(now).is_ok(), "c1 ended, and the person's own turn does not count");
    }

    #[test]
    fn at_most_twenty_turns_start_in_an_hour() {
        let guard = Guard::default();
        let start = Instant::now();
        for _ in 0..MAX_PER_HOUR {
            guard.admit(start).unwrap();
        }
        assert!(guard.admit(start + Duration::from_secs(60)).is_err());
        assert!(guard.admit(start + HOUR).is_ok(), "the hour has passed");
    }
}
