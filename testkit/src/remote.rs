//! A stateful stand-in for git and gh against one remote repository. It answers the calls of
//! `faber_engine::project::remote` without a process, a network or a login, and it can fail the way the real
//! tools do: not installed, logged out, a rejected push, or a link that drops after the effect happened.

use faber_engine::project::remote::{Call, Lost, Reply, Runner, Tool};
use serde_json::{Value, json};

/// How one effect ends as the caller sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Ok,
    /// The tool answers with a non-zero exit and the remote is untouched.
    Rejected,
    /// The link drops before the remote saw anything.
    LostBefore,
    /// The remote took it, then the link dropped before the answer.
    LostAfter,
}

#[derive(Debug, Clone)]
pub struct FakePr {
    pub number: u64,
    pub head_ref: String,
    pub head_oid: String,
    pub base: String,
    pub draft: bool,
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
    /// The effective push URL of every remote name.
    pub url: String,
    /// Command-bearing keys found in the repository's own config.
    pub local_config: Vec<String>,
    /// What `git log -p` prints for the commits that are not on the remote yet.
    pub outgoing: String,
    /// The branch on the remote.
    pub remote_head: Option<String>,
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
            url: "https://example.invalid/owner/repo.git".into(),
            local_config: vec![],
            outgoing: "+fn main() {}\n".into(),
            remote_head: None,
            ancestors: vec![],
            push: Delivery::Ok,
            create: Delivery::Ok,
            merge: Delivery::Ok,
            reads_down: false,
            down_after_effect: false,
            prs: vec![],
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
    pub fn pr_json(pr: &FakePr, remote: &FakeRemote) -> Value {
        json!({
            "number": pr.number,
            "url": format!("https://example.invalid/{}/pull/{}", remote.slug, pr.number),
            "isDraft": pr.draft,
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
        self.calls.iter().filter(|c| c.tool == Tool::Git && plain(&c.args).first() == Some(&"push")).collect()
    }

    pub fn gh_calls(&self, verb: &str) -> Vec<&Call> {
        self.calls.iter().filter(|c| c.tool == Tool::Gh && plain(&c.args).get(..2) == Some(&["pr", verb][..])).collect()
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
            ["config", ..] if self.local_config.is_empty() => fail(1),
            ["config", ..] => ok(self.local_config.join("\n")),
            ["remote", "get-url", "--push", _] => ok(format!("{}\n", self.url)),
            ["log", ..] => ok(self.outgoing.clone()),
            ["ls-remote", ..] => {
                if let Some(down) = self.read() {
                    return down;
                }
                let branch = args.last().copied().unwrap_or_default();
                match &self.remote_head {
                    Some(oid) => ok(format!("{oid}\t{branch}\n")),
                    None => ok(""),
                }
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
                if matches!(delivery, Delivery::Ok | Delivery::LostAfter) {
                    self.remote_head = Some(oid);
                }
                self.after_effect();
                match delivery {
                    Delivery::Ok => ok(""),
                    Delivery::Rejected => fail(1),
                    Delivery::LostBefore | Delivery::LostAfter => Err(Lost::Interrupted),
                }
            }
            _ => fail(129),
        }
    }

    fn gh(&mut self, args: &[&str]) -> Result<Reply, Lost> {
        match args {
            ["--version"] => ok("gh version 2.0.0\n"),
            ["auth", "status", ..] if self.logged_out => fail(1),
            ["auth", "status", ..] => ok(""),
            ["api", "user", ..] => ok(format!("{}\n", self.login)),
            ["repo", "view", ..] => ok(format!("{}\n", self.slug)),
            ["pr", "list", ..] => {
                if let Some(down) = self.read() {
                    return down;
                }
                let head = flag(args, "--head").unwrap_or_default();
                let list: Vec<Value> = self.prs.iter().filter(|p| p.head_ref == head).map(|p| Self::pr_json(p, self)).collect();
                ok(Value::Array(list).to_string())
            }
            ["pr", "create", ..] => {
                let head = flag(args, "--head").unwrap_or_default().to_string();
                let base = flag(args, "--base").unwrap_or_default().to_string();
                let delivery = self.create;
                if matches!(delivery, Delivery::Ok | Delivery::LostAfter) {
                    let number = self.prs.iter().map(|p| p.number).max().unwrap_or(0) + 1;
                    let draft = args.contains(&"--draft");
                    let oid = self.remote_head.clone().unwrap_or_default();
                    self.prs.push(FakePr { number, head_ref: head, head_oid: oid, base, draft, state: "OPEN".into(), checks: vec![] });
                }
                self.after_effect();
                match delivery {
                    Delivery::Ok => ok(format!("https://example.invalid/{}/pull/{}\n", self.slug, self.prs.last().map_or(0, |p| p.number))),
                    Delivery::Rejected => fail(1),
                    Delivery::LostBefore | Delivery::LostAfter => Err(Lost::Interrupted),
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
                if matches!(delivery, Delivery::Ok | Delivery::LostAfter) {
                    pr.state = "MERGED".into();
                }
                self.after_effect();
                match delivery {
                    Delivery::Ok => ok(""),
                    Delivery::Rejected => fail(1),
                    Delivery::LostBefore | Delivery::LostAfter => Err(Lost::Interrupted),
                }
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
