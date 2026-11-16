//! Conversation-owned control of the installed app. Never controls the person's renderer.
use super::*;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct Apps(Mutex<HashMap<String, Entry>>);
struct Entry {
    owner: String,
    busy: bool,
    cancelled: bool,
    used: Instant,
    ready: tokio::sync::watch::Sender<bool>,
    idle: Option<tauri::async_runtime::JoinHandle<()>>,
}
pub(crate) fn ready(app: &AppHandle, label: &str) {
    if let Some(e) = crate::lock(&app.state::<Apps>().0).get(label) {
        e.ready.send_replace(true);
    }
}

pub(crate) fn owner(app: &AppHandle, view: &Webview) -> Option<String> {
    let url = view.url().ok()?;
    let origin = app.get_webview("main")?.url().ok()?.origin();
    if url.as_str() == "about:blank" || url.origin() != origin || !crate::screen_navigation_allowed(&url) {
        return None;
    }
    crate::lock(&app.state::<Apps>().0).get(view.label()).filter(|e| !e.cancelled).map(|e| e.owner.clone())
}
// Never broadcast another conversation's content into an agent-controlled renderer.
pub(crate) fn emit(app: &AppHandle, event: &Value) {
    let Some(apps) = app.try_state::<Apps>() else { return };
    let finished: Vec<_> = crate::lock(&apps.0).iter().filter(|(_, e)| event_finishes(event, &e.owner)).map(|(l, _)| l.clone()).collect();
    for label in finished {
        close(app, &label);
    }
    let labels: Vec<_> =
        crate::lock(&apps.0).iter().filter(|(_, e)| !e.cancelled && event_allowed(event, &e.owner)).map(|(l, _)| l.clone()).collect();
    for label in labels {
        let _ = app.emit_to(tauri::EventTarget::webview(label), "faber:event", event);
    }
}
fn event_finishes(event: &Value, owner: &str) -> bool {
    event["conversationId"].as_str() == Some(owner)
        && event["type"] == "message"
        && matches!(event["message"]["status"].as_str(), Some("done" | "stopped" | "error"))
}
fn event_allowed(event: &Value, owner: &str) -> bool {
    let Some(kind) = event["type"].as_str() else { return false };
    if matches!(kind, "commands" | "message" | "asides" | "queue" | "context" | "contextParts" | "plan" | "halted") {
        return event["conversationId"].as_str() == Some(owner);
    }
    matches!(kind, "list" | "catalog" | "settings" | "quota" | "usage" | "lagged") && event.get("conversationId").is_none()
}
pub(crate) fn close_all(app: &AppHandle) {
    let labels: Vec<_> = crate::lock(&app.state::<Apps>().0).keys().cloned().collect();
    for label in labels {
        close(app, &label);
    }
}
fn active(app: &AppHandle, label: &str) -> Result<(), String> {
    if crate::lock(&app.state::<Apps>().0).get(label).is_some_and(|e| !e.cancelled) { Ok(()) } else { Err("app session closed".into()) }
}
fn close(app: &AppHandle, label: &str) {
    let pending = {
        let apps = app.state::<Apps>();
        let mut entries = crate::lock(&apps.0);
        if let Some(entry) = entries.get_mut(label) {
            entry.cancelled = true;
            entry.ready.send_replace(true);
            entry.busy
        } else {
            false
        }
    };
    if !pending {
        remove(app, label);
    }
    if let Some(view) = app.get_webview(label) {
        let _ = view.close();
    }
}
fn remove(app: &AppHandle, label: &str) {
    if let Some(entry) = crate::lock(&app.state::<Apps>().0).remove(label)
        && let Some(idle) = entry.idle
    {
        idle.abort();
    }
    if let Some(view) = app.get_webview(label) {
        let _ = view.close();
    }
}
// Dropping an interrupted action revokes attribution before any late native completion.
struct Session {
    app: AppHandle,
    label: String,
    keep: bool,
}
impl Drop for Session {
    fn drop(&mut self) {
        if !self.keep {
            remove(&self.app, &self.label);
        }
    }
}

