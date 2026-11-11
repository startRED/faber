//! Faber's "call a helper" tool (R161): a project's agent hands one piece of work or a review to a selected vendor, which
//! runs it in a short-lived session of its own in the same folder and with the same permission mode. Both CLIs call the tool
//! in their own way (Claude Code through an MCP server the host answers over stdio, Codex as a dynamic tool), and both get
//! the helper's final report back as the tool's result.

use crate::model::Vendor;
use crate::route::Profile;
use serde_json::{Value, json};

/// The name both drivers give the call, whatever the CLI calls it, so the screen and the engine know one name.
pub const TOOL: &str = "Helper";
/// The tool as Codex knows it, and as Faber's MCP server lists it to Claude Code.
pub const NAME: &str = "helper";
/// The tool as Claude Code names it: `mcp__<server>__<tool>`.
pub const CLAUDE_NAME: &str = "mcp__faber__helper";
/// Faber's own MCP server for Claude Code: an `sdk` server lives in the host, which answers it over stdio.
pub const CLAUDE_MCP_CONFIG: &str = r#"{"mcpServers":{"faber":{"type":"sdk","name":"faber"}}}"#;
/// Helpers one conversation may run at once; each is a CLI process.
pub const MAX_PER_CONVERSATION: usize = 3;

/// Whether a session offers the tool, or is a helper itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Helper {
    #[default]
    Off,
    /// A project's own agent: it may call a helper.
    Offers,
    /// A helper: it leaves no history in the CLI and cannot call a helper of its own.
    Is,
    /// An independent reviewer: helper restrictions without a team channel.
    Reviewer,
    /// A quick question on the side (R250): it may only read, search and list the working folder, and leaves no history.
    Aside,
}

const DESCRIPTION: &str = "Split by kind of work, not size. For each piece give title, brief, job, kind, difficulty and scope: Faber picks the right model and keeps both AIs on the team when available. Many files, logs or searches: read; a self-contained change: build; at a hard point ask for advice (advise) before spending effort; finished work: review. Start a dependent piece after its prerequisite reports: helpers share this folder and see its files. Do a piece yourself only when it needs a stronger model than any helper can be. The brief must stand alone: goal, files and what done looks like. Reports are short; longer reports are saved in a file. helper({to, answer}) answers and waits; {to, tell} instructs immediately; {to, wait:true} waits for a question or report.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "to": { "type": "string" },
            "answer": { "type": "string", "maxLength": 1000 },
            "tell": { "type": "string", "maxLength": 1000 },
            "wait": { "type": "boolean" },
            "title": { "type": "string", "description": "A few words naming the piece of work, shown to the person." },
            "brief": { "type": "string", "description": "Everything the helper needs to do the work on its own." },
            "job": { "type": "string", "enum": ["read", "build", "review", "advise"], "description": "Read, change, review, or read-only advice; default review." },
            "difficulty": { "type": "string", "enum": ["easy", "medium", "hard"], "description": "easy: sort, extract, summarise, find a file, read a log, rename; medium: everyday document, scoped code, routine investigation; hard: cross-file feature, unknown bug, migration, costly subtle mistake. Default medium." },
            "scope": { "type": "string", "enum": ["tight", "open"], "default": "tight", "description": "tight: named bug/file/function, spec, one deliverable; open: explore and decide, many files/sources, migration, major refactor, architecture, deep research, long unattended run." },
            "kind": { "type": "string", "enum": ["chat", "writing", "code", "math", "research", "image", "other"], "description": "chat: talk/facts; writing: texts, documents, slides, spreadsheets, translation, plans; code: programs, debugging, review; math: calculations/proofs; research: analysis, comparisons, explanations, advice; image: make/edit pictures; other. Default code." }
        },
        "additionalProperties": false
    })
}

/// The tool as a Codex `dynamicTools` entry.
pub fn codex_tool() -> Value {
    json!({ "type": "function", "name": NAME, "description": DESCRIPTION, "inputSchema": schema() })
}

pub const TEAM_NAME: &str = "team";
pub const TEAM_CLAUDE_NAME: &str = "mcp__faber__team";
pub const NO_ANSWER: &str = "No answer: decide yourself and say what you assumed in your report.";
pub const TEAM_LIMIT: usize = 6;
pub const TEAM_LENGTH: usize = 1000;

/// Both drivers offer this only to helpers, which cannot delegate or request approvals.
pub fn team_tool() -> Value {
    json!({"name": TEAM_NAME, "description": "who lists the team; ask asks the lead and waits; tell sends a short note to a named teammate. Decide yourself whenever possible. At most 6 messages, 1000 characters each.",
        "inputSchema": {"type":"object", "properties": {
            "do":{"type":"string","enum":["who","ask","tell"]},
            "to":{"type":"string"}, "text":{"type":"string","maxLength":1000}},
            "required":["do"], "additionalProperties":false}})
}

pub fn team_codex_tool() -> Value {
    let mut tool = team_tool();
    tool["type"] = json!("function");
    tool
}

/// Paths named in a brief, without reading the folder or sending its contents.
pub fn brief_files(brief: &str) -> Vec<String> {
    let mut files = Vec::new();
    for word in brief.split_whitespace() {
        let path = word
            .trim_start_matches(['`', '\'', '"', '(', ')', '[', ']', ',', ';'])
            .trim_end_matches(['`', '\'', '"', '(', ')', '[', ']', ',', ';', '.']);
        let path = path
            .rsplit_once(':')
            .filter(|(_, line)| !line.is_empty() && line.chars().all(|c| c.is_ascii_digit()))
            .map_or(path, |(file, _)| file);
        let named = word.starts_with(['`', '\'', '"'])
            || path.contains(['/', '\\'])
            || path.starts_with('.')
            || matches!(path, "Makefile" | "Dockerfile" | "Justfile" | "README" | "LICENSE");
        let extension =
            path.rsplit_once('.').is_some_and(|(_, ext)| !ext.is_empty() && ext.len() <= 8 && ext.chars().all(char::is_alphanumeric));
        if (named || extension)
            && !path.trim_matches(['.', '/', '\\']).is_empty()
            && !path.contains("://")
            && !files.iter().any(|f| f == path)
        {
            files.push(path.to_string());
        }
    }
    files
}

