use super::*;
use crate::budget::Billing;
use crate::signals::Signals;
use std::sync::atomic::{AtomicUsize, Ordering};

fn engine(root: &crate::workspace::TestRoot) -> Engine {
    let engine = Engine::new(
        EngineOptions {
            data_dir: root.path().into(),
            #[cfg(windows)]
            chat_dir: root.work().into(),
            clis: BTreeMap::from([(Vendor::Claude, Cli { command: "not-a-cli".into(), prefix: vec![] })]),
            spawn: Arc::new(|_, _, _| panic!("title tests must never start a process")),
            windows: false,
            claude_env_billed: false,
            max_warm: 0,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    engine.note_account_events(
        Vendor::Claude,
        vec![EngineEvent::Signals(Signals { billing: Some(Billing::Subscription), ..Default::default() })],
    );
    engine
}

fn exchange(e: &Engine, text: &str) -> String {
    e.with(|c| {
        let id = c.blank(Area::Chat, None);
        let conversation = c.convs.get_mut(&id).unwrap();
        conversation.title = fallback(text, "en");
        conversation.messages = vec![
            Message::new("user".into(), Role::User, text.into(), now()),
            Message::new("answer".into(), Role::Assistant, "Vamos resolver isso.".into(), now()),
        ];
        conversation.messages[1].status = Some(Status::Done);
        c.track_title(&id);
        id
    })
}

async fn settle() {
    for _ in 0..12 {
        tokio::task::yield_now().await;
    }
}

#[test]
fn title_six_words_quotes_controls_and_localized_link_hosts() {
    assert_eq!(clean("\"Um título\u{202e} curto com seis palavras extras\"\n"), "Um título curto com seis palavras");
    assert_eq!(clean("\n\u{200b}\"\""), "");
    assert_eq!(clean(&"a".repeat(200)).chars().count(), 120);
    for (language, expected) in [
        ("en", "Link: example.com"),
        ("pt-BR", "Link: example.com"),
        ("es", "Enlace: example.com"),
        ("fr", "Lien : example.com"),
        ("de", "Link: example.com"),
    ] {
        assert_eq!(fallback("https://user:secret@example.com/private?q=secret#fragment", language), expected);
    }
    assert_eq!(fallback("https://example.com", "unknown"), "Link: example.com");
    assert_eq!(fallback("Explain https://example.com", "es"), "Explain https://example.com");
}

#[tokio::test]
async fn title_once_after_first_answer_in_message_language_and_settings_for_code() {
    let root = crate::workspace::TestRoot::new();
    let e = engine(&root);
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    e.with(|c| {
        c.title_ask = Some(Arc::new(move |prompt| {
            counted.fetch_add(1, Ordering::SeqCst);
            assert!(prompt.contains("language of the first user message"));
            assert!(prompt.contains("only links or code"));
            assert!(prompt.contains("Settings language: pt-BR"));
            assert!(prompt.contains("Vamos resolver isso."));
            Box::pin(async { Some("\"Consertar o computador de casa agora mesmo\"".into()) })
        }))
    });
    e.save_settings(Settings { language: Some("pt-BR".into()), ..Default::default() }).unwrap();
    let id = exchange(&e, "Meu computador não liga");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "nothing runs before the first answer");
    e.with(|c| c.end_turn(&id, Some("answer".into()), None));
    settle().await;
    assert_eq!(e.open(&id).unwrap().title, "Consertar o computador de casa agora");
    e.with(|c| c.end_turn(&id, Some("answer".into()), None));
    settle().await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    e.shutdown();
}

#[tokio::test]
async fn title_first_exchange_ignores_newer_messages_and_preserves_excluded_conversations() {
    let root = crate::workspace::TestRoot::new();
    let e = engine(&root);
    e.with(|c| {
        c.title_ask = Some(Arc::new(|prompt| {
            assert!(!prompt.contains("Newer message"));
            Box::pin(async { Some("Primeira conversa".into()) })
        }))
    });
    let id = exchange(&e, "Primeira mensagem");
    e.with(|c| {
        c.convs.get_mut(&id).unwrap().messages.push(Message::new("next".into(), Role::User, "Newer message".into(), now()));
        c.end_turn(&id, Some("answer".into()), None);
    });
    settle().await;
    assert_eq!(e.open(&id).unwrap().title, "Primeira conversa");
    let picked_up = exchange(&e, "Imported name");
    e.with(|c| {
        c.titles.remove(&picked_up);
        c.convs.get_mut(&picked_up).unwrap().sessions.insert(Vendor::Claude, "existing-native-session".into());
        c.track_title(&picked_up);
        assert!(!c.titles.contains_key(&picked_up));
    });
    e.shutdown();
}

#[tokio::test]
async fn title_router_replacement_does_not_start_unrelated_model_work() {
    let root = crate::workspace::TestRoot::new();
    let e = engine(&root);
    e.with(|c| c.opts.ask = Some(Arc::new(|_| panic!("the recorded router is not a title provider"))));
    let id = exchange(&e, "Recorded conversation");
    e.with(|c| c.end_turn(&id, Some("answer".into()), None));
    assert!(e.with(|c| c.titles.is_empty()));
    assert_eq!(e.open(&id).unwrap().title, "Recorded conversation");
    e.shutdown();
}

#[tokio::test]
async fn title_manual_rename_even_to_original_name_and_changed_titles_win() {
    let root = crate::workspace::TestRoot::new();
    let e = engine(&root);
    for manual in [true, false] {
        let id = exchange(&e, "Original name");
        if manual {
            e.rename(&id, "My name");
            e.rename(&id, "Original name");
        } else {
            e.with(|c| c.convs.get_mut(&id).unwrap().title = "Changed elsewhere".into());
        }
        e.with(|c| c.finish_title(&id, Some("Generated name")));
        assert_eq!(e.open(&id).unwrap().title, if manual { "Original name" } else { "Changed elsewhere" });
    }
    e.shutdown();
}

#[tokio::test]
async fn title_quota_and_billing_denials_never_call_a_model() {
    let root = crate::workspace::TestRoot::new();
    let e = engine(&root);
    e.with(|c| c.title_ask = Some(Arc::new(|_| panic!("denied title request"))));
    e.note_account_events(
        Vendor::Claude,
        vec![EngineEvent::Quota { window: "five_hour".into(), used: 1.0, resets_at: unix_secs() + 3600.0 }],
    );
    let id = exchange(&e, "No quota");
    e.with(|c| c.title_after_answer(&id, true));
    assert_eq!(e.open(&id).unwrap().title, "No quota");
    e.note_account_events(
        Vendor::Claude,
        vec![EngineEvent::Signals(Signals { billing: Some(Billing::UsageBilled), ..Default::default() })],
    );
    e.note_account_events(
        Vendor::Claude,
        vec![EngineEvent::Quota { window: "five_hour".into(), used: 0.0, resets_at: unix_secs() + 3600.0 }],
    );
    let id = exchange(&e, "No paid titles");
    e.with(|c| c.title_after_answer(&id, true));
    assert_eq!(e.open(&id).unwrap().title, "No paid titles");
    e.shutdown();
}

#[tokio::test]
async fn title_deleted_and_shutdown_conversations_cancel_pending_work() {
    let root = crate::workspace::TestRoot::new();
    let e = engine(&root);
    let id = exchange(&e, "Delete this");
    let closed = Arc::new(AtomicUsize::new(0));
    let add_stop = |id: &str| {
        let counted = closed.clone();
        e.with(|c| {
            c.titles.get_mut(id).unwrap().stop = Some(Arc::new(Mutex::new(Some(Session::from_fn(move |command| {
                if matches!(command, crate::events::SessionCommand::Close) {
                    counted.fetch_add(1, Ordering::SeqCst);
                }
            })))))
        });
    };
    add_stop(&id);
    e.with(|c| c.remove(&id)).unwrap();
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert!(e.with(|c| !c.titles.contains_key(&id)));
    let id = exchange(&e, "Close this");
    add_stop(&id);
    e.shutdown();
    assert_eq!(closed.load(Ordering::SeqCst), 2);
    assert!(e.with(|c| c.titles.is_empty()));
    e.with(|c| c.finish_title(&id, Some("Late answer")));
    assert_eq!(e.open(&id).unwrap().title, "Close this");
}

#[tokio::test(start_paused = true)]
async fn title_timeout_keeps_fallback_and_late_answer_cannot_rename() {
    let root = crate::workspace::TestRoot::new();
    let e = engine(&root);
    e.with(|c| {
        c.title_ask = Some(Arc::new(|_| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(31)).await;
                Some("Too late".into())
            })
        }))
    });
    let id = exchange(&e, "https://example.com/private");
    e.with(|c| c.title_after_answer(&id, true));
    settle().await;
    tokio::time::advance(Duration::from_secs(30)).await;
    settle().await;
    assert_eq!(e.open(&id).unwrap().title, "Link: example.com");
    assert!(e.with(|c| !c.titles.contains_key(&id)));
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(e.open(&id).unwrap().title, "Link: example.com");
    e.shutdown();
}
