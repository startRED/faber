//! A project conversation follows its pull request (R200). When the agent opens, readies or queues a pull request, the
//! conversation follows it: Faber reads its checks with the person's own gh now and then, and when they fail or the pull
//! request conflicts with its base, it continues the conversation by itself with one short message, so the agent fixes
//! it. Passing, merged or closed ends the following with a quiet line, and nothing is spent on green.
//!
//! This file is the part with no process and no clock of its own: what a command's output names, what a reading means,
//! and when to look again. `engine/pr_watch.rs` runs it.

use crate::model::{Follow, FollowState};
use crate::project::remote::{CiState, Look, PrState};
use std::time::Duration;

/// The first look after a pull request is bound or changes: CI takes minutes, so sooner would only find it pending.
pub const FIRST: Duration = Duration::from_secs(90);
/// Looks grow apart by half each time nothing changed, up to this.
pub const LONGEST: Duration = Duration::from_secs(600);
/// Nothing changed for this long (a red pull request nobody pushes to): following stops.
pub const QUIET_MS: u64 = 6 * 3600 * 1000;
/// A pull request that still has no check at all after this long has none to follow.
pub const NO_CHECKS_MS: u64 = 15 * 60 * 1000;
/// gh failed this many times in a row: following stops and says so.
pub const MAX_FAILS: u32 = 5;

/// How a command line touches a pull request. The strongest first: a line that pushes and then opens one opens it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `gh pr create`, `ready` or `reopen`: its output is the pull request's address.
    Opens,
    /// Faber's own `node scripts/land.mjs <number>`: it prints `#<number> is queued as …`.
    Queues,
    /// `git push`: a pull request already followed gets new checks.
    Pushes,
}

/// What a command line runs, read by the program position of each of its commands as the model nudge reads checks, so a
/// search for the words (`rg 'gh pr create'`) or an argument holding them runs none of them. A shell launcher's own line
/// (`powershell -Command "…"`, as Codex runs commands on Windows) is read again.
pub fn kind(command: &str) -> Option<Kind> {
    commands(command, 0, false)
        .into_iter()
        .filter_map(|action| match action {
            Action::Follow(kind) => Some(kind),
            Action::Commit | Action::Inspects => None,
        })
        .min()
}

/// Whether a successful command confirms `git commit`, using the same quote-aware parser as pull request following.
/// A successful alternative or pipeline cannot confirm an individual commit, so ambiguous shell chains do not count.
pub fn commits(command: &str) -> bool {
    commands(command, 0, true).contains(&Action::Commit)
}

/// Whether a successful command made a commit: the parser of [`commits`] finds `git commit`, and Git's output
/// shows the commit line (`[main 1a2b3c4] fix`) or, when its options are a plain commit, nothing (`--quiet`). A command
/// that only shows what would be committed prints a status instead, however its options were spelled.
pub fn committed(command: &str, output: &str) -> bool {
    // shortcut: the command and its kept output are the only evidence, so a few odd lines still miscount: an output cut at
    // the 4000 characters a tool keeps lets the options decide alone (an abbreviated `--dry` then counts), a later command
    // printing a summary-shaped line counts, and a quiet commit followed by other output does not. Compare HEAD before and
    // after the reply if a person ever sees one of these.
    let cut = output.chars().count() >= 4000;
    let actions = commands(command, 0, true);
    let printed = || output.lines().any(|line| commit_line(line.trim_start()));
    let commit = actions.contains(&Action::Commit);
    if cut {
        return commit;
    }
    (commit && (output.trim().is_empty() || printed())) || (actions.contains(&Action::Inspects) && printed())
}

