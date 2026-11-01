//! The pane's rules against a fake driver: no window, no process, no network. The fake keeps the contract of `Driver` (a view
//! opens once, a call on a closed view fails) and plays a page: it reports a load when asked for one and answers the DevTools
//! calls the pane makes.

use super::*;
use crate::{BoxFuture, Res};
use base64::Engine as _;
use faber_engine::TabStep;
use faber_engine::browser::tabs::Tabs;
use faber_engine::browser::{Browser, Page, Request};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

const A: &str = "https://example.org/a";
const B: &str = "https://example.org/b";

#[derive(Default)]
struct Fake {
    log: Mutex<Vec<String>>,
    calls: Mutex<Vec<(View, &'static str, Value)>>,
    open: Mutex<HashSet<&'static str>>,
    urls: Mutex<HashMap<&'static str, String>>,
    pane: OnceLock<Weak<Pane>>,
    /// A method whose call never answers.
    hang: Mutex<Option<&'static str>>,
    fail_load: AtomicBool,
}

impl Fake {
    fn log(&self) -> Vec<String> {
        crate::lock(&self.log).clone()
    }

    fn logged(&self, prefix: &str) -> usize {
        self.log().iter().filter(|line| line.starts_with(prefix)).count()
    }

    fn calls_of(&self, method: &str) -> Vec<Value> {
        crate::lock(&self.calls).iter().filter(|(_, m, _)| *m == method).map(|(_, _, params)| params.clone()).collect()
    }

    fn answer(&self, view: View, method: &'static str, params: &Value) -> Value {
        let url = crate::lock(&self.urls).get(view.label()).cloned().unwrap_or_default();
        match method {
            "Page.getFrameTree" => json!({ "frameTree": { "frame": { "id": "frame" } } }),
            "Page.createIsolatedWorld" => json!({ "executionContextId": 7 }),
            "Page.captureScreenshot" => json!({ "data": base64::engine::general_purpose::STANDARD.encode(b"jpeg") }),
            "Page.getNavigationHistory" => json!({
                "currentIndex": 1,
                "entries": [
                    { "id": 10, "url": "https://example.org/before", "title": "Before" },
                    { "id": 11, "url": url, "title": "Fake page" },
                    { "id": 12, "url": "https://example.org/after", "title": "After" },
                ],
            }),
            "Page.addScriptToEvaluateOnNewDocument" => {
                json!({ "identifier": format!("script{}", self.calls_of("Page.addScriptToEvaluateOnNewDocument").len()) })
            }
            "Runtime.evaluate" => {
                let expression = params["expression"].as_str().unwrap_or_default();
                let value = if expression.trim_end().ends_with("({ kind: 'settle' })") {
                    json!({ "changed": false })
                } else if expression.contains("kind: 'fits'") {
                    json!({ "wider": 0, "cut": [], "x": 0, "y": 0 })
                } else if expression.contains("href: String(location.href)") {
                    json!({ "turn": "", "href": url, "url": url, "title": "Fake page", "text": "Hello", "x": 0, "y": 0 })
                } else {
                    json!({ "ok": true })
                };
                json!({ "result": { "value": value } })
            }
            _ => json!({}),
        }
    }
}

impl Driver for Fake {
    fn open(&self, view: View, profile: String) -> BoxFuture<'_, Res<()>> {
        Box::pin(async move {
            if !crate::lock(&self.open).insert(view.label()) {
                return Err("already open".into());
            }
            crate::lock(&self.log).push(format!("open {} {profile}", view.label()));
            Ok(())
        })
    }

    fn close(&self, view: View) {
        crate::lock(&self.open).remove(view.label());
        crate::lock(&self.log).push(format!("close {}", view.label()));
    }

    fn place(&self, view: View, place: Place) -> Res<()> {
        if !crate::lock(&self.open).contains(view.label()) {
            return Err("not open".into());
        }
        crate::lock(&self.log).push(match place {
            Place::Parked => format!("place {} parked", view.label()),
            Place::Over { x, y, width, height } => format!("place {} {x} {y} {width} {height}", view.label()),
        });
        Ok(())
    }