/// What Faber's MCP server answers to one message Claude Code sends it.
#[derive(Debug, PartialEq)]
pub enum Mcp {
    /// The answer goes back at once (`mcp_response`).
    Answer(Value),
    /// A call of one of its tools (`NAME`, `crate::browser::NAME`, `crate::sibling::NAME`, `crate::verify::NAME`, `crate::map::NAME` or one of `crate::progress` and `crate::toolbox`): the answer waits for the work. `id` is the tool
    /// call's own id, which names the call on screen.
    Call { tool: &'static str, id: String, rpc: Value, input: Value },
}

/// The tools Faber's MCP server holds for one session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tools {
    pub helper: bool,
    pub team: bool,
    pub browse: bool,
    /// Conversations the agent starts by itself (`crate::sibling`).
    pub conversation: bool,
    /// The project's checks (`crate::verify`).
    pub verify: bool,
    /// The project's map (`crate::map`).
    pub map: bool,
    /// The plan, the status and the closing box (`crate::progress`), and with them the read-only `quota`, `catalog` and
    /// `memory_search`, and `offer_bigger` (`crate::toolbox`): a project's own agent in any permission mode, since they read nothing of
    /// the folder.
    pub progress: bool,
}

impl Tools {
    pub fn any(self) -> bool {
        self.helper || self.team || self.browse || self.conversation || self.verify || self.map || self.progress
    }
}

/// Faber's MCP server, which holds the tools the session offers. Claude Code sends its JSON-RPC messages inside
/// `mcp_message` control requests.
pub fn mcp(message: &Value, tools: Tools) -> Mcp {
    let rpc = message["id"].clone();
    let answer = |result: Value| Mcp::Answer(json!({ "jsonrpc": "2.0", "id": rpc, "result": result }));
    match message["method"].as_str().unwrap_or_default() {
        "initialize" => answer(json!({
            "protocolVersion": message["params"]["protocolVersion"],
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "faber", "version": env!("CARGO_PKG_VERSION") }
        })),
        "tools/list" => {
            let mut list = vec![];
            if tools.team {
                list.push(team_tool());
            }
            if tools.helper {
                list.push(json!({ "name": NAME, "description": DESCRIPTION, "inputSchema": schema() }));
            }
            if tools.progress {
                list.push(crate::review::mcp_tool());
            }
            if tools.browse {
                list.push(crate::browser::mcp_tool());
            }
            if tools.conversation {
                list.push(crate::sibling::mcp_tool());
            }
            if tools.verify {
                list.push(crate::verify::mcp_tool());
            }
            if tools.map {
                list.push(crate::map::mcp_tool());
            }
            if tools.progress {
                list.extend(crate::progress::mcp_tools());
                list.extend(crate::toolbox::mcp_tools());
            }
            answer(json!({ "tools": list }))
        }
        "tools/call" if let Some(tool) = called(&message["params"]["name"], tools) => {
            let meta = &message["params"]["_meta"]["claudecode/toolUseId"];
            // The tool call's id came in `_meta` on every CLI recorded (2.1.296); without it a fresh id tells calls apart, since the
            // JSON-RPC ids start again in every process and a resumed conversation would reuse them.
            let id = meta.as_str().map(String::from).unwrap_or_else(|| format!("{tool}-{}", uuid::Uuid::new_v4().simple()));
            Mcp::Call { tool, id, rpc, input: message["params"]["arguments"].clone() }
        }
        // A notification needs no answer, but the control request that carried it does.
        _ if rpc.is_null() => answer(json!({})),
        _ => Mcp::Answer(json!({ "jsonrpc": "2.0", "id": rpc, "error": { "code": -32601, "message": "Faber has no such method" } })),
    }
}

/// The offered tool a call names, if any.
fn called(name: &Value, tools: Tools) -> Option<&'static str> {
    match name.as_str()? {
        TEAM_NAME if tools.team => Some(TEAM_NAME),
        NAME if tools.helper => Some(NAME),
        crate::review::NAME if tools.progress => Some(crate::review::NAME),
        crate::browser::NAME if tools.browse => Some(crate::browser::NAME),
        crate::sibling::NAME if tools.conversation => Some(crate::sibling::NAME),
        crate::verify::NAME if tools.verify => Some(crate::verify::NAME),
        crate::map::NAME if tools.map => Some(crate::map::NAME),
        crate::progress::PLAN if tools.progress => Some(crate::progress::PLAN),
        crate::progress::STATUS if tools.progress => Some(crate::progress::STATUS),
        crate::progress::WRAP_UP if tools.progress => Some(crate::progress::WRAP_UP),
        crate::toolbox::SETTINGS if tools.progress => Some(crate::toolbox::SETTINGS),
        crate::toolbox::QUOTA if tools.progress => Some(crate::toolbox::QUOTA),
        crate::toolbox::CATALOG if tools.progress => Some(crate::toolbox::CATALOG),
        crate::toolbox::MEMORY_SEARCH if tools.progress => Some(crate::toolbox::MEMORY_SEARCH),
        crate::toolbox::CREATIVE if tools.progress => Some(crate::toolbox::CREATIVE),
        crate::toolbox::OFFER_BIGGER if tools.progress => Some(crate::toolbox::OFFER_BIGGER),
        _ => None,
    }
}

/// The answer to a tool call, once the helper is done.
pub fn mcp_result(rpc: &Value, ok: bool, text: &str, pictures: &[String]) -> Value {
    let mut content = vec![json!({ "type": "text", "text": text })];
    content.extend(pictures.iter().map(|p| json!({ "type": "image", "data": p, "mimeType": "image/jpeg" })));
    json!({ "jsonrpc": "2.0", "id": rpc, "result": { "content": content, "isError": !ok } })
}

/// The same answer as a Codex dynamic tool's result: the text, then each JPEG picture (base64) as an image the model sees.
pub fn dynamic_result(ok: bool, text: &str, pictures: &[String]) -> Value {
    let mut items = vec![json!({ "type": "inputText", "text": text })];
    items.extend(pictures.iter().map(|p| json!({ "type": "inputImage", "imageUrl": format!("data:image/jpeg;base64,{p}") })));
    json!({ "success": ok, "contentItems": items })
}

