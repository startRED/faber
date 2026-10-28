//! A real opaque frame follows the saved web permission. Its public HTTPS responses come only from a local fixture.

use super::{Result, Scenario, ensure, no_cli, pause, same};
use crate::app::Ctx;
use crate::cdp::HTTPS_FIXTURE_ORIGIN;
use crate::page::{Page, css};
use crate::server::Server;
use serde_json::{Value, json};

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "visual answers: remote reply images and styles send nothing with Web search off or Live", run: reply_network },
        Scenario {
            name: "visual answers: saved web permission gates HTTPS fixtures while private addresses and Faber stay blocked",
            run: visual_network,
        },
    ]
}

fn save_web(page: &mut Page, value: &str) -> Result {
    page.eval(&format!("window.__TAURI_INTERNALS__.invoke('save_settings', {{patch:{{webSearch:{}}}}}).then(()=>true)", json!(value)))?;
    Ok(())
}

fn report(page: &mut Page, count: usize, version: u32, live: bool) -> Result {
    page.wait_for(
        "the visual page to report its actual network and sandbox results",
        &format!("window.__visualNetworkReports.length==={count}"),
    )?;
    let actual = page.eval("window.__visualNetworkReports.at(-1)")?;
    same(actual["version"].clone(), json!(version), "the selected version ran its own scripts")?;
    same(actual["public"].clone(), json!(if live { "setup-replay" } else { "blocked" }), "only live may fetch the HTTPS fixture")?;
    same(actual["external"].clone(), json!(live), "only live may load an external HTTPS script")?;
    same(actual["moduleSawExternal"].clone(), json!(live), "an earlier inline module waits for the later external classic script")?;
    same(actual["externalModule"].clone(), json!(live), "an external HTTPS module runs after the classic script")?;
    same(actual["noCorsClassic"].clone(), json!(live), "a classic script without authored crossorigin can load without CORS headers")?;
    same(actual["loadNoCors"].clone(), json!(if live { "Ready" } else { "Waiting" }), "the no-CORS classic completes normally")?;
    same(actual["explicitCors"].clone(), json!("Failed"), "an authored anonymous classic still requires the fixture's CORS authorization")?;
    same(actual["moduleCors"].clone(), json!("Failed"), "a module still requires CORS even without an authored crossorigin attribute")?;
    same(actual["integrityRan"].clone(), json!(false), "a mismatched integrity hash prevents executing the external script")?;
    same(actual["integrityError"].clone(), json!("Failed"), "the rejected integrity hash delivers the authored error callback")?;
    same(
        actual["metadata"].clone(),
        json!(live),
        "the executed script keeps authored data attributes and its id, without nonce or loader markers",
    )?;
    for kind in ["Classic", "Module"] {
        same(
            actual[format!("load{kind}")].clone(),
            json!(if live { "Ready" } else { "Waiting" }),
            &format!("the external {kind} script's authored onload runs on its executed element"),
        )?;
        same(
            actual[format!("error{kind}")].clone(),
            json!("Failed"),
            &format!("the external {kind} script's authored onerror runs on its executed element"),
        )?;
    }
    for resource in ["Image", "Stylesheet"] {
        same(
            actual[format!("load{resource}")].clone(),
            json!(if live { "Ready" } else { "Blocked" }),
            &format!("the {resource} callback keeps its actual element"),
        )?;
        same(
            actual[format!("error{resource}")].clone(),
            json!("Failed"),
            &format!("the {resource} error fallback keeps its actual element"),
        )?;
    }
    same(
        actual["loadDataImage"].clone(),
        json!("Ready"),
        "the parser's data image event is captured and delivered ahead of the delayed external classic",
    )?;
    same(actual["cssImport"].clone(), json!(live), "CSS imports follow the same saved web permission")?;
    same(actual["cssUrlBlocked"].clone(), json!(true), &format!("a CSS url cannot reach Faber's IPC origin even with Live: {actual}"))?;
    same(actual["webSocket"].clone(), json!("blocked"), "a private WSS constructor is denied before it can start a handshake")?;
    same(
        actual["webSocketStream"].clone(),
        json!("blocked"),
        "a private WSS stream constructor is denied before it can start a handshake",
    )?;
    for name in [
        "WebSocket",
        "WebSocketStream",
        "WebTransport",
        "RTCPeerConnection",
        "webkitRTCPeerConnection",
        "RTCDataChannel",
        "webkitRTCDataChannel",
    ] {
        same(actual[name].clone(), json!(true), &format!("{name} stays unavailable and cannot be replaced by the page"))?;
    }
    same(actual["parent"].clone(), json!(false), "the page cannot read Faber's document")?;
    same(actual["storage"].clone(), json!(false), "the page cannot use Faber's storage")?;
    same(actual["cookies"].clone(), json!(false), "the opaque page cannot read cookies")?;
    for key in ["ipc", "assets", "otherVisualPath"] {
        same(actual[key].clone(), json!("blocked"), &format!("the frame's policy prevents an actual {key} fetch"))?;
    }
    if live {
        let violations = actual["blockedUris"].as_array().ok_or("the frame did not report its CSP violations")?;
        for origin in ["http://ipc.localhost", "http://tauri.localhost", actual["visualOrigin"].as_str().ok_or("missing visual origin")?] {
            ensure(violations.iter().any(|value| value.as_str().is_some_and(|uri| uri.starts_with(origin))), || {
                format!("the frame must report CSP denying {origin}, rather than a missing IPC global or a failed server: {violations:?}")
            })?;
        }
    }
    same(actual["http"].clone(), json!("blocked"), "plain HTTP stays blocked")?;
    // Without the native denial, TLS against the controlled HTTP server fails rather than returning HTTP 403.
    same(
        actual["private"].clone(),
        if live { json!(403) } else { json!("blocked") },
        "a private HTTPS request receives only the native denial",
    )?;
    same(
        actual["forgedPrivate"].clone(),
        if live { json!(403) } else { json!("blocked") },
        "authored Origin and Sec-Fetch-Site headers cannot bypass private-address denial",
    )?;
    same(
        actual["redirectPrivate"].clone(),
        if live { json!(403) } else { json!("blocked") },
        "the native handler denies the private HTTPS destination of a real public HTTPS redirect",
    )?;
    same(
        page.eval("document.querySelector('.visual-frame').getAttribute('sandbox')")?,
        json!("allow-scripts"),
        "the sandbox never gains same-origin",
    )?;
    Ok(())
}