    fn load(&self, view: View, url: String) -> BoxFuture<'_, Res<()>> {
        Box::pin(async move {
            if !crate::lock(&self.open).contains(view.label()) {
                return Err("not open".into());
            }
            if self.fail_load.load(Ordering::Relaxed) {
                return Err("no network".into());
            }
            crate::lock(&self.log).push(format!("load {} {url}", view.label()));
            crate::lock(&self.urls).insert(view.label(), url.clone());
            if let Some(pane) = self.pane.get().and_then(Weak::upgrade) {
                pane.event(view, ViewEvent::Started { url: url.clone() });
                pane.event(view, ViewEvent::Finished { url });
            }
            Ok(())
        })
    }

    fn call(&self, view: View, method: &'static str, params: Value) -> BoxFuture<'_, Res<Value>> {
        Box::pin(async move {
            if !crate::lock(&self.open).contains(view.label()) {
                return Err("not open".into());
            }
            let answer = self.answer(view, method, &params);
            crate::lock(&self.calls).push((view, method, params));
            if *crate::lock(&self.hang) == Some(method) {
                std::future::pending::<()>().await;
            }
            Ok(answer)
        })
    }

    fn clear(&self, view: View) -> BoxFuture<'_, Res<()>> {
        Box::pin(async move {
            crate::lock(&self.log).push(format!("clear {}", view.label()));
            Ok(())
        })
    }
}

#[derive(Default)]
struct FakeHome {
    events: Mutex<Vec<PaneEvent>>,
    tabs: Mutex<HashMap<String, Tabs>>,
    seen: Mutex<Vec<(String, Option<String>, String, Option<String>)>>,
}

impl FakeHome {
    fn events(&self) -> Vec<PaneEvent> {
        crate::lock(&self.events).clone()
    }
}

impl Home for FakeHome {
    fn tabs(&self, conversation: String) -> BoxFuture<'_, Option<Tabs>> {
        Box::pin(async move { crate::lock(&self.tabs).get(&conversation).cloned() })
    }

    fn tab(&self, conversation: String, step: TabStep) -> BoxFuture<'_, Option<(Tabs, Option<String>)>> {
        Box::pin(async move {
            let mut all = crate::lock(&self.tabs);
            let tabs = all.entry(conversation).or_default();
            let tab = match step {
                TabStep::New => Some(tabs.new_tab()),
                TabStep::Show(url) => Some(tabs.show(&url)),
                TabStep::Bring(tab) => tabs.bring(&tab).then_some(tab),
                TabStep::Clear => {
                    *tabs = Tabs::default();
                    None
                }
                _ => None,
            };
            Some((tabs.clone(), tab))
        })
    }

    fn seen(&self, conversation: String, tab: Option<String>, url: String, title: Option<String>) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            let changed = crate::lock(&self.tabs).entry(conversation.clone()).or_default().record(tab.as_deref(), &url, title.as_deref());
            crate::lock(&self.seen).push((conversation, tab, url, title));
            changed
        })
    }

    fn tell(&self, event: PaneEvent) {
        crate::lock(&self.events).push(event);
    }

    fn profiles(&self) -> Vec<firefox::Profile> {
        vec![firefox::Profile::stand_in("firefox/a", "Default")]
    }

    fn sites(&self, _: &firefox::Profile, _: i64) -> Result<Vec<firefox::Site>, &'static str> {
        Ok(vec![
            firefox::Site { name: "example.com".into(), cookies: 1, kind: None, hosts: vec![".example.com".into()] },
            firefox::Site { name: "other.org".into(), cookies: 1, kind: Some(firefox::Kind::Money), hosts: vec!["other.org".into()] },
        ])
    }

    fn cookies(&self, _: &firefox::Profile, hosts: &[String], _: i64) -> Result<Vec<firefox::Cookie>, &'static str> {
        Ok(hosts
            .iter()
            .map(|host| firefox::Cookie {
                name: "sid".into(),
                value: "secret".into(),
                host: host.clone(),
                path: "/".into(),
                secure: true,
                http_only: true,
                same_site: None,
                expires: 2.0e9,
            })
            .collect())
    }
}

fn world() -> (Arc<Pane>, Arc<Fake>, Arc<FakeHome>) {
    let (fake, home) = (Arc::new(Fake::default()), Arc::new(FakeHome::default()));
    let pane = Pane::new(fake.clone(), home.clone(), Handle::current());
    let _ = fake.pane.set(Arc::downgrade(&pane));
    (pane, fake, home)
}

