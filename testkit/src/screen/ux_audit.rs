//! The walkthrough driver's fake AI selection and its shared end-to-end proof.

use super::{Result, Scenario, ask, claude_chat, ensure, no_cli, start_settings};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};
use serde_json::json;

/// Uses only the existing sandbox and recordings; the marker is a synthetic sign-in, never a credential.
pub fn launch(ctx: &Ctx, codex: bool, no_ai: bool) -> Result<Launch> {
    if codex && no_ai {
        return Err("--codex cannot be combined with --no-ai".into());
    }
    if no_ai {
        return Ok(no_cli());
    }
    if !codex {
        return Ok(claude_chat(ctx));
    }
    std::fs::write(ctx.sb.state.join("codex.signed-in"), "true").map_err(|e| e.to_string())?;
    Ok(Launch { codex: CliSlot::Fake, codex_recording: Some(ctx.paths.fixtures.join("setup/codex.setup.json")), ..Launch::default() })
}

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "ux audit: both fake AIs answer through the walkthrough launch", run: both_ais },
        Scenario { name: "catalog compact: a focused heading stays below Back and Escape returns to its card", run: compact_catalog },
        Scenario { name: EXTERNAL_LINKS, run: external_links },
    ]
}

fn compact_catalog(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "fr")?;
    let mut app = ctx.launch(no_cli())?;
    app.page.click(&css("[data-catalog-tab]"))?;
    for (width, height) in [(1920, 1080), (1280, 800), (720, 480)] {
        super::shell::set_size(&mut app.page, width, height)?;
        app.page.wait_for("the catalog cards", "!!document.querySelector('[data-catalog-card]')")?;
        let card = app.page.eval("document.querySelector('[data-catalog-card]')?.dataset.catalogCard")?;
        app.page.click(&css(&format!("[data-catalog-card={card}]")))?;
        app.page.wait_visible(&css(".catalog-detail h1"))?;
        ctx.shot(&mut app, &format!("catalog-focused-heading-{width}x{height}"))?;
        ensure(app.page.eval("(() => { const h = document.querySelector('.catalog-detail h1'), b = document.querySelector('.catalog-return'); const r = h.getBoundingClientRect(); return document.activeElement === h && r.top >= b.getBoundingClientRect().bottom && r.bottom <= innerHeight; })()")? == json!(true), || "the focused heading is obscured by Back or outside the compact viewport".into())?;
        app.page.press("Escape")?;
        super::same(app.page.eval("document.activeElement?.dataset.catalogCard")?, card, "Escape returns focus to the original card")?;
    }
    ctx.shot(&mut app, "catalog-compact-returned-card")?;
    Ok(())
}

pub const EXTERNAL_LINKS: &str = "ux audit: open-link records URLs in the hidden sandbox without starting a browser";

const BROWSERS: [&str; 6] = ["chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe"];

fn browser(row: &crate::proctree::ProcRow) -> bool {
    BROWSERS.contains(&row.name.to_ascii_lowercase().as_str())
}

/// A new browser process Faber did not start: outside the app's tree, and started by the system's scheduler (an update
/// task) or by a browser that was already running before the links were opened (its own tab or helper process; on CI,
/// 10/11/2026, a Firefox open on the runner started one mid-check).
fn not_ours(
    row: &crate::proctree::ProcRow,
    rows: &[crate::proctree::ProcRow],
    app_tree: &[crate::proctree::ProcRow],
    before: &[crate::proctree::ProcRow],
) -> bool {
    !crate::proctree::same_process(row, app_tree)
        && rows.iter().find(|parent| parent.pid == row.parent).is_some_and(|parent| {
            parent.born <= row.born
                && (matches!(parent.name.to_ascii_lowercase().as_str(), "svchost.exe" | "taskhostw.exe" | "taskeng.exe" | "services.exe")
                    || (browser(parent) && crate::proctree::same_process(parent, before)))
        })
}

