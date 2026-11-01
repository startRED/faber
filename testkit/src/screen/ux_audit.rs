//! The walkthrough driver's fake AI selection and its shared end-to-end proof.

use super::{Result, Scenario, ask, claude_chat, ensure, no_cli, start_settings};
use crate::app::Ctx;
use crate::sandbox::{CliSlot, Launch};

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
    Ok(Launch {
        codex: CliSlot::Fake,
        codex_recording: Some(ctx.paths.fixtures.join("setup/codex.setup.json")),
        ..Launch::default()
    })
}

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: "ux audit: both fake AIs answer through the walkthrough launch", run: both_ais }]
}

fn both_ais(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    for (codex, vendor, model, words) in [(false, "claude", "Haiku 5.5", "Rayleigh"), (true, "codex", "GPT-6 Luna", "scatter")] {
        let mut app = ctx.launch(launch(ctx, codex, false)?)?;
        let page = &mut app.page;
        page.wait_for(
            "the fake AI subscription",
            &format!("window.__TAURI_INTERNALS__.invoke('state').then(s=>s.usage?.some(u=>u.vendor==='{vendor}' && !u.billingUnknown && !u.billedPerUse))"),
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