async fn agent(pane: &Arc<Pane>, conversation: &str, tab: &str, request: Request) -> Result<Page, String> {
    PaneBrowser(pane.clone()).act(conversation.into(), tab.into(), request).await
}

async fn open(pane: &Arc<Pane>, conversation: &str, tab: &str, url: &str) -> Page {
    agent(pane, conversation, tab, Request::Open(url.into())).await.unwrap()
}

/// The profile a log line `open <view> <profile>` names.
fn profile_of(log: &[String], view: &str) -> Vec<String> {
    log.iter().filter_map(|line| line.strip_prefix(&format!("open {view} "))).map(str::to_string).collect()
}

#[tokio::test(start_paused = true)]
async fn the_agent_opens_a_page_on_a_private_profile_and_reads_it() {
    let (pane, fake, home) = world();
    let page = open(&pane, "c", "t1", A).await;
    assert_eq!((page.url.as_str(), page.title.as_str(), page.text.as_str()), (A, "Fake page", "Hello"));
    assert_eq!(page.picture.as_deref(), Some(&b"jpeg"[..]), "a picture of what is in view");
    let log = fake.log();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log[0].starts_with("open site faber-pane-") && log[1] == format!("load site {A}"), "{log:?}");
    assert_eq!(
        home.events().into_iter().filter(|e| !matches!(e, PaneEvent::Tabs(_))).collect::<Vec<_>>(),
        [
            PaneEvent::Busy { conversation: "c".into(), tab: "t1".into(), busy: true },
            PaneEvent::Loading(true),
            PaneEvent::Loading(false),
            PaneEvent::Page(A.into()),
            PaneEvent::Busy { conversation: "c".into(), tab: "t1".into(), busy: false },
        ],
        "the screen hears the page the person sees, and that the agent worked"
    );
    // The page is kept in the conversation's tab once it settled.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let seen = crate::lock(&home.seen).clone();
    assert!(seen.contains(&("c".into(), Some("t1".into()), A.into(), Some("Fake page".into()))), "{seen:?}");
}

#[tokio::test(start_paused = true)]
async fn only_web_pages_open_and_a_refused_address_opens_no_view() {
    let (pane, fake, _) = world();
    for bad in ["file:///C:/x", "http://tauri.localhost/", "https://ipc.localhost/x", "javascript:alert(1)", "nonsense"] {
        assert!(agent(&pane, "c", "t1", Request::Open(bad.into())).await.is_err(), "{bad}");
    }
    assert!(!pane.show_page("c", "file:///C:/x").await && !pane.go("c", "data:text/html,x").await);
    assert!(fake.log().is_empty(), "{:?}", fake.log());
}

#[tokio::test(start_paused = true)]
async fn a_click_runs_only_on_a_page_of_the_same_conversation_that_was_read() {
    let (pane, fake, _) = world();
    let early = agent(&pane, "c", "t1", Request::Click("More".into())).await;
    assert!(early.unwrap_err().starts_with("no page of yours is open"));
    open(&pane, "c", "t1", A).await;
    let other = agent(&pane, "d", "t1", Request::Read).await;
    assert!(other.is_err(), "another conversation's agent never reads it");
    assert!(agent(&pane, "c", "t1", Request::Click("More".into())).await.is_ok());
    let acted: Vec<Value> = fake
        .calls_of("Runtime.evaluate")
        .into_iter()
        .filter(|p| p["expression"].as_str().is_some_and(|e| e.contains("\"kind\":\"click\"")))
        .collect();
    assert_eq!(acted.len(), 1);
    assert!(acted[0]["expression"].as_str().unwrap().contains(&format!("\"at\":\"{A}\"")), "it runs on the address that was read");
    assert_eq!(acted[0]["contextId"], 7, "in the world of Faber's own");
}

