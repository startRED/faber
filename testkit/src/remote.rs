//! A stateful stand-in for git and gh against one remote repository. It answers the calls of
//! `faber_engine::project::remote` without a process, a network or a login, and it can fail the way the real
//! tools do: not installed, logged out, a rejected push, a link that drops after the effect happened, or a tool
//! that had to be killed.
//!
//! It is as strict as the real tools about what matters to safety: the outgoing commits are visible to a scan only if
//! the scan asks git for them the way git needs (every blob as text, merge commits against each parent, excluding
//! what the live remote advertises and nothing else); the repository's config comes back by scope; a network read
//! that runs inside the repository is recorded as such.

use faber_engine::project::remote::{Call, Lost, Reply, Runner, Tool};
use serde_json::{Value, json};

/// How one effect ends as the caller sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Ok,
    /// The tool answers with a non-zero exit and the remote is untouched.
    Rejected,
    /// The link drops before the remote saw anything: the tool exits with an error.
    LostBefore,
    /// The remote took it, then the link dropped: the tool exits with an error.
    LostAfter,
    /// The tool had to be killed before it said anything, and the remote saw nothing.
    TimedOutBefore,
    /// The tool had to be killed, and the remote took it anyway.
    TimedOutAfter,
}

impl Delivery {
    fn lands(self) -> bool {
        matches!(self, Delivery::Ok | Delivery::LostAfter | Delivery::TimedOutAfter)
    }
    fn answer(self) -> Result<Reply, Lost> {
        match self {
            Delivery::Ok => ok(""),
            Delivery::Rejected => fail(1),
            Delivery::LostBefore | Delivery::LostAfter => fail(128),
            Delivery::TimedOutBefore | Delivery::TimedOutAfter => Err(Lost::Interrupted),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FakePr {
    pub number: u64,
    pub head_ref: String,
    pub head_oid: String,
    pub base: String,
    pub draft: bool,
    /// From a fork of the repository rather than from it.
    pub cross: bool,
    /// `OPEN`, `MERGED` or `CLOSED`, as gh prints it.
    pub state: String,
    /// Entries of `statusCheckRollup`.
    pub checks: Vec<Value>,
}

pub struct FakeRemote {
    /// Every call, in order, with the `-c` safety options still on.
    pub calls: Vec<Call>,
    pub git_missing: bool,
    pub gh_missing: bool,
    pub logged_out: bool,
    pub login: String,
    pub slug: String,
    /// What `git remote get-url --push --all` prints, one per line.
    pub push_urls: Vec<String>,
    /// `(scope, key)` of the repository's config, as `git config --list --show-scope --name-only` prints it.
    pub config: Vec<(String, String)>,
    /// The exit code of `git config --list`.
    pub config_code: i32,
    /// What `git log -p` prints for the commits the live remote does not have, if it was asked for them properly.
    pub outgoing: String,
    /// The exit code of `git log`.
    pub log_code: i32,
    /// The own branch on the remote.
    pub remote_head: Option<String>,
    /// Other heads and tags the remote advertises.
    pub live_oids: Vec<String>,
    /// `(older, newer)` pairs for which `merge-base --is-ancestor` succeeds.
    pub ancestors: Vec<(String, String)>,
    pub push: Delivery,
    pub create: Delivery,
    pub merge: Delivery,
    /// Every read that needs the network fails.
    pub reads_down: bool,
    /// The network goes down right after the next effect, so only a later read can tell what happened.
    pub down_after_effect: bool,
    pub prs: Vec<FakePr>,
    /// `pr list --head` returns every PR, as a loose gh match would, so the engine has to filter by branch itself.
    pub loose_head: bool,
}

impl Default for FakeRemote {
    fn default() -> Self {
        FakeRemote {
            calls: vec![],
            git_missing: false,
            gh_missing: false,
            logged_out: false,
            login: "owner".into(),
            slug: "owner/repo".into(),
            push_urls: vec![Self::URL.into()],
            config: vec![
                ("system".into(), "core.autocrlf".into()),
                ("global".into(), "credential.helper".into()),
                ("local".into(), "core.bare".into()),
            ],
            config_code: 0,
            outgoing: "+fn main() {}\n".into(),
            log_code: 0,
            remote_head: None,
            live_oids: vec![],
            ancestors: vec![],
            push: Delivery::Ok,
            create: Delivery::Ok,
            merge: Delivery::Ok,
            reads_down: false,
            down_after_effect: false,
            prs: vec![],
            loose_head: false,
        }
    }
}

/// The call without the `-c key=value` safety options git is always started with.
fn plain(args: &[String]) -> Vec<&str> {
    let mut out = vec![];
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "-c" {
            it.next();
        } else {
            out.push(arg.as_str());
        }
    }
    out
}

/// The value of `--name value` or `--name=value`.
fn flag<'a>(args: &[&'a str], name: &str) -> Option<&'a str> {
    args.iter().enumerate().find_map(|(i, a)| match a.strip_prefix(name) {
        Some("") => args.get(i + 1).copied(),
        Some(rest) => rest.strip_prefix('='),
        None => None,
    })
}

