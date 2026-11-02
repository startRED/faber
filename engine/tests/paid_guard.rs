//! Account-wide billing changes stop work before any further tool can run.
use crate::support;

use faber_engine::budget::Billing;
use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Permissions, Settings, Status, Vendor};
use faber_engine::router::Choice;
use faber_engine::signals::Signals;
use faber_engine::{Engine, EngineOptions, SendRequest};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn paid_notices_stop_an_unapproved_subscription_turn_and_clear_permissions() {
    for (index, notice) in [
        Signals { overage_active: Some(true), ..Signals::default() },
        Signals { billing: Some(Billing::UsageBilled), ..Signals::default() },
        Signals { billing: Some(Billing::Unknown), ..Signals::default() },
        Signals { credit_balance: Some(9.0), ..Signals::default() },
    ]
    .into_iter()
    .enumerate()
    {
        let dir = support::DataFolder::new();
        let fake = support::fake(support::claude_two_turns("guard"));
        let engine = Engine::new(
            EngineOptions {
                data_dir: dir.path().into(),
                #[cfg(windows)]
                chat_dir: dir.chat_path().into(),
                clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
                spawn: fake.spawn,
                windows: false,
                claude_env_billed: false,
                max_warm: 3,
                idle: Duration::from_secs(120),
                runtime: tokio::runtime::Handle::current(),
                ask: Some(Arc::new(|_| Box::pin(async { Some(r#"{"kind":"chat","difficulty":"easy","confidence":0.95}"#.into()) }))),
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        engine.note_account_events(
            Vendor::Claude,
            vec![EngineEvent::Signals(Signals { billing: Some(Billing::Subscription), credit_balance: Some(10.0), ..Signals::default() })],
        );
        engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
        let id = engine
            .send(SendRequest {
                conversation_id: None,
                area: Area::Chat,
                project_id: None,
                text: "first".into(),
                images: vec![],
                files: vec![],
                choice: Choice { vendor: "auto".into(), model: None, effort: None },
            })
            .unwrap();
        support::until("the turn never asked for permission", || engine.open(&id).unwrap().messages.last().unwrap().permissions.is_some())
            .await;
        engine.note_account_events(Vendor::Claude, vec![EngineEvent::Signals(notice)]);
        let reply = engine.open(&id).unwrap().messages.pop().unwrap();
        assert_eq!(reply.status, Some(if matches!(index, 0 | 3) { Status::Error } else { Status::Stopped }));
        assert!(reply.permissions.is_none());
        assert!(engine.state().running.items.is_empty());
        engine.shutdown();
    }
}