#[tokio::test(start_paused = true)]
async fn the_agent_waits_while_the_person_has_the_pane_and_then_hears_that_they_used_it() {
    let (pane, fake, _) = world();
    open(&pane, "c", "t1", A).await;
    assert!(!pane.place(7, 1, 10.0, 20.0, 800.0, 600.0).await, "before the screen said it loaded, no box places the pane");
    pane.park(7, 0);
    assert!(!pane.place(7, 1, f64::NAN, 0.0, 10.0, 10.0).await && !pane.place(7, 1, 0.0, 0.0, 0.0, 10.0).await);
    assert!(pane.place(7, 1, 10.0, 20.0, 800.0, 600.0).await);
    assert!(fake.log().contains(&"place site 10 20 800 600".to_string()), "{:?}", fake.log());
    let waiting = tokio::spawn({
        let pane = pane.clone();
        async move { agent(&pane, "c", "t1", Request::Read).await }
    });
    tokio::time::sleep(Duration::from_secs(31)).await;
    let refused = waiting.await.unwrap();
    assert!(refused.unwrap_err().starts_with("the person is using Faber's browser"));
    // The person looks at another site, then hands the pane back.
    pane.event(View::Main, ViewEvent::Started { url: "https://docs.rs/x?code=1".into() });
    pane.event(View::Main, ViewEvent::Finished { url: "https://docs.rs/x?code=1".into() });
    pane.park(7, 1);
    assert_eq!(fake.log().last().map(String::as_str), Some("place site parked"));
    let page = agent(&pane, "c", "t1", Request::Read).await.unwrap();
    assert!(page.person, "the agent hears that the person used it");
    assert_eq!(page.visited, ["docs.rs"], "by the site's name only");
    let again = agent(&pane, "c", "t1", Request::Read).await.unwrap();
    assert!(!again.person && again.visited.is_empty(), "and only once");
}

#[tokio::test(start_paused = true)]
async fn a_pane_nobody_uses_closes_and_the_next_page_is_a_new_one() {
    let (pane, fake, _) = world();
    open(&pane, "c", "t1", A).await;
    tokio::time::sleep(rules::IDLE + Duration::from_secs(10)).await;
    assert_eq!(fake.logged("close site"), 1, "{:?}", fake.log());
    assert!(agent(&pane, "c", "t1", Request::Read).await.is_err(), "no page is open any more");
    open(&pane, "c", "t1", A).await;
    let profiles = profile_of(&fake.log(), "site");
    assert_eq!(profiles.len(), 2);
    assert_ne!(profiles[0], profiles[1], "a pane made later never shares a profile with an earlier one");
}

#[tokio::test(start_paused = true)]
async fn a_pane_on_view_does_not_close_when_idle() {
    let (pane, fake, _) = world();
    open(&pane, "c", "t1", A).await;
    pane.park(7, 0);
    assert!(pane.place(7, 1, 0.0, 0.0, 400.0, 300.0).await);
    tokio::time::sleep(rules::IDLE * 2).await;
    assert_eq!(fake.logged("close"), 0);
}

#[tokio::test(start_paused = true)]
async fn another_conversation_gets_a_pane_of_its_own_on_a_new_profile() {
    let (pane, fake, _) = world();
    open(&pane, "a", "t1", A).await;
    open(&pane, "b", "t1", B).await;
    let log = fake.log();
    let clear = log.iter().position(|l| l == "clear site").expect("the old page's data goes while it runs");
    let close = log.iter().position(|l| l == "close site").expect("and the old pane closes");
    let reopen = log.iter().rposition(|l| l.starts_with("open site ")).unwrap();
    assert!(clear < close && close < reopen, "{log:?}");
    let profiles = profile_of(&log, "site");
    assert_ne!(profiles[0], profiles[1]);
    assert_eq!(fake.urls.lock().unwrap()["site"], B);
}

#[tokio::test(start_paused = true)]
async fn the_agent_s_second_tab_loads_behind_on_the_profile_of_the_first() {
    let (pane, fake, home) = world();
    assert!(pane.show_page("c", A).await, "the person's tab");
    let second = {
        let mut tabs = crate::lock(&home.tabs);
        tabs.get_mut("c").unwrap().add(String::new(), String::new())
    };
    let page = open(&pane, "c", &second, B).await;
    assert_eq!(page.url, B);
    let log = fake.log();
    let (front, behind) = (profile_of(&log, "site"), profile_of(&log, "site-agent"));
    assert_eq!((front.len(), behind.len()), (1, 1), "{log:?}");
    assert_eq!(front, behind, "one conversation's sign-ins are one profile");
    assert!(log.contains(&format!("load site {A}")) && log.contains(&format!("load site-agent {B}")), "{log:?}");
    assert_eq!(fake.urls.lock().unwrap()["site"], A, "the person's page stays as it was");
    pane.close();
    assert_eq!(fake.logged("close site-agent"), 1, "the pane behind never outlives the first");
}