/// The other vendor, used when an independent review is available.
pub fn other(vendor: Vendor) -> Vendor {
    match vendor {
        Vendor::Claude => Vendor::Codex,
        Vendor::Codex => Vendor::Claude,
        // Not yet: nothing runs on Gemini, so its helper would be Claude's.
        Vendor::Gemini => Vendor::Claude,
    }
}

/// The cheapest profile that clears the hard coding bar (39), or the medium bar (32) in economy mode.
pub fn profile(candidates: &[Profile], vendor: Vendor, economy: bool) -> Option<Profile> {
    let bar: u8 = if economy { 32 } else { 39 };
    candidates
        .iter()
        .filter(|p| p.vendor == vendor && p.agent >= bar)
        .min_by(|a, b| a.cost.total_cmp(&b.cost))
        .or_else(|| candidates.iter().filter(|p| p.vendor == vendor).max_by_key(|p| p.agent))
        .copied()
}

/// Normalized at the tool boundary, shared by selection and permission checks.
#[derive(Debug, Clone, Copy)]
pub struct Piece {
    pub job: &'static str,
    pub kind: &'static str,
    pub difficulty: &'static str,
    pub scope: &'static str,
}

impl Piece {
    pub fn of(input: &Value) -> Self {
        Self {
            job: ["read", "build", "review", "advise"].into_iter().find(|v| Some(*v) == input["job"].as_str()).unwrap_or("review"),
            kind: crate::route::KINDS.into_iter().find(|v| Some(*v) == input["kind"].as_str()).unwrap_or("code"),
            difficulty: crate::route::LEVELS.into_iter().find(|v| Some(*v) == input["difficulty"].as_str()).unwrap_or("medium"),
            scope: crate::route::SCOPES.into_iter().find(|v| Some(*v) == input["scope"].as_str()).unwrap_or("tight"),
        }
    }

    pub fn read_only(self) -> bool {
        matches!(self.job, "read" | "advise")
    }
}

pub const NO_ADVISER: &str = "You are already the strongest model available for this piece; decide yourself.";

/// Select a helper using the same project bars and quota factors as Auto; makers includes the lead.
pub fn select(
    piece: Piece,
    lead: Option<Profile>,
    lead_vendor: Vendor,
    candidates: &[Profile],
    makers: &[Vendor],
    factor: &dyn Fn(&Profile) -> crate::route::Factor,
    economy: bool,
) -> Option<Profile> {
    let Piece { job, kind, difficulty, scope } = piece;
    if job == "review" {
        let other = other(lead_vendor);
        return profile(candidates, if candidates.iter().any(|p| p.vendor == other) { other } else { lead_vendor }, economy);
    }
    let level = if economy {
        match difficulty {
            "hard" => "medium",
            _ => "easy",
        }
    } else {
        difficulty
    };
    let rung = if job == "advise" {
        if economy { "hard" } else { "expert" }
    } else if level == "hard" && scope == "open" {
        "expert"
    } else {
        level
    };
    let bar = if job == "read" { (24, 15) } else { crate::route::bar(kind, rung, true) };
    let clearing: Vec<Profile> = candidates.iter().copied().filter(|p| p.iq >= bar.0 && p.agent >= bar.1).collect();
    let mut eligible: Vec<Profile> = clearing
        .iter()
        .copied()
        .filter(|p| match job {
            "build" => lead.is_none_or(|l| p.iq <= l.iq && p.agent <= l.agent),
            "advise" => lead.is_some_and(|l| p.iq >= l.iq && p.agent >= l.agent && (p.iq > l.iq || p.agent > l.agent)),
            _ => true,
        })
        .collect();
    // A build above the lead's own level still gets a model that can do it (as before the mix): the cap only orders the choice.
    if job == "build" && eligible.is_empty() {
        eligible = clearing;
    }
    let other = other(lead_vendor);
    let mixed: Vec<Profile> = eligible.iter().copied().filter(|p| p.vendor == other).collect();
    let pool = if !economy && !makers.contains(&other) && !mixed.is_empty() { &mixed } else { &eligible };
    if matches!(job, "read" | "advise") {
        pool.iter()
            .min_by(|a, b| {
                let price = |p: &Profile| {
                    let f = factor(p);
                    (p.cost as f64 + f.read) * f.w * if job == "read" { f.m.max(1.0) } else { f.m }
                };
                price(a).total_cmp(&price(b))
            })
            .copied()
    } else {
        crate::route::choose_paced_scoped(kind, level, "machine", scope, 1.0, true, pool, factor).map(|r| r.profile)
    }
}

/// What the helper's session is asked: the agent's own title and brief, with a bounded team channel.
pub fn prompt(input: &Value) -> Option<String> {
    worker_prompt(input, true)
}

