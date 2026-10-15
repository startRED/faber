//! Keeping the quota readings fresh without asking a model anything. Claude Code answers `/usage` itself; Codex's server
//! answers `account/rateLimits/read`. Both are short-lived hidden processes that Faber starts on the schedule in
//! `cadence.rs`: at start, on a timer, after a request ends, on focus and on wake. Readings that arrive during a request
//! (Claude's rate-limit events, Codex's rate-limit updates) go through the same `note_quota`.

use super::*;
use crate::account::{UsageRead, claude_usage};

const CLAUDE_TIMEOUT: Duration = Duration::from_secs(45);
const CODEX_TIMEOUT: Duration = Duration::from_secs(20);

impl Core {
    /// Starts one reading for each vendor named that is not being read already.
    pub(super) fn read_quotas(&mut self, vendors: Vec<Vendor>) {
        for vendor in vendors {
            let Some(cli) = self.opts.clis.get(&vendor).cloned() else { continue };
            // Claude's reset times are printed in the PC's own time: nothing is read until the screen has said what that is.
            if (vendor == Vendor::Claude && (!self.offset_known || self.usage_command_spent)) || !self.reading.insert(vendor) {
                continue;
            }
            let (me, spawn, cwd, windows) = (self.me.clone(), self.opts.spawn.clone(), self.opts.data_dir.clone(), self.opts.windows);
            let (now, offset) = (self.secs(), self.utc_offset);
            self.opts.runtime.spawn(async move {
                match vendor {
                    Vendor::Claude => {
                        let read = claude_usage(&cli, &spawn, &cwd, CLAUDE_TIMEOUT, now, offset).await;
                        let Some(core) = me.upgrade() else { return };
                        let mut c = core.lock().unwrap_or_else(|e| e.into_inner());
                        c.reading.remove(&vendor);
                        match read {
                            UsageRead::Windows(found) => {
                                found.into_iter().for_each(|(window, used, resets_at)| c.note_quota(vendor, window, used, resets_at))
                            }
                            UsageRead::Spent => c.usage_command_spent = true,
                            UsageRead::Failed => {}
                        }
                    }
                    Vendor::Codex => {
                        let events = read_codex_limits(&cli.command, &cwd, windows, spawner(&spawn, &cli), CODEX_TIMEOUT).await;
                        let Some(core) = me.upgrade() else { return };
                        let mut c = core.lock().unwrap_or_else(|e| e.into_inner());
                        c.reading.remove(&vendor);
                        // A reading every few minutes must not make three quick failures ask the person which plan they have: a
                        // failed read counts against the plan at most once per plan check interval.
                        let now = c.secs();
                        if !events.is_empty() || !(0.0..PLAN_EVERY).contains(&(now - c.codex_failed_at)) {
                            c.codex_failed_at = now;
                            c.codex_plan_checked(events);
                        }
                        c.emit_usage();
                    }
                }
            });
        }
    }

    /// Something happened that makes a fresh reading worth having (a request ended, the window was focused).
    pub(super) fn quota_event(&mut self, reason: Reason) {
        if !self.watching {
            return;
        }
        let (now, vendors) = (self.secs(), self.vendors());
        let due = self.cadence.event(now, &vendors, reason);
        self.read_quotas(due);
    }

    /// The watcher's look at the clock.
    pub(super) fn quota_tick(&mut self) {
        let (now, vendors) = (self.secs(), self.vendors());
        let due = self.cadence.tick(now, &vendors).into_iter().map(|(v, _)| v).collect();
        self.read_quotas(due);
    }
}

impl Engine {
    /// The window was shown, hidden or focused (the screen says). Gaining focus reads at once; while hidden, readings slow down.
    pub fn set_attention(&self, visible: bool, focused: bool) {
        self.with(|c| {
            if !c.watching {
                return;
            }
            let (now, vendors) = (c.secs(), c.vendors());
            let due = c.cadence.attention(now, &vendors, visible, focused);
            c.read_quotas(due);
        })
    }
}
