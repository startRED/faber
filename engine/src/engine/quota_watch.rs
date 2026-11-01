//! Keeping the quota readings fresh without asking a model anything. Claude Code answers `/usage` itself; Codex's server
//! answers `account/rateLimits/read`. (A Claude Code whose `/usage` prints no percentages is read by one tiny request instead,
//! `account::claude_probe`.) Both are short-lived hidden processes that Faber starts on the schedule in
//! `cadence.rs`: at start, on a timer, after a request ends, on focus and on wake. Readings that arrive during a request
//! (Claude's rate-limit events, Codex's rate-limit updates) go through the same `note_quota`.

use super::*;
use crate::account::{UsageRead, claude_probe, claude_usage};
use crate::ledger::VendorLedger;

const CLAUDE_TIMEOUT: Duration = Duration::from_secs(45);
const CODEX_TIMEOUT: Duration = Duration::from_secs(20);

/// Marks a vendor’s reading as under way for exactly as long as the task that does it lives: whichever way that task ends
/// (an answer, a failure, the engine going away, a panic), the mark is lifted, so a vendor is never left unread for good.
struct InFlight {
    core: std::sync::Weak<Mutex<Core>>,
    vendor: Vendor,
}

impl Drop for InFlight {
    fn drop(&mut self) {
        if let Some(core) = self.core.upgrade() {
            core.lock().unwrap_or_else(|e| e.into_inner()).reading.remove(&self.vendor);
        }
    }
}

/// How old the CLI's word that extra usage is off may be for a probe to rely on it. Every turn and every probe renews it.
const PROBE_FRESH: f64 = 3600.0;

/// Whether a Claude request made only to read the windows can never be charged: not billed by use, not drawing on extra
/// usage, and extra usage said to be off within the last hour and no earlier than the last probe that gave no clean
/// reading (`unfinished_at`), so a request past the limit is refused, not paid. Nothing known is no probe: it then waits for
/// a real turn to say so. Left open: extra usage switched on in that hour while a window is out, at most one probe of under
/// US$0.005, after which its own event stops the next.
fn probe_is_free(v: &VendorLedger, now: f64, unfinished_at: f64) -> bool {
    v.billing != Billing::UsageBilled
        && !v.overage_active
        && v.paid_fallback == Some(false)
        && v.paid_fallback_at.is_some_and(|at| at >= unfinished_at && (0.0..PROBE_FRESH).contains(&(now - at)))
}