pub fn worker_prompt(input: &Value, team: bool) -> Option<String> {
    let brief = input["brief"].as_str().map(str::trim).filter(|b| !b.is_empty())?;
    let title = input["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).unwrap_or("A piece of work");
    let decisions = if team {
        "Decide whatever you reasonably can yourself; ask the lead with team only for a choice that is not yours and costly to get wrong. Tell a teammate directly when your work changes what it must do; team who lists them."
    } else {
        "Decide yourself and state any assumptions in your report."
    };
    let report = if Piece::of(input).job == "advise" {
        "Finish with a report of about 1,200 characters at most. Give advice only; change no file. Report the approach you recommend, the main risk, and what to check."
    } else {
        "Finish with a report of about 1,200 characters at most, in this order: the outcome in one or two sentences; files changed; checks run and their results; anything you could not do or assumed."
    };
    Some(format!(
        "You are a helper. Another AI agent working in this folder handed you the piece of work below. You cannot see its \
         conversation. {decisions} {report}\n\nTask: {title}\n\n{brief}"
    ))
}

/// A message the person wrote to a helper at work (R161 step 5), as the helper reads it: it still reports to the agent.
pub fn from_person(text: &str) -> String {
    format!(
        "The person following your work writes to you:

{}

Take it into account, and still finish with your short report for the agent.",
        text.trim()
    )
}

/// What the agent reads under the report when the person wrote to the helper: words the agent itself never saw.
pub fn told_note(told: &[String]) -> String {
    let quoted: Vec<String> = told.iter().map(|t| format!("\"{}\"", t.trim())).collect();
    format!(
        "

While it worked, the person wrote to the helper: {}.",
        quoted.join("; ")
    )
}

/// What goes back to the agent when the helper ends: its report, plus what it was not allowed to do, which the agent can do
/// itself, where the person approves it.
pub fn report(ok: bool, text: &str, refused: &[String], error: Option<&str>) -> (bool, String) {
    let mut out = text.trim().to_string();
    if !ok || out.is_empty() {
        let why = error.filter(|e| !e.is_empty()).unwrap_or("it ended without a report");
        out = if out.is_empty() {
            format!("The helper stopped: {why}.")
        } else {
            format!("{out}\n\nThe helper stopped before it finished: {why}.")
        };
    }
    if !refused.is_empty() {
        out.push_str(&format!(
            "\n\nThe helper was not allowed to use these without the person's approval, so it did not: {}.",
            refused.join(", ")
        ));
    }
    (ok && !text.trim().is_empty(), out)
}

/// Keep the full report beside verify logs and bound the text returned to the agent.
pub fn short_report(folder: &std::path::Path, text: &str, ok: bool) -> String {
    if text.chars().count() <= 2000 {
        return text.to_string();
    }
    let prefix: String = text.chars().take(1600).collect();
    let end = prefix
        .rfind("\n\n")
        .or_else(|| {
            prefix
                .char_indices()
                .rev()
                .find(|(i, c)| matches!(c, '.' | '?' | '!') && prefix[i + 1..].starts_with(char::is_whitespace))
                .map(|(i, _)| i + 1)
        })
        .unwrap_or(prefix.len());
    let excerpt = prefix[..end].trim_end();
    let status = if ok { "" } else { "\nThe helper stopped or failed." };
    let failed = || format!("{excerpt}{status}\nThe full report could not be saved; it is visible in the helper view.");
    let faber = folder.join(".faber");
    let dir = faber.join("verify");
    let linked = |path: &std::path::Path| std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    if linked(&faber)
        || linked(&dir)
        || std::fs::create_dir_all(&dir).is_err()
        || linked(&faber)
        || linked(&dir)
        || !matches!((folder.canonicalize(), dir.canonicalize()), (Ok(root), Ok(real)) if real.starts_with(&root))
    {
        return failed();
    }
    if let Ok(mut ignore) = std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(".gitignore")) {
        let _ = std::io::Write::write_all(&mut ignore, b"*\n");
    }
    let name = format!("helper-{}.log", uuid::Uuid::new_v4().simple());
    if std::fs::write(dir.join(&name), text).is_err() {
        return failed();
    }
    let mut logs: Vec<_> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_file())
                && e.file_name().to_str().is_some_and(|n| n.starts_with("helper-") && n.ends_with(".log"))
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.file_name() == name.as_str(), e.path())))
        .collect();
    logs.sort_by(|a, b| b.cmp(a));
    for (_, _, path) in logs.into_iter().skip(20) {
        let _ = std::fs::remove_file(path);
    }
    format!("{excerpt}{status}\nFull report: .faber/verify/{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_jobs_and_keeps_long_reports() {
        let all = crate::route::PROFILES;
        let neutral = |_: &Profile| crate::route::Factor::NEUTRAL;
        let pick = |job, difficulty, vendor, candidates: &[Profile]| {
            select(
                Piece { job, kind: "code", difficulty, scope: "tight" },
                None,
                vendor,
                candidates,
                &[Vendor::Claude, Vendor::Codex],
                &neutral,
                false,
            )
            .unwrap()
        };
        assert_eq!(pick("read", "easy", Vendor::Claude, &all).model, "gpt-6.1-terra");
        assert!(pick("build", "easy", Vendor::Claude, &all).agent < pick("build", "hard", Vendor::Claude, &all).agent);
        assert_eq!(pick("review", "medium", Vendor::Claude, &all), profile(&all, Vendor::Codex, false).unwrap());
        for vendor in [Vendor::Claude, Vendor::Codex] {
            let only: Vec<_> = all.iter().copied().filter(|p| p.vendor == vendor).collect();
            for job in ["read", "build", "review"] {
                assert_eq!(pick(job, "medium", vendor, &only).vendor, vendor);
            }
        }
        let scarce =
            |p: &Profile| crate::route::Factor { m: if p.vendor == Vendor::Claude { 10.0 } else { 1.0 }, ..crate::route::Factor::NEUTRAL };
        assert_eq!(
            select(Piece::of(&json!({"job":"read"})), None, Vendor::Claude, &all, &[Vendor::Claude, Vendor::Codex], &scarce, false)
                .unwrap()
                .vendor,
            Vendor::Codex
        );
        let dir = crate::workspace::TestRoot::new();
        let full = format!("{}.\n\n{}", "a".repeat(1000), "b".repeat(1500));
        let short = short_report(dir.path(), &full, false);
        assert!(short.contains("helper stopped or failed"));
        let path = short.lines().last().unwrap().strip_prefix("Full report: ").unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join(path)).unwrap(), full);
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, "file").unwrap();
        let missing = short_report(&blocked, &full, false);
        assert!(missing.chars().count() < 2000 && missing.contains("could not be saved") && missing.contains("stopped or failed"));
    }

    #[test]
    fn helper_reports_keep_only_the_newest_twenty_logs() {
        let root = crate::workspace::TestRoot::new();
        let dir = root.path().join(".faber/verify");
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..23 {
            let path = dir.join(format!("helper-{i:032}.log"));
            std::fs::write(&path, "old report").unwrap();
            let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(100 + i);
            std::fs::File::options().write(true).open(path).unwrap().set_times(std::fs::FileTimes::new().set_modified(time)).unwrap();
        }
        for name in ["verify-001.log", "other.log", "helper-note.txt"] {
            std::fs::write(dir.join(name), "keep").unwrap();
        }
        std::fs::create_dir(dir.join("helper-directory.log")).unwrap();
        let full = "é".repeat(2500);
        let short = short_report(root.path(), &full, true);
        let path = short.lines().last().unwrap().strip_prefix("Full report: ").unwrap();
        assert_eq!(std::fs::read_to_string(root.path().join(path)).unwrap(), full);
        for i in 0..23 {
            assert_eq!(dir.join(format!("helper-{i:032}.log")).exists(), i >= 4, "log {i}");
        }
        for name in ["verify-001.log", "other.log", "helper-note.txt"] {
            assert_eq!(std::fs::read_to_string(dir.join(name)).unwrap(), "keep");
        }
        assert!(dir.join("helper-directory.log").is_dir());
        for _ in 0..25 {
            let short = short_report(root.path(), &full, true);
            let path = short.lines().last().unwrap().strip_prefix("Full report: ").unwrap();
            assert_eq!(std::fs::read_to_string(root.path().join(path)).unwrap(), full);
        }
        assert_eq!(
            std::fs::read_dir(&dir)
                .unwrap()
                .map(Result::unwrap)
                .filter(|e| e.file_type().unwrap().is_file()
                    && e.file_name().to_string_lossy().starts_with("helper-")
                    && e.path().extension().is_some_and(|x| x == "log"))
                .count(),
            20
        );
    }

    #[test]
    fn long_report_ends_at_a_sentence_boundary() {
        let dir = crate::workspace::TestRoot::new();
        for punctuation in ['?', '!'] {
            let sentence = format!("{}{} ", "é".repeat(99), punctuation);
            let full = sentence.repeat(25);
            let short = short_report(dir.path(), &full, true);
            assert!(short.lines().next().unwrap().ends_with(punctuation), "{short}");
        }
    }

    #[test]
    fn the_mcp_server_lists_one_tool_and_hands_its_call_on_with_the_tool_call_id() {
        let mcp = |m: &Value| {
            mcp(m, Tools { team: false, helper: true, browse: false, conversation: false, verify: false, map: false, progress: false })
        };
        let init = json!({"method":"initialize","params":{"protocolVersion":"2025-11-25"},"jsonrpc":"2.0","id":0});
        let Mcp::Answer(a) = mcp(&init) else { panic!() };
        assert_eq!((a["id"].clone(), a["result"]["protocolVersion"].clone()), (json!(0), json!("2025-11-25")));
        let Mcp::Answer(list) = mcp(&json!({"method":"tools/list","jsonrpc":"2.0","id":1})) else { panic!() };
        assert_eq!(list["result"]["tools"][0]["name"], NAME);
        assert_eq!(
            mcp(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})),
            Mcp::Answer(json!({"jsonrpc":"2.0","id":null,"result":{}}))
        );
        let call = json!({"method":"tools/call","params":{"name":"helper","arguments":{"title":"Check","brief":"Look."},
            "_meta":{"claudecode/toolUseId":"toolu_1","progressToken":2}},"jsonrpc":"2.0","id":2});
        assert_eq!(
            mcp(&call),
            Mcp::Call { tool: NAME, id: "toolu_1".into(), rpc: json!(2), input: json!({"title":"Check","brief":"Look."}) }
        );
        let Mcp::Answer(unknown) = mcp(&json!({"method":"tools/call","params":{"name":"other"},"jsonrpc":"2.0","id":3})) else { panic!() };
        assert_eq!(unknown["error"]["code"], -32601);
    }

    // R161 step 7: the browser sits beside the helper, and a session lists and answers only the tools it offers.
    #[test]
    fn pictures_reach_the_agent_after_the_text_in_both_clis_and_none_is_only_text() {
        let pictures = ["AQI=".to_string(), "Aw==".to_string()];
        let claude = mcp_result(&json!(7), true, "Checked", &pictures);
        assert_eq!(
            claude["result"]["content"],
            json!([
                { "type": "text", "text": "Checked" },
                { "type": "image", "data": "AQI=", "mimeType": "image/jpeg" },
                { "type": "image", "data": "Aw==", "mimeType": "image/jpeg" }
            ])
        );
        let codex = dynamic_result(true, "Checked", &pictures[..1]);
        assert_eq!(
            codex,
            json!({ "success": true, "contentItems": [
                { "type": "inputText", "text": "Checked" },
                { "type": "inputImage", "imageUrl": "data:image/jpeg;base64,AQI=" }
            ] })
        );
        assert_eq!(mcp_result(&json!(1), false, "No", &[])["result"]["content"], json!([{ "type": "text", "text": "No" }]));
        assert_eq!(dynamic_result(false, "No", &[])["contentItems"], json!([{ "type": "inputText", "text": "No" }]));
    }

    #[test]
    fn the_mcp_server_holds_only_the_tools_the_session_offers() {
        let list = |tools| match mcp(&json!({"method":"tools/list","jsonrpc":"2.0","id":1}), tools) {
            Mcp::Answer(a) => {
                a["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect::<Vec<_>>()
            }
            Mcp::Call { .. } => panic!(),
        };
        assert_eq!(
            list(Tools { team: false, helper: true, browse: true, conversation: false, verify: false, map: false, progress: false }),
            ["helper", "browse"]
        );
        assert_eq!(
            list(Tools { team: false, helper: false, browse: true, conversation: false, verify: false, map: false, progress: false }),
            ["browse"]
        );
        assert_eq!(
            list(Tools { team: false, helper: false, browse: false, conversation: false, verify: true, map: true, progress: false }),
            ["verify", "map"]
        );
        let map = json!({"method":"tools/call","params":{"name":"map","arguments":{}},"jsonrpc":"2.0","id":5});
        assert!(matches!(mcp(&map, Tools { verify: true, map: true, ..Tools::default() }), Mcp::Call { tool: crate::map::NAME, .. }));
        assert!(matches!(mcp(&map, Tools { verify: true, ..Tools::default() }), Mcp::Answer(_)), "a map that is not offered is refused");
        let call = json!({"method":"tools/call","params":{"name":"browse","arguments":{"url":"https://a.org"},
            "_meta":{"claudecode/toolUseId":"toolu_2"}},"jsonrpc":"2.0","id":4});
        assert_eq!(
            mcp(&call, Tools { team: false, helper: false, browse: true, conversation: false, verify: false, map: false, progress: false }),
            Mcp::Call { tool: crate::browser::NAME, id: "toolu_2".into(), rpc: json!(4), input: json!({"url":"https://a.org"}) }
        );
        let Mcp::Answer(refused) =
            mcp(&call, Tools { team: false, helper: true, browse: false, conversation: false, verify: false, map: false, progress: false })
        else {
            panic!("a tool not offered is never called")
        };
        assert_eq!(refused["error"]["code"], -32601);
    }

    #[test]
    fn the_quota_the_catalog_the_memory_search_and_the_bigger_offer_come_with_the_project_agents_own_tools_and_with_no_other_session() {
        let call =
            |name: &str, tools| mcp(&json!({"method":"tools/call","params":{"name":name,"arguments":{}},"jsonrpc":"2.0","id":9}), tools);
        let offered = Tools { progress: true, ..Tools::default() };
        for name in ["settings", "quota", "catalog", "memory_search", "offer_bigger", "judge_creative"] {
            assert!(matches!(call(name, offered), Mcp::Call { tool, .. } if tool == name), "{name} is offered with the progress tools");
            assert!(
                matches!(call(name, Tools { verify: true, map: true, ..Tools::default() }), Mcp::Answer(_)),
                "{name} is refused elsewhere"
            );
        }
        assert!(matches!(call("wrap_up", offered), Mcp::Call { tool, .. } if tool == "wrap_up"), "legacy calls remain available");
        let Mcp::Answer(list) = mcp(&json!({"method":"tools/list","jsonrpc":"2.0","id":1}), offered) else { panic!() };
        let names: Vec<_> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["review", "plan", "status", "judge_creative", "settings", "quota", "catalog", "memory_search", "offer_bigger"]);
    }

    #[test]
    fn a_helper_runs_the_cheapest_model_of_the_other_vendor_that_is_good_at_agent_work() {
        let all = crate::route::PROFILES;
        assert_eq!(profile(&all, Vendor::Claude, false).map(|p| (p.model, p.effort)), Some(("claude-opus-5-5", "medium")));
        assert_eq!(profile(&all, Vendor::Codex, false).map(|p| (p.model, p.effort)), Some(("gpt-6.1-terra", "high")));
        // Economy mode (rule 7) lowers the bar, so support work runs one rung cheaper; Haiku and luna stay out.
        assert_eq!(profile(&all, Vendor::Claude, true).map(|p| (p.model, p.effort)), Some(("claude-sonnet-5-5", "high")));
        assert_eq!(profile(&all, Vendor::Codex, true).map(|p| (p.model, p.effort)), Some(("gpt-6.1-terra", "medium")));
        let only_luna: Vec<_> = all.into_iter().filter(|p| p.model == "gpt-6-luna").collect();
        assert_eq!(profile(&only_luna, Vendor::Codex, false).map(|p| p.model), Some("gpt-6-luna"));
        assert_eq!(profile(&only_luna, Vendor::Claude, false), None);
    }

    #[test]
    fn the_report_says_what_the_helper_could_not_do_and_an_empty_brief_is_no_task() {
        assert_eq!(prompt(&json!({"title":"x","brief":"  "})), None);
        assert!(prompt(&json!({"title":"Check","brief":"Look at a.rs."})).unwrap().ends_with("Task: Check\n\nLook at a.rs."));
        assert_eq!(report(true, " Fine. ", &[], None), (true, "Fine.".into()));
        let (ok, text) = report(true, "Read it.", &["Bash".into()], None);
        assert!(ok && text.starts_with("Read it.") && text.contains("approval, so it did not: Bash."), "{text}");
        assert_eq!(report(false, "", &[], Some("error.quotaOut")), (false, "The helper stopped: error.quotaOut.".into()));
        assert_eq!(report(true, "", &[], None), (false, "The helper stopped: it ended without a report.".into()));
    }
}

