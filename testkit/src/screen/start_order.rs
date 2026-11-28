//! The window shows on the screen's first painted frame, not by the shell's failsafe, and the host answers within a bound.
//! The Electron shell times the reveal even when a test run has its window shown already; the Tauri test build builds its
//! window shown and writes no reveal line, so there the scenario says so in a note and checks nothing more.

use super::{Result, Scenario, ensure, no_cli, start_settings};
use crate::app::{Ctx, Shell};
use crate::sandbox::Launch;
use std::time::{Duration, Instant};

pub(super) const NAME: &str = "the window shows on its first painted frame, before the host answers (empty profile)";

/// The host's first answer on an empty test profile. Worst seen on the maintainer's PC: 1.85 s (2026-11-26, Electron test
/// build, window built at 1.69 s). The docs ask for 3 s median and 10 s worst on a real profile; an empty one is tighter.
const HOST_BOUND: Duration = Duration::from_secs(5);
/// How long the second start holds the host's answers (`--test-host-hold`): past the shell's 3 s failsafe.
const HELD: Duration = Duration::from_millis(4500);

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: NAME, run: shows_on_paint }]
}

/// `name=123ms` in a startup line, as milliseconds.
fn mark(line: &str, name: &str) -> Option<u64> {
    line.split_whitespace().find_map(|part| part.strip_prefix(name)?.strip_prefix('=')?.strip_suffix("ms")?.parse().ok())
}

/// The newest start line once there are `count`, waiting for the shell to write it (it writes once both the reveal and
/// the host's first answer are known).
fn start_line(ctx: &Ctx, count: usize) -> Result<String> {
    let log = ctx.sb.data.join("startup.log");
    let began = Instant::now();
    loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        let lines: Vec<_> = text.lines().filter(|line| line.contains(" shown ")).collect();
        if lines.len() >= count {
            return Ok(lines[count - 1].to_string());
        }
        ensure(began.elapsed() < Duration::from_secs(40), || format!("startup.log has no start line {count} after 40 s: {text:?}"))?;
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn shows_on_paint(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let app = ctx.launch(no_cli())?;
    if Shell::chosen() != Shell::Electron {
        ctx.notes.push("Tauri test build: its window is built shown and writes no reveal line; run with FABER_TEST_SHELL=electron".into());
        return ctx.close(app.id);
    }
    let line = start_line(ctx, 1)?;
    ctx.close(app.id)?;
    ensure(line.contains("by=paint"), || format!("the window must show on its first frame, not by the failsafe: {line}"))?;
    let host = Duration::from_millis(mark(&line, "host").unwrap_or(u64::MAX));
    ensure(host <= HOST_BOUND, || format!("the host answered after {host:?}, over {HOST_BOUND:?}: {line}"))?;
    // A slow engine start, as on a real profile: the host answers nothing for 4.5 s, past the shell's 3 s failsafe. The
    // window still shows on its first frame, before the host answers. Before 2026-11-26 the screen waited for the host's
    // state before saying it painted: here that read `shown=4645ms by=paint host=4515ms` (a test window is on screen from
    // the start, so the failsafe never fires in a test; on a person's PC it showed the window at 3.3 s).
    let held = ctx.launch(Launch { args: vec![format!("--test-host-hold={}", HELD.as_millis())], ..no_cli() })?;
    let slow = start_line(ctx, 2)?;
    ctx.close(held.id)?;
    ensure(slow.contains("by=paint"), || format!("with a slow host the window must still show on its first frame: {slow}"))?;
    let (shown, host) = (mark(&slow, "shown").unwrap_or(u64::MAX), mark(&slow, "host").unwrap_or(0));
    ensure(shown < host, || format!("the window must show before the host answers: {slow}"))?;
    ctx.notes.push(format!("start: {line}; held host: {slow}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::mark;

    #[test]
    fn startup_marks_read_as_milliseconds() {
        let line = "1700000000 shown window=250ms shown=590ms by=paint host=410ms";
        assert_eq!((mark(line, "window"), mark(line, "shown"), mark(line, "host")), (Some(250), Some(590), Some(410)));
        assert_eq!(mark("1 shown window=250ms shown=none by=none host=none", "host"), None);
    }
}