fn checked(p: &Value, testing: bool) -> Result<&str, String> {
    let obj = p.as_object().ok_or("invalid app request")?;
    if obj.keys().any(|k| !["app", "action", "ref", "text", "up", "size"].contains(&k.as_str())) {
        return Err("unknown app argument".into());
    }
    if p["app"].as_str() != Some(if testing { "faber-test" } else { "faber-live" }) {
        return Err("explicit app target does not match this backend".into());
    }
    let action = p.get("action").map_or(Some("read"), Value::as_str).ok_or("invalid action")?;
    if !["read", "click", "type", "scroll", "screenshot", "close"].contains(&action) {
        return Err("unsupported app action".into());
    }
    if matches!(action, "click" | "type") && !p["ref"].as_u64().is_some_and(|n| n > 0 && n <= u32::MAX as u64) {
        return Err("read first and use an element number".into());
    }
    if action == "type" && !p["text"].as_str().is_some_and(|s| s.chars().count() <= 2000) {
        return Err("text must be at most 2000 characters".into());
    }
    if p.get("up").is_some_and(|v| !v.is_boolean()) {
        return Err("up must be boolean".into());
    }
    if let Some(size) = p.get("size") {
        let (w, h) = size.as_str().and_then(|s| s.split_once('x')).ok_or("invalid size")?;
        for n in [w, h] {
            if !n.parse::<u32>().is_ok_and(|n| (300..=2560).contains(&n)) {
                return Err("size must be 300..2560".into());
            }
        }
    }
    Ok(action)
}
async fn call(view: &Webview, method: &str, params: Value) -> Result<Value, String> {
    let raw = tokio::time::timeout(CALL, native::devtools(view, method, params.to_string()))
        .await
        .map_err(|_| "app control timed out")?
        .map_err(|e| e.to_string())??;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}