#[cfg(test)]
mod mixing_tests {
    use super::*;
    use crate::route::{Factor, PROFILES};

    const BOTH: &[Vendor] = &[Vendor::Claude, Vendor::Codex];
    fn neutral(_: &Profile) -> Factor {
        Factor::NEUTRAL
    }
    fn piece(job: &'static str, difficulty: &'static str, scope: &'static str) -> Piece {
        Piece { job, difficulty, scope, kind: "code" }
    }
    fn lead(vendor: Vendor) -> Profile {
        PROFILES.into_iter().filter(|p| p.vendor == vendor).max_by_key(|p| (p.iq, p.agent)).unwrap()
    }

    #[test]
    fn first_read_and_build_mix_each_lead_then_return_to_price() {
        for vendor in BOTH {
            for job in ["read", "build"] {
                let task = piece(job, "easy", "tight");
                let first = select(task, Some(lead(*vendor)), *vendor, &PROFILES, &[*vendor], &neutral, false).unwrap();
                assert_eq!(first.vendor, other(*vendor), "{vendor:?}/{job}");
                let next = select(task, Some(lead(*vendor)), *vendor, &PROFILES, BOTH, &neutral, false).unwrap();
                assert_eq!((next.model, next.effort), ("gpt-6.1-terra", "low"));
                let costly_other = |p: &Profile| Factor { m: if p.vendor == other(*vendor) { 10.0 } else { 1.0 }, ..Factor::NEUTRAL };
                assert_eq!(
                    select(task, Some(lead(*vendor)), *vendor, &PROFILES, &[*vendor], &costly_other, false).unwrap().vendor,
                    other(*vendor)
                );
            }
        }
    }