fn visual_network(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done", "appearance": { "base": "sala" },
        "tutorialDone": true, "webSearch": "off" }))?;
    let server = Server::start(b"if(document.currentScript){window.visualExternalLoaded=true;}else window.visualExternalModuleLoaded=window.visualExternalLoaded===true;".to_vec()).map_err(|e| e.to_string())?;
    let private = server.origin.replacen("http:", "https:", 1);
    let messages: Vec<_> = (1..=2).map(|version| {
        let source = format!(r#"```html
<title>Web permission fixture</title><p>Version {version}</p><button onclick="this.textContent='Clicked'">Click</button>
<p id="loadClassic">Waiting</p><p id="loadModule">Waiting</p><p id="errorClassic">Waiting</p><p id="errorModule">Waiting</p>
<p id="loadImage">Waiting</p><p id="errorImage">Waiting</p><p id="loadStylesheet">Waiting</p><p id="errorStylesheet">Waiting</p>
<p id="loadDataImage">Waiting</p>
<div id="cssProbe" style="width:1px;height:1px"></div>
<script>
function visualResourceReady(event, id, tag) {{event.currentTarget.setAttribute('data-event-ready','true');document.getElementById(id).textContent=this.tagName===tag && event.currentTarget===this && event.target===this?'Ready':'Wrong target';}}
function visualResourceFailed(event, id, tag, state='Failed') {{event.currentTarget.setAttribute('data-event-failed','true');document.getElementById(id).textContent=this.tagName===tag && event.currentTarget===this && event.target===this?state:'Wrong target';}}
</script>
<img src="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/%3E" onload="visualResourceReady.call(this,event,'loadDataImage','IMG');if(window.visualExternalLoaded)document.getElementById('loadDataImage').textContent='Deferred'">
<img src="{HTTPS_FIXTURE_ORIGIN}/fast.svg" onload="visualResourceReady.call(this,event,'loadImage','IMG')" onerror="visualResourceFailed.call(this,event,'loadImage','IMG','Blocked')">
<img src="{HTTPS_FIXTURE_ORIGIN}/missing.svg" onerror="visualResourceFailed.call(this,event,'errorImage','IMG')">
<link rel="stylesheet" href="{HTTPS_FIXTURE_ORIGIN}/fast.css" onload="visualResourceReady.call(this,event,'loadStylesheet','LINK')" onerror="visualResourceFailed.call(this,event,'loadStylesheet','LINK','Blocked')">
<link rel="stylesheet" href="{HTTPS_FIXTURE_ORIGIN}/missing.css" onerror="visualResourceFailed.call(this,event,'errorStylesheet','LINK')">
<script type="module">window.visualModuleSawExternal=window.visualExternalLoaded===true;</script>
<script type="module" src="{HTTPS_FIXTURE_ORIGIN}/codex.zip" onload="document.getElementById('loadModule').textContent=this.tagName==='SCRIPT' && this.src==='{HTTPS_FIXTURE_ORIGIN}/codex.zip'?'Ready':'Wrong target'"></script>
<script type="module" src="{HTTPS_FIXTURE_ORIGIN}/missing.js" onerror="document.getElementById('errorModule').textContent=this.tagName==='SCRIPT' && this.src==='{HTTPS_FIXTURE_ORIGIN}/missing.js'?'Failed':'Wrong target'"></script>
<script src="{HTTPS_FIXTURE_ORIGIN}/codex.zip" onload="document.getElementById('loadClassic').textContent=this.tagName==='SCRIPT' && this.src==='{HTTPS_FIXTURE_ORIGIN}/codex.zip'?'Ready':'Wrong target'"></script>
<script src="{HTTPS_FIXTURE_ORIGIN}/missing.js" onerror="document.getElementById('errorClassic').textContent=this.tagName==='SCRIPT' && this.src==='{HTTPS_FIXTURE_ORIGIN}/missing.js'?'Failed':'Wrong target'"></script>
<script src="{HTTPS_FIXTURE_ORIGIN}/classic-no-cors.js" onload="window.visualNoCorsLoad=this.hasAttribute('crossorigin')?'Wrong CORS mode':'Ready'"></script>
<script src="{HTTPS_FIXTURE_ORIGIN}/classic-no-cors.js" crossorigin="anonymous" onload="window.visualExplicitCors='Unexpected load'" onerror="window.visualExplicitCors='Failed'"></script>
<script type="module" src="{HTTPS_FIXTURE_ORIGIN}/classic-no-cors.js" onload="window.visualModuleCors='Unexpected load'" onerror="window.visualModuleCors='Failed'"></script>
<script src="{HTTPS_FIXTURE_ORIGIN}/integrity.js" crossorigin="anonymous" integrity="sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" onload="window.visualIntegrityError='Unexpected load'" onerror="window.visualIntegrityError='Failed'"></script>
<script id="metadata-script" data-chart="budget" data-faber-script="forged" nonce="authored" src="{HTTPS_FIXTURE_ORIGIN}/metadata.js" crossorigin="anonymous"></script>
<script>
document.addEventListener('DOMContentLoaded', async () => {{
  const result={{version:{version},external:window.visualExternalLoaded===true,moduleSawExternal:window.visualModuleSawExternal===true,externalModule:window.visualExternalModuleLoaded===true,parent:false,storage:false,cookies:false,blockedUris:[],visualOrigin:location.origin}};
  result.noCorsClassic=window.visualNoCorsClassic===true;
  result.loadNoCors=window.visualNoCorsLoad||'Waiting';
  result.explicitCors=window.visualExplicitCors;
  result.moduleCors=window.visualModuleCors;
  result.integrityRan=window.visualIntegrityRan===true;
  result.integrityError=window.visualIntegrityError;
  result.metadata=window.visualMetadata===true;
  for (const name of ['loadClassic','loadModule','errorClassic','errorModule','loadImage','errorImage','loadStylesheet','errorStylesheet','loadDataImage']) result[name]=document.getElementById(name).textContent;
  result.webSocket='allowed';
  try {{const socket=new WebSocket('{private_ws}/');socket.close();}} catch {{result.webSocket='blocked';}}
  result.webSocketStream='allowed';
  try {{new WebSocketStream('{private_ws}/');}} catch {{result.webSocketStream='blocked';}}
  for (const name of ['WebSocket','WebSocketStream','WebTransport','RTCPeerConnection','webkitRTCPeerConnection','RTCDataChannel','webkitRTCDataChannel']) {{
    const descriptor=Object.getOwnPropertyDescriptor(window,name);
    result[name]=window[name]===undefined && descriptor?.configurable===false && descriptor?.writable===false;
  }}
  let cssBlocked;
  const blocked=new Promise(resolve=>{{cssBlocked=resolve;}}), blockedTimer=setTimeout(cssBlocked,1000);
  document.addEventListener('securitypolicyviolation',e=>{{
    result.blockedUris.push(e.blockedURI);
    if(e.blockedURI.endsWith('/visual-css-probe'))cssBlocked();
  }});
  const css=document.createElement('style');
  const styled=new Promise(resolve=>{{css.addEventListener('load',resolve,{{once:true}});css.addEventListener('error',resolve,{{once:true}});setTimeout(resolve,1000);}});
  css.textContent='@import url("{HTTPS_FIXTURE_ORIGIN}/import.css");#cssProbe{{background-image:url("http://ipc.localhost/visual-css-probe")}}';document.head.append(css);
  await styled;
  result.cssImport=getComputedStyle(document.getElementById('cssProbe')).getPropertyValue('--visual-import').trim()==='Ready';
  await blocked;
  clearTimeout(blockedTimer);
  try {{result.parent=!!parent.document;}} catch {{}}
  try {{result.storage=!!localStorage;}} catch {{}}
  try {{result.cookies=typeof document.cookie==='string';}} catch {{}}
  const get=async(url,denial=false,headers) => {{
    const controller=new AbortController(), timer=setTimeout(()=>controller.abort(),2500);
    try {{ const response=await fetch(url,{{signal:controller.signal,headers}});
      return denial?response.status:(await response.json()).tag_name;
    }} catch(e) {{return e.name==='AbortError'?'timeout':'blocked';}} finally {{clearTimeout(timer);}}
  }};
  result.public=await get('{HTTPS_FIXTURE_ORIGIN}/releases/latest');
  result.http=await get('{}'+'/releases/latest');
  result.private=await get('{private}/releases/latest',true);
  result.forgedPrivate=await get('{private}/releases/latest',true,{{Origin:'http://tauri.localhost','Sec-Fetch-Site':'same-origin'}});
  result.redirectPrivate=await get('{HTTPS_FIXTURE_ORIGIN}/',true);
  result.ipc=await get('http://ipc.localhost/state',true);
  result.assets=await get('http://tauri.localhost/',true);
  result.otherVisualPath=await get(new URL('/not-the-registered-page.html',location.href).href,true);
  await new Promise(resolve=>setTimeout(resolve,0));
  result.cssUrlBlocked=result.blockedUris.some(uri=>uri.endsWith('/visual-css-probe'));
  parent.postMessage({{visualNetworkResult:result}},'*');
}});
</script>
```"#, server.origin, private_ws = private.replacen("https:", "wss:", 1));
        json!({ "id": format!("web-{version}"), "role": "assistant", "text": source, "at": version,
            "vendor": "claude", "status": "done" })
    }).collect();
    let conversation = json!({"id":"visual-network", "title":"Visual network", "area":"chat", "sessions":{},
        "createdAt":1,"updatedAt":2,"messages":messages});
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(folder.join("visual-network.json"), conversation.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(no_cli())?;
    app.page.cdp.serve_https_fixture(&server.origin)?;
    app.page.eval("window.__visualNetworkReports=[];window.addEventListener('message',e=>{if(e.origin==='null' && e.source===document.querySelector('.visual-frame')?.contentWindow && e.data?.visualNetworkResult)window.__visualNetworkReports.push(e.data.visualNetworkResult);});void 0")?;
    app.page.click(&css(".list .item").has_text("Visual network"))?;
    app.page.click(&css(".visual-card .visual-open").last())?;
    report(&mut app.page, 1, 2, false)?;
    same(server.requests(), Vec::<String>::new(), "off sends no request even to the controlled HTTPS fixture")?;
    app.page
        .eval("window.__networkFrame=document.querySelector('.visual-frame');window.__networkSource=window.__networkFrame.src;void 0")?;
    save_web(&mut app.page, "live")?;
    same(
        app.page.eval("document.querySelector('.visual-frame').src===window.__networkSource")?,
        json!(true),
        "saving web permission leaves the open page intact",
    )?;
    same(app.page.eval("window.__visualNetworkReports.length")?, json!(1), "saving permission does not rerun the open page")?;
    app.page.click(&css(".visual-prev"))?;
    report(&mut app.page, 2, 1, true)?;
    same(
        app.page.eval("document.querySelector('.visual-frame')===window.__networkFrame")?,
        json!(true),
        "changing permission and version reuses the frame",
    )?;
    same(
        server.requests(),
        vec!["/codex.zip".to_string(), "/codex.zip".to_string(), "/releases/latest".to_string(), "/".to_string()],
        "only live requests reach the controlled fixture",
    )?;
    save_web(&mut app.page, "cached")?;
    app.page.click(&css(".visual-next"))?;
    report(&mut app.page, 3, 2, false)?;
    same(server.requests().len(), 4, "cached is offline, independent of the CLI's search cache")?;
    let close = app.page.named("button", "run.close")?;
    app.page.click(&close)?;
    app.page.wait_count(&css(".visual-pane"), 0)?;
    same(app.page.count(&css("iframe"))?, 0, "closing removes the network-enabled frame too")?;
    save_web(&mut app.page, "live")?;
    app.page.click(&css(".visual-card .visual-open").last())?;
    report(&mut app.page, 4, 2, true)?;
    same(server.requests().len(), 8, "reopening snapshots the saved live permission")?;
    app.page.eval("window.__networkFrame=null;window.__networkSource=null;void 0")?;
    ctx.shot(&mut app, "visual-network-live")?;
    app.page.click(&close)?;
    app.page.wait_gone(&css(".visual-frame"))?;
    ensure(server.unexpected().is_empty(), || format!("unexpected fixture traffic: {:?}", server.unexpected()))?;
    let reports: Value = app.page.eval("window.__visualNetworkReports")?;
    ctx.notes.push(format!("visual network fixture reports: {reports}; local fixture requests: {:?}; HTTPS interception never disables CSP or native request filtering", server.requests()));
    Ok(())
}

fn reply_network(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done", "appearance": { "base": "sala" },
        "tutorialDone": true, "webSearch": "off" }))?;
    let server = Server::start(b"unused".to_vec()).map_err(|e| e.to_string())?;
    let private = server.origin.replacen("http:", "https:", 1);
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    for setting in ["off", "live"] {
        let markdown = format!(
            r#"![Remote image]({HTTPS_FIXTURE_ORIGIN}/releases/latest)

<p style="background-image:url('{HTTPS_FIXTURE_ORIGIN}/releases/latest')">Remote background</p>
<style>@import url('{HTTPS_FIXTURE_ORIGIN}/releases/latest');@font-face{{font-family:RemoteReply{setting};src:url('{HTTPS_FIXTURE_ORIGIN}/codex.zip?reply={setting}')}}@font-face{{font-family:PrivateReply{setting};src:url('{private}/codex.zip?reply={setting}')}}</style>
<p style="font-family:RemoteReply{setting}">Remote font</p><p style="font-family:PrivateReply{setting}">Private font</p>
<video preload="auto" src="{HTTPS_FIXTURE_ORIGIN}/releases/latest"></video>
<audio preload="auto" src="{HTTPS_FIXTURE_ORIGIN}/releases/latest"></audio>"#
        );
        let conversation = json!({ "id": format!("remote-reply-{setting}"), "title": format!("Remote reply {setting}"),
            "area":"chat", "sessions":{}, "createdAt":1, "updatedAt":2,
            "messages":[{ "id":format!("remote-{setting}"), "role":"assistant", "text":markdown,
                "at":1, "vendor":"claude", "status":"done" }] });
        std::fs::write(folder.join(format!("remote-reply-{setting}.json")), conversation.to_string()).map_err(|e| e.to_string())?;
    }
    let mut app = ctx.launch(no_cli())?;
    app.page.cdp.serve_https_fixture(&server.origin)?;
    app.page.eval("window.__replyNetworkViolations=[];document.addEventListener('securitypolicyviolation',e=>window.__replyNetworkViolations.push({uri:e.blockedURI,directive:e.effectiveDirective}));void 0")?;
    for setting in ["off", "live"] {
        save_web(&mut app.page, setting)?;
        app.page.click(&css(".list .item").has_text(&format!("Remote reply {setting}")))?;
        app.page.wait_visible(&css(".md img[alt='Remote image']"))?;
        app.page.wait_for(
            "the reply's remote image to be blocked by Faber's own policy",
            "window.__replyNetworkViolations.some(e=>e.uri.startsWith('https://visual-fixture.example')&&e.directive==='img-src')",
        )?;
        app.page.wait_for(
            "Faber's own policy to block a private font before its real Origin could authorize the request",
            &format!("window.__replyNetworkViolations.some(e=>e.uri.startsWith({})&&e.directive==='font-src')", json!(private)),
        )?;
        pause(0.3);
        same(app.page.count(&css(".visual-frame"))?, 0, "reading a reply never opens a visual page")?;
        same(
            server.requests(),
            Vec::<String>::new(),
            &format!("Web search {setting} sends no image, background, import, font or media from the main document"),
        )?;
        app.page.eval("window.__replyNetworkViolations=[];void 0")?;
    }
    ctx.notes.push("Remote reply images, style url(), @import, public/private fonts, video and audio caused no fixture traffic with Web search off or Live; no visual was opened.".into());
    Ok(())
}