fn ok(stdout: impl Into<String>) -> Result<Reply, Lost> {
    Ok(Reply { code: 0, stdout: stdout.into() })
}
fn fail(code: i32) -> Result<Reply, Lost> {
    Ok(Reply { code, stdout: String::new() })
}

impl FakeRemote {
    /// The push URL the tests grant.
    pub const URL: &'static str = "https://example.invalid/owner/repo.git";

    pub fn pr_json(pr: &FakePr, remote: &FakeRemote) -> Value {
        json!({
            "number": pr.number,
            "url": format!("https://example.invalid/{}/pull/{}", remote.slug, pr.number),
            "isDraft": pr.draft,
            "isCrossRepository": pr.cross,
            "state": pr.state,
            "headRefName": pr.head_ref,
            "headRefOid": pr.head_oid,
            "baseRefName": pr.base,
            "statusCheckRollup": pr.checks,
        })
    }

    /// A completed or running check run, as gh prints it.
    pub fn check_run(status: &str, conclusion: &str) -> Value {
        json!({ "__typename": "CheckRun", "name": format!("{status}-{conclusion}"), "status": status, "conclusion": conclusion })
    }

    pub fn pushes(&self) -> Vec<&Call> {
        self.git_calls("push")
    }

    pub fn git_calls(&self, verb: &str) -> Vec<&Call> {
        self.calls.iter().filter(|c| c.tool == Tool::Git && plain(&c.args).first() == Some(&verb)).collect()
    }

    pub fn gh_calls(&self, verb: &str) -> Vec<&Call> {
        self.calls.iter().filter(|c| c.tool == Tool::Gh && plain(&c.args).get(..2) == Some(&["pr", verb][..])).collect()
    }

    /// Every heads and tags the remote advertises, the own branch included.
    pub fn advertised(&self) -> Vec<String> {
        let mut all: Vec<String> = self.remote_head.iter().cloned().chain(self.live_oids.iter().cloned()).collect();
        all.sort();
        all.dedup();
        all
    }

    /// Whether any call carried an option that forces, deletes or bypasses protection.
    pub fn used_force(&self) -> bool {
        self.calls.iter().any(|c| {
            plain(&c.args).iter().any(|a| {
                a.starts_with('+')
                    || a.contains(":+")
                    || ["--force", "-f", "--force-with-lease", "--delete-branch", "--admin", "--delete", "--mirror"]
                        .iter()
                        .any(|f| a == f || a.starts_with(&format!("{f}=")))
            })
        })
    }

    fn after_effect(&mut self) {
        if self.down_after_effect {
            self.reads_down = true;
        }
    }

    fn read(&self) -> Option<Result<Reply, Lost>> {
        self.reads_down.then_some(Err(Lost::Interrupted))
    }

    fn git(&mut self, args: &[&str]) -> Result<Reply, Lost> {
        match args {
            ["config", "--list", "--show-scope", "--name-only"] => {
                if self.config_code != 0 {
                    return fail(self.config_code);
                }
                ok(self.config.iter().map(|(scope, key)| format!("{scope}\t{key}\n")).collect::<String>())
            }
            ["remote", "get-url", "--push", "--all", _] => ok(self.push_urls.iter().map(|u| format!("{u}\n")).collect::<String>()),
            ["log", rest @ ..] => {
                if self.log_code != 0 {
                    return Ok(Reply { code: self.log_code, stdout: String::new() });
                }
                // The secret is in the output only if the scan asked for every blob as text, merges against each
                // parent, messages, and excluded exactly what the live remote advertises.
                let split = rest.iter().position(|a| *a == "--not").unwrap_or(rest.len());
                let (head, mut excluded) = (&rest[..split], rest.get(split + 1..).unwrap_or(&[]).to_vec());
                excluded.sort_unstable();
                let wanted = self.advertised();
                let thorough = ["--text", "-m", "--no-renames", "--ignore-missing", "--format=%B"].iter().all(|f| head.contains(f));
                let exact = excluded == wanted.iter().map(String::as_str).collect::<Vec<_>>();
                ok(if thorough && exact { self.outgoing.clone() } else { "(nothing to see)\n".to_string() })
            }
            ["ls-remote", "--heads", "--tags", _url] => {
                if let Some(down) = self.read() {
                    return down;
                }
                let mut out = String::new();
                if let Some(oid) = &self.remote_head {
                    out.push_str(&format!("{oid}\trefs/heads/faber/m1\n"));
                }
                for oid in &self.live_oids {
                    out.push_str(&format!("{oid}\trefs/tags/v1\n"));
                }
                ok(out)
            }
            ["merge-base", "--is-ancestor", older, newer] => {
                if self.ancestors.iter().any(|(a, b)| a == older && b == newer) {
                    ok("")
                } else {
                    fail(1)
                }
            }
            ["push", ..] => {
                let spec = args.iter().find(|a| a.contains(":refs/heads/")).copied().unwrap_or_default();
                let oid = spec.split(':').next().unwrap_or_default().to_string();
                let delivery = self.push;
                if delivery.lands() {
                    self.remote_head = Some(oid);
                }
                self.after_effect();
                delivery.answer()
            }
            _ => fail(129),
        }
    }