    #[test]
    fn unavailable_other_and_economy_keep_the_price_pick() {
        for vendor in BOTH {
            let only: Vec<_> = PROFILES.into_iter().filter(|p| p.vendor == *vendor).collect();
            for job in ["read", "build"] {
                for difficulty in crate::route::LEVELS {
                    let task = piece(job, difficulty, "tight");
                    // Candidates already exclude disconnected, billed or exhausted families at the engine boundary.
                    assert_eq!(
                        select(task, Some(lead(*vendor)), *vendor, &only, &[*vendor], &neutral, false),
                        select(task, Some(lead(*vendor)), *vendor, &only, BOTH, &neutral, false)
                    );
                    assert_eq!(
                        select(task, Some(lead(*vendor)), *vendor, &PROFILES, &[*vendor], &neutral, true),
                        select(task, Some(lead(*vendor)), *vendor, &PROFILES, BOTH, &neutral, true)
                    );
                }
            }
        }
    }

    #[test]
    fn reviews_always_prefer_the_other_maker_even_after_the_mix() {
        for vendor in BOTH {
            for makers in [&[*vendor][..], BOTH] {
                assert_eq!(
                    select(piece("review", "medium", "tight"), Some(lead(*vendor)), *vendor, &PROFILES, makers, &neutral, false)
                        .unwrap()
                        .vendor,
                    other(*vendor)
                );
            }
        }
    }