fn external_links(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    let before = crate::proctree::snapshot()?;
    let urls = ["https://example.com/a?q=%22test%22#part", "http://example.org/"];
    for url in urls {
        app.page.eval(&format!("window.__TAURI_INTERNALS__.invoke('open_link', {{url:{}}}).then(()=>true)", serde_json::json!(url)))?;
    }
    let log = ctx.sb.data.join("blocked-opens.jsonl");
    let text = std::fs::read_to_string(&log).map_err(|e| format!("{}: {e}", log.display()))?;
    let recorded: Vec<String> = text.lines().map(serde_json::from_str).collect::<std::result::Result<_, _>>().map_err(|e| e.to_string())?;
    super::same(recorded, urls.map(str::to_string).to_vec(), "recorded external links")?;
    for url in ["file:///C:/Windows", "javascript:alert(1)"] {
        let answer = app
            .page
            .eval(&format!("window.__TAURI_INTERNALS__.invoke('open_link', {{url:{}}}).then(()=>null,e=>e)", serde_json::json!(url)))?;
        super::same(answer, serde_json::json!("error.link.web"), "invalid external link")?;
    }
    super::same(std::fs::read_to_string(log).map_err(|e| e.to_string())?, text, "invalid links leave no record")?;
    for _ in 0..10 {
        let rows = crate::proctree::snapshot()?;
        let app_tree = crate::proctree::descendants(&rows, app.pid);
        let new_browsers: Vec<_> = rows
            .iter()
            .filter(|p| browser(p) && !crate::proctree::same_process(p, &before) && !not_ours(p, &rows, &app_tree, &before))
            .map(|p| (p, rows.iter().find(|parent| parent.pid == p.parent).map(|parent| parent.name.as_str())))
            .collect();
        ensure(new_browsers.is_empty(), || format!("open-link started browser processes (process, parent name): {new_browsers:?}"))?;
        super::pause(0.1);
    }
    ctx.close(app.id)
}