impl Core {
    /// Starts one reading for each vendor named that is not being read already.
    pub(super) fn read_quotas(&mut self, vendors: Vec<Vendor>) {
        if self.closing {
            return;
        }
        for vendor in vendors {
            let Some(cli) = self.opts.clis.get(&vendor).cloned() else { continue };
            // Claude's reset times are printed in the PC's own time: nothing is read until the screen has said what that is.
            if (vendor == Vendor::Claude && (!self.offset_known || self.usage_command_spent)) || !self.reading.insert(vendor) {
                continue;
            }
            let (me, spawn, cwd, windows) = (self.me.clone(), self.opts.spawn.clone(), self.opts.data_dir.clone(), self.opts.windows);
            let (now, offset) = (self.secs(), self.utc_offset);
            self.opts.runtime.spawn(async move {
                let _in_flight = InFlight { core: me.clone(), vendor };
                match vendor {
                    Vendor::Claude => {
                        let mut read = claude_usage(&cli, &spawn, &cwd, CLAUDE_TIMEOUT, now, offset).await;
                        // A Claude Code whose `/usage` prints no percentages is read by its smallest real request instead. That
                        // request is a model turn, so its windows are never zero-turn evidence (R131).
                        let probed = read == UsageRead::NoWindows
                            && me.upgrade().is_some_and(|core| {
                                let c = core.lock().unwrap_or_else(|e| e.into_inner());
                                probe_is_free(c.ledger.of(vendor), c.secs(), c.probe_unfinished_at)
                            });
                        let mut said = None;
                        if probed {
                            let (found, signals) = claude_probe(&cli, &spawn, &cwd, CLAUDE_TIMEOUT).await;
                            read = if found.is_empty() { UsageRead::Failed } else { UsageRead::Windows(found) };
                            said = Some(signals).filter(|s| !s.is_empty());
                        }
                        let Some(core) = me.upgrade() else { return };
                        let mut c = core.lock().unwrap_or_else(|e| e.into_inner());
                        if probed && read == UsageRead::Failed {
                            c.probe_unfinished_at = c.secs();
                        }
                        if let Some(signals) = said {
                            c.note_account_event(vendor, EngineEvent::Signals(signals));
                        }
                        match read {
                            UsageRead::Windows(found) => {
                                let now = c.secs();
                                let events: Vec<_> = found
                                    .into_iter()
                                    .map(|(window, used, resets_at)| EngineEvent::Quota {
                                        window: window.into(),
                                        used,
                                        resets_at: c.ledger.of(vendor).canonical_reset(window, resets_at, now),
                                    })
                                    .collect();
                                if probed {
                                    events.into_iter().for_each(|event| c.note_account_event(vendor, event));
                                } else {
                                    c.zero_turn_checked(vendor, events);
                                }
                                c.reading_done(vendor);
                            }
                            UsageRead::Spent => c.usage_command_spent = true,
                            UsageRead::NoWindows | UsageRead::Failed => {}
                        }
                    }
                    Vendor::Codex => {
                        let events = read_codex_limits(&cli.command, &cwd, windows, spawner(&spawn, &cli), CODEX_TIMEOUT).await;
                        let Some(core) = me.upgrade() else { return };
                        let mut c = core.lock().unwrap_or_else(|e| e.into_inner());
                        // A reading every few minutes must not make three quick failures ask the person which plan they have: a
                        // failed read counts against the plan at most once per plan check interval.
                        let now = c.secs();
                        if !events.is_empty() || !(0.0..PLAN_EVERY).contains(&(now - c.codex_failed_at)) {
                            c.codex_failed_at = now;
                            c.codex_plan_checked(events);
                        }
                        c.emit_usage();
                    }
                    // Not yet: no CLI is discovered for it, so the loop above skipped it.
                    Vendor::Gemini => {}
                }
            });
        }
    }

    /// Something happened that makes a fresh reading worth having (a request ended). The shared debounce decides.
    pub(super) fn quota_event(&mut self) {
        if !self.watching {
            return;
        }
        let (now, vendors) = (self.secs(), self.vendors());
        let due = self.cadence.event(now, &vendors);
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

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_791_620_000.0;

    fn said_off(at: f64) -> VendorLedger {
        VendorLedger { billing: Billing::Subscription, paid_fallback: Some(false), paid_fallback_at: Some(at), ..Default::default() }
    }

    #[test]
    fn a_probe_runs_only_where_it_cannot_be_charged() {
        assert!(!probe_is_free(&VendorLedger::default(), NOW, f64::MIN), "nothing known");
        assert!(probe_is_free(&said_off(NOW - 600.0), NOW, f64::MIN), "extra usage said to be off lately");
        assert!(!probe_is_free(&said_off(NOW - 2.0 * PROBE_FRESH), NOW, f64::MIN), "said too long ago, before a restart say");
        assert!(!probe_is_free(&said_off(NOW + 600.0), NOW, f64::MIN), "said in the future: a clock set back");
        let mut on = said_off(NOW - 600.0);
        on.paid_fallback = Some(true);
        assert!(!probe_is_free(&on, NOW, f64::MIN), "extra usage may answer");
        on.paid_fallback = None;
        assert!(!probe_is_free(&on, NOW, f64::MIN), "extra usage not ruled out");
        let mut extra = said_off(NOW - 600.0);
        extra.overage_active = true;
        assert!(!probe_is_free(&extra, NOW, f64::MIN), "extra usage is running");
        let mut billed = said_off(NOW - 600.0);
        billed.billing = Billing::UsageBilled;
        assert!(!probe_is_free(&billed, NOW, f64::MIN), "every request is paid");
        assert!(!probe_is_free(&said_off(NOW - 600.0), NOW, NOW - 300.0), "a probe gave no clean reading since: what it saw is lost");
        assert!(probe_is_free(&said_off(NOW - 600.0), NOW, NOW - 600.0), "word from that same probe counts");
    }
}
