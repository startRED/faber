//! T3 entry points share Core's existing lock, Auto table and budget.json transaction.
use super::*;
use crate::project::budget::{self, Budget, Change, Invocation, Status};
use crate::route::{Asked, Routed};

impl Engine {
    pub fn new_mission(&self, project: &Project, goal: &str, base: &str) -> Result<crate::project::Mission, String> {
        let repo = project.repo.as_ref().ok_or("reason.grant.otherRepo")?;
        let grant = project.grant.as_ref().ok_or("reason.grant.none")?;
        Ok(crate::project::Mission::new(
            &Uuid::new_v4().to_string(),
            &project.id,
            &repo.common_dir,
            goal,
            base,
            grant.revision,
            self.mission_time(),
        ))
    }
    pub(crate) fn mission_time(&self) -> u64 {
        self.with(|c| c.secs() as u64)
    }
    pub(crate) fn mission_sessions(&self, mission: &str) -> Vec<Session> {
        self.with(|c| c.mission_sessions.get(mission).cloned().unwrap_or_default())
    }
    pub(crate) fn pause_mission_budget(&self, mission: &str) -> Result<(), String> {
        let absent = self.with(|c| {
            let ledger = c.store.ledger_checked().map_err(|e| e.key().to_string())?;
            let budget = Budget::load(&ledger).map_err(str::to_string)?;
            Ok::<_, String>(!budget.grants.contains_key(mission) && !budget.invocations.values().any(|r| r.request.mission == mission))
        })?;
        if absent {
            return Ok(());
        }
        self.mission_budget(Change::Pause { mission: mission.into(), paused: true }).map(|_| ())
    }
    pub(crate) fn mission_reservation(&self, id: &str) -> Result<budget::Reservation, String> {
        self.with(|c| {
            let ledger = c.store.ledger_checked().map_err(|e| e.key().to_string())?;
            Budget::load(&ledger).map_err(str::to_string)?.invocations.get(id).cloned().ok_or("reason.budget.identity".into())
        })
    }
    pub(crate) fn mission_unsettled(&self, mission: &str) -> Result<bool, String> {
        self.with(|c| {
            let ledger = c.store.ledger_checked().map_err(|e| e.key().to_string())?;
            let budget = Budget::load(&ledger).map_err(str::to_string)?;
            Ok(budget.invocations.values().any(|r| r.request.mission == mission && (!r.settled || !r.terminated)))
        })
    }
    /// The board reads the same durable cost and daily line as admission; absent measurements stay unknown.
    pub fn mission_readings(&self, mission: &str) -> Result<(crate::project::wire::CostView, Vec<crate::project::wire::PaceView>), String> {
        self.with(|c| {
            c.store.ledger_checked().map_err(|e| e.key().to_string())?;
            let b = Budget::load(&c.ledger).map_err(str::to_string)?;
            let now = c.secs();
            let pace = Vendor::ALL
                .into_iter()
                .flat_map(|vendor| {
                    let v = c.ledger.of(vendor);
                    v.readings
                        .iter()
                        .filter(|(_, r)| r.resets_at > now)
                        .map(move |(name, r)| {
                            let room = v.day_baselines.get(name).and_then(|base| {
                                let start = crate::budget::daily_start(r.resets_at, now)?;
                                (base.reading.resets_at == r.resets_at
                                    && base.reading.at <= start
                                    && start - base.reading.at <= crate::budget::STALE_SECS)
                                    .then(|| crate::budget::daily_room(base.reading.used, r.used, r.resets_at, now))
                                    .flatten()
                            });
                            crate::project::wire::PaceView {
                                vendor,
                                window: name.clone(),
                                used: Some(r.used),
                                room,
                                read_at: Some(r.read_at),
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            Ok((b.cost_view(mission), pace))
        })
    }

    /// Persist admission before T5 dispatches; reports remain admissible while paused or out of quota.
    pub fn mission_budget(&self, change: Change) -> Result<Status, String> {
        self.with(|c| {
            c.store.ledger_checked().map_err(|e| e.key().to_string())?;
            if c.projects_closed || c.ledger_failed {
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

    /// Re-read current mission authority; a UI revision or a worker brief never serves as a grant.
    pub fn mission_authority(&self, mission: &crate::project::Mission) -> Result<crate::project::Grant, String> {
        self.with(|c| {
            let project = c
                .store
                .projects_checked()
                .map_err(|e| e.key().to_string())?
                .into_iter()
                .find(|p| p.id == mission.project_id)
                .ok_or("reason.grant.none")?;
            let repo = project.repo.as_ref().filter(|r| r.common_dir == mission.repo).ok_or("reason.grant.otherRepo")?;
            let grant = project.grant.ok_or("reason.grant.none")?;
            grant.allows(repo, mission.grant_revision, crate::project::Effect::Worktree).map_err(str::to_string)?;
            Ok(grant)
        })
    }
    /// Feed a classified brief into Auto's existing paced table with mission admission as one extra filter.
    /// This spends no quota; T5 supplies the verdict and a verified exposure from its qualified driver.
    pub fn mission_route(&self, verdict: &Asked, request: &Invocation, only: Option<Vendor>) -> Result<Routed, String> {
        self.with(|c| {
            c.store.ledger_checked().map_err(|e| e.key().to_string())?;
            if c.projects_closed || c.ledger_failed {
                return Err("error.store.write".into());
            }
            let b = Budget::load(&c.ledger).map_err(str::to_string)?;
            c.table_for(verdict, only, true, |profile| {
                // A driver maximum belongs to this exact profile, never another Auto candidate.
                profile.vendor == request.vendor
                    && profile.model == request.model
                    && profile.effort == request.effort
                    && b.can_admit(&c.ledger, request, c.secs()).is_ok()
            })
            .ok_or_else(|| "reason.budget.admission".into())
        })
    }
}

impl Core {
    /// Foreground work takes priority; its active count closes new Projects admission, never Chat.
    pub(super) fn begin_unbounded(&mut self, vendor: Vendor) -> Result<(), String> {
        let now = self.secs();
        if budget::start_unbounded(&mut self.ledger, vendor, now).is_err() {
            self.projects_closed = true;
        }
        // Mission storage can close Projects, but never the person's foreground work.
        self.save_ledger(true);
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
    fn new_engine(root: &Path) -> Engine {
        Engine::new(
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
        )
    }
    fn engine(root: &Path) -> Engine {
        let e = new_engine(root);
        e.set_time_source(Arc::new(|| NOW));
        e.with(|c| {
            c.codex_models = vec!["gpt-6.1-sol".into()];
            let v = &mut c.ledger.codex;
            v.billing = Billing::Subscription;
            v.paid_fallback = Some(false);
            v.paid_fallback_at = Some(NOW);
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

    fn runner_fixture(e: &Engine, repo: &Path) -> (crate::project::store::Journal, crate::project::run::Runner) {
        let mut p = e.add_project(&repo.to_string_lossy());
        let before = p.clone();
        p.repo = Some(crate::project::RepoIdentity::new("repo", &repo.to_string_lossy()));
        p.grant = Some(crate::project::Grant::new("repo", crate::project::GitMode::Seguro, 3, 1));
        e.update_project(&before, &p).unwrap();
        let j = crate::project::store::Journal::open(repo, &e.with(|c| c.opts.data_dir.clone()), "m1").unwrap();
        let r = crate::project::run::Runner::create(&j, crate::project::Mission::new("m1", &p.id, "repo", "Fix", &"1".repeat(40), 3, 1))
            .unwrap();
        e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(250_000) }).unwrap();
        (j, r)
    }

    #[tokio::test]
    async fn opus_m1_stop_resume_reopens_only_reconciled_budget() {
        let (dir, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let e = engine(dir.path());
        let (j, mut r) = runner_fixture(&e, repo.path());
        r.stop(&j, &e).await.unwrap();
        assert!(Budget::load(&e.ledger()).unwrap().grants["m1"].paused);
        r.resume_authorized(&j, &e).unwrap();
        assert!(!Budget::load(&e.ledger()).unwrap().grants["m1"].paused);
        e.shutdown();
    }

    #[tokio::test]
    async fn screen_stop_interrupts_only_owned_mission_sessions_even_on_checkpoint_failure() {
        let (dir, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let e = engine(dir.path());
        let (j, mut r) = runner_fixture(&e, repo.path());
        let commands = Arc::new(Mutex::new(vec![]));
        let recorded = commands.clone();
        let session = Session::from_fn(move |command| {
            recorded.lock().unwrap().push(match command {
                crate::events::SessionCommand::Interrupt => "interrupt",
                crate::events::SessionCommand::Close => "close",
                _ => "unexpected",
            })
        });
        e.with(|c| {
            c.mission_sessions.insert("m1".into(), vec![session]);
            c.mission_sessions.insert("other".into(), vec![Session::from_fn(|_| panic!("another mission was interrupted"))]);
        });
        std::fs::create_dir(j.private().join("run.json.tmp")).unwrap();
        assert!(r.stop(&j, &e).await.is_err());
        assert!(Budget::load(&e.ledger()).unwrap().grants["m1"].paused);
        assert_eq!(*commands.lock().unwrap(), vec!["interrupt", "close"]);
        e.with(|c| {
            c.mission_sessions.remove("other");
        });
        e.shutdown();
    }

    #[tokio::test]
    async fn a_worker_session_alive_in_a_project_keeps_it_from_being_removed_whatever_the_disk_says() {
        let (dir, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let e = engine(dir.path());
        // No mission record anywhere on disk: only the live session says the project is working.
        let project = e.add_project(&repo.path().to_string_lossy());
        e.with(|c| {
            c.mission_sessions.insert("m1".into(), vec![Session::from_fn(|_| {})]);
            c.mission_projects.insert("m1".into(), project.id.clone());
        });
        assert_eq!(e.remove_project(&project.id), Err("error.project.busy".to_string()));
        e.with(|c| {
            c.mission_sessions.remove("m1");
            c.mission_projects.remove("m1");
        });
        assert_eq!(e.remove_project(&project.id), Ok(()));
        e.shutdown();
    }

    fn canonical_area() -> tempfile::TempDir {
        tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
    }

    #[tokio::test]
    async fn local_update_pause_stops_owned_workers_without_releasing_leases_or_spend() {
        let (dir, repo) = (canonical_area(), canonical_area());
        let layout = crate::project::update::SlotLayout::new(
            &dir.path().join("dev"),
            &dir.path().join("installed-data"),
            &dir.path().join("installed-release"),
        )
        .unwrap();
        crate::project::update::UpdateManager::new(layout.clone()).unwrap();
        let e = engine(&layout.dev_root().join("data"));
        let (j, mut r) = runner_fixture(&e, repo.path());
        r.mission.state = crate::project::MissionState::Running;
        r.save(&j).unwrap();
        e.mission_budget(Change::Admit(request())).unwrap();
        let before = Budget::load(&e.ledger()).unwrap().invocations["i1"].clone();
        let mut runtime = crate::project::Runtime::new();
        runtime.leases.push(crate::project::Lease::new("repo", "m1", "w1", 1, vec!["src".into()], &"1".repeat(40), &"2".repeat(64), 1));
        j.save_runtime(&runtime).unwrap();
        let commands = Arc::new(Mutex::new(vec![]));
        let captured = commands.clone();
        e.with(|c| {
            c.mission_sessions.insert(
                "m1".into(),
                vec![Session::from_fn(move |command| {
                    captured.lock().unwrap().push(match command {
                        crate::events::SessionCommand::Interrupt => "interrupt",
                        crate::events::SessionCommand::Close => "close",
                        _ => "unexpected",
                    });
                })],
            );
        });
        e.pause_missions_for_update().await.unwrap();
        assert_eq!(crate::project::run::Runner::load(&j).unwrap().mission.state, crate::project::MissionState::Paused);
        assert_eq!(*commands.lock().unwrap(), ["interrupt", "close"]);
        assert!(e.mission_sessions("m1").is_empty());
        assert_eq!(j.runtime().unwrap(), runtime);
        let after = Budget::load(&e.ledger()).unwrap();
        assert!(after.grants["m1"].paused);
        assert_eq!(after.invocations["i1"].paid_micros, before.paid_micros);
        assert!(!after.invocations["i1"].settled && !after.invocations["i1"].terminated);
        runtime.leases[0].state = crate::project::LeaseState::Released;
        j.save_runtime(&runtime).unwrap();
        assert_eq!(layout.missions_quiescent(), Err(crate::project::update::UpdateError::ActiveOperations));
        e.shutdown();
    }

    #[tokio::test]
    async fn local_update_admission_is_coordinated_and_remains_closed_through_probation() {
        use crate::project::update::{CanaryVerdict, ExactTree, Slot, SlotLayout, SwapCheck, UpdateManager};
        let (dir, repo) = (canonical_area(), canonical_area());
        let layout =
            SlotLayout::new(&dir.path().join("dev"), &dir.path().join("installed-data"), &dir.path().join("installed-release")).unwrap();
        let manager = UpdateManager::new(layout.clone()).unwrap();
        let payload = b"baseline";
        let tree =
            ExactTree::new(&"1".repeat(40), &"2".repeat(40), &ExactTree::compute_build_hash(payload), crate::project::SCHEMA).unwrap();
        std::fs::write(layout.slot_dir(Slot::A).join(format!("faber{}", std::env::consts::EXE_SUFFIX)), payload).unwrap();
        manager.adopt_baseline(Slot::A, tree.clone(), payload, 100).unwrap();
        let e = engine(&layout.dev_root().join("data"));
        e.set_local_update(layout.clone());
        let admission = e.mission_update_admission().unwrap();
        assert!(layout.coordinate().is_err());
        drop(admission);
        let mut grant = crate::project::Grant::new("repo", crate::project::GitMode::Seguro, 1, 100);
        grant.self_update = true;
        let identity = crate::project::RepoIdentity::new("repo", "checkout");
        manager.prepare_next(tree.clone(), &grant, &identity, 1, &[], 101).unwrap();
        manager
            .record_canary_verdict(
                CanaryVerdict::Pass {
                    verified_at: 101,
                    receipt_id: "canary".into(),
                    tree,
                    proof_receipt: "proof".into(),
                    compatible_schemas: vec![1],
                },
                payload,
                101,
            )
            .unwrap();
        manager.enter_quiescence(false, false).unwrap();
        manager.begin_swap(&grant, &identity, 1, SwapCheck { payload, active_window: false, active_operations: false }, 102).unwrap();
        manager.begin_probation(103).unwrap();
        manager.acknowledge_health(104).unwrap();
        manager.complete_first_operation(104).unwrap();
        let (j, mut r) = runner_fixture(&e, repo.path());
        assert_eq!(r.resume_authorized(&j, &e), Err("error.update.activeOperations".into()));
        assert_eq!(e.dispatch_mission_worker(j.clone(), request()).await, Err("error.update.activeOperations".into()));
        assert_eq!(crate::project::run::Runner::start(&j, &e, r.mission.clone()).unwrap_err(), "error.update.activeOperations");
        manager.finish_probation(403).unwrap();
        assert!(e.mission_update_admission().is_ok());
        e.shutdown();
    }

    #[tokio::test]
    async fn local_update_completed_mission_closes_warm_workers_without_reopening_authority() {
        let (dir, repo) = (canonical_area(), canonical_area());
        let e = engine(dir.path());
        let (j, mut r) = runner_fixture(&e, repo.path());
        e.mission_budget(Change::Admit(request())).unwrap();
        r.mission.state = crate::project::MissionState::Done;
        r.save(&j).unwrap();
        let commands = Arc::new(Mutex::new(vec![]));
        let captured = commands.clone();
        e.with(|c| {
            c.mission_sessions.insert(
                "m1".into(),
                vec![Session::from_fn(move |command| {
                    captured.lock().unwrap().push(match command {
                        crate::events::SessionCommand::Interrupt => "interrupt",
                        crate::events::SessionCommand::Close => "close",
                        _ => "unexpected",
                    });
                })],
            );
        });
        e.pause_missions_for_update().await.unwrap();
        assert_eq!(*commands.lock().unwrap(), ["interrupt", "close"]);
        assert_eq!(crate::project::run::Runner::load(&j).unwrap().mission.state, crate::project::MissionState::Done);
        assert!(e.mission_sessions("m1").is_empty());
        let budget = Budget::load(&e.ledger()).unwrap();
        assert!(budget.grants["m1"].paused);
        assert!(!budget.invocations["i1"].settled && !budget.invocations["i1"].terminated);
        e.shutdown();
    }

    #[tokio::test]
    async fn opus_h2_settled_dispatch_closes_before_resume_and_cleanup() {
        let (dir, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let e = engine(dir.path());
        let (j, mut r) = runner_fixture(&e, repo.path());
        e.mission_budget(Change::Admit(request())).unwrap();
        let mut intent = crate::project::Intent::new("i1", "m1", crate::project::IntentKind::Dispatch, 3, 1);
        intent.task = Some("w1".into());
        intent.expected.insert("invocation".into(), "i1".into());
        j.record_intent(&intent).unwrap();
        e.mission_budget(Change::Report {
            id: "i1".into(),
            event: EngineEvent::Done {
                ok: true,
                reason: "completed".into(),
                cost_total_usd: None,
                input_tokens: 1000,
                output_tokens: 100,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                cache_write_1h_tokens: 0,
                incomplete: false,
            },
            terminated: true,
        })
        .unwrap();
        assert!(Budget::load(&e.ledger()).unwrap().invocations["i1"].settled);
        r.pause(&j).unwrap();
        r.resume_authorized(&j, &e).unwrap();
        assert!(j.pending().unwrap().is_empty());
        let public = std::fs::read_to_string(j.records().join("receipts/i1.json")).unwrap();
        assert!(!public.contains("\"s1\""), "dispatch receipts must not publish private session identities");
        r.cleanup_task(&j, "w1", crate::project::lease::Termination { process: None, tree_dead: true, cost_settled: true }).unwrap();
        e.shutdown();
    }

    #[tokio::test]
    async fn opus_h2_native_start_closes_dispatch_but_unknown_spend_stays_parked() {
        let (dir, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let e = engine(dir.path());
        let (j, mut r) = runner_fixture(&e, repo.path());
        let mut brief = crate::project::Brief::new(
            "w1",
            1,
            crate::project::Role::Research,
            "Read",
            &"1".repeat(40),
            crate::project::ContractPin { id: "c1".into(), digest: "a".repeat(64) },
        );
        brief.grant_micros = Some(100_000);
        brief.stop = "Done or blocked".into();
        brief.scope = vec!["fixture".into()];
        r.workers.insert(
            "w1".into(),
            crate::project::worker::Worker::new(
                brief,
                crate::project::worker::ModelPin {
                    vendor: Vendor::Codex,
                    model: "gpt-6.1-sol".into(),
                    effort: "medium".into(),
                    cli_version: "0.162.1".into(),
                },
                3,
            )
            .unwrap(),
        );
        r.save(&j).unwrap();
        e.mission_budget(Change::Admit(request())).unwrap();
        let mut intent = crate::project::Intent::new("i1", "m1", crate::project::IntentKind::Dispatch, 3, 1);
        intent.task = Some("w1".into());
        intent.expected.insert("invocation".into(), "i1".into());
        j.record_intent(&intent).unwrap();
        e.mission_event(&j, "i1", "w1", &mut String::new(), EngineEvent::Started { session_id: "s1".into(), model: "gpt-6.1-sol".into() })
            .unwrap();
        assert!(j.pending().unwrap().is_empty());
        assert_eq!(crate::project::run::Runner::load(&j).unwrap().workers["w1"].answered.len(), 1);
        e.mission_budget(Change::Report {
            id: "i1".into(),
            event: EngineEvent::Error { message: "error.cli.closed".into(), fatal: true },
            terminated: true,
        })
        .unwrap();
        r.pause(&j).unwrap();
        e.pause_mission_budget("m1").unwrap();
        assert_eq!(r.resume_authorized(&j, &e), Err("reason.project.reconcile".into()));
        assert!(Budget::load(&e.ledger()).unwrap().grants["m1"].paused);
        e.shutdown();
    }

    #[tokio::test]
    async fn screen_dispatch_uses_t2_before_any_session_or_spend() {
        let (dir, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let e = engine(dir.path());
        let (j, r) = runner_fixture(&e, repo.path());
        assert_eq!(e.dispatch_mission_worker(j.clone(), request()).await, Err("reason.project.restrictionsUnproven".into()));
        assert!(j.pending().unwrap().is_empty());
        assert!(Budget::load(&e.ledger()).unwrap().invocations.is_empty());
        assert!(e.mission_sessions(&r.mission.id).is_empty());
        e.shutdown();
    }

    #[tokio::test]
    async fn removing_a_live_chat_retires_its_running_counter_once() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        let closed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = closed.clone();
        let id = e.with(|c| {
            let id = c.blank(Area::Chat, None);
            c.begin_unbounded(Vendor::Codex).unwrap();
            c.live.insert(
                id.clone(),
                Live {
                    vendor: Vendor::Codex,
                    model: Some("gpt-6.1-sol".into()),
                    running: None,
                    effort: "medium".into(),
                    sid: 1,
                    session: Session::from_fn(move |cmd| {
                        if matches!(cmd, crate::events::SessionCommand::Close) {
                            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                    }),
                    turn: Some("turn".into()),
                    break_pending: false,
                    answer_started: false,
                    used: 0,
                    cli: Cli { command: "not-a-cli".into(), prefix: vec![] },
                    resumed: false,
                    by_hand: true,
                    approved: false,
                    started: NOW,
                    steering: vec![],
                },
            );
            c.remove(&id);
            id
        });
        assert_eq!(Budget::load(&e.ledger()).unwrap().unbounded[&Vendor::Codex], 0);
        assert_eq!(e.ledger().codex.unmetered[&2], 1);
        e.with(|c| c.remove(&id));
        assert_eq!(e.ledger().codex.unmetered[&2], 1);
        assert_eq!(closed.load(std::sync::atomic::Ordering::SeqCst), 1);
        e.shutdown();
    }

    #[tokio::test]
    async fn mission_storage_failures_never_block_chat_or_overwrite_the_owner() {
        for failure in 0..4 {
            let dir = tempfile::tempdir().unwrap();
            let first = engine(dir.path());
            if failure > 0 {
                first.with(|c| {
                    let b = c.ledger.extra.get_mut("missionBudget").unwrap();
                    match failure {
                        1 => b["schema"] = serde_json::json!(99),
                        2 => b["invocations"] = serde_json::json!("bad receipts"),
                        _ => b["unbounded"] = serde_json::json!("bad active counts"),
                    }
                });
                first.with(|c| c.save_ledger(true));
                first.shutdown();
            }
            let before = std::fs::read(dir.path().join("budget.json")).unwrap();
            let second = new_engine(dir.path());
            second.set_time_source(Arc::new(|| NOW));
            second.with(|c| {
                c.begin_unbounded(Vendor::Codex).unwrap();
                c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", Some(0.01), false);
                c.begin_unbounded(Vendor::Codex).unwrap();
                c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", None, true);
                assert_eq!(c.ledger.codex.unmetered[&2], 1);
            });
            assert!(second.mission_budget(Change::Allocate { vendor: Vendor::Codex, cap_micros: 1 }).is_err());
            second.shutdown();
            assert!(std::fs::read(dir.path().join("budget.json")).unwrap() == before, "Chat must preserve the authoritative budget");
            if failure > 0 {
                assert!(std::fs::read(dir.path().join("budget.json.quarantine")).unwrap() == before);
            }
            first.shutdown();
        }
    }

    #[tokio::test]
    async fn a_failed_chat_journal_write_keeps_chat_available_and_projects_closed() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        std::fs::create_dir(dir.path().join("budget.json.tmp")).unwrap();
        e.with(|c| {
            c.begin_unbounded(Vendor::Codex).unwrap();
            c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", Some(0.01), false);
            c.begin_unbounded(Vendor::Codex).unwrap();
        });
        assert!(e.mission_budget(Change::Allocate { vendor: Vendor::Codex, cap_micros: 1 }).is_err());
        std::fs::remove_dir(dir.path().join("budget.json.tmp")).unwrap();
        e.with(|c| c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", Some(0.01), false));
        assert_eq!(Budget::load(&e.ledger()).unwrap().unbounded[&Vendor::Codex], 0);
        e.mission_budget(Change::Allocate { vendor: Vendor::Codex, cap_micros: 1 }).unwrap();
        e.shutdown();
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
    async fn separate_engines_cannot_own_the_same_spending_directory() {
        let dir = tempfile::tempdir().unwrap();
        let first = engine(dir.path());
        let second = Engine::new(
            EngineOptions {
                data_dir: dir.path().into(),
                clis: BTreeMap::new(),
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
        second.set_time_source(Arc::new(|| NOW));
        second.with(|c| {
            c.ledger.codex.billing = Billing::Subscription;
            c.ledger.codex.paid_fallback = Some(false);
        });
        first.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(250_000) }).unwrap();
        assert!(second.mission_budget(Change::Grant { mission: "m2".into(), grant: grant(250_000) }).is_err());
        second.with(|c| c.save_ledger(true));
        assert!(Budget::load(&first.with(|c| c.store.ledger_checked().unwrap())).unwrap().grants.contains_key("m1"));
        second.shutdown();
        first.shutdown();
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
        assert!(e.with(|c| c.allows(Vendor::Codex, None, Purpose::Router, false)).is_ok());
        assert!(e.with(|c| c.begin_unbounded(Vendor::Codex)).is_ok());
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
        assert_eq!(Budget::load(&saved).unwrap().unbounded[&Vendor::Codex], 0);
        assert_eq!(saved.codex.unmetered[&2], 1);
    }

    #[tokio::test]
    async fn every_chat_path_remains_available_with_paused_or_unknown_missions() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(250_000) }).unwrap();
        e.mission_budget(Change::Admit(request())).unwrap();
        e.mission_budget(Change::Report {
            id: "i1".into(),
            event: EngineEvent::Error { message: "error.cli.closed".into(), fatal: true },
            terminated: true,
        })
        .unwrap();
        e.mission_budget(Change::Pause { mission: "m1".into(), paused: true }).unwrap();
        for purpose in
            [Purpose::UserTurn { by_hand: true }, Purpose::UserTurn { by_hand: false }, Purpose::Router, Purpose::Pulse, Purpose::Summary]
        {
            assert!(e.with(|c| c.allows(Vendor::Codex, None, purpose, false)).is_ok());
        }
        e.with(|c| c.begin_unbounded(Vendor::Codex)).unwrap();
        assert!(e.mission_budget(Change::Continue { id: "i1".into() }).is_err());
        e.with(|c| c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", Some(0.01), false));
        e.shutdown();
    }

    #[tokio::test]
    async fn unknown_chat_cost_blocks_only_that_day_not_a_permanent_running_counter() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        e.with(|c| c.begin_unbounded(Vendor::Codex)).unwrap();
        e.with(|c| c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", None, true));
        assert_eq!(Budget::load(&e.ledger()).unwrap().unbounded[&Vendor::Codex], 0);
        assert!(e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(100_000) }).is_err());
        e.set_time_source(Arc::new(|| 3.0 * DAY));
        e.with(|c| {
            c.ledger.codex.record("seven_day", 0.26, 7.0 * DAY, 3.0 * DAY, 0);
            c.ledger.codex.paid_fallback_at = Some(3.0 * DAY);
        });
        e.mission_budget(Change::Allocate { vendor: Vendor::Codex, cap_micros: 400_000 }).unwrap();
        e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(100_000) }).unwrap();
        e.shutdown();
    }

    #[tokio::test]
    async fn routing_never_transplants_an_exposure_to_another_profile() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        e.with(|c| c.codex_models.push("gpt-6-luna".into()));
        e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(250_000) }).unwrap();
        let verdict = crate::route::asked(r#"{"kind":"chat","difficulty":"easy","confidence":1}"#).unwrap();
        let mut r = request();
        // The supplied maximum certifies only this exact model and effort.
        r.model = "unqualified-model".into();
        assert!(e.mission_route(&verdict, &r, None).is_err());
        r = request();
        r.effort = "unqualified-effort".into();
        assert!(e.mission_route(&verdict, &r, None).is_err());
        let r = request();
        let selected = e.mission_route(&verdict, &r, None).unwrap().profile;
        assert_eq!((selected.vendor, selected.model, selected.effort), (r.vendor, r.model.as_str(), r.effort.as_str()));
        e.shutdown();
    }

    #[tokio::test]
    async fn chat_hot_path_does_not_parse_mission_receipts() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        e.with(|c| {
            c.ledger.extra.get_mut("missionBudget").unwrap()["invocations"] = serde_json::json!("unreadable receipts");
            assert!(c.allows(Vendor::Codex, None, Purpose::Router, false).is_ok());
            c.begin_unbounded(Vendor::Codex).unwrap();
            c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", Some(0.01), false);
        });
        assert!(e.mission_budget(Change::Admit(request())).is_err());
        e.shutdown();
    }

    #[tokio::test]
    async fn omitted_credit_fields_never_renew_paid_fallback_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(250_000) }).unwrap();
        e.with(|c| c.apply_signals(Vendor::Codex, Signals { paid_fallback: Some(false), ..Default::default() }));
        e.set_time_source(Arc::new(|| NOW + 7.0 * 3600.0));
        e.with(|c| {
            c.note_quota(Vendor::Codex, "seven_day", 0.25, 7.0 * DAY);
            c.apply_signals(Vendor::Codex, Signals { reach: Some(Reach::Clear), ..Default::default() });
        });
        assert!(e.mission_budget(Change::Admit(request())).is_err());
        e.with(|c| c.apply_signals(Vendor::Codex, Signals { paid_fallback: Some(false), ..Default::default() }));
        e.mission_budget(Change::Admit(request())).unwrap();
        e.shutdown();
    }

    #[tokio::test]
    async fn billing_mode_is_not_stored_as_a_subscription_fact() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        e.with(|c| c.opts.claude_env_billed = true);
        e.mission_budget(Change::Grant { mission: "m1".into(), grant: grant(100_000) }).unwrap();
        let saved = e.with(|c| c.store.ledger_checked().unwrap());
        assert_eq!(saved.claude.billing, Billing::Unknown);
        assert!(serde_json::to_value(saved).unwrap()["claude"].get("billing").is_none());
        e.shutdown();
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
