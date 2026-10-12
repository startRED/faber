//! The quota pulse: Claude Code reports its quota windows only when it answers, so when the reading is old, a one-word question
//! to the cheapest model asks for a fresh one. It runs in the background and nobody waits for it.

use super::auto::{Stop, stop_router};
use super::*;
use crate::budget::{Purpose, gate};

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
        if gate(&self.ledger.claude.snap(now), None, Purpose::Pulse, false, now).is_err() {
            return;
        }
        self.last_pulse = now;
        let (me, stop) = (self.me.clone(), Stop::default());
        let closer = stop.clone();
        let sink: EventSink = Arc::new(move |e| match e {
            EngineEvent::Quota { .. } | EngineEvent::Signals(_) => {
                if let Some(core) = me.upgrade() {
                    core.lock().unwrap_or_else(|e| e.into_inner()).note_account_event(Vendor::Claude, e);
                }
            }
            EngineEvent::Done { .. } | EngineEvent::Error { .. } => {
                stop_router(Some(closer.clone()));
                if let Some(core) = me.upgrade() {
                    core.lock().unwrap_or_else(|e| e.into_inner()).note_span(Vendor::Claude, now, PULSE_MODEL, None);
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
            permissions: Permissions::DontAsk,
            instructions: Some("Reply with the single word ok.".into()),
            user_name: None,
        };
        let session = start_claude(
            ClaudeOptions {
                bin: cli.command.clone(),
                cwd: self.opts.data_dir.clone(),
                launch,
                spawn: spawner(&self.opts.spawn, &cli),
                sandbox: None,
            },
            sink,
        );
        *stop.lock().unwrap_or_else(|e| e.into_inner()) = Some(session.clone());
        session.send(UserTurn::text("ok"));
    }
}
