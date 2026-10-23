//! Drives a hidden, sandboxed test build over stdin, one JSON operation per line, and answers one JSON line each.
//! Operations: eval, click, fill, press, shot, call (any DevTools method), memory, cpu, quit.
//!
//! Options: `--lang <code>` (default pt-BR), `--first-run` (no settings but the language, so the first run shows),
//! `--no-ai` (no CLI at all; the default is the fake Claude replaying `claude/chat.jsonl`), `--build` (builds the fake CLI
//! and the test app first, at the build priority). `scripts/first-user.mjs` uses it for the first-user review.
//! `--project-demo` seeds the existing isolated Demo repository and uses the fake chat replay, without first run.

use faber_testkit::{
    app::{Ctx, Paths},
    page::css,
    screen::{claude_chat, no_cli, projects_ui, start_settings},
    testbuild::build_for_run,
};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    if has("--project-demo") && (has("--first-run") || has("--no-ai")) {
        return Err("--project-demo cannot be combined with --first-run or --no-ai".into());
    }
    let lang = args.iter().position(|a| a == "--lang").and_then(|i| args.get(i + 1)).map_or("pt-BR", String::as_str);
    let mut paths = Paths::discover()?;
    // The example lives in `<target>/debug/examples`; the fake CLI is built beside the runner, in `<target>/debug`.
    let target = paths.target.parent().ok_or("the test target has no parent folder")?.to_path_buf();
    paths.fake_cli = target.join(format!("debug/fake-cli{}", std::env::consts::EXE_SUFFIX));
    if has("--build") {
        build_for_run(&paths, 2)?;
    }
    let mut ctx = Ctx::new(&paths)?;
    if has("--first-run") {
        ctx.write_settings(&json!({ "language": lang }))?;
    } else {
        start_settings(&ctx, lang)?;
    }
    let project = if has("--project-demo") { Some(projects_ui::fixture(&mut ctx)?) } else { None };
    let mut launch = if has("--no-ai") { no_cli() } else { claude_chat(&ctx) };
    if let Some(project) = project {
        launch.git_bin = project.git_bin;
    }
    let mut app = ctx.launch(launch)?;
    println!("READY");
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let cmd: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if cmd["op"] == "quit" {
            break;
        }
        let s = cmd["value"].as_str().unwrap_or("");
        let result: Result<Value, String> = match cmd["op"].as_str() {
            Some("eval") => app.page.eval(s),
            Some("click") => app.page.click(&css(s)).map(|()| json!(true)),
            Some("fill") => app.page.fill(&css(cmd["selector"].as_str().unwrap_or("textarea")), s).map(|()| json!(true)),
            Some("press") => app.page.press(s).map(|()| json!(true)),
            Some("shot") => app.page.screenshot(std::path::Path::new(s)).map(|()| json!(true)),
            Some("call") => app.page.cdp.call(s, cmd["params"].clone()),
            Some("memory") => ctx.memory_mb(app.id).map(|m| json!(m)),
            Some("cpu") => ctx.cpu_ms(app.id).map(|m| json!(m)),
            _ => Err("unknown operation".into()),
        };
        println!(
            "{}",
            match result {
                Ok(v) => json!({"ok":v}),
                Err(e) => json!({"error":e}),
            }
        );
        io::stdout().flush().map_err(|e| e.to_string())?;
    }
    ctx.finish()?;
    println!("CLEAN");
    Ok(())
}