async fn eval(view: &Webview, world: &Value, expression: &str) -> Result<Value, String> {
    let r =
        call(view, "Runtime.evaluate", json!({"expression":expression,"contextId":world,"returnByValue":true,"awaitPromise":true})).await?;
    if r.get("exceptionDetails").is_some() {
        return Err("stale or unavailable control; read again".into());
    }
    Ok(r["result"]["value"].clone())
}
// Resolve missing descendants through their existing ancestor, including Windows junctions.
fn canonical_data(path: &std::path::Path) -> Option<std::path::PathBuf> {
    match path.canonicalize() {
        Ok(real) => Some(std::path::PathBuf::from(real.to_string_lossy().to_lowercase())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Some(canonical_data(path.parent()?)?.join(path.file_name()?.to_string_lossy().to_lowercase()))
        }
        Err(_) => None,
    }
}
fn disjoint_data(a: &std::path::Path, b: &std::path::Path) -> bool {
    !a.starts_with(b) && !b.starts_with(a)
}
fn isolated_data(data: &std::path::Path, normal: &std::path::Path) -> bool {
    canonical_data(data).zip(canonical_data(normal)).is_some_and(|(a, b)| disjoint_data(&a, &b))
}
pub(crate) async fn act(app: AppHandle, conversation: String, p: Value) -> Result<Page, String> {
    let testing = app.state::<crate::Faber>().hidden;
    if testing {
        let data = app.state::<crate::Faber>().data_dir.clone();
        let normal = crate::default_data_dir(&app.path().config_dir().map_err(|e| e.to_string())?);
        let explicit = std::env::var_os("FABER_DATA_DIR");
        if !explicit.as_ref().is_some_and(|p| std::path::Path::new(p).is_absolute()) || !isolated_data(&data, &normal) {
            return Err("test app requires its own explicit data directory and browser profile".into());
        }
    }
    let action = checked(&p, testing)?.to_string();
    if conversation.is_empty() || conversation.len() > 200 {
        return Err("missing app session".into());
    }
    let existing = crate::lock(&app.state::<Apps>().0).iter().find(|(_, e)| e.owner == conversation).map(|(l, _)| l.clone());
    if action == "close" {
        if let Some(label) = existing {
            close(&app, &label);
        }
        return Ok(Page { title: p["app"].as_str().unwrap().into(), text: "App session closed.".into(), ..Page::default() });
    }
    let fresh = existing.is_none();
    let label = existing.unwrap_or_else(|| format!("app-{}", uuid::Uuid::new_v4()));
    {
        let apps = app.state::<Apps>();
        let mut entries = crate::lock(&apps.0);
        if let Some(e) = entries.get_mut(&label) {
            if e.cancelled {
                return Err("app session closed".into());
            }
            if e.busy {
                return Err("app session busy".into());
            }
            if let Some(idle) = e.idle.take() {
                idle.abort();
            }
            e.busy = true;
        } else {
            if !entries.is_empty() {
                return Err("close an app session first".into());
            }
            entries.insert(
                label.clone(),
                Entry {
                    owner: conversation,
                    busy: true,
                    cancelled: false,
                    used: Instant::now(),
                    ready: tokio::sync::watch::channel(false).0,
                    idle: None,
                },
            );
        }
    }
    let mut session = Session { app: app.clone(), label: label.clone(), keep: false };
    let view = if fresh {
        let (ready, mut loaded) = tokio::sync::watch::channel(false);
        let origin = app.get_webview("main").ok_or("main renderer closed")?.url().map_err(|e| e.to_string())?.origin();
        let loaded_origin = origin.clone();
        let navigation_origin = origin.clone();
        let builder = WebviewBuilder::new(&label, WebviewUrl::External("about:blank".parse().unwrap()))
            .incognito(true)
            .on_navigation(move |url| {
                url.as_str() != "about:blank" && url.origin() == navigation_origin && crate::screen_navigation_allowed(url)
            })
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_download(|_, _| false)
            .on_page_load(move |_, p| {
                if matches!(p.event(), PageLoadEvent::Finished)
                    && p.url().origin() == loaded_origin
                    && crate::screen_navigation_allowed(p.url())
                {
                    ready.send_replace(true);
                }
            });
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.get_webview("main")
            .ok_or("main renderer closed")?
            .with_webview(move |host| {
                let _ = tx.send(builder.with_environment(host.environment()));
            })
            .map_err(|e| e.to_string())?;
        let builder = tokio::time::timeout(CALL, rx).await.map_err(|_| "app environment timed out")?.map_err(|e| e.to_string())?;
        active(&app, &label)?;
        let view = app.get_window("main").ok_or("main window closed")?.add_child(builder, PARKED, SIZE).map_err(|e| e.to_string())?;
        active(&app, &label)?;
        // WebView2 shares incognito storage while any sibling remains alive. Clear only
        // our app origin before its first script can read another owner's drafts.
        call(&view, "Storage.clearDataForOrigin", json!({"origin": origin.ascii_serialization(), "storageTypes":"all"})).await?;
        active(&app, &label)?;
        let mut url = app.get_webview("main").ok_or("main renderer closed")?.url().map_err(|e| e.to_string())?;
        url.set_path("/index.html");
        url.set_query(None);
        url.set_fragment(None);
        view.navigate(url).map_err(|e| e.to_string())?;
        tokio::time::timeout(LOAD, loaded.wait_for(|v| *v)).await.map_err(|_| "app load timed out")?.map_err(|e| e.to_string())?;
        view
    } else {
        app.get_webview(&label).ok_or("app session closed")?
    };
    active(&app, &label)?;
    if let Some(size) = p["size"].as_str() {
        let (w, h) = size.split_once('x').unwrap();
        view.set_size(LogicalSize::new(w.parse::<f64>().unwrap(), h.parse::<f64>().unwrap())).map_err(|e| e.to_string())?;
    }
    let tree = call(&view, "Page.getFrameTree", json!({})).await?;
    let world =
        call(&view, "Page.createIsolatedWorld", json!({"frameId":tree["frameTree"]["frame"]["id"],"worldName":"faber-app-control"}))
            .await?["executionContextId"]
            .clone();
    let mut ready = crate::lock(&app.state::<Apps>().0).get(&label).ok_or("app session closed")?.ready.subscribe();
    tokio::time::timeout(LOAD, ready.wait_for(|v| *v)).await.map_err(|_| "app bootstrap timed out")?.map_err(|e| e.to_string())?;
    active(&app, &label)?;
    // Bootstrap must really mount the app; a loaded HTML shell is insufficient.
    let mounted = eval(&view, &world, "!!document.querySelector('#root')?.children.length && !document.getElementById('first')").await?;
    if mounted != true {
        return Err("app UI is not ready; read again".into());
    }
    if matches!(action.as_str(), "click" | "type") {
        let expression = format!(
            r#"(() => {{ const n=globalThis.__appRefs?.nodes.get({}); if(!n||!n.el.isConnected)throw Error(); const el=n.el; if(el.matches(':disabled,[readonly],[aria-disabled="true"],input[type="password"],[autocomplete*="password"],[autocomplete="one-time-code"]'))throw Error(); if({} && !el.matches('textarea,input:not([type]),input[type="text"],input[type="search"],input[type="email"],input[type="url"],[contenteditable="true"]'))throw Error(); const label=(el.getAttribute('aria-label')||el.innerText||el.getAttribute('placeholder')||el.getAttribute('name')||el.tagName).slice(0,160);if(label!==n.label || el.getAttribute('role')!==n.role)throw Error();const b=el.getBoundingClientRect(),x=b.x+b.width/2,y=b.y+b.height/2;if(x<0||y<0||x>=innerWidth||y>=innerHeight||!el.contains(document.elementFromPoint(x,y)))throw Error();return {{x,y}}; }})()"#,
            p["ref"],
            action == "type"
        );
        let point = eval(&view, &world, &format!("(() => {{try {{return ({expression});}} catch {{return null;}}}})()")).await?;
        if point.is_null() {
            finish(&app, &label)?;
            session.keep = true;
            return Err("stale or unavailable control; read again".into());
        }
        active(&app, &label)?;
        for kind in ["mousePressed", "mouseReleased"] {
            call(&view, "Input.dispatchMouseEvent", json!({"type":kind,"x":point["x"],"y":point["y"],"button":"left","clickCount":1}))
                .await?;
        }
        if action == "type" {
            let focused = eval(&view, &world, &format!("(() => {{ const n=globalThis.__appRefs?.nodes.get({});return !!n && document.activeElement===n.el && !n.el.matches(':disabled,[readonly]'); }})()",p["ref"])).await?;
            if focused != true {
                finish(&app, &label)?;
                session.keep = true;
                return Err("stale or unavailable control; read again".into());
            }
            active(&app, &label)?;
            call(&view, "Input.insertText", json!({"text":p["text"]})).await?;
        }
    } else if action == "scroll" {
        call(
            &view,
            "Input.dispatchMouseEvent",
            json!({"type":"mouseWheel","x":150,"y":150,"deltaX":0,"deltaY":if p["up"] == true {-600} else {600}}),
        )
        .await?;
    }
    eval(&view, &world, "new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve(true))))").await?;
    let snapshot = eval(&view, &world, SNAPSHOT).await?;
    let picture = call(&view, "Page.captureScreenshot", json!({"format":"jpeg","quality":65})).await?;
    let picture = base64::engine::general_purpose::STANDARD
        .decode(picture["data"].as_str().ok_or("app screenshot missing")?)
        .map_err(|e| e.to_string())?;
    finish(&app, &label)?;
    session.keep = true;
    Ok(Page {
        url: p["app"].as_str().unwrap().into(),
        title: snapshot["title"].as_str().unwrap_or("Faber").into(),
        text: snapshot["text"].as_str().unwrap_or_default().into(),
        picture: Some(picture),
        ..Page::default()
    })
}

