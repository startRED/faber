//! The quota pulse: Claude Code reports its quota windows only when it answers, so when the reading is old, a one-word question
//! to the cheapest model asks for a fresh one. It runs in the background and nobody waits for it.

use super::auto::{Stop, stop_router};
use super::*;
use crate::budget::{Purpose, family_of};

/// A reading older than this is worth asking for again, and a pulse is not repeated sooner either.
const PULSE_EVERY: f64 = 20.0 * 60.0;
const PULSE_MODEL: &str = "claude-haiku-5-5";

impl Core {
    /// Starts a pulse when the Claude reading is old and the gate lets Claude answer. A clock that went back counts as old.
    pub(super) fn pulse_if_due(&mut self) {
        let Some(cli) = self.opts.clis.get(&Vendor::Claude).cloned() else { return };
        let now = self.secs();
        let newest = self.ledger.claude.readings.values().map(|r| r.read_at).fold(f64::MIN, f64::max);
        let due = |t: f64| !(0.0..PULSE_EVERY).contains(&(now - t));
        if !due(newest) || !due(self.last_pulse) {
            return;
        }
        if self.allows(Vendor::Claude, family_of(PULSE_MODEL), Purpose::Pulse, false).is_err() {
            return;
        }
        if self.begin_unbounded(Vendor::Claude).is_err() {
            return;
        }
        self.last_pulse = now;
        let (me, stop) = (self.me.clone(), Stop::default());
        let closer = stop.clone();
        let started = Arc::new(Mutex::new(Some(now)));
        let finished = started.clone();
        let timeout_core = me.clone();
        let sink: EventSink = Arc::new(move |e| match e {
            EngineEvent::Quota { .. } | EngineEvent::Signals(_) => {
                if let Some(core) = me.upgrade() {
                    let mut c = core.lock().unwrap_or_else(|e| e.into_inner());
                    c.note_account_event(Vendor::Claude, e);
                    if c.allows(Vendor::Claude, family_of(PULSE_MODEL), Purpose::Pulse, false).is_err() {
                        stop_router(Some(closer.clone()));
                    }
                }
            }
            e @ (EngineEvent::Done { .. } | EngineEvent::Error { .. }) => {
                stop_router(Some(closer.clone()));
                if let (Some(at), Some(core)) = (finished.lock().unwrap_or_else(|e| e.into_inner()).take(), me.upgrade()) {
                    let (cost, incomplete) = usage::fresh_price(&e, PULSE_MODEL);
                    core.lock().unwrap_or_else(|e| e.into_inner()).note_span(Vendor::Claude, at, PULSE_MODEL, cost, incomplete);
                }
            }
            _ => {}
        });
        let launch = ClaudeLaunch {
            area: Area::Chat,
            model: Some(PULSE_MODEL.into()),
            effort: Some("low".into()),
            resume: None,
            user_hooks: false,
            instructions: Some("Reply with the single word ok.".into()),
            user_name: None,
        };
        let session = start_claude(
            ClaudeOptions { bin: cli.command.clone(), cwd: self.opts.data_dir.clone(), launch, spawn: spawner(&self.opts.spawn, &cli) },
            sink,
        );
        *stop.lock().unwrap_or_else(|e| e.into_inner()) = Some(session.clone());
        session.send(UserTurn::text("ok"));
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            stop_router(Some(stop));
            if let (Some(at), Some(core)) = (started.lock().unwrap_or_else(|e| e.into_inner()).take(), timeout_core.upgrade()) {
                core.lock().unwrap_or_else(|e| e.into_inner()).note_span(Vendor::Claude, at, PULSE_MODEL, None, true);
            }
        });
    }
}