    fn gh(&mut self, args: &[&str]) -> Result<Reply, Lost> {
        match args {
            ["auth", "status", ..] if self.logged_out => fail(1),
            ["auth", "status", ..] => ok(""),
            ["api", "user", ..] => ok(format!("{}\n", self.login)),
            ["repo", "view", ..] => ok(format!("{}\n", self.slug)),
            ["pr", "list", ..] => {
                if let Some(down) = self.read() {
                    return down;
                }
                let head = flag(args, "--head").unwrap_or_default();
                // Without `--state all` gh lists open PRs only, so a merged or closed one would not be seen.
                let all = flag(args, "--state") == Some("all");
                let list: Vec<Value> = self
                    .prs
                    .iter()
                    .filter(|p| (self.loose_head || p.head_ref == head) && (all || p.state == "OPEN"))
                    .map(|p| Self::pr_json(p, self))
                    .collect();
                ok(Value::Array(list).to_string())
            }
            ["pr", "create", ..] => {
                let head = flag(args, "--head").unwrap_or_default().to_string();
                let base = flag(args, "--base").unwrap_or_default().to_string();
                let delivery = self.create;
                if delivery.lands() {
                    let number = self.prs.iter().map(|p| p.number).max().unwrap_or(0) + 1;
                    let draft = args.contains(&"--draft");
                    let oid = self.remote_head.clone().unwrap_or_default();
                    self.prs.push(FakePr {
                        number,
                        head_ref: head,
                        head_oid: oid,
                        base,
                        draft,
                        cross: false,
                        state: "OPEN".into(),
                        checks: vec![],
                    });
                }
                self.after_effect();
                match delivery {
                    Delivery::Ok => ok(format!("https://example.invalid/{}/pull/{}\n", self.slug, self.prs.last().map_or(0, |p| p.number))),
                    other => other.answer(),
                }
            }
            ["pr", "view", number, ..] => {
                if let Some(down) = self.read() {
                    return down;
                }
                match self.prs.iter().find(|p| p.number.to_string() == *number) {
                    Some(pr) => ok(Self::pr_json(pr, self).to_string()),
                    None => fail(1),
                }
            }
            ["pr", "ready", number, ..] => {
                match self.prs.iter_mut().find(|p| p.number.to_string() == *number) {
                    Some(pr) => pr.draft = false,
                    None => return fail(1),
                }
                ok("")
            }
            ["pr", "merge", number, ..] => {
                let guard = flag(args, "--match-head-commit").map(str::to_string);
                let delivery = self.merge;
                let Some(pr) = self.prs.iter_mut().find(|p| p.number.to_string() == *number) else { return fail(1) };
                if guard.as_deref() != Some(pr.head_oid.as_str()) || pr.draft || pr.state != "OPEN" {
                    return fail(1);
                }
                if delivery.lands() {
                    pr.state = "MERGED".into();
                }
                self.after_effect();
                delivery.answer()
            }
            _ => fail(1),
        }
    }
}

impl Runner for FakeRemote {
    fn run(&mut self, call: &Call) -> Result<Reply, Lost> {
        self.calls.push(call.clone());
        let args = plain(&call.args).into_iter().map(str::to_string).collect::<Vec<_>>();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        match call.tool {
            Tool::Git if self.git_missing => Err(Lost::Missing),
            Tool::Gh if self.gh_missing => Err(Lost::Missing),
            Tool::Git => self.git(&args),
            Tool::Gh => self.gh(&args),
        }
    }
}

#[cfg(test)]
mod tests;