fn finish(app: &AppHandle, label: &str) -> Result<(), String> {
    active(app, label)?;
    let used = Instant::now();
    {
        let apps = app.state::<Apps>();
        let mut entries = crate::lock(&apps.0);
        let entry = entries.get_mut(label).ok_or("app session closed")?;
        if entry.cancelled {
            return Err("app session closed".into());
        }
        entry.busy = false;
        entry.used = used;
    }
    let idle_app = app.clone();
    let idle_label = label.to_string();
    let idle = tauri::async_runtime::spawn(async move {
        let (app, label) = (idle_app, idle_label);
        tokio::time::sleep(BEHIND_IDLE).await;
        // Busy sessions never expire beneath an action. Each completion schedules its own expiry.
        if crate::lock(&app.state::<Apps>().0).get(&label).is_some_and(|e| !e.busy && e.used == used) {
            close(&app, &label);
        }
    });
    if let Some(entry) = crate::lock(&app.state::<Apps>().0).get_mut(label) {
        entry.idle = Some(idle);
    } else {
        idle.abort();
    }
    Ok(())
}

const SNAPSHOT: &str = r#"(() => {
 const s = globalThis.__appRefs ||= {next:0, ids:new WeakMap(), nodes:new Map()};
 s.nodes.clear();
 const rows = [];
 for (const el of document.querySelectorAll('button,a,input,textarea,select,[role="button"],[role="option"],[role="menuitem"],[role="tab"],[role="checkbox"],[role="radio"],[role="switch"],[contenteditable="true"]')) {
  const b=el.getBoundingClientRect(); if (!b.width || !b.height || b.bottom<0 || b.top>innerHeight) continue;
  if (el.matches('input[type="password"],input[autocomplete*="password"],input[autocomplete="one-time-code"]')) continue;
  let id=s.ids.get(el); if (!id) {id=++s.next;s.ids.set(el,id);}
  const label=(el.getAttribute('aria-label')||el.innerText||el.getAttribute('placeholder')||el.getAttribute('name')||el.tagName).slice(0,160);
  s.nodes.set(id,{el,label,role:el.getAttribute('role')}); rows.push(id+' '+el.tagName+' '+label); if(rows.length>=500) break;
 }
 return {title:document.title,text:document.body.innerText.slice(0,18000)+'\n'+rows.join('\n')};
})()"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_data_trees_must_be_disjoint() {
        use std::path::Path;
        let base = Path::new("c:/data/faber");
        for candidate in ["c:/data/faber", "c:/data", "c:/data/faber/child"] {
            assert!(!disjoint_data(Path::new(candidate), base));
        }
        assert!(disjoint_data(Path::new("c:/data/faber-test"), base));
    }
    #[test]
    fn only_owner_terminal_messages_close_the_renderer() {
        for status in ["done", "stopped", "error"] {
            let event = json!({"type":"message","conversationId":"own","message":{"status":status}});
            assert!(event_finishes(&event, "own"));
            assert!(!event_finishes(&event, "sibling"));
        }
        assert!(!event_finishes(&json!({"type":"message","conversationId":"own","message":{"status":"streaming"}}), "own"));
        assert!(!event_finishes(&json!({"type":"running","conversationId":"own","message":{"status":"done"}}), "own"));
    }
    #[test]
    fn app_events_never_leak_other_conversations() {
        assert!(event_allowed(&json!({"type":"message","conversationId":"own"}), "own"));
        assert!(!event_allowed(&json!({"type":"message","conversationId":"other"}), "own"));
        assert!(!event_allowed(&json!({"type":"running","items":[{"id":"other"}]}), "own"));
        assert!(!event_allowed(&json!({"type":"asks","asks":[{"by":"other"}]}), "own"));
        assert!(event_allowed(&json!({"type":"list"}), "own"));
    }
    #[test]
    fn app_request_rejects_authority_spoofing_and_invalid_input() {
        assert!(checked(&json!({"app":"faber-live","action":"read"}), false).is_ok());
        for p in [
            json!({"app":"faber-test"}),
            json!({"app":"faber-live","conversation":"other"}),
            json!({"app":"faber-live","action":"type","ref":1}),
            json!({"app":"faber-live","action":"click","ref":"button"}),
            json!({"app":"faber-live","size":"1x1"}),
            json!({"app":"faber-live","action":"evaluate"}),
            json!({"app":"faber-live","action":"type","ref":1,"text":"x".repeat(2001)}),
            json!({"app":"faber-live","action":"click","ref":"r1;evil()"}),
            json!({"app":"faber-live","up":"yes"}),
            json!({"app":"faber-live","by":"person"}),
        ] {
            assert!(checked(&p, false).is_err(), "{p}");
        }
        assert!(checked(&json!({"app":"faber-test"}), true).is_ok());
    }
}