    // The previous picker is retained only as a test oracle for the report and unchanged cases.
    fn before(task: Piece, l: Profile, candidates: &[Profile], economy: bool) -> Option<Profile> {
        if task.job == "advise" {
            return None;
        }
        if task.job == "review" {
            let other = other(l.vendor);
            return profile(candidates, if candidates.iter().any(|p| p.vendor == other) { other } else { l.vendor }, economy);
        }
        let level = if economy { if task.difficulty == "hard" { "medium" } else { "easy" } } else { task.difficulty };
        let bar = if task.job == "read" { (24, 15) } else { crate::route::bar(task.kind, level, true) };
        let eligible: Vec<_> = candidates.iter().copied().filter(|p| p.iq >= bar.0 && p.agent >= bar.1).collect();
        let within: Vec<_> = eligible.iter().copied().filter(|p| p.iq <= l.iq && p.agent <= l.agent).collect();
        let pool = if task.job == "build" && !within.is_empty() { &within } else { &eligible };
        if task.job == "read" {
            pool.iter().min_by(|a, b| a.cost.total_cmp(&b.cost)).copied()
        } else {
            crate::route::choose_paced_scoped(task.kind, level, "machine", "tight", 1.0, true, pool, &neutral).map(|r| r.profile)
        }
    }

