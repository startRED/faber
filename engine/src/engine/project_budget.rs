//! T3 entry points share Core's existing lock, Auto table and budget.json transaction.
use super::*;
use crate::project::budget::{self, Budget, Change, Invocation, Status};
use crate::route::{Asked, Routed};

impl Engine {
    /// Persist admission before T5 dispatches; reports remain admissible while paused or out of quota.
    pub fn mission_budget(&self, change: Change) -> Result<Status, String> {
        self.with(|c| {
            c.store.ledger_checked().map_err(|e| e.key().to_string())?;
            if c.ledger_failed {
                return Err("error.store.write".into());
            }
            let mut next = c.ledger.clone();
            if c.opts.claude_env_billed {
                next.claude.billing = Billing::UsageBilled;
            }
            let status = budget::apply(&mut next, change, c.secs(), c.utc_offset).map_err(str::to_string)?;
            c.store.save_ledger(&next).map_err(|e| e.key().to_string())?;
            c.ledger = next;
            c.ledger_saved = c.secs();
            c.emit_usage();
            Ok(status)
        })
    }

    /// Feed a classified brief into Auto's existing paced table with mission admission as one extra filter.
    /// This spends no quota; T5 supplies the verdict and a verified exposure from its qualified driver.
    pub fn mission_route(&self, verdict: &Asked, request: &Invocation, only: Option<Vendor>) -> Result<Routed, String> {
        self.with(|c| {
            c.store.ledger_checked().map_err(|e| e.key().to_string())?;
            if c.ledger_failed {
                return Err("error.store.write".into());
            }
            let b = Budget::load(&c.ledger).map_err(str::to_string)?;
            c.table_for(verdict, only, true, |profile| {
                let mut r = request.clone();
                r.vendor = profile.vendor;
                r.model = profile.model.into();
                r.effort = profile.effort.into();
                b.can_admit(&c.ledger, &r, c.secs()).is_ok()
            })
            .ok_or_else(|| "reason.budget.admission".into())
        })
    }
}

impl Core {
    /// Unbounded foreground/auxiliary work is journaled before send and cannot borrow a mission reservation.
    pub(super) fn begin_unbounded(&mut self, vendor: Vendor) -> Result<(), String> {
        // Chat can relearn a damaged cache; its quarantine still blocks all Projects admission.
        if self.ledger_failed {
            return Err("error.store.write".into());
        }
        let mut next = self.ledger.clone();
        budget::start_unbounded(&mut next, vendor).map_err(|_| "error.quotaOut".to_string())?;
        self.store.save_ledger(&next).map_err(|e| e.key().to_string())?;
        self.ledger = next;
        self.ledger_saved = self.secs();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learn::Span;
    use crate::project::Extra;
    use crate::project::budget::{Grant, Kind};
    use std::sync::Barrier;

    const DAY: f64 = 86_400.0;
    const NOW: f64 = 2.0 * DAY + 3600.0;
    fn grant(cap: u64) -> Grant {
        Grant {
            cap_micros: cap,
            worker_micros: cap,
            vendors: BTreeMap::from([(Vendor::Codex, cap)]),
            review_micros: 20_000,
            repair_micros: 20_000,
            canary_micros: 20_000,
            paused: false,
            extra: Extra::new(),
        }
    }
    fn request() -> Invocation {
        Invocation {
            id: "i1".into(),
            mission: "m1".into(),
            worker: "w1".into(),
            kind: Kind::Worker,
            vendor: Vendor::Codex,
            model: "gpt-6.1-sol".into(),
            effort: "medium".into(),
            session: "s1".into(),
            fresh: true,
            exposure_micros: Some(100_000),
            extra: Extra::new(),
        }
    }
    fn engine(root: &Path) -> Engine {
        let e = Engine::new(
            EngineOptions {
                data_dir: root.into(),
                clis: BTreeMap::from([(Vendor::Codex, Cli { command: "not-a-cli".into(), prefix: vec![] })]),
                spawn: Arc::new(|_, _, _| Err(std::io::Error::other("no process in budget tests"))),
                windows: false,
                claude_env_billed: false,
                max_warm: 1,
                idle: Duration::from_secs(30),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        );
        e.set_time_source(Arc::new(|| NOW));
        e.with(|c| {
            c.codex_models = vec!["gpt-6.1-sol".into()];
            let v = &mut c.ledger.codex;
            v.billing = Billing::Subscription;
            v.paid_fallback = Some(false);
            for i in 0..=8 {
                let at = 2.0 * DAY - (8 - i) as f64 * 1800.0;
                v.record("seven_day", i as f64 * 0.03, 7.0 * DAY, at, 0);
                if i > 0 {
                    v.span(Span { start: at - 1800.0, end: at, model: "gpt-6.1-sol".into(), cost: Some(0.25), incomplete: false });
                }
            }
            v.record("seven_day", 0.25, 7.0 * DAY, NOW, 0);
            c.store.save_ledger(&c.ledger).unwrap();
        });
        e.mission_budget(Change::Allocate { vendor: Vendor::Codex, cap_micros: 400_000 }).unwrap();
        e
    }

    #[tokio::test]
    async fn concurrent_handles_share_one_durable_allocation() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        let barrier = Arc::new(Barrier::new(2));
        let attempts: Vec<_> = (0..2)
            .map(|n| {
                let (e, barrier) = (e.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    e.mission_budget(Change::Grant { mission: format!("m{n}"), grant: grant(250_000) })
                })
            })
            .collect();
        let successes = attempts.into_iter().map(|thread| thread.join().unwrap()).filter(Result::is_ok).count();
        assert_eq!(successes, 1);
        assert_eq!(Budget::load(&e.with(|c| c.store.ledger_checked().unwrap())).unwrap().grants.len(), 1);
        e.shutdown();
    }