/// Fixed native observations for the existing hidden harness; absent from installed builds.
#[cfg(feature = "test-hooks")]
pub(crate) async fn proof(app: &AppHandle) -> Result<Value, String> {
    let label = crate::lock(&app.state::<Apps>().0).keys().next().cloned().ok_or("no app session")?;
    let view = app.get_webview(&label).ok_or("no app renderer")?;
    let owner = crate::lock(&app.state::<Apps>().0).get(&label).unwrap().owner.clone();
    for (target, marker) in [("main", "main-only"), ("unrelated-proof-view", "unrelated-only")] {
        app.emit_to(tauri::EventTarget::webview(target), "faber:event", json!({"type":"proof", "__appCanary":marker}))
            .map_err(|e| e.to_string())?;
    }
    app.emit_to(tauri::EventTarget::webview("main"), "faber:show-conversation", "screen-notice-proof").map_err(|e| e.to_string())?;
    app.emit_to(tauri::EventTarget::webview("main"), "faber:update", json!({"phase":"idle"})).map_err(|e| e.to_string())?;
    emit(app, &json!({"type":"commands", "conversationId":owner, "__appCanary":"own-filtered"}));
    emit(app, &json!({"type":"commands", "conversationId":"unrelated", "__appCanary":"unrelated-filtered"}));
    call(&view, "Runtime.evaluate", json!({
        "expression": r#"(async () => {
          globalThis.__proofEvents ||= [];
          if (!globalThis.__proofListening) {
            globalThis.__proofListening = true;
            for (const kind of ['click','input']) document.addEventListener(kind, e => globalThis.__proofEvents.push({kind,trusted:e.isTrusted,role:e.target?.closest?.('[role]')?.getAttribute('role')}),true);
          }
          try { await window.__TAURI_INTERNALS__.invoke('save_settings',{patch:{language:'en'}}); } catch {}
          let denial;
          try {await window.__TAURI_INTERNALS__.invoke('answer_ask',{id:'nonexistent-proof',allow:true});denial='ALLOWED';}
          catch(e){denial=String(e);}
          return {events:globalThis.__proofEvents,hostEvents:globalThis.__appEventProof,denial,width:innerWidth,height:innerHeight};
        })()"#,
        "returnByValue": true, "awaitPromise":true
    })).await.map(|r| { let mut v=r["result"]["value"].clone();v["buildSource"]=json!(env!("CARGO_MANIFEST_DIR"));v["revision"]=json!("scoped-events-2");v })
}

