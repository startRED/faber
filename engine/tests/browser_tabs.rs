//! A conversation keeps its tabs in Faber's browser (R431): the page the person or the agent left comes back after the
//! pane closed, another conversation and a restart, and never leaves Faber in an export.
use crate::support;
use faber_engine::cli::Cli;
use faber_engine::export::Words;
use faber_engine::model::{Conversation, Vendor};
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn engine(dir: &crate::support::DataFolder) -> Engine {
    Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: BTreeMap::<Vendor, Cli>::new(),
            spawn: support::fake(vec![]).spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 0,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap()
}

fn saved(dir: &Path) -> Conversation {
    let mut conversation: Conversation = serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
    conversation.piece_values.clear();
    conversation.browser = None;
    Store::new(dir).save_checked(&conversation).unwrap();
    conversation
}

#[tokio::test]
async fn the_last_page_of_a_conversation_comes_back_after_a_restart_and_never_in_its_export() {
    let dir = support::DataFolder::new();
    let id = saved(dir.path()).id;
    let first = engine(&dir);
    assert_eq!(first.browser_tabs(&id), None, "a conversation that never had a page has no tabs");
    first.browser_seen(&id, None, "https://docs.example.org/start?q=secret-word", None);
    first.browser_seen(&id, None, "https://docs.example.org/start?q=secret-word", Some("Getting started"));
    // Anything the pane refuses never becomes a tab.
    first.browser_seen(&id, None, "file:///C:/Users/me/notes.txt", Some("Notes"));
    first.browser_seen("gone", None, "https://example.org/", None);
    drop(first);

    let again = engine(&dir);
    let tabs = again.browser_tabs(&id).expect("kept in the conversation's file");
    let front = tabs.front_tab().expect("a tab in front");
    assert_eq!((front.url.as_str(), front.title.as_str()), ("https://docs.example.org/start?q=secret-word", "Getting started"));
    assert_eq!(tabs.tabs.len(), 1);
    assert_eq!(again.browser_tabs("gone"), None);

    let words = Words { you: "You".into(), auto: "Auto".into(), attachments: "Attachments".into(), files: "Files changed".into() };
    let (_, exported) = again.export_conversation(&id, &words).unwrap();
    assert!(!exported.contains("docs.example.org") && !exported.contains("Getting started"), "{exported}");

    again.remove(&id);
    assert_eq!(again.browser_tabs(&id), None, "the tabs leave with the conversation");
}

/// Records which tab each of the agent's calls acted in, and answers every one with the page it asked for.
#[derive(Default)]
struct Tabbed(std::sync::Mutex<Vec<(String, String)>>);

impl faber_engine::browser::Browser for Tabbed {
    fn act(
        &self,
        conversation: String,
        tab: String,
        request: faber_engine::browser::Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<faber_engine::browser::Page, String>> + Send>> {
        self.0.lock().unwrap().push((conversation, tab));
        let url = match request {
            faber_engine::browser::Request::Open(url) => url,
            _ => "https://example.org/agent".into(),
        };
        Box::pin(async move { Ok(faber_engine::browser::Page { url, text: "A page.".into(), ..Default::default() }) })
    }
}

async fn wire(pipe: &support::Pipe, ready: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
    tokio::time::timeout(crate::support::DEADLINE, async {
        loop {
            if let Some(m) = pipe.lines.lock().unwrap().iter().find(|m| ready(m)).cloned() {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing line on the fake wire: {:?}", pipe.lines.lock().unwrap()))
}

async fn browse(pipe: &mut support::Pipe, id: u64, arguments: serde_json::Value) -> (String, bool) {
    pipe.say(serde_json::json!({"id":id,"method":"item/tool/call","params":{"threadId":"main","turnId":"turn-main",
        "callId":format!("call-{id}"),"tool":"browse","arguments":arguments}}))
        .await;
    let result = wire(pipe, |m| m["id"] == id && m.get("result").is_some()).await["result"].clone();
    (result["contentItems"][0]["text"].as_str().unwrap_or_default().to_string(), result["success"] == true)
}

#[tokio::test]
async fn the_agent_opens_and_acts_in_its_own_tab_never_on_the_page_the_person_is_on() {
    use faber_engine::model::{Area, Permissions, Settings};
    let (dir, folder) = (support::DataFolder::new(), support::work_folder());
    let (spawn, made) = support::pipes();
    let app = Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    let browser = Arc::new(Tabbed::default());
    app.set_browser(browser.clone());
    app.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Settings::default() }).unwrap();
    let project = app.add_project(&folder.path().to_string_lossy()).unwrap().id;
    let id = app
        .send(faber_engine::SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project),
            text: "Check the docs".into(),
            images: vec![],
            files: vec![],
            choice: faber_engine::router::Choice { vendor: "codex".into(), model: None, effort: None },
        })
        .unwrap();
    let mut pipe = support::next_pipe(&made).await;
    let init = wire(&pipe, |m| m["method"] == "initialize").await;
    pipe.say(serde_json::json!({"id":init["id"],"result":{}})).await;
    let start = wire(&pipe, |m| m["method"] == "thread/start").await;
    pipe.say(serde_json::json!({"id":start["id"],"result":{"thread":{"id":"main"}}})).await;
    let turn = wire(&pipe, |m| m["method"] == "turn/start").await;
    pipe.say(serde_json::json!({"id":turn["id"],"result":{"turn":{"id":"turn-main"}}})).await;

    // Nothing of the agent's is open: an action has no page to act on, and never reaches the browser.
    let (said, ok) = browse(&mut pipe, 900, serde_json::json!({ "action": "read" })).await;
    assert!(!ok && said.contains("no page of yours is open"), "{said}");
    assert!(browser.0.lock().unwrap().is_empty());

    // The person is on a page of their own in front.
    let (_, mine) = app.browser_tab(&id, faber_engine::TabStep::New).unwrap();
    let mine = mine.unwrap();
    app.browser_seen(&id, Some(&mine), "https://example.org/mine", Some("Mine"));

    let (said, ok) = browse(&mut pipe, 901, serde_json::json!({ "url": "https://docs.example.org/" })).await;
    assert!(ok, "{said}");
    let (said, ok) = browse(&mut pipe, 902, serde_json::json!({ "action": "read" })).await;
    assert!(ok, "{said}");
    let calls = browser.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|(c, tab)| c == &id && tab == &calls[0].1), "{calls:?}");
    assert_ne!(calls[0].1, mine, "the agent's page opens in a tab of its own");
    let tabs = app.browser_tabs(&id).unwrap();
    assert_eq!(
        (tabs.front.as_deref(), tabs.agent.as_deref()),
        (Some(mine.as_str()), Some(calls[0].1.as_str())),
        "the person's stays in front"
    );

    // The person closes the agent's tab: its next action has nothing to act on, and its next page gets a new tab.
    app.browser_tab(&id, faber_engine::TabStep::Close(calls[0].1.clone())).unwrap();
    let (said, ok) = browse(&mut pipe, 903, serde_json::json!({ "action": "click", "target": "Next" })).await;
    assert!(!ok && said.contains("no page of yours is open"), "{said}");
    let (_, ok) = browse(&mut pipe, 904, serde_json::json!({ "url": "https://docs.example.org/b" })).await;
    assert!(ok);
    let last = browser.0.lock().unwrap().last().unwrap().1.clone();
    assert!(last != calls[0].1 && last != mine, "{last}");
    app.shutdown();
}
