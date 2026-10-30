//! The ledger's count of requests whose cost is not known yet (an answer, a router question, a pulse).
use super::*;
use crate::project::budget;

impl Core {
    /// Counts a request in flight whose cost is not known yet. A failed count closes the ledger's writes, never the
    /// person's foreground work.
    pub(super) fn begin_unbounded(&mut self, vendor: Vendor) -> Result<(), String> {
        let now = self.secs();
        if budget::start_unbounded(&mut self.ledger, vendor, now).is_err() {
            self.projects_closed = true;
        }
        self.save_ledger(true);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learn::Span;
    use crate::project::budget::Budget;

    const DAY: f64 = 86_400.0;
    const NOW: f64 = 2.0 * DAY + 3600.0;
    fn new_engine(root: &crate::workspace::TestRoot) -> Engine {
        Engine::new(
            EngineOptions {
                data_dir: root.path().into(),
                #[cfg(windows)]
                chat_dir: root.work().into(),
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
        .unwrap()
    }

    #[tokio::test]
    async fn r131_only_the_no_model_account_check_stamps_zero_turn_evidence() {
        let root = crate::workspace::TestRoot::new();
        let e = new_engine(&root);
        e.set_time_source(Arc::new(|| NOW));
        let quota = EngineEvent::Quota { window: "seven_day".into(), used: 0.30, resets_at: 7.0 * DAY };
        e.note_account_events(
            Vendor::Codex,
            vec![
                quota.clone(),
                EngineEvent::Signals(Signals { billing: Some(Billing::Subscription), paid_fallback: Some(false), ..Default::default() }),
            ],
        );
        assert!(e.ledger().codex.zero_turn_readings.is_empty());
        e.with(|c| c.zero_turn_checked(Vendor::Codex, vec![quota]));
        assert_eq!(e.ledger().codex.zero_turn_readings["seven_day"].read_at, NOW);
        e.with(|c| {
            c.zero_turn_checked(
                Vendor::Codex,
                vec![EngineEvent::Quota { window: "seven_day".into(), used: f64::NAN, resets_at: 7.0 * DAY }],
            )
        });
        assert_eq!(e.ledger().codex.zero_turn_readings["seven_day"].used, 0.30);
        e.shutdown();
    }

    #[tokio::test]
    async fn a_slow_quota_query_does_not_delay_the_completed_subscription_check() {
        let root = crate::workspace::TestRoot::new();
        let e = new_engine(&root);
        let (writer, reader) = tokio::io::duplex(64);
        let reader = Arc::new(Mutex::new(Some(reader)));
        let (started, ready) = tokio::sync::oneshot::channel();
        let started = Arc::new(Mutex::new(Some(started)));
        e.with(|c| {
            c.opts.clis = BTreeMap::from([
                (Vendor::Claude, Cli { command: "account-fixture".into(), prefix: vec![] }),
                (Vendor::Codex, Cli { command: "limits-fixture".into(), prefix: vec![] }),
            ]);
            c.opts.spawn = Arc::new(move |bin, args, _| {
                let stdout: Box<dyn tokio::io::AsyncRead + Send + Unpin> = if args == ["auth", "status"] {
                    Box::new(std::io::Cursor::new(br#"{"loggedIn":true,"subscriptionType":"max","authMethod":"claude.ai"}"#.to_vec()))
                } else if bin == "limits-fixture" {
                    Box::new(std::io::Cursor::new(b"{\"id\":1,\"result\":{}}\n{\"id\":2,\"result\":{\"rateLimits\":null}}\n{\"id\":3,\"result\":{\"account\":{\"type\":\"chatgpt\",\"planType\":\"pro\"}}}\n".to_vec()))
                } else {
                    let _ = started.lock().unwrap().take().unwrap().send(());
                    Box::new(reader.lock().unwrap().take().unwrap())
                };
                Ok(crate::proc::Proc { stdin: Box::new(tokio::io::sink()), stdout, stderr: None, child: None, contained: None })
            });
        });
        let checking = e.clone();
        let task = tokio::spawn(async move {
            checking.check_plans(Duration::from_secs(1)).await;
        });
        ready.await.unwrap();
        assert_eq!(e.ledger().claude.billing, Billing::Subscription);
        assert_eq!(e.ledger().codex.billing, Billing::Subscription);
        drop(writer);
        task.await.unwrap();
        e.shutdown();
    }
    fn engine(root: &crate::workspace::TestRoot) -> Engine {
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
        e
    }

    #[tokio::test]
    async fn removing_a_live_chat_retires_its_running_counter_once() {
        let dir = crate::workspace::TestRoot::new();
        let e = engine(&dir);
        let closed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = closed.clone();
        let id = e.with(|c| {
            let id = c.blank(Area::Chat, None);
            c.begin_unbounded(Vendor::Codex).unwrap();
            c.live.insert(
                id.clone(),
                Live {
                    tool_kit: Default::default(),
                    start_card: None,
                    kit_identity: None,
                    kit_resource_digest: None,
                    guide: Default::default(),
                    kit_pending: false,
                    prepared: false,
                    kit_resources: None,
                    group_files: None,
                    group_context: None,
                    vendor: Vendor::Codex,
                    model: Some("gpt-6.1-sol".into()),
                    running: None,
                    capabilities: Default::default(),
                    ended: None,
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
                    settled: false,
                    open_tools: Vec::new(),
                    used: 0,
                    cli: Cli { command: "not-a-cli".into(), prefix: vec![] },
                    resumed: false,
                    by_hand: true,
                    approved: false,
                    started: NOW,
                    steering: vec![],
                    held: None,
                    cwd: ".".into(),
                    visual_root: None,
                    permissions: crate::model::Permissions::Ask,
                    tuning: Default::default(),
                    research: None,
                    retiring: false,
                    was_idle: false,
                    runs: crate::model::Permissions::Ask,
                    served: false,
                    unsaved_session: None,
                },
            );
            c.remove(&id).unwrap();
            id
        });
        assert_eq!(Budget::load(&e.ledger()).unwrap().unbounded[&Vendor::Codex], 0);
        assert_eq!(e.ledger().codex.unmetered[&2], 1);
        e.with(|c| c.remove(&id)).unwrap();
        assert_eq!(e.ledger().codex.unmetered[&2], 1);
        assert_eq!(closed.load(std::sync::atomic::Ordering::SeqCst), 1);
        e.shutdown();
    }

    #[tokio::test]
    async fn a_damaged_spending_record_never_blocks_chat_or_gets_overwritten() {
        for failure in 0..3 {
            let dir = crate::workspace::TestRoot::new();
            let first = engine(&dir);
            first.with(|c| {
                c.begin_unbounded(Vendor::Codex).unwrap();
                c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", Some(0.01), false);
            });
            if failure > 0 {
                first.with(|c| {
                    let b = c.ledger.extra.get_mut("missionBudget").unwrap();
                    match failure {
                        1 => b["schema"] = serde_json::json!(99),
                        _ => b["unbounded"] = serde_json::json!("bad active counts"),
                    }
                });
                first.with(|c| c.save_ledger(true));
                first.shutdown();
            }
            let before = std::fs::read(dir.path().join("budget.json")).unwrap();
            let second = new_engine(&dir);
            second.set_time_source(Arc::new(|| NOW));
            second.with(|c| {
                c.begin_unbounded(Vendor::Codex).unwrap();
                c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", Some(0.01), false);
                c.begin_unbounded(Vendor::Codex).unwrap();
                c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", None, true);
                assert_eq!(c.ledger.codex.unmetered[&2], 1);
                assert!(c.projects_closed, "the ledger is not this engine's to write");
            });
            second.shutdown();
            assert!(std::fs::read(dir.path().join("budget.json")).unwrap() == before, "Chat must preserve the authoritative budget");
            if failure > 0 {
                assert!(std::fs::read(dir.path().join("budget.json.quarantine")).unwrap() == before);
            }
            first.shutdown();
        }
    }

    #[tokio::test]
    async fn a_failed_ledger_write_keeps_chat_available_and_is_tried_again() {
        let dir = crate::workspace::TestRoot::new();
        let e = engine(&dir);
        std::fs::create_dir(dir.path().join("budget.json.tmp")).unwrap();
        e.with(|c| {
            c.begin_unbounded(Vendor::Codex).unwrap();
            c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", Some(0.01), false);
            c.begin_unbounded(Vendor::Codex).unwrap();
            assert!(c.ledger_failed);
        });
        std::fs::remove_dir(dir.path().join("budget.json.tmp")).unwrap();
        e.with(|c| c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", Some(0.01), false));
        assert_eq!(Budget::load(&e.ledger()).unwrap().unbounded[&Vendor::Codex], 0);
        assert!(!e.with(|c| c.ledger_failed));
        e.shutdown();
    }

    #[tokio::test]
    async fn unbounded_chat_and_auxiliary_usage_survive_restart() {
        let dir = crate::workspace::TestRoot::new();
        let e = engine(&dir);
        e.with(|c| c.begin_unbounded(Vendor::Codex)).unwrap();
        e.with(|c| c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", None, true));
        e.shutdown();
        let saved = Store::new(dir.path()).ledger_checked().unwrap();
        assert_eq!(Budget::load(&saved).unwrap().unbounded[&Vendor::Codex], 0);
        assert_eq!(saved.codex.unmetered[&2], 1);
    }

    #[tokio::test]
    async fn every_chat_path_remains_available_while_a_request_of_unknown_cost_runs() {
        let dir = crate::workspace::TestRoot::new();
        let e = engine(&dir);
        e.with(|c| c.begin_unbounded(Vendor::Codex)).unwrap();
        for purpose in [Purpose::UserTurn { by_hand: true }, Purpose::UserTurn { by_hand: false }, Purpose::Router, Purpose::Summary] {
            assert!(e.with(|c| c.allows(Vendor::Codex, None, purpose, false)).is_ok());
        }
        e.with(|c| c.note_span(Vendor::Codex, NOW, "gpt-6.1-sol", Some(0.01), false));
        e.shutdown();
    }
}