#[cfg(feature = "test-hooks")]
pub(crate) async fn storage_proof(app: &AppHandle) -> Result<Value, String> {
    if app.get_webview("app-proof-keeper").is_none() {
        let builder = WebviewBuilder::new("app-proof-keeper", WebviewUrl::External("about:blank".parse().unwrap())).incognito(true);
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.get_webview("main")
            .ok_or("main closed")?
            .with_webview(move |host| {
                let _ = tx.send(builder.with_environment(host.environment()));
            })
            .map_err(|e| e.to_string())?;
        let builder = rx.await.map_err(|e| e.to_string())?;
        app.get_window("main").ok_or("main closed")?.add_child(builder, PARKED, SIZE).map_err(|e| e.to_string())?;
    }
    let label = crate::lock(&app.state::<Apps>().0).keys().next().cloned().ok_or("no session")?;
    let view = app.get_webview(&label).ok_or("no view")?;
    call(&view, "Runtime.evaluate", json!({"expression":"(() => { const before=localStorage.getItem('app-proof-canary'); localStorage.setItem('app-proof-canary','owner-A'); return {before}; })()", "returnByValue":true})).await.map(|r| r["result"]["value"].clone())
}

#[cfg(feature = "test-hooks")]
pub(crate) async fn palette_proof(app: &AppHandle) -> Result<Value, String> {
    let label = crate::lock(&app.state::<Apps>().0).keys().next().cloned().ok_or("no session")?;
    let view = app.get_webview(&label).ok_or("no view")?;
    for kind in ["keyDown", "keyUp"] {
        call(&view, "Input.dispatchKeyEvent", json!({"type":kind,"key":"k","code":"KeyK","modifiers":2,"windowsVirtualKeyCode":75}))
            .await?;
    }
    Ok(json!(true))
}