fn both_ais(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    for (codex, vendor, model, words) in [(true, "codex", "GPT-6 Luna", "scatter"), (false, "claude", "Haiku 5.5", "Rayleigh")] {
        let mut app = ctx.launch(launch(ctx, codex, false)?)?;
        let page = &mut app.page;
        page.wait_for(
            "the fake AI subscription",
            &format!("(window.faberShell ?? window.__TAURI_INTERNALS__).invoke('state').then(s=>s.usage?.some(u=>u.vendor==='{vendor}' && !u.billingUnknown && !u.billedPerUse))"),
        )?;
        let new = page.named("button", "side.new")?;
        page.click(&new)?;
        ask(page, "In one sentence: why is the sky blue?")?;
        let answer = page.named("article", "answer.label")?;
        page.wait_visible(&answer.clone().css(".actions"))?;
        page.wait_visible(&answer.clone().has_text(words))?;
        let text = page.inner_text(&answer)?;
        ensure(text.contains(model), || format!("{vendor} must answer with its own recorded model: {text}"))?;
        ctx.close(app.id)?;
    }
    ensure(ctx.records()?.iter().any(|c| c.vendor == "codex" && c.args.first().is_some_and(|arg| arg == "app-server")), || {
        "the walkthrough never started the fake Codex app server".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Paths;
    use crate::sandbox::child_env;
    use serde_json::Value;

    #[test]
    fn only_browsers_the_scheduler_or_an_open_browser_started_outside_the_app_tree_are_ignored() {
        use crate::proctree::{ProcRow, descendants};
        let row = |pid, parent, name: &str| ProcRow { pid, parent, name: name.into(), born: u64::from(pid) };
        for name in ["svchost.exe", "TASKHOSTW.EXE", "taskeng.exe", "services.exe"] {
            let rows = [row(1, 0, "faber.exe"), row(2, 0, name), row(3, 2, "firefox.exe")];
            assert!(not_ours(&rows[2], &rows, &descendants(&rows, 1), &[]));
            let rows = [row(1, 0, "faber.exe"), row(2, 1, name), row(3, 2, "firefox.exe")];
            assert!(!not_ours(&rows[2], &rows, &descendants(&rows, 1), &[]));
        }
        // A scheduler process that appeared after the browser may only have reused its parent's pid.
        let rows = [row(1, 0, "faber.exe"), ProcRow { born: 9, ..row(2, 0, "svchost.exe") }, row(3, 2, "firefox.exe")];
        assert!(!not_ours(&rows[2], &rows, &descendants(&rows, 1), &[]));
        let rows =
            [row(1, 0, "faber.exe"), row(2, 0, "other.exe"), row(3, 1, "firefox.exe"), row(4, 2, "firefox.exe"), row(5, 99, "firefox.exe")];
        let tree = descendants(&rows, 1);
        for browser in &rows[2..] {
            assert!(!not_ours(browser, &rows, &tree, &[]));
        }
        // A browser already open before the links were opened starts its own tab process: not Faber's. Faber's own
        // browser, or one that is new itself, still counts, whoever its child is.
        let open = row(2, 0, "firefox.exe");
        let rows = [row(1, 0, "faber.exe"), open.clone(), row(3, 2, "firefox.exe")];
        assert!(not_ours(&rows[2], &rows, &descendants(&rows, 1), std::slice::from_ref(&open)));
        assert!(!not_ours(&rows[2], &rows, &descendants(&rows, 1), &[]), "a browser that is new itself is not an excuse");
        let rows = [row(1, 0, "faber.exe"), row(2, 1, "firefox.exe"), row(3, 2, "firefox.exe")];
        assert!(!not_ours(&rows[2], &rows, &descendants(&rows, 1), &rows[1..2]), "a browser Faber started is never excused");
        let rows = [row(1, 0, "faber.exe"), open.clone(), row(3, 2, "notepad.exe"), row(4, 3, "firefox.exe")];
        assert!(!not_ours(&rows[3], &rows, &descendants(&rows, 1), std::slice::from_ref(&open)), "only a browser parent excuses");
    }

    #[test]
    fn ux_audit_codex_uses_only_the_fake_sandbox() {
        let paths = Paths::discover().unwrap();
        let mut ctx = Ctx::new(&paths).unwrap();
        let launch = launch(&ctx, true, false).unwrap();
        assert_eq!(launch.codex, CliSlot::Fake);
        assert_eq!(launch.claude, CliSlot::Off);
        assert_eq!(std::fs::read_to_string(ctx.sb.state.join("codex.signed-in")).unwrap(), "true");
        let env = child_env(&[], &ctx.sb, &launch, &paths.fake_cli);
        let config: Value = serde_json::from_str(&env["FABER_CLI_CODEX"]).unwrap();
        assert_eq!(config["command"], paths.fake_cli.to_string_lossy().as_ref());
        assert_eq!(config["prefix"][0], "--faber-recording");
        assert_eq!(config["prefix"][1], paths.fixtures.join("setup/codex.setup.json").to_string_lossy().as_ref());
        assert_eq!(env["CODEX_HOME"], ctx.sb.profile.join("fake-home").to_string_lossy());
        assert_eq!(env["HOME"], ctx.sb.profile.to_string_lossy());
        assert_eq!(env["FABER_CLI_CLAUDE"], "");
        assert!(super::launch(&ctx, true, true).is_err());
        let off = super::launch(&ctx, false, true).unwrap();
        assert_eq!((off.claude, off.codex), (CliSlot::Off, CliSlot::Off));
        let default = super::launch(&ctx, false, false).unwrap();
        assert_eq!((default.claude, default.codex), (CliSlot::Fake, CliSlot::Off));
        assert_eq!(default.recording, Some(paths.fixtures.join("claude/chat.jsonl")));
        ctx.finish().unwrap();
    }
}