#[tokio::test(start_paused = true)]
async fn forgetting_signins_closes_both_views_and_reopens_on_a_new_profile() {
    let (pane, fake, home) = world();
    assert!(pane.show_page("c", A).await);
    let second = crate::lock(&home.tabs).get_mut("c").unwrap().add(String::new(), String::new());
    open(&pane, "c", &second, B).await;
    pane.cookie_forget("c".into()).await.unwrap();
    let log = fake.log();
    let profiles = profile_of(&log, "site");
    assert_eq!(profiles.len(), 2);
    assert_ne!(profiles[0], profiles[1]);
    assert_eq!(fake.logged("close site-agent"), 1);
    assert!(!pane.behind.as_ref().unwrap().is_open());
}

#[tokio::test(start_paused = true)]
async fn a_view_that_is_gone_closes_the_pane_and_its_late_reports_change_nothing() {
    let (pane, fake, home) = world();
    open(&pane, "c", "t1", A).await;
    let heard = home.events().len();
    pane.event(View::Main, ViewEvent::Gone);
    assert_eq!(fake.logged("close site"), 1);
    pane.event(View::Main, ViewEvent::Started { url: B.into() });
    pane.event(View::Main, ViewEvent::Finished { url: B.into() });
    pane.event(View::Behind, ViewEvent::Titled);
    assert_eq!(home.events().len(), heard, "a report after the close is ignored");
    assert!(agent(&pane, "c", "t1", Request::Read).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn a_pane_whose_first_page_fails_closes_instead_of_waiting_blank() {
    let (pane, fake, _) = world();
    fake.fail_load.store(true, Ordering::Relaxed);
    assert!(agent(&pane, "c", "t1", Request::Open(A.into())).await.is_err());
    assert_eq!(fake.logged("close site"), 1, "{:?}", fake.log());
    fake.fail_load.store(false, Ordering::Relaxed);
    open(&pane, "c", "t1", A).await;
    assert_eq!(fake.logged("open site "), 2, "the next page makes a new view");
}

#[tokio::test(start_paused = true)]
async fn a_page_that_stops_answering_closes_the_pane() {
    let (pane, fake, _) = world();
    open(&pane, "c", "t1", A).await;
    *crate::lock(&fake.hang) = Some("Page.getFrameTree");
    let error = agent(&pane, "c", "t1", Request::Read).await.unwrap_err();
    assert_eq!(error, "the page stopped answering");
    assert_eq!(fake.logged("close site"), 1);
}

#[tokio::test(start_paused = true)]
async fn the_screen_size_is_drawn_as_that_screen_for_the_agent_and_fitted_for_the_person() {
    let (pane, fake, _) = world();
    let info = pane.site_size(true, Some("390x844".into())).await.unwrap();
    assert_eq!(info["now"], "390x844");
    assert!(info["sizes"].as_array().is_some_and(|s| !s.is_empty()));
    let page = open(&pane, "c", "t1", A).await;
    assert_eq!(page.size.map(|s| s.name()).as_deref(), Some("390x844"));
    let drawn = fake.calls_of("Emulation.setDeviceMetricsOverride");
    assert_eq!((drawn[0]["width"].as_u64(), drawn[0]["mobile"].as_bool()), (Some(390), Some(true)));
    assert_eq!(fake.calls_of("Emulation.setTouchEmulationEnabled")[0]["enabled"], true);
    // The person sees it shrunk to fit their box, once per box.
    pane.park(7, 0);
    assert!(pane.place(7, 1, 0.0, 0.0, 195.0, 422.0).await);
    let drawn = fake.calls_of("Emulation.setDeviceMetricsOverride");
    assert_eq!(drawn.last().unwrap()["scale"], 0.5);
    assert!(pane.place(7, 1, 0.0, 0.0, 195.0, 422.0).await);
    assert_eq!(fake.calls_of("Emulation.setDeviceMetricsOverride").len(), drawn.len(), "the same box again sends nothing");
    // Picking the pane's own size gives the window back.
    pane.site_size(true, None).await.unwrap();
    assert_eq!(fake.calls_of("Emulation.clearDeviceMetricsOverride").len(), 1);
    assert_eq!(fake.calls_of("Emulation.setTouchEmulationEnabled").last().unwrap()["enabled"], false);
}

#[tokio::test(start_paused = true)]
async fn the_agent_looks_at_several_sizes_and_the_page_comes_back_to_its_own() {
    let (pane, fake, _) = world();
    open(&pane, "c", "t1", A).await;
    let sizes = vec![faber_engine::screens::named("1920x1080").unwrap(), faber_engine::screens::named("390x844").unwrap()];
    let page = agent(&pane, "c", "t1", Request::Sizes(sizes)).await.unwrap();
    assert_eq!(page.looks.len(), 2);
    assert!(page.looks.iter().all(|l| l.picture.is_some() && l.turn.is_none()));
    let widths: Vec<u64> = fake.calls_of("Emulation.setDeviceMetricsOverride").iter().filter_map(|p| p["width"].as_u64()).collect();
    assert_eq!(widths, [1920, 390], "{widths:?}");
    assert_eq!(fake.calls_of("Emulation.clearDeviceMetricsOverride").len(), 1, "and back at the pane's own size");
}

#[tokio::test(start_paused = true)]
async fn the_page_takes_the_corners_of_the_card_once_per_change() {
    let (pane, fake, _) = world();
    open(&pane, "c", "t1", A).await;
    pane.set_round(12.0, [1, 2, 3]).await;
    let added = fake.calls_of("Page.addScriptToEvaluateOnNewDocument");
    assert_eq!(added.len(), 1);
    let source = added[0]["source"].as_str().unwrap();
    assert!(source.contains("12") && source.contains("1, 2, 3") && !source.contains("__R__"), "{source}");
    assert_eq!(added[0]["worldName"], "faber-round");
    pane.set_round(12.0, [1, 2, 3]).await;
    assert_eq!(fake.calls_of("Page.addScriptToEvaluateOnNewDocument").len(), 1, "the same corners again send nothing");
    pane.set_round(500.0, [1, 2, 3]).await;
    assert_eq!(fake.calls_of("Page.removeScriptToEvaluateOnNewDocument")[0]["identifier"], "script0", "the old script goes first");
    let added = fake.calls_of("Page.addScriptToEvaluateOnNewDocument");
    assert!(added[1]["source"].as_str().unwrap().contains("32"), "no corner is rounder than the card's");
    pane.set_round(f64::NAN, [0, 0, 0]).await;
    assert!(
        fake.calls_of("Page.addScriptToEvaluateOnNewDocument")[2]["source"].as_str().unwrap().contains("const r = 0;"),
        "an odd radius is square"
    );
}

#[tokio::test(start_paused = true)]
async fn back_forward_and_reload_work_on_the_page_the_person_looks_at_only() {
    let (pane, fake, _) = world();
    open(&pane, "c", "t1", A).await;
    assert!(!pane.step("c", "back").await, "a pane out of sight is not the person's to move");
    pane.park(7, 0);
    assert!(pane.place(7, 1, 0.0, 0.0, 400.0, 300.0).await);
    assert_eq!(pane.site_history("c".into()).await.unwrap(), [true, true]);
    assert_eq!(pane.site_history("d".into()).await.unwrap(), [false, false]);
    assert!(!pane.step("d", "back").await);
    assert!(pane.step("c", "back").await && pane.step("c", "forward").await && pane.step("c", "reload").await);
    assert!(!pane.step("c", "sideways").await);
    let to: Vec<Value> = fake.calls_of("Page.navigateToHistoryEntry").iter().map(|p| p["entryId"].clone()).collect();
    assert_eq!(to, [10, 12]);
    assert_eq!(fake.calls_of("Page.reload").len(), 1);
}

#[tokio::test]
async fn sign_ins_come_over_to_one_conversation_only_and_go_when_forgotten() {
    let (pane, fake, _) = world();
    assert!(pane.site_open("c".into(), A.into()).await.unwrap());
    let profiles = pane.cookie_profiles().await.unwrap();
    assert_eq!(profiles, json!([{ "id": "firefox/a", "name": "Default", "lastUsed": true }]));
    let listed = pane.cookie_sites("firefox/a".into()).await.unwrap();
    let ticket = listed["ticket"].as_str().unwrap().to_string();
    assert_eq!(listed["sites"][1], json!({ "name": "other.org", "cookies": 1, "kind": "money" }));
    assert!(!listed.to_string().contains("secret"), "no cookie crosses into the screen");
    assert_eq!(pane.cookie_import("c".into(), "wrong".into(), vec!["example.com".into()]).await, Err("error.cookies.stale".into()));
    assert_eq!(pane.cookie_import("c".into(), ticket.clone(), vec!["nothing.net".into()]).await, Err("error.cookies.stale".into()));
    assert_eq!(pane.cookie_import("c".into(), ticket, vec!["example.com".into()]).await, Ok(1));
    assert_eq!(pane.cookie_held("c".into()).await.unwrap(), ["example.com"]);
    assert_eq!(pane.cookie_held("d".into()).await.unwrap(), Vec::<String>::new());
    let set = fake.calls_of("Network.setCookies");
    assert_eq!(set.len(), 1);
    assert_eq!(set[0]["cookies"][0]["domain"], ".example.com");
    assert_eq!(fake.calls_of("Page.reload").len(), 1, "the page on view loads again with them");
    // Another conversation's page never gets them.
    assert!(pane.site_open("d".into(), B.into()).await.unwrap());
    assert_eq!(fake.calls_of("Network.setCookies").len(), 1);
    // Its own conversation's next pane does, before the page loads.
    assert!(pane.site_open("c".into(), A.into()).await.unwrap());
    assert_eq!(fake.calls_of("Network.setCookies").len(), 2);
    let before = fake.logged("close site");
    pane.cookie_forget("c".into()).await.unwrap();
    assert_eq!(fake.logged("close site"), before + 1);
    assert!(pane.is_open(), "the page reopens on a fresh profile");
    assert!(pane.cookie_held("c".into()).await.unwrap().is_empty());
}

#[tokio::test]
async fn clearing_a_conversation_closes_its_page_and_empties_its_tabs() {
    let (pane, fake, home) = world();
    assert!(pane.site_open("c".into(), A.into()).await.unwrap());
    assert!(!crate::lock(&home.tabs)["c"].tabs.is_empty());
    pane.site_clear("c".into()).await.unwrap();
    assert_eq!(fake.logged("clear site"), 1);
    assert_eq!(fake.logged("close site"), 1);
    assert!(crate::lock(&home.tabs)["c"].tabs.is_empty());
    // Another conversation's page is left as it is.
    assert!(pane.site_open("d".into(), B.into()).await.unwrap());
    pane.site_clear("c".into()).await.unwrap();
    assert_eq!(fake.logged("close site"), 1);
}

#[tokio::test]
async fn the_person_s_tab_steps_answer_with_the_tabs_and_the_tab_they_concern() {
    let (pane, _, home) = world();
    let answer = pane.site_tab("c".into(), "new".into(), None).await.unwrap().unwrap();
    assert_eq!(answer["tab"], "t1");
    assert_eq!(answer["tabs"]["front"], "t1");
    assert_eq!(pane.site_tab("c".into(), "bring".into(), None).await, Ok(None), "a step that names no tab it needs does nothing");
    assert_eq!(pane.site_tab("c".into(), "nonsense".into(), Some("t1".into())).await, Ok(None));
    assert!(home.events().contains(&PaneEvent::Tabs("c".into())), "the strip hears of it");
}

#[tokio::test]
async fn the_screen_hears_the_pane_by_the_names_it_always_had() {
    let wire: Vec<_> = [
        PaneEvent::Loading(true),
        PaneEvent::Page("u".into()),
        PaneEvent::Tabs("c".into()),
        PaneEvent::Busy { conversation: "c".into(), tab: "t".into(), busy: false },
    ]
    .iter()
    .map(PaneEvent::wire)
    .collect();
    assert_eq!(
        wire,
        [
            ("faber:site-loading", json!(true)),
            ("faber:site", json!("u")),
            ("faber:site-tabs", json!("c")),
            ("faber:site-busy", json!({ "conversation": "c", "tab": "t", "busy": false })),
        ]
    );
}