    #[test]
    fn picks_before_and_after_for_the_result_table() {
        let row = |p: Option<Profile>| p.map_or_else(|| "unavailable".to_string(), |p| format!("{} / {}", p.model, p.effort));
        println!("| Lead | Job | Difficulty | Scope | Connected | Before | After (first piece) | After (mixed) |");
        println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
        for vendor in BOTH {
            let l = if *vendor == Vendor::Claude {
                PROFILES.into_iter().find(|p| p.model == "claude-opus-5-5" && p.effort == "xhigh").unwrap()
            } else {
                lead(*vendor)
            };
            for job in ["read", "build", "review", "advise"] {
                for difficulty in crate::route::LEVELS {
                    for scope in crate::route::SCOPES {
                        for connected in ["both", "lead only"] {
                            let candidates: Vec<_> = PROFILES.into_iter().filter(|p| connected == "both" || p.vendor == *vendor).collect();
                            let task = piece(job, difficulty, scope);
                            let old = before(task, l, &candidates, false);
                            let first = select(task, Some(l), *vendor, &candidates, &[*vendor], &neutral, false);
                            let mixed = select(task, Some(l), *vendor, &candidates, BOTH, &neutral, false);
                            if job != "advise" && (scope == "tight" || difficulty != "hard" || job != "build") {
                                assert_eq!(old, mixed, "{task:?}/{connected}");
                            }
                            println!(
                                "| {vendor:?} | {job} | {difficulty} | {scope} | {connected} | {} | {} | {} |",
                                row(old),
                                row(first),
                                row(mixed)
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn builds_stay_within_the_lead_when_it_can_do_the_piece_and_open_hard_reaches_expert() {
        for l in PROFILES {
            for kind in crate::route::KINDS {
                for difficulty in crate::route::LEVELS {
                    for scope in crate::route::SCOPES {
                        for economy in [false, true] {
                            let task = Piece { kind, ..piece("build", difficulty, scope) };
                            let within: Vec<_> = PROFILES.into_iter().filter(|p| p.iq <= l.iq && p.agent <= l.agent).collect();
                            let p = select(task, Some(l), l.vendor, &PROFILES, &[l.vendor], &neutral, economy).unwrap();
                            // The lead's level is the cap whenever a row inside it can do the piece; else a stronger row does it.
                            if select(task, Some(l), l.vendor, &within, &[l.vendor], &neutral, economy).is_some() {
                                assert!(p.iq <= l.iq && p.agent <= l.agent, "{task:?}: {l:?} -> {p:?}");
                            } else {
                                assert!(p.iq > l.iq || p.agent > l.agent, "{task:?}: {l:?} -> {p:?}");
                            }
                        }
                    }
                }
            }
        }
        let l = lead(Vendor::Claude);
        let tight = select(piece("build", "hard", "tight"), Some(l), l.vendor, &PROFILES, BOTH, &neutral, false).unwrap();
        let open = select(piece("build", "hard", "open"), Some(l), l.vendor, &PROFILES, BOTH, &neutral, false).unwrap();
        assert_eq!((tight.iq, tight.agent), (46, 39));
        assert!(open.iq >= 50 && open.agent >= 42);
        let capped = PROFILES.into_iter().find(|p| p.model == "gpt-6.1-terra" && p.effort == "medium").unwrap();
        let above =
            select(piece("build", "hard", "open"), Some(capped), capped.vendor, &PROFILES, &[capped.vendor], &neutral, false).unwrap();
        assert!(above.iq >= 50 && above.agent >= 42, "a piece above the lead still gets a model that can do it: {above:?}");
        let below_bar: Vec<_> =
            PROFILES.into_iter().filter(|p| p.vendor == Vendor::Claude && p.iq < 46 || p.vendor == Vendor::Codex).collect();
        assert_eq!(
            select(
                piece("build", "hard", "tight"),
                Some(lead(Vendor::Codex)),
                Vendor::Codex,
                &below_bar,
                &[Vendor::Codex],
                &neutral,
                false
            )
            .unwrap()
            .vendor,
            Vendor::Codex
        );
    }

    #[test]
    fn adviser_dominates_the_lead_respects_mix_and_quota_and_is_read_only() {
        let task = piece("advise", "easy", "tight");
        assert!(task.read_only());
        assert!(piece("read", "hard", "open").read_only());
        assert!(!piece("build", "hard", "open").read_only());
        assert!(!piece("review", "hard", "open").read_only());
        for vendor in BOTH {
            let l = PROFILES
                .into_iter()
                .find(|p| p.vendor == *vendor && p.effort == "medium" && matches!(p.model, "claude-opus-5-5" | "gpt-6.1-sol"))
                .unwrap();
            let p = select(task, Some(l), *vendor, &PROFILES, &[*vendor], &neutral, false).unwrap();
            assert_eq!(p.vendor, other(*vendor));
            assert!(p.iq >= 50 && p.agent >= 42 && p.iq >= l.iq && p.agent >= l.agent && (p.iq > l.iq || p.agent > l.agent));
        }
        let l = PROFILES.into_iter().find(|p| p.model == "gpt-6.1-terra" && p.effort == "low").unwrap();
        let cheaper = |p: &Profile| Factor { m: if p.model == "claude-opus-5-5" { 0.01 } else { 1.0 }, ..Factor::NEUTRAL };
        assert_eq!(select(task, Some(l), l.vendor, &PROFILES, BOTH, &cheaper, false).unwrap().model, "claude-opus-5-5");
        let expert = select(task, Some(l), l.vendor, &PROFILES, BOTH, &neutral, false).unwrap();
        let hard = select(task, Some(l), l.vendor, &PROFILES, BOTH, &neutral, true).unwrap();
        assert_eq!((expert.iq, expert.agent), (50, 43));
        assert_eq!((hard.iq, hard.agent), (46, 39));
        let strongest = lead(Vendor::Claude);
        assert!(select(task, Some(strongest), strongest.vendor, &PROFILES, BOTH, &neutral, false).is_none());
        assert!(select(task, None, Vendor::Claude, &PROFILES, BOTH, &neutral, false).is_none());
        let prompt = prompt(&json!({"job":"advise", "brief":"Why does the approach fail?"})).unwrap();
        for words in ["change no file", "approach you recommend", "main risk", "what to check", "1,200 characters"] {
            assert!(prompt.contains(words), "{words}");
        }
    }

    #[test]
    fn tool_guide_is_concrete_bounded_and_maker_neutral_in_both_transports() {
        assert!(DESCRIPTION.len() < 900, "{}", DESCRIPTION.len());
        for tool in [
            codex_tool(),
            match mcp(&json!({"id":1,"method":"tools/list"}), Tools { helper: true, ..Default::default() }) {
                Mcp::Answer(a) => a["result"]["tools"][0].clone(),
                _ => panic!(),
            },
        ] {
            let text = format!("{}{}", tool["description"], tool["inputSchema"]).to_lowercase();
            for name in [
                "claude",
                "anthropic",
                "codex",
                "openai",
                "chatgpt",
                "gemini",
                "google",
                "fable",
                "opus",
                "sonnet",
                "haiku",
                "gpt-",
                "astra",
                "terra",
                "luna",
            ] {
                assert!(!text.contains(name), "{name}");
            }
            assert_eq!(tool["inputSchema"]["properties"]["scope"]["default"], "tight");
            assert_eq!(tool["inputSchema"]["properties"]["job"]["enum"], json!(["read", "build", "review", "advise"]));
        }
        assert_eq!(Piece::of(&json!({"scope":"OPEN"})).scope, "tight");
        assert_eq!(Piece::of(&json!({"scope":"open"})).scope, "open");
    }
}

#[cfg(test)]
mod team_tests {
    use super::*;

    #[test]
    fn team_is_helper_only_and_uses_the_existing_reply_transport() {
        let tools = Tools { team: true, ..Default::default() };
        let Mcp::Answer(list) = mcp(&json!({"id":1,"method":"tools/list"}), tools) else { panic!() };
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 1);
        assert_eq!(list["result"]["tools"][0]["name"], TEAM_NAME);
        assert_eq!(team_codex_tool()["name"], TEAM_NAME);
        let call =
            json!({"id":2,"method":"tools/call","params":{"name":"team","arguments":{"do":"who"},"_meta":{"claudecode/toolUseId":"who"}}});
        assert!(matches!(mcp(&call, tools), Mcp::Call { tool: TEAM_NAME, id, .. } if id == "who"));
        assert!(matches!(mcp(&call, Tools { helper: true, ..Default::default() }), Mcp::Answer(_)));
        assert!(matches!(mcp(&json!({"id":3,"method":"tools/call","params":{"name":"helper"}}), tools), Mcp::Answer(_)));
    }

    #[test]
    fn team_roster_extracts_only_named_files_without_reading_them() {
        assert_eq!(
            brief_files("Change `engine/a.rs:12`, ui/b.ts and engine/a.rs. Keep https://example.test out. Read E:\\work\\config.toml."),
            ["engine/a.rs", "ui/b.ts", "E:\\work\\config.toml"]
        );
        assert_eq!(
            brief_files("Change Makefile and .gitignore. Also `scripts/check`, `./.env:2`, [.dockerignore], and Makefile."),
            ["Makefile", ".gitignore", "scripts/check", "./.env", ".dockerignore"]
        );
        assert_eq!(brief_files(",engine/a.rs; ;Makefile `..` ../ ./"), ["engine/a.rs", "Makefile"]);
        assert!(brief_files("Review the changes and report.").is_empty());
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    #[test]
    fn team_tool_schemas_have_no_top_level_composition() {
        let Mcp::Answer(list) = mcp(
            &json!({"id":1,"method":"tools/list"}),
            Tools { team: true, helper: true, browse: true, conversation: true, verify: true, map: true, progress: true },
        ) else {
            panic!()
        };
        for tool in list["result"]["tools"].as_array().unwrap().iter().chain([codex_tool(), team_codex_tool()].iter()) {
            for keyword in ["anyOf", "oneOf", "allOf"] {
                assert!(tool["inputSchema"].get(keyword).is_none(), "{}: {keyword}", tool["name"]);
            }
        }
        assert!(codex_tool()["inputSchema"].get("required").is_none());
    }
}