/// `[<branch or detached HEAD> (root-commit)? <abbreviated id>] <subject>`.
fn commit_line(line: &str) -> bool {
    let Some(inside) = line.strip_prefix('[').and_then(|rest| rest.split_once(']')).map(|(inside, _)| inside) else {
        return false;
    };
    inside.rsplit_once(' ').is_some_and(|(_, id)| (7..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[derive(PartialEq, Eq)]
enum Action {
    Follow(Kind),
    Commit,
    /// `git commit` whose options seem only to show what would be committed; its output decides.
    Inspects,
}

fn commands(line: &str, depth: u8, require_success: bool) -> Vec<Action> {
    let words = if require_success { crate::nudge::successful_segments(line) } else { crate::nudge::segments(line) };
    words.unwrap_or_default().iter().flat_map(|words| runs(words, depth, require_success)).collect()
}

fn runs(words: &[String], depth: u8, require_success: bool) -> Vec<Action> {
    let assignment =
        |w: &String| w.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_alphanumeric() || c == '_'));
    let words: Vec<&str> = words.iter().skip_while(|w| assignment(w)).map(String::as_str).collect();
    let Some((first, rest)) = words.split_first() else { return vec![] };
    // The first word that is not an option, past the options that take a value (`git -C <folder> push`).
    let verb = |takes: &[&str]| {
        let mut words = rest.iter();
        while let Some(w) = words.next() {
            if takes.contains(w) {
                words.next();
            } else if !w.starts_with('-') {
                return Some(*w);
            }
        }
        None
    };
    let program = crate::nudge::program(first);
    match program.as_str() {
        "gh" => (verb(&["-R", "--repo"]) == Some("pr")
            && rest.iter().skip_while(|w| **w != "pr").nth(1).is_some_and(|w| matches!(*w, "create" | "ready" | "reopen")))
        .then_some(Action::Follow(Kind::Opens))
        .into_iter()
        .collect(),
        "node" => verb(&[])
            .is_some_and(|script| script.replace('\\', "/").rsplit('/').next() == Some("land.mjs"))
            .then_some(Action::Follow(Kind::Queues))
            .into_iter()
            .collect(),
        "git" => match verb(&["-C", "-c"]) {
            Some("push") => vec![Action::Follow(Kind::Pushes)],
            Some("commit") if inspects(rest.iter().skip_while(|w| **w != "commit").skip(1).copied()) => vec![Action::Inspects],
            Some("commit") => vec![Action::Commit],
            _ => vec![],
        },
        "bash" | "sh" | "zsh" | "pwsh" | "powershell" | "cmd" if depth < 2 => {
            let Some(flag) = rest.iter().position(|w| matches!(w.to_ascii_lowercase().as_str(), "-c" | "-lc" | "-ic" | "-command" | "/c"))
            else {
                return vec![];
            };
            // The others run the one word after their flag. `cmd /c` runs the words after it: one quoted word is a line
            // to read again, several are read as they were split, so a quoted argument never turns back into commands.
            let after = &rest[flag + 1..];
            match after {
                [line] => commands(line, depth + 1, require_success),
                _ if program == "cmd" => runs(&after.iter().map(|w| w.to_string()).collect::<Vec<_>>(), depth + 1, require_success),
                [line, ..] => commands(line, depth + 1, require_success),
                [] => vec![],
            }
        }
        _ => vec![],
    }
}

/// Whether `git commit`'s arguments only show what would be committed (`--dry-run`, and `--short`, `--porcelain` and
/// `--long`, `-z` and `--null`, which imply it) or ask for help. An option's value (`-m --short`, `-am -z`) is never read
/// as an option.
fn inspects<'a>(args: impl Iterator<Item = &'a str>) -> bool {
    const TAKES: &[&str] = &[
        "-m",
        "--message",
        "-F",
        "--file",
        "-C",
        "--reuse-message",
        "-c",
        "--reedit-message",
        "-t",
        "--template",
        "--author",
        "--date",
        "--cleanup",
        "--fixup",
        "--squash",
        "--trailer",
        "--pathspec-from-file",
    ];
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg {
            "--" => return false,
            "--dry-run" | "--short" | "--porcelain" | "--long" | "--null" | "--help" => return true,
            _ if TAKES.contains(&arg) => {
                args.next();
            }
            // A cluster of short options (`-az`): one that takes a value ends it, with the value after it or in the next word.
            _ if arg.len() > 1 && arg.starts_with('-') && !arg.starts_with("--") => {
                for (at, c) in arg.char_indices().skip(1) {
                    match c {
                        'z' | 'h' => return true,
                        'm' | 'F' | 'C' | 'c' | 't' => {
                            if at + 1 == arg.len() {
                                args.next();
                            }
                            break;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// `owner/name`.
    pub repo: String,
    pub number: u64,
    pub url: String,
}

fn name_ok(name: &str) -> bool {
    (1..=100).contains(&name.len()) && name != "." && name != ".." && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
}

/// Every `https://github.com/<owner>/<name>/pull/<number>` in a text, in order.
pub fn urls(text: &str) -> Vec<Found> {
    const HOST: &str = "https://github.com/";
    let mut found = vec![];
    let mut rest = text;
    while let Some(at) = rest.find(HOST) {
        rest = &rest[at + HOST.len()..];
        let path: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || "/_-.".contains(*c)).collect();
        let parts: Vec<&str> = path.split('/').collect();
        if let [owner, name, "pull", number, ..] = parts.as_slice()
            && name_ok(owner)
            && name_ok(name)
            && let Ok(number) = number.parse::<u64>()
            && number > 0
        {
            found.push(Found { repo: format!("{owner}/{name}"), number, url: format!("{HOST}{owner}/{name}/pull/{number}") });
        }
    }
    found
}

/// Every `owner/name#number` in a text, the way `gh pr ready` and `gh pr reopen` name the pull request they changed.
pub fn refs(text: &str) -> Vec<Found> {
    text.split(|c: char| c.is_whitespace() || "()[],:;'\"".contains(c))
        .filter_map(|word| {
            let (repo, number) = word.split_once('#')?;
            let (owner, name) = repo.split_once('/')?;
            let number: u64 = number.trim_end_matches('.').parse().ok().filter(|n| *n > 0)?;
            (name_ok(owner) && name_ok(name)).then(|| Found {
                repo: repo.into(),
                number,
                url: format!("https://github.com/{owner}/{name}/pull/{number}"),
            })
        })
        .collect()
}

/// The number in `land.mjs`'s `#<number> is queued as …`.
pub fn queued(output: &str) -> Option<u64> {
    output.lines().rev().find_map(|line| line.trim().strip_prefix('#')?.split_once(" is queued as ")?.0.parse().ok())
}

/// What a command and its output say about a pull request: the one it opened, or the number it queued. `known` finds the
/// address of a number the conversation showed before, since `land.mjs` prints only the number.
pub fn found(command: &str, output: &str, known: impl Fn(u64) -> Option<Found>) -> Option<(Kind, Option<Found>)> {
    let kind = kind(command)?;
    let found = match kind {
        // gh names the pull request before its title, which may name others.
        Kind::Opens => urls(output).pop().or_else(|| refs(output).into_iter().next()),
        Kind::Queues => queued(output).and_then(|n| urls(output).into_iter().rfind(|f| f.number == n).or_else(|| known(n))),
        Kind::Pushes => None,
    };
    Some((kind, found))
}

/// A new pull request to follow.
pub fn start(found: Found, now_ms: u64) -> Follow {
    Follow {
        repo: found.repo,
        number: found.number,
        url: found.url,
        state: FollowState::Pending,
        failed: vec![],
        head: None,
        woke: None,
        held: false,
        kept: false,
        problem: None,
        since: now_ms,
    }
}

/// What a reading asks of the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// Look again later.
    Keep,
    /// Continue the conversation with this message, unless the same thing was already woken for (`Follow::woke`).
    Wake { why: &'static str, key: String, text: String },
    /// Following ended; the line says how.
    End,
}

/// Reads one look into the follow and says what follows. `changed` is whether anything a person sees moved.
pub fn step(f: &mut Follow, look: &Look, now_ms: u64) -> (Next, bool) {
    let before = (f.state, f.failed.clone(), f.head.clone());
    if f.head.as_deref() != Some(look.head.as_str()) {
        f.head = Some(look.head.clone());
        f.since = now_ms;
        f.held = false;
    }
    let short = &look.head[..look.head.len().min(9)];
    let next = match look.state {
        PrState::Merged => {
            f.state = FollowState::Merged;
            Next::End
        }
        PrState::Closed => {
            f.state = FollowState::Closed;
            Next::End
        }
        PrState::Open if look.conflict => {
            f.state = FollowState::Conflict;
            let text = format!(
                "[Faber] Pull request #{} ({}) now has a merge conflict with its base branch. Bring the branch up to date, settle the conflict, run the checks, push, and queue it again the way it was queued before. Keep your answer short. Reply in the language of the conversation.",
                f.number, f.repo
            );
            Next::Wake { why: "wake.conflict", key: look.head.clone(), text }
        }
        PrState::Open => match look.ci.state {
            CiState::Failing => {
                f.state = FollowState::Failing;
                f.failed = look.failures.iter().map(|x| x.name.clone()).collect();
                let lines: Vec<String> = look
                    .failures
                    .iter()
                    .map(|x| {
                        let said = x.said.as_deref().map(|s| format!(": {s}")).unwrap_or_default();
                        let link = x.link.as_deref().map(|l| format!(" ({l})")).unwrap_or_default();
                        format!("- {}{said}{link}", x.name)
                    })
                    .collect();
                let text = format!(
                    "[Faber] Checks failed on pull request #{} ({}) at commit {short}:\n{}\nRead why (for GitHub Actions, `gh run view <run id> --log-failed` shows the failing lines), fix it, run the checks, push, and queue it again the way it was queued before. Keep your answer short. Reply in the language of the conversation.",
                    f.number,
                    f.repo,
                    lines.join("\n")
                );
                Next::Wake { why: "wake.checksFailed", key: look.head.clone(), text }
            }
            CiState::Passing => {
                f.state = FollowState::Passing;
                f.failed.clear();
                Next::End
            }
            CiState::None if now_ms.saturating_sub(f.since) >= NO_CHECKS_MS => {
                f.state = FollowState::Stopped;
                f.problem = Some("follow.noChecks".into());
                Next::End
            }
            CiState::Pending | CiState::None => {
                f.state = FollowState::Pending;
                f.failed.clear();
                Next::Keep
            }
        },
    };
    let next = match next {
        // A commit already woken for, or told to the person, whatever failed on it, waits for the agent's push: a new commit
        // is a new chance. A base that moves under the same commit cannot make it spend a second turn.
        Next::Wake { key, .. } if f.woke.as_deref() == Some(key.as_str()) => Next::Keep,
        other => other,
    };
    let next = if next == Next::Keep && now_ms.saturating_sub(f.since) >= QUIET_MS {
        f.state = FollowState::Stopped;
        f.problem = Some("follow.quiet".into());
        Next::End
    } else {
        next
    };
    let changed = before != (f.state, f.failed.clone(), f.head.clone());
    (next, changed)
}

/// When to look next: soon after a change, then further apart.
pub fn delay(previous: Duration, changed: bool) -> Duration {
    if changed { FIRST } else { (previous + previous / 2).min(LONGEST) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::remote::{Ci, Failure};

    const URL: &str = "https://github.com/acme/app/pull/93";

    fn look(state: PrState, ci: CiState, head: &str) -> Look {
        let failures = if ci == CiState::Failing {
            vec![Failure { name: "gate".into(), link: Some("https://github.com/acme/app/actions/runs/7".into()), said: None }]
        } else {
            vec![]
        };
        Look { state, head: head.into(), ci: Ci { state: ci, head: head.into(), failed: vec![], total: 1 }, conflict: false, failures }
    }

    fn follow() -> Follow {
        start(Found { repo: "acme/app".into(), number: 93, url: URL.into() }, 0)
    }

    #[test]
    fn saved_changes_are_commits_in_program_position_not_quoted_examples_or_follow_commands() {
        for line in [
            "git commit -m fix",
            "git commit --quiet -m fix\n",
            "git -C 'a folder' commit -m fix && git push",
            "pwsh.exe -Command \"git commit -m fix && git push\"",
            "cmd /c git commit -m fix",
            "git commit -m --short",
            "git commit -am -z",
            "git commit -mz",
            "git commit -F msg.txt -- --long",
        ] {
            assert!(commits(line), "{line}");
        }
        for line in [
            "echo \"example && git commit -m demo\"",
            "rg 'git commit' README.md",
            "cmd /c echo \"example & git commit -m demo\"",
            "git push",
            "gh pr create",
            "gh pr ready 2",
            "gh pr reopen 2",
            "gh pr merge 2",
            "node scripts/land.mjs 2",
            "git merge feature",
            "git commit --dry-run",
            "git commit --help",
            "git commit --short",
            "git commit -a --porcelain",
            "git commit --long -m fix",
            "git commit -z",
            "git commit --null",
            "git commit -az -m fix",
            "git commit -m 'unclosed",
            "git diff --quiet || git commit -am fix",
            "false && git commit -m fix || true",
            "git commit -m fix; true",
            "git commit -m fix | cat",
            "echo done # && git commit -m fix",
        ] {
            assert!(!commits(line), "{line}");
        }
        assert_eq!(kind("git commit -m fix && git push && gh pr create"), Some(Kind::Opens));
    }

    #[test]
    fn a_commit_counts_only_when_git_printed_one() {
        for (line, output) in [
            ("git commit -m fix", "[main 1a2b3c4] fix\n 1 file changed, 2 insertions(+)"),
            ("git commit -qm fix", ""),
            ("git commit -Sz --no-gpg-sign -m fix", "[main (root-commit) 1a2b3c4] fix"),
            ("git commit --null --no-null -m fix", "[detached HEAD 0123abcd] fix"),
            ("git commit --mes -z", "[feature/x 1a2b3c4d5e] -z"),
            ("git commit -m fix && git push", "[main 1a2b3c4] fix\nTo github.com:acme/app.git"),
        ] {
            assert!(committed(line, output), "{line}");
        }
        for (line, output) in [
            ("git commit --dry", "On branch main\nChanges to be committed:\n\tmodified:   a.txt"),
            ("git commit --porcelain", "M  a.txt"),
            ("git commit -z", "M  a.txt\0"),
            ("git commit --dry-run -m fix", ""),
            ("git push", "[main 1a2b3c4] fix"),
            ("git commit -m fix", "[not an id] fix"),
        ] {
            assert!(!committed(line, output), "{line}");
        }
    }

    #[test]
    fn a_command_that_opens_or_queues_names_its_pull_request() {
        let made = format!("Creating pull request for me:fix into main in acme/app\n\n{URL}\n");
        let (seen, bound) = found("git push -u origin fix && gh  pr create --fill", &made, |_| None).unwrap();
        assert_eq!(seen, Kind::Opens);
        assert_eq!(bound, Some(Found { repo: "acme/app".into(), number: 93, url: URL.into() }));
        let known = |n| (n == 93).then(|| Found { repo: "acme/app".into(), number: 93, url: URL.into() });
        let queued = "#93 is queued as 1a2b3c4d5 on queue/pr-93. CI checks it, lands it and marks the pull request merged.";
        assert_eq!(found("node scripts/land.mjs 93", queued, known).unwrap().1.map(|f| f.number), Some(93));
        assert_eq!(found("node scripts/land.mjs 94", "#94 is queued as 1a2b on queue/pr-94.", known).unwrap().1, None, "never seen");
        assert_eq!(found("git push", "To github.com:acme/app.git", |_| None), Some((Kind::Pushes, None)));
        assert_eq!(found("gh pr view 93", URL, |_| None), None, "reading a pull request is not opening it");
        assert_eq!(found("cargo test", URL, |_| None), None);
        // Only what runs counts: a search for the words, or an argument holding them, opens nothing.
        assert_eq!(kind("rg 'gh pr create' README.md"), None);
        assert_eq!(kind("echo gh pr create && grep land.mjs AGENTS.md"), None);
        assert_eq!(kind("git log --grep push"), None);
        assert_eq!(kind("powershell.exe -Command \"gh pr create --fill\""), Some(Kind::Opens), "Codex on Windows");
        assert_eq!(kind("cmd /c gh pr create --fill"), Some(Kind::Opens));
        assert_eq!(kind("cmd /c \"gh pr create --fill\""), Some(Kind::Opens));
        assert_eq!(kind("cmd /c echo \"x & gh pr create https://github.com/acme/app/pull/93\""), None, "only echo runs");
        assert_eq!(kind("GH_PROMPT_DISABLED=1 gh -R acme/app pr ready 93"), Some(Kind::Opens));
        assert_eq!(kind("git -C app push origin fix"), Some(Kind::Pushes));
        assert_eq!(kind(r"node .\scripts\land.mjs 93"), Some(Kind::Queues));
        assert_eq!(kind("git push && gh pr create --fill"), Some(Kind::Opens), "the strongest wins");
        // `ready` and `reopen` print the short form.
        let ready = found("gh pr ready 93", "✓ Pull request acme/app#93 is marked as \"ready for review\"", |_| None).unwrap().1;
        assert_eq!(ready.map(|f| f.url), Some(URL.to_string()));
        assert_eq!(
            found("gh pr reopen 93", "✓ Reopened pull request acme/app#93 (Fix acme/app#94)", |_| None).unwrap().1.map(|f| f.number),
            Some(93)
        );
    }

    #[test]
    fn only_a_plain_github_pull_request_address_counts() {
        assert_eq!(urls("see https://github.com/acme/app/pull/new/fix").len(), 0, "git push's suggestion has no number yet");
        assert_eq!(urls("https://github.com/acme/app/pull/0").len(), 0);
        assert_eq!(
            urls("https://github.com/a/b/issues/3 https://github.com/a/b/pull/4#x").iter().map(|f| f.number).collect::<Vec<_>>(),
            [4]
        );
        assert_eq!(urls("https://github.com/../b/pull/4").len(), 0);
    }

    #[test]
    fn red_wakes_once_per_commit_and_green_or_merged_ends() {
        let mut f = follow();
        assert_eq!(step(&mut f, &look(PrState::Open, CiState::Pending, "aaa"), 1).0, Next::Keep);
        let (next, changed) = step(&mut f, &look(PrState::Open, CiState::Failing, "aaa"), 2);
        let Next::Wake { why, key, text } = next else { panic!("{next:?}") };
        assert!(changed && why == "wake.checksFailed" && f.state == FollowState::Failing && f.failed == ["gate"]);
        assert!(text.contains("#93") && text.contains("- gate (https://github.com/acme/app/actions/runs/7)"), "{text}");
        f.woke = Some(key);
        assert_eq!(
            step(&mut f, &look(PrState::Open, CiState::Failing, "aaa"), 3),
            (Next::Keep, false),
            "the same failure is not woken twice"
        );
        assert_eq!(step(&mut f, &look(PrState::Open, CiState::Pending, "bbb"), 4).0, Next::Keep, "the fix runs");
        assert!(
            matches!(step(&mut f, &look(PrState::Open, CiState::Failing, "bbb"), 5).0, Next::Wake { .. }),
            "a new commit, a new chance"
        );
        assert_eq!(step(&mut f, &look(PrState::Open, CiState::Passing, "ccc"), 6).0, Next::End);
        assert_eq!(f.state, FollowState::Passing);
        let mut f = follow();
        assert_eq!(step(&mut f, &look(PrState::Merged, CiState::Passing, "aaa"), 1).0, Next::End);
        assert_eq!(f.state, FollowState::Merged);
        // A commit woken for once is never woken for again, whatever fails on it next (a moving base makes a conflict).
        let mut f = follow();
        let Next::Wake { key, .. } = step(&mut f, &look(PrState::Open, CiState::Failing, "aaa"), 1).0 else { panic!() };
        f.woke = Some(key);
        let mut conflict = look(PrState::Open, CiState::Failing, "aaa");
        conflict.conflict = true;
        assert_eq!(step(&mut f, &conflict, 2).0, Next::Keep);
        assert_eq!(step(&mut f, &look(PrState::Open, CiState::Failing, "aaa"), 3).0, Next::Keep);
    }

    #[test]
    fn a_conflict_wakes_and_quiet_or_no_checks_stop() {
        let mut f = follow();
        let mut conflict = look(PrState::Open, CiState::Passing, "aaa");
        conflict.conflict = true;
        assert!(matches!(step(&mut f, &conflict, 1).0, Next::Wake { why: "wake.conflict", .. }));
        let mut f = follow();
        assert_eq!(step(&mut f, &look(PrState::Open, CiState::None, "aaa"), 1).0, Next::Keep, "checks may not have started yet");
        assert_eq!(step(&mut f, &look(PrState::Open, CiState::None, "aaa"), 1 + NO_CHECKS_MS).0, Next::End);
        assert_eq!(f.problem.as_deref(), Some("follow.noChecks"));
        let mut f = follow();
        step(&mut f, &look(PrState::Open, CiState::Pending, "aaa"), 1);
        assert_eq!(step(&mut f, &look(PrState::Open, CiState::Pending, "aaa"), 1 + QUIET_MS).0, Next::End);
        assert_eq!((f.state, f.problem.as_deref()), (FollowState::Stopped, Some("follow.quiet")));
    }

    struct Gh(Vec<crate::project::remote::Call>, String);

    impl crate::project::remote::Runner for Gh {
        fn run(&mut self, call: &crate::project::remote::Call) -> Result<crate::project::remote::Reply, crate::project::remote::Lost> {
            self.0.push(call.clone());
            Ok(crate::project::remote::Reply { code: 0, stdout: self.1.clone() })
        }
        fn neutral(&self) -> std::path::PathBuf {
            "/faber/neutral".into()
        }
    }

    #[test]
    fn gh_is_asked_from_faber_s_own_folder_and_its_checks_are_read() {
        let head = "a".repeat(40);
        let view = serde_json::json!({ "number": 93, "url": URL, "isDraft": false, "state": "OPEN", "headRefName": "fix",
            "headRefOid": head, "baseRefName": "main", "isCrossRepository": false, "mergeable": "CONFLICTING",
            "statusCheckRollup": [
                { "__typename": "CheckRun", "name": "gate", "status": "COMPLETED", "conclusion": "FAILURE",
                  "detailsUrl": "https://github.com/acme/app/actions/runs/7/job/8" },
                { "__typename": "StatusContext", "context": "queue", "state": "FAILURE", "description": "rust failed\non\twindows",
                  "targetUrl": "javascript:alert(1)" },
                { "__typename": "CheckRun", "name": "lint", "status": "IN_PROGRESS", "conclusion": null } ] });
        let mut gh = Gh(vec![], view.to_string());
        let look = crate::project::remote::look(&mut gh, "acme/app", 93).unwrap();
        assert_eq!(gh.0[0].cwd, std::path::PathBuf::from("/faber/neutral"), "never inside the person's repository");
        assert_eq!(gh.0[0].args[..5], ["pr", "view", "93", "--repo", "acme/app"]);
        assert!(look.conflict && look.ci.state == CiState::Failing && look.state == PrState::Open);
        assert_eq!(look.failures[0].link.as_deref(), Some("https://github.com/acme/app/actions/runs/7/job/8"));
        assert_eq!(
            (look.failures[1].name.as_str(), look.failures[1].said.as_deref(), &look.failures[1].link),
            ("queue", Some("rust failed on windows"), &None)
        );
        assert!(crate::project::remote::look(&mut gh, "acme/app --web", 93).is_err(), "a repository is only owner/name");
    }

    #[test]
    fn looks_grow_apart_while_nothing_changes() {
        assert_eq!(delay(LONGEST, true), FIRST);
        assert_eq!(delay(FIRST, false), Duration::from_secs(135));
        assert_eq!(delay(Duration::from_secs(500), false), LONGEST);
    }
}