    #[tokio::test]
    async fn mission_routing_uses_auto_and_persists_admission_before_effects() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(250_000) }).unwrap();
        let verdict = crate::route::asked(r#"{"kind":"code","difficulty":"medium","confidence":1}"#).unwrap();
        let r = request();
        assert_eq!(e.mission_route(&verdict, &r, None).unwrap().profile.model, "gpt-6.1-sol");
        assert!(e.mission_route(&verdict, &r, Some(Vendor::Claude)).is_err(), "a required other vendor is never silently replaced");
        e.mission_budget(Change::Admit(r)).unwrap();
        assert!(Budget::load(&e.with(|c| c.store.ledger_checked().unwrap())).unwrap().invocations.contains_key("i1"));
        assert!(e.with(|c| c.allows(Vendor::Codex, None, Purpose::Router, false)).is_err());
        assert!(e.with(|c| c.begin_unbounded(Vendor::Codex)).is_err());
        e.shutdown();
    }

    #[tokio::test]
    async fn unbounded_chat_and_auxiliary_usage_survive_restart_and_block_admission() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        e.with(|c| c.begin_unbounded(Vendor::Codex)).unwrap();
        assert!(e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(100_000) }).is_err());
        e.with(|c| c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", None, true));
        e.shutdown();
        let saved = Store::new(dir.path()).ledger_checked().unwrap();
        assert_eq!(Budget::load(&saved).unwrap().unbounded[&Vendor::Codex], 1);
    }

    #[tokio::test]
    async fn corrupt_or_failed_safety_writes_never_admit_an_effect() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        std::fs::create_dir(dir.path().join("budget.json.tmp")).unwrap();
        assert!(e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(100_000) }).is_err());
        assert!(Budget::load(&e.ledger()).unwrap().grants.is_empty());
        std::fs::remove_dir(dir.path().join("budget.json.tmp")).unwrap();
        std::fs::write(dir.path().join("budget.json"), "{").unwrap();
        assert!(e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(100_000) }).is_err());
        assert!(dir.path().join("budget.json.quarantine").is_file());
        e.shutdown();
    }
}
