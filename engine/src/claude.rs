//! Claude Code driver: one long-lived `claude -p` process per conversation, spoken to in stream-json over stdio.

mod background;
mod commands;

use crate::events::{Capabilities, Delivery, EngineEvent, Session, SessionCommand, UserTurn, failure_reason};
use crate::model::{Area, Permissions, Tuning, Vendor, WebSearch};
use crate::persona::{PersonaInput, persona};
use crate::proc::{self, Spawner};
use crate::router::{EFFORT, default_model};
use crate::signals::claude_signals;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Debug, Clone)]
pub struct ClaudeLaunch {
    pub area: Area,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub resume: Option<String>,
    pub user_hooks: bool,
    pub permissions: Permissions,
    /// A side question (such as routing) with its own instructions: no tools, no MCP servers.
    pub instructions: Option<String>,
    /// How the user wants to be called, told to the model once, when the conversation starts (and again, unchanged, each time
    /// it resumes). Ignored for a side question, which has instructions of its own.
    pub user_name: Option<String>,
    /// Fixed research instruction alongside the ordinary persona.
    pub research: Option<crate::persona::Research>,
    /// The person's Chat group words, passed unchanged beside Faber's persona.
    pub group_instructions: Option<String>,
    /// What the installed CLI is known to accept (see `ClaudeFlags`); the default passes none of the newer flags.
    pub flags: ClaudeFlags,
    /// Web search from the settings (R140). Chat and the person's own project sessions only: a side question never reads it.
    pub tuning: Tuning,
    /// Faber's helper tool (R161): offered to a project's own agent, never to a side question.
    pub helper: crate::helper::Helper,
    /// Faber's own browser (R161 step 7): offered to a project's own agent in full access, where the shell has one.
    pub browse: bool,
    /// Faber's conversation tool (`crate::sibling`): a project's own agent that no other agent started.
    pub conversations: bool,
    /// Faber's checks (`crate::verify`): a project's agent in full access, where the shell gives a runner.
    pub verify: bool,
    /// The plan, the status and the closing box (`crate::progress`): a project's own agent.
    pub progress: bool,
    /// The skills installed from Faber's catalog (`crate::tools`), for a project's agent outside Isolated.
    pub tools: crate::tools::Delivery,
}

/// The tools Faber's MCP server holds for a launch: the helper for a project's own agent, the browser where it is offered.
fn mcp_tools(launch: &ClaudeLaunch) -> crate::helper::Tools {
    crate::helper::Tools {
        helper: launch.helper == crate::helper::Helper::Offers,
        browse: launch.browse,
        conversation: launch.conversations,
        verify: launch.verify,
        // The map only reads files: it goes where the checks go.
        map: launch.verify,
        progress: launch.progress,
    }
}

/// Catalog instructions reach user sessions in both areas; Isolated keeps customizations off.
pub fn offers_tools(_area: Area, permissions: Permissions) -> bool {
    permissions != Permissions::Isolated
}

/// Catalog servers require Full access. The store additionally requires an explicit file-free, setup-free Chat review.
pub fn offers_servers(_area: Area, permissions: Permissions) -> bool {
    permissions == Permissions::FullAccess
}

/// Flags that only newer CLIs take, read once from `claude --help` (`from_help`). A flag the CLI does not list is never
/// passed, so an older CLI starts exactly as before: the default (nothing known) is the safe one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClaudeFlags {
    /// `--autocompact <auto|tokens>`: where the CLI compacts a long conversation (checked on 2.1.296).
    pub autocompact: bool,
    /// `--permission-prompts none`: a request that would prompt is denied at once instead of waiting for an answer (2.1.296).
    pub permission_prompts: bool,
    /// `auto` among the `--permission-mode` choices: the CLI's own reviewer approves the safe actions (R140).
    pub auto_mode: bool,
    /// `--plugin-dir <path>`: a plugin loaded from a folder for this session only (2.1.302), how the catalog's skills arrive.
    pub plugin_dir: bool,
}

impl ClaudeFlags {
    /// Only an option line counts: indented, starting with the flag (after an optional short alias) and followed by its value
    /// placeholder or the gap before its description. The flag named in a sentence, an error or a description is no support.
    pub fn from_help(help: &str) -> Self {
        let declaring = |line: &str, flag: &str| {
            if !line.starts_with([' ', '\t']) {
                return false;
            }
            let mut rest = line.trim_start();
            if rest.starts_with('-') && !rest.starts_with("--") {
                rest = rest.split_once(", ").map_or(rest, |(_, after)| after.trim_start());
            }
            rest.strip_prefix(flag).is_some_and(|tail| {
                tail.is_empty() || tail.starts_with("  ") || tail.starts_with('\t') || tail.trim_start().starts_with(['<', '['])
            })
        };
        let declares = |flag: &str| help.lines().any(|line| declaring(line, flag));
        // The choices of `--permission-mode` are quoted words on the option's own lines, up to the next option.
        let mut block = help.lines().skip_while(|line| !declaring(line, "--permission-mode"));
        let mut modes = block.next().unwrap_or_default().to_string();
        for line in block.take_while(|line| !line.trim_start().starts_with('-')) {
            modes.push(' ');
            modes.push_str(line);
        }
        ClaudeFlags {
            autocompact: declares("--autocompact"),
            permission_prompts: declares("--permission-prompts"),
            auto_mode: modes.contains("\"auto\""),
            plugin_dir: declares("--plugin-dir"),
        }
    }
}

// Chat stays lean: a fixed list of built-in tools. Ask keeps Chat restricted with no MCP servers; Isolated also drops every
// startup customization; full access leaves the person's own integrations on.
const CHAT_TOOLS: &str = "Read,Write,Edit,Glob,Grep,Bash,WebSearch,WebFetch,AskUserQuestion";

/// A quick question on the side (R250) reads, searches and lists, and nothing else.
pub const ASIDE_TOOLS: &str = "Read,Grep,Glob";

/// What a fork of the conversation's session is told for a quick question on the side, on top of what it already knows.
pub const ASIDE_FORKED: &str = "This is a quick side question from the person about this conversation, asked while the main agent \
keeps working. Answer it briefly and plainly. You can only read, search and list files in the working folder: you cannot edit \
files, run commands or browse. If the answer needs an action, say so in one line; the person can send it to the main agent.";

/// The empty integration loadout of a side question: no MCP servers, no settings files, no hooks.
fn claude_exposure_args() -> Vec<String> {
    [
        "--strict-mcp-config",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--safe-mode",
        "--setting-sources",
        "",
        "--settings",
        "{\"disableAllHooks\":true}",
    ]
    .map(String::from)
    .to_vec()
}

// Deny rules apply in bypassPermissions too; command patterns are not an OS sandbox, so a command can still reach what the
// file tools cannot. Read rules also stop Edit and Write on the same path in recent CLIs; the Edit rules say it for older ones.
// https://code.claude.com/docs/en/permissions#read-and-edit
const CREDENTIAL_DIRS: [&str; 10] = [
    "~/.claude",
    "~/.codex",
    "~/.gemini",
    "~/.ssh",
    "~/.aws",
    "~/.docker",
    "~/.azure",
    "~/.kube",
    "~/.config/gh",
    "~/AppData/Roaming/GitHub CLI",
];
const CREDENTIAL_FILES: [&str; 4] = ["~/.claude.json*", "~/.git-credentials", "~/.npmrc", "//**/*updater.key*"];

/// Every deny rule of the isolated and full-access modes, one string per rule. They travel in the settings JSON, an array
/// that keeps each rule whole (a rule with a space, such as the GitHub CLI folder, cannot be split there). Where a documented
/// variable (CLAUDE_CONFIG_DIR and the like) moved a sign-in folder, the new place is denied as well.
pub fn tool_deny(var: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut rules = vec!["Edit(//**/.git/**)".to_string()];
    let moved = crate::permissions::relocated_credentials(var);
    let dirs = CREDENTIAL_DIRS.iter().map(|d| format!("{d}/**"));
    let files = CREDENTIAL_FILES.iter().map(|f| (*f).to_string());
    // A network path is written with two slashes as well: how the CLI spells one in a rule could not be checked.
    let moved = moved.into_iter().flat_map(|(p, file)| {
        let forms = if p.starts_with("//") {
            vec![p.clone(), crate::permissions::claude_rule_path(&p)]
        } else {
            vec![crate::permissions::claude_rule_path(&p)]
        };
        forms.into_iter().map(move |rule| if file { rule } else { format!("{rule}/**") })
    });
    for path in dirs.chain(files).chain(moved) {
        rules.push(format!("Read({path})"));
        rules.push(format!("Edit({path})"));
    }
    rules
}

/// The deny rules the web setting adds (R140): Off drops both web tools; Cached has no equivalent in Claude Code, so it keeps
/// searching and stops fetching pages.
fn web_deny(web: WebSearch) -> Vec<String> {
    match web {
        WebSearch::Live => vec![],
        WebSearch::Cached => vec!["WebFetch".into()],
        WebSearch::Off => vec!["WebSearch".into(), "WebFetch".into()],
    }
}

/// The tools a guarded Chat session may use without a card (R140).
fn web_allowed(web: WebSearch) -> Option<&'static str> {
    match web {
        WebSearch::Live => Some("WebSearch,WebFetch"),
        WebSearch::Cached => Some("WebSearch"),
        WebSearch::Off => None,
    }
}

/// Claude Code loads Faber's own guide as a plugin where Projects load the catalog's, when its CLI takes a plugin folder.
fn guide_plugin_loaded(launch: &ClaudeLaunch) -> bool {
    launch.area == Area::Project && offers_tools(launch.area, launch.permissions) && launch.flags.plugin_dir
}

/// The one rule that lets a guarded session use the guide with no card: its skill, or reading its file.
fn guide_allowed(guide: &crate::guide::Guide, plugin_loaded: bool) -> Option<String> {
    match guide {
        crate::guide::Guide::Plugin { .. } if plugin_loaded => Some(crate::guide::SKILL_RULE.into()),
        crate::guide::Guide::Index { .. } => {
            let file = crate::project::git::native_path(&guide.file()?);
            Some(format!("Read({})", crate::permissions::claude_rule_path(&file)))
        }
        _ => None,
    }
}

/// The native permission mode for the next CLI launch.
pub fn permission_mode(mode: Permissions) -> &'static str {
    match mode {
        Permissions::Ask => "default",
        Permissions::AcceptEdits => "acceptEdits",
        Permissions::Plan => "plan",
        Permissions::Auto => "auto",
        Permissions::FullAccess => "bypassPermissions",
        Permissions::Isolated => "default",
    }
}

/// Allows only this tool request; permission modes are chosen when the next process launches.
fn allow_response(tool: &str, input: &Value, reply: Option<&Value>) -> Value {
    let mut response = json!({ "behavior": "allow", "updatedInput": input });
    // A question the agent asked comes back with the user's choices: the question list as it was, plus `answers`.
    if let (true, Some(answers)) = (tool == "AskUserQuestion", reply) {
        response["updatedInput"]["answers"] = answers.clone();
    }
    response
}

/// What a compaction Faber asks for keeps (`/compact <instructions>`, checked on 2.1.302): the summary is all the next turn
/// knows, so the person's words and what is left must survive it, and the file contents and command output it already
/// acted on may go. The CLI's own compaction near its limit keeps its default.
pub const COMPACT_FOCUS: &str = "Keep the person's requests word for word, the plan and what is left of it, decisions and why, open problems, \
the files changed and the commands that check them. Drop file contents and command output already acted on.";

/// How full a conversation gets before Claude Code compacts it. All three models hold 1M and the CLI compacts near 967K,
/// deep into context rot. Haiku thrashes at 100K (the base context eats the room, measured), so it compacts at 150K (R158);
/// Opus and Sonnet are starting points to measure, not proven optima. Unknown models keep the CLI's own.
fn autocompact_window(model: &str) -> Option<String> {
    autocompact_tokens(model).map(|tokens| format!("{}k", tokens / 1000))
}

/// The same point in tokens: the size Faber's context meter counts the conversation against when it passes `--autocompact`.
pub fn autocompact_tokens(model: &str) -> Option<u64> {
    let model = model.to_ascii_lowercase();
    [("haiku", 150_000), ("sonnet", 300_000), ("opus", 400_000)]
        .into_iter()
        .find(|(name, _)| model.contains(name))
        .map(|(_, tokens)| tokens)
}

/// Arguments for one long-lived Claude Code process that takes many turns over stdin.
/// Every guarded mode names its permission mode (Ask: `default`, ask before file changes and commands that are not plain
/// reading; Accept edits: `acceptEdits`; Plan: `plan`; Auto: `auto`, or accept edits when the CLI does not list it), so neither
/// the person's CLI settings nor an earlier session can pick a looser one. Full access ("Do not ask") bypasses approvals, keeps the
/// person's integrations and only denies the credential folders. Isolated disables startup integrations and routes tools to
/// Faber's working-folder policy. A side question (router, pulse) always gets the isolated treatment.
pub fn claude_args(launch: &ClaudeLaunch) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--exclude-dynamic-system-prompt-sections",
    ]
    .map(String::from)
    .to_vec();
    // The host always answers the CLI's permission requests over stdio, Full access included: the engine denies and parks them there, as
    // `--permission-prompts none` did, except a question the agent asks the person (`AskUserQuestion`), a card in every mode.
    args.extend(["--permission-prompt-tool".into(), "stdio".into()]);
    let env = |name: &str| std::env::var(name).ok();
    let aside = launch.helper == crate::helper::Helper::Aside;
    // The caller verifies the used folders once, immediately before this launch.
    let tools = &launch.tools;
    if aside {
        // A quick question on the side (R250): only reads in the working folder, none of the person's integrations, and no
        // history: a fork of the conversation's session leaves that session as it was. Every read asks, so the driver checks it with
        // the isolated mode's working-folder policy (secrets, links leading out), whatever mode the conversation is in.
        let settings = json!({ "disableAllHooks": true,
            "permissions": { "blockReadsOutsideWorkingDirectories": true, "deny": tool_deny(&env), "ask": ["Read(//**)", "*"] } });
        for a in
            ["--tools", ASIDE_TOOLS, "--strict-mcp-config", "--mcp-config", "{\"mcpServers\":{}}", "--safe-mode", "--setting-sources", ""]
        {
            args.push(a.into());
        }
        args.extend(["--permission-mode".into(), "bypassPermissions".into(), "--settings".into(), settings.to_string()]);
        args.extend(["--append-system-prompt".into(), launch.instructions.clone().unwrap_or_else(|| ASIDE_FORKED.into())]);
        args.push("--no-session-persistence".into());
        if launch.resume.is_some() {
            args.push("--fork-session".into());
        }
    } else if let Some(instructions) = &launch.instructions {
        // Whatever mode the person chose, Faber's own requests run without the person's hooks, settings files, MCP servers and
        // plugins: a hook that fired on every background routing turn would run the person's commands unasked.
        for a in ["--system-prompt", instructions, "--tools", "", "--no-session-persistence"] {
            args.push(a.into());
        }
        // It has no tools to ask for, so a request that came up anyway is refused, never asked (as the usage probes do).
        args.extend(["--permission-mode".into(), "dontAsk".into()]);
        args.extend(claude_exposure_args());
    } else {
        let mut settings = json!({});
        if launch.area == Area::Chat {
            args.extend([
                "--tools".into(),
                format!("{CHAT_TOOLS}{}", if launch.permissions == Permissions::Plan { ",ExitPlanMode" } else { "" }),
            ]);
            match launch.permissions {
                mode if mode.guarded() => {
                    args.push("--strict-mcp-config".into());
                    args.push("--restricted".into());
                }
                Permissions::Isolated => args.extend(["--strict-mcp-config".into(), "--safe-mode".into()]),
                // Full access keeps the person's integrations; Skill is still excluded from Chat's native tools.
                _ => {}
            }
        }
        // What a guarded session may use without a card (R140): Chat's web tools and Faber's own guide, a skill or one file to read.
        if launch.permissions.guarded() {
            let web = web_allowed(launch.tuning.web).filter(|_| launch.area == Area::Chat).map(String::from);
            let allowed: Vec<String> = web.into_iter().chain(guide_allowed(&tools.guide, guide_plugin_loaded(launch))).collect();
            if !allowed.is_empty() {
                args.push("--allowedTools".into());
                args.extend(allowed);
            }
        }
        let web = web_deny(launch.tuning.web);
        let mut catalog_deny = Vec::new();
        if launch.permissions != Permissions::Isolated
            && let Some(root) = tools.tools_root()
        {
            let root = crate::project::git::native_path(&root);
            let path = crate::permissions::claude_rule_path(&root);
            catalog_deny.extend([format!("Edit({path}/**)"), format!("Write({path}/**)")]);
        }
        if let Some(guide) = tools.guide.folder() {
            let path = crate::permissions::claude_rule_path(&crate::project::git::native_path(guide));
            catalog_deny.extend([format!("Edit({path}/**)"), format!("Write({path}/**)")]);
        }
        let deny = || tool_deny(&env).into_iter().chain(web.iter().cloned()).chain(catalog_deny.iter().cloned()).collect::<Vec<_>>();
        match launch.permissions.for_claude(launch.flags) {
            Permissions::Isolated => {
                if launch.area == Area::Project {
                    args.extend(["--safe-mode".into(), "--strict-mcp-config".into()]);
                }
                // Restricted mode refuses bypassPermissions. Chat disables customizations with safe mode and
                // keeps its lean tools/MCP setup, with the same host policy and sandbox as Projects.
                args.extend(["--setting-sources".into(), "user".into()]);
                args.extend(["--permission-mode".into(), "bypassPermissions".into()]);
                settings["permissions"] = json!({ "blockReadsOutsideWorkingDirectories": true, "deny": tool_deny(&env),
                    "ask": ["Read(//**)", "Edit(//**)", "Bash", "PowerShell", "*"] });
                settings["sandbox"] = json!({ "autoAllowBashIfSandboxed": false });
                settings["env"] = crate::permissions::child_env(std::path::Path::new("."));
            }
            // Full access (R121): no sandbox, no ask rules, network on. Deny rules still apply in bypassPermissions.
            Permissions::FullAccess => {
                args.extend(["--permission-mode".into(), "bypassPermissions".into()]);
                settings["permissions"] = json!({ "deny": deny() });
            }
            // Every guarded mode carries the same credential denies as full access: a person's own rule or an approval card
            // must not open them.
            mode => {
                args.extend(["--permission-mode".into(), permission_mode(mode).into()]);
                settings["permissions"] = json!({ "deny": deny() });
            }
        }
        let hooks_off = match launch.permissions {
            Permissions::Isolated => true,
            Permissions::FullAccess => false,
            _ => launch.area == Area::Project && !launch.user_hooks,
        };
        if hooks_off {
            settings["disableAllHooks"] = true.into();
        }
        if settings.as_object().is_some_and(|s| !s.is_empty()) {
            args.push("--settings".into());
            args.push(settings.to_string());
        }
    }
    if launch.instructions.is_none() && !aside {
        args.push("--append-system-prompt".into());
        let mut words = persona(&PersonaInput { name: launch.user_name.as_deref(), tuning: &launch.tuning, research: launch.research });
        if launch.area == Area::Project {
            words.push_str(crate::persona::PROJECT_DETAILS);
        }
        if let Some(instructions) = &launch.group_instructions {
            words.push_str("\n\nThe person's instructions for this chat group:\n");
            words.push_str(instructions);
        }
        // Catalog context travels in the current user turn: resumed CLIs can retain the original system prompt.
        args.push(words);
        // R161: Faber's own MCP server, answered by this driver over stdio, holds the helper tool and the browser.
        // The catalog's servers join it in the same config, where Faber offers its tools.
        let servers: Vec<_> = tools
            .servers
            .iter()
            .filter(|s| offers_servers(launch.area, launch.permissions) && !tools.disabled_servers.contains(&s.name))
            .collect();
        if !servers.is_empty() {
            let mut config: Value =
                serde_json::from_str(if mcp_tools(launch).any() { crate::helper::CLAUDE_MCP_CONFIG } else { r#"{"mcpServers":{}}"# })
                    .unwrap_or_default();
            for s in servers {
                config["mcpServers"][&s.name] = json!({ "type": "stdio", "command": s.command, "args": s.args, "env": s.env });
            }
            args.extend(["--mcp-config".into(), config.to_string()]);
        } else if mcp_tools(launch).any() {
            args.extend(["--mcp-config".into(), crate::helper::CLAUDE_MCP_CONFIG.into()]);
        }
        // The catalog's skills, as one plugin from Faber's store, where Faber offers them.
        if let Some(plugin) = tools
            .plugin
            .as_ref()
            .filter(|_| launch.area == Area::Project && offers_tools(launch.area, launch.permissions) && launch.flags.plugin_dir)
        {
            args.extend(["--plugin-dir".into(), plugin.to_string_lossy().into_owned()]);
        }
        // Faber's own guide (R18), a plugin of its own, so only the skill's short description costs tokens on a turn.
        if let crate::guide::Guide::Plugin { dir, .. } = &tools.guide
            && guide_plugin_loaded(launch)
        {
            args.extend(["--plugin-dir".into(), dir.to_string_lossy().into_owned()]);
        }
        // The CLI echoes each line it is given when it takes it in, so a line the person wrote while it works is known to be
        // read, and by which turn. A helper keeps no history.
        args.push("--replay-user-messages".into());
        if launch.helper == crate::helper::Helper::Is {
            args.push("--no-session-persistence".into());
        }
    }
    // Rule 2 of docs/product/models.md: every start names its model and effort, never the CLI's own pick. A caller that chose
    // none (the engine resolves its choice first) gets the defaults the engine would have shown.
    let model = launch.model.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| default_model(Vendor::Claude, None, &[]));
    let effort = launch.effort.clone().filter(|e| !e.is_empty()).unwrap_or_else(|| EFFORT.into());
    // Side questions keep the CLI default, and an older CLI that does not list the flag would refuse to start with it.
    let compacts = launch.flags.autocompact && launch.instructions.is_none() && !aside;
    if let Some(window) = compacts.then(|| autocompact_window(&model)).flatten() {
        args.extend(["--autocompact".into(), window]);
    }
    args.extend(["--model".into(), model, "--effort".into(), effort]);
    if let Some(resume) = &launch.resume {
        args.extend(["--resume".into(), resume.clone()]);
    }
    args
}

fn block_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().filter_map(|c| c["text"].as_str()).collect(),
        _ => String::new(),
    }
}

/// How a refused approval comes back as the tool's error: the text Faber answers with, and the older wording of the recordings.
const DECLINED: [&str; 2] = ["The user declined.", "The user denied"];

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// What fills the context, from Claude Code's answer to `get_context_usage`: the categories it counts as used, summed into
/// Faber's parts (the system part, tools, files, the conversation) as `ctx.part.*` keys, in that order. Free room, the
/// compaction buffer and deferred tools hold nothing; a category Faber does not know counts as other, so the parts always
/// add up to what Claude Code counted.
pub fn context_parts(usage: &Value) -> Vec<crate::model::ContextPart> {
    const KEYS: [&str; 5] = ["ctx.part.system", "ctx.part.tools", "ctx.part.files", "ctx.part.conversation", "ctx.part.other"];
    let mut sums = [0u64; 5];
    for category in usage["categories"].as_array().into_iter().flatten() {
        let name = category["name"].as_str().unwrap_or_default().to_ascii_lowercase();
        let held = match category["kind"].as_str() {
            Some(kind) => kind == "used",
            None => !name.contains("free") && !name.contains("buffer"),
        };
        if !held || category["isDeferred"] == true {
            continue;
        }
        let part = if name.contains("message") {
            3
        } else if name.contains("tool") {
            1
        } else if name.contains("memory") || name.contains("file") {
            2
        } else if ["system", "agent", "skill", "command"].iter().any(|word| name.contains(word)) {
            0
        } else {
            4
        };
        sums[part] += category["tokens"].as_u64().unwrap_or(0);
    }
    KEYS.iter()
        .zip(sums)
        .filter(|(_, tokens)| *tokens > 0)
        .map(|(key, tokens)| crate::model::ContextPart { key: (*key).into(), tokens })
        .collect()
}

/// Turns one stream-json line into engine events. Claude Code's JSON is loosely typed, so fields are read
/// defensively; lines that carry nothing the app shows give no events.
pub fn parse_claude_message(msg: &Value) -> Vec<EngineEvent> {
    match msg["type"].as_str().unwrap_or_default() {
        "system" if msg["subtype"] == "init" => {
            let mut events = match (msg["session_id"].as_str(), msg["model"].as_str()) {
                (Some(id), Some(model)) => vec![EngineEvent::Started { session_id: id.into(), model: model.into() }],
                _ => vec![EngineEvent::Error { message: "Claude Code started without a session id.".into(), fatal: false }],
            };
            if let Some(names) = msg["slash_commands"].as_array() {
                // Other built-ins change Faber's controls, need the terminal, manage the CLI or can spend money.
                const ALLOWED: &[&str] = &["compact", "context", "init", "review", "security-review", "recap", "insights"];
                let skills = msg["skills"].as_array();
                let terminal = msg["terminal_slash_commands"].as_array();
                let commands = names
                    .iter()
                    .filter_map(|name| {
                        let name = name.as_str()?;
                        let skill = skills.is_some_and(|skills| skills.iter().any(|s| s == name));
                        (!name.starts_with('_')
                            && (skill || (ALLOWED.contains(&name) && !terminal.is_some_and(|t| t.iter().any(|s| s == name)))))
                        .then(|| crate::model::Command { name: name.into(), skill, note: None })
                    })
                    .collect();
                events.push(EngineEvent::Commands(commands));
            }
            if let Some(servers) = msg["mcp_servers"].as_array() {
                events.push(EngineEvent::McpServers(crate::mcp_servers::reported(servers, "status")));
            }
            // What the CLI says it can do: the engine keeps it, and no feature is read from its version (rule 6).
            events.extend(Capabilities::from_init(msg).map(EngineEvent::Capabilities));
            events
        }
        // A subagent's own words stream too (`parent_tool_use_id` names the agent call that started it): they are its work, never
        // the answer, so only its tool calls are shown, inside its step.
        "stream_event" if !msg["parent_tool_use_id"].is_null() => vec![],
        "stream_event" => {
            let event = &msg["event"];
            if event["type"] == "message_delta" && event["delta"]["stop_reason"] == "end_turn" {
                return vec![EngineEvent::Settled];
            }
            if event["type"] == "content_block_start" && event["content_block"]["type"] == "text" {
                return vec![EngineEvent::Break { final_answer: false }];
            }
            if event["type"] != "content_block_delta" {
                return vec![];
            }
            let delta = &event["delta"];
            match delta["type"].as_str() {
                Some("text_delta") => vec![EngineEvent::Text(s(&delta["text"]))],
                Some("thinking_delta") => vec![EngineEvent::Thinking(s(&delta["thinking"]))],
                _ => vec![],
            }
        }
        // Text already arrived as deltas; only finished tool calls are new here. A subagent's calls name the agent call that
        // started it.
        "assistant" => {
            let parent = msg["parent_tool_use_id"].as_str().map(String::from);
            // What the main thread holds in its context: the prompt of its latest response (a subagent's own never counts). A
            // response with no prompt is a synthetic one, not a reading.
            let context = parent
                .is_none()
                .then(|| {
                    let u = &msg["message"]["usage"];
                    ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"]
                        .iter()
                        .map(|k| u[*k].as_u64().unwrap_or(0))
                        .sum::<u64>()
                })
                .filter(|used| *used > 0)
                .map(|used| EngineEvent::Context { used: Some(used), window: None });
            context
                .into_iter()
                .chain(items(&msg["message"]["content"]).filter(|c| c["type"] == "tool_use").map(|c| {
                    let name = match c["name"].as_str() {
                        Some(crate::helper::CLAUDE_NAME) => crate::helper::TOOL.to_string(),
                        Some(crate::browser::CLAUDE_NAME) => crate::browser::TOOL.to_string(),
                        Some(crate::sibling::CLAUDE_NAME) => crate::sibling::TOOL.to_string(),
                        Some(crate::verify::CLAUDE_NAME) => crate::verify::TOOL.to_string(),
                        Some(crate::map::CLAUDE_NAME) => crate::map::TOOL.to_string(),
                        Some(name) if let Some(tool) = crate::progress::claude_tool(name) => tool.to_string(),
                        _ => s(&c["name"]),
                    };
                    EngineEvent::Tool { id: s(&c["id"]), name, input: c["input"].clone(), parent: parent.clone() }
                }))
                .collect()
        }
        // The conversation was compacted (by Faber, by the agent or by the CLI's own limit); the summary's size is reported.
        "system" if msg["subtype"] == "compact_boundary" && msg["parent_tool_use_id"].is_null() => {
            vec![EngineEvent::Compacted { after: msg["compact_metadata"]["post_tokens"].as_u64() }]
        }
        "user" => items(&msg["message"]["content"])
            .filter(|c| c["type"] == "tool_result")
            .map(|c| {
                let (ok, text) = (c["is_error"] != true, block_text(&c["content"]));
                EngineEvent::ToolResult {
                    id: s(&c["tool_use_id"]),
                    ok,
                    declined: !ok && DECLINED.iter().any(|d| text.starts_with(d)),
                    text,
                }
            })
            .collect(),
        "system" if msg["subtype"] == "task_notification" && msg["tool_use_id"].is_string() => {
            vec![EngineEvent::HelperDone { id: s(&msg["tool_use_id"]), ok: msg["status"] == "completed", text: s(&msg["summary"]) }]
        }
        "control_request" if msg["request"]["subtype"] == "can_use_tool" => vec![EngineEvent::Permission {
            request_id: s(&msg["request_id"]),
            tool: s(&msg["request"]["tool_name"]),
            input: msg["request"]["input"].clone(),
        }],
        "rate_limit_event" => {
            let windows = msg["rate_limit_info"]["unifiedWindows"].as_object().into_iter().flatten().filter_map(|(window, w)| {
                Some(EngineEvent::Quota { window: window.clone(), used: w["utilization"].as_f64()?, resets_at: w["resetsAt"].as_f64()? })
            });
            windows.chain(Some(claude_signals(msg)).filter(|s| !s.is_empty()).map(EngineEvent::Signals)).collect()
        }
        "result" => result_events(msg),
        _ => vec![],
    }
}

/// A finished turn. `usage` describes the main response only, while `modelUsage` adds the sub-model work of the same turn
/// (web fetches and the like), so a turn whose `modelUsage` prompt is larger is marked incomplete; its cost stays valid.
/// Whether a result's own text is the plan's limit (and not another limit such as the context's).
fn says_plan_limit(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    ["you've hit your limit", "usage limit", "usage_limit_reached", "weekly limit", "out of credits"].iter().any(|p| t.contains(p))
}

fn result_events(msg: &Value) -> Vec<EngineEvent> {
    let u = &msg["usage"];
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    let (fresh, read, write) = (n("input_tokens"), n("cache_read_input_tokens"), n("cache_creation_input_tokens"));
    let prompt = fresh + read + write;
    // `usage` describes the main response only; `modelUsage` adds the sub-model work of the same turn.
    let whole_turn: u64 = msg["modelUsage"]
        .as_object()
        .map(|models| {
            models
                .values()
                .map(|m| {
                    ["inputTokens", "cacheReadInputTokens", "cacheCreationInputTokens"]
                        .iter()
                        .map(|k| m[*k].as_u64().unwrap_or(0))
                        .sum::<u64>()
                })
                .sum()
        })
        .unwrap_or(0);
    // An aborted turn reports a total of 0 and an empty usage: no counters, which is not a cost of 0.
    let total = msg["total_cost_usd"].as_f64();
    let counters = total.is_some_and(|t| t > 0.0) || prompt + n("output_tokens") > 0;
    let warm = cache_warm_secs(u).map(|secs| EngineEvent::CacheWarm { secs, started_ms: None });
    // A request the CLI denied for want of anyone to answer it; each is kept for the person, never allowed on its own.
    let parked = items(&msg["permission_denials"]).map(|d| EngineEvent::Parked {
        tool_use_id: s(&d["tool_use_id"]),
        tool: s(&d["tool_name"]),
        input: d["tool_input"].clone(),
    });
    // The window of the model that did most of the turn's work: the main thread's, not a sub-model's.
    let window = msg["modelUsage"]
        .as_object()
        .into_iter()
        .flat_map(|models| models.values())
        .max_by_key(|m| {
            ["inputTokens", "cacheReadInputTokens", "cacheCreationInputTokens"].iter().map(|k| m[*k].as_u64().unwrap_or(0)).sum::<u64>()
        })
        .and_then(|m| m["contextWindow"].as_u64())
        .map(|window| EngineEvent::Context { used: None, window: Some(window) });
    let ok = msg["subtype"] == "success" && msg["is_error"] != true;
    // Why a turn that did not end well ended, from the CLI's terminal reason (its subtype when it names none), as a key.
    let why = (!ok).then(|| failure_reason(msg["terminal_reason"].as_str(), msg["subtype"].as_str()));
    window
        .into_iter()
        .chain(warm)
        .chain(parked)
        .chain(why.map(EngineEvent::Reason))
        .chain([EngineEvent::Done {
            ok,
            reason: msg["errors"]
                .as_array()
                .filter(|errors| !errors.is_empty())
                .map(|errors| errors.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "))
                // A plan limit can arrive as a "success" result flagged as an error, its words only in `result`: keep them, so the
                // limit is still recognised as one. Only the plan's own phrases: "Context limit reached" is not a plan limit.
                .or_else(|| msg["result"].as_str().filter(|r| !ok && says_plan_limit(r)).map(String::from))
                .unwrap_or_else(|| msg["terminal_reason"].as_str().or(msg["subtype"].as_str()).unwrap_or_default().into()),
            cost_total_usd: total.filter(|_| counters),
            input_tokens: prompt,
            output_tokens: n("output_tokens"),
            cache_read_tokens: read,
            cache_write_tokens: write,
            cache_write_1h_tokens: u["cache_creation"]["ephemeral_1h_input_tokens"].as_u64().unwrap_or(0),
            incomplete: whole_turn > prompt,
        }])
        .collect()
}

/// How long this turn's cache writes stay warm: an hour when the 1-hour bucket was written (a subscription writes every entry
/// that way), else five minutes when only the 5-minute bucket was. None when neither is named (an older CLI, or a turn that
/// wrote nothing), which the screen shows as nothing. Mixed buckets count as the long one: the main conversation wrote it.
pub fn cache_warm_secs(usage: &Value) -> Option<u64> {
    let bucket = |k: &str| usage["cache_creation"][k].as_u64().unwrap_or(0);
    if bucket("ephemeral_1h_input_tokens") > 0 {
        Some(3600)
    } else if bucket("ephemeral_5m_input_tokens") > 0 {
        Some(300)
    } else {
        None
    }
}
fn items(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

pub type EventSink = Arc<dyn Fn(EngineEvent) + Send + Sync>;

pub struct ClaudeOptions {
    pub bin: String,
    pub cwd: PathBuf,
    pub launch: ClaudeLaunch,
    pub spawn: Spawner,
    pub sandbox: Option<crate::cli::Cli>,
}

pub fn start_claude(opts: ClaudeOptions, on_event: EventSink) -> Session {
    let args = claude_args(&opts.launch);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    // The process starts right away, so a warmed-up session is already booting while the user types.
    // A denial in the result only means "nobody could answer" when the CLI ran with no prompts; with the host answering, a denial is
    // the person's own refusal (or the engine's), which is no request to keep.
    let parking = args.windows(2).any(|a| a == ["--permission-prompts", "none"]);
    let started = (opts.spawn)(&opts.bin, &args, &opts.cwd);
    tokio::spawn(async move {
        let mut p = match started {
            Ok(p) => p,
            Err(err) => {
                on_event(EngineEvent::Error { message: format!("Claude Code could not start: {err}"), fatal: true });
                return on_event(EngineEvent::WriterExit { confirmed: true });
            }
        };
        let tail = proc::stderr_tail(p.stderr.take());
        let mut lines = BufReader::new(p.stdout).lines();
        let mut stdin = p.stdin;
        let mut child = p.child;
        let mut job = p.contained;
        let no_process = child.is_none() && job.is_none();
        let mut shutdown_confirmed = false;
        let mut pending: HashMap<String, (String, Value)> = HashMap::new();
        // Helper calls waiting for their report: tool call id to the control request and the JSON-RPC id that carried it.
        let mut helpers: HashMap<String, (String, Value)> = HashMap::new();
        let mut closed = false;
        let mut retiring = false;
        let mut close_deadline = None;
        let mut control_id = 0u32;
        // The id of the `get_context_usage` request still waiting for its answer.
        let mut usage_request: Option<String> = None;
        let mut commands = commands::Commands::default();
        let mut command_reads: HashMap<String, (String, bool)> = HashMap::new();
        let mut command_stops: HashMap<String, tokio::sync::oneshot::Sender<Delivery>> = HashMap::new();
        let mut watch = proc::TurnWatch::default();
        let mut queued: Vec<UserTurn> = vec![];
        // Lines not yet echoed back, in order, with their words: the tag of one the person wrote while a turn ran, none for a
        // turn's own. An echo is matched by its words, so a recording made before the echo existed reads the same.
        let replay = opts.launch.instructions.is_none();
        let mut unread: VecDeque<(Option<String>, String)> = VecDeque::new();
        // When the latest request of the running turn began: the cache lifetime counts from it, not from the result.
        let mut request_began: Option<u64> = None;
        // When the running turn was handed to the CLI: its request goes out at that moment. Set once per turn and never moved by
        // the lines that follow (status, retries), so later requests of a tool turn count from the first: never more warmth than there is.
        let mut turn_sent_ms = unix_ms();
        let mut background = background::Background::default();
        let mut writers = background::Writers::default();
        let mut tree_check = None;
        on_event(writers.snapshot());
        'driver: loop {
            if retiring && !watch.active() && !writers.busy() {
                shutdown_confirmed |= proc::stop_confirmed(&mut *stdin, &mut child, &mut job).await;
                writers.shutdown(shutdown_confirmed, false);
                on_event(writers.snapshot());
                break;
            }
            let deadline = close_deadline.or(watch.deadline());
            tokio::select! {
                () = proc::wait_deadline(tree_check) => {
                    if let Some(container) = job.as_ref() { writers.observe_tree(container.descendants()); }
                    tree_check = writers.tree_pending().then(|| tokio::time::Instant::now() + std::time::Duration::from_secs(1));
                    on_event(writers.snapshot());
                }
                () = proc::wait_deadline(deadline) => {
                    if closed || watch.stopping() {
                        let confirmed = proc::stop_confirmed(&mut *stdin, &mut child, &mut job).await;
                        shutdown_confirmed |= confirmed;
                        writers.shutdown(confirmed, watch.active());
                        on_event(writers.snapshot());
                        on_event(EngineEvent::Error { message: "error.cli.interruptTimeout".into(), fatal: true });
                        break;
                    }
                    watch.interrupt();
                    writers.interrupt();
                    on_event(writers.snapshot());
                    control_id += 1;
                    let msg = json!({ "type": "control_request", "request_id": format!("faber-{control_id}"), "request": { "subtype": "interrupt" } });
                    let _ = proc::write_line(&mut *stdin, &msg).await;
                }
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else {
                        shutdown_confirmed |= proc::stop_confirmed(&mut *stdin, &mut child, &mut job).await;
                        writers.shutdown(shutdown_confirmed, watch.active());
                        on_event(writers.snapshot());
                        if !closed {
                            let code = tokio::time::timeout(std::time::Duration::from_secs(1), proc::exit_code(&mut child))
                                .await.unwrap_or_else(|_| "unknown".into());
                            let tail = tail.lock().unwrap().clone();
                            on_event(EngineEvent::Error { message: format!("Claude Code stopped (exit {code}). {tail}").trim().into(), fatal: true });
                        }
                        break;
                    };
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Value>(&line) {
                        Ok(msg) => {
                            if msg["type"] == "control_response" && usage_request.is_some() && msg["response"]["request_id"].as_str() == usage_request.as_deref() {
                                usage_request = None;
                                if msg["response"]["subtype"] == "success" {
                                    on_event(EngineEvent::ContextParts { parts: context_parts(&msg["response"]["response"]) });
                                }
                                continue;
                            }
                            if msg["type"] == "control_response" {
                                let response = &msg["response"];
                                if let Some(request) = response["request_id"].as_str() {
                                    if let Some((task, dirty)) = command_reads.remove(request) {
                                        if let Some(output) = response["response"]["output"].as_str() {
                                            on_event(EngineEvent::Background(crate::background::Update::Output { id: task.clone(), text: output.into(), replace: true }));
                                        }
                                        if dirty {
                                            control_id += 1;
                                            let id = format!("faber-{control_id}");
                                            let request = json!({"type":"control_request","request_id":id,"request":{"subtype":"get_task_output","task_id":task}});
                                            if proc::write_line(&mut *stdin, &request).await.is_ok() { command_reads.insert(id, (task, false)); }
                                        }
                                        continue;
                                    }
                                    if let Some(ack) = command_stops.remove(request) {
                                        let _ = ack.send(if response["subtype"] == "success" { Delivery::Delivered } else { Delivery::WriteFailed });
                                        continue;
                                    }
                                }
                            }
                            // A task edge requests its latest tail once. No timer, file watching or model turn.
                            let final_read = commands.read_event(&msg);
                            let news = commands.observe(&msg);
                            for update in news { on_event(EngineEvent::Background(update)); }
                            if let Some(task) = final_read.or_else(|| commands.read_event(&msg)) {
                                if let Some((_, dirty)) = command_reads.values_mut().find(|(id, _)| *id == task) { *dirty = true; }
                                else if command_reads.len() < crate::background::MAX_COMMANDS {
                                    control_id += 1;
                                    let id = format!("faber-{control_id}");
                                    let request = json!({"type":"control_request","request_id":id,"request":{"subtype":"get_task_output","task_id":task}});
                                    if proc::write_line(&mut *stdin, &request).await.is_ok() { command_reads.insert(id, (task, false)); }
                                }
                            }
                            writers.observe(&msg);
                            if msg["type"] == "result" || (msg["type"] == "system" &&
                                matches!(msg["subtype"].as_str(), Some("init" | "task_started" | "task_notification")))
                            {
                                if let Some(container) = job.as_ref() { writers.observe_tree(container.descendants()); }
                                tree_check = writers.tree_pending().then(|| tokio::time::Instant::now() + std::time::Duration::from_secs(1));
                                if writers.tree_pending() { on_event(writers.snapshot()); }
                            }
                            // Announce tool launches before their calls reach the engine, but idle only after Done.
                            if matches!(msg["type"].as_str(), Some("assistant" | "user"))
                                || (msg["type"] == "system" && matches!(msg["subtype"].as_str(), Some("task_started" | "task_notification")))
                            {
                                on_event(writers.snapshot());
                            }
                            if matches!(msg["type"].as_str(), Some("assistant" | "user"))
                                && msg["parent_tool_use_id"].is_null()
                                && let Some(entry) = msg["uuid"].as_str().filter(|id| !id.is_empty())
                            {
                                on_event(EngineEvent::Cursor { entry: Some(entry.into()), turn: None });
                            }
                            if msg["type"] == "stream_event" && msg["event"]["type"] == "message_start" && msg["parent_tool_use_id"].is_null() {
                                request_began = Some(turn_sent_ms);
                            }
                            let finished = background.ends_reply(&msg, watch.stopping());
                            if finished {
                                // Bash tasks may be announced after their original reply has ended.
                                commands.end_turn();
                                watch.finish();
                            }
                            if replay && msg["type"] == "user" && msg["isReplay"] == true {
                                let said = words(&msg["message"]["content"]);
                                if let Some((Some(tag), _)) = unread.iter().position(|(_, w)| *w == said).and_then(|at| unread.remove(at)) {
                                    on_event(EngineEvent::Injected { tag: Some(tag) });
                                }
                                continue;
                            }
                            // R161: Faber's MCP server. Only a session that offers one of its tools has it; the CLI waits for each answer.
                            if msg["type"] == "control_request" && msg["request"]["subtype"] == "mcp_message" {
                                let tools = mcp_tools(&opts.launch);
                                let offered = tools.any() && msg["request"]["server_name"] == "faber";
                                let step = if offered { crate::helper::mcp(&msg["request"]["message"], tools) } else {
                                    crate::helper::Mcp::Answer(json!({"jsonrpc":"2.0","id":msg["request"]["message"]["id"],"error":{"code":-32601,"message":"No such server"}}))
                                };
                                match step {
                                    crate::helper::Mcp::Answer(answer) => {
                                        let reply = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": msg["request_id"],
                                            "response": { "mcp_response": answer } } });
                                        let _ = proc::write_line(&mut *stdin, &reply).await;
                                    }
                                    crate::helper::Mcp::Call { tool, id, rpc, input } => {
                                        // Both answers come back as a `HelperReply` for the call's id.
                                        helpers.insert(id.clone(), (s(&msg["request_id"]), rpc));
                                        on_event(match tool {
                                            crate::browser::NAME => EngineEvent::BrowseCall { id, input },
                                            crate::sibling::NAME => EngineEvent::ConversationCall { id, input },
                                            crate::verify::NAME => EngineEvent::VerifyCall { id, input },
                                            crate::map::NAME => EngineEvent::MapCall { id, input },
                                            name if let Some(tool) = crate::progress::tool_of(name) => EngineEvent::ProgressCall { id, tool, input },
                                            _ => EngineEvent::HelperCall { id, input },
                                        });
                                    }
                                }
                                continue;
                            }
                            for mut event in parse_claude_message(&msg) {
                                if matches!(event, EngineEvent::Settled) && background.owes() {
                                    continue;
                                }
                                if matches!(event, EngineEvent::Done { .. }) {
                                    if !finished {
                                        background.hold(event);
                                        continue;
                                    }
                                    event = background.close(event);
                                }
                                if matches!(event, EngineEvent::Parked { .. }) && !parking {
                                    continue;
                                }
                                if let EngineEvent::CacheWarm { secs, .. } = event {
                                    // A turn whose request start was never seen has no trustworthy expiry: nothing is shown.
                                    let Some(began) = request_began.take() else { continue };
                                    event = EngineEvent::CacheWarm { secs, started_ms: Some(began) };
                                }
                                if let EngineEvent::Permission { request_id, tool, input } = &event {
                                    // Calling the helper changes nothing by itself: what the helper does is held to the same mode. The
                                    // browser is offered only in full access, which asks nothing, and the engine checks every address.
                                    if (tool == crate::helper::CLAUDE_NAME && opts.launch.helper == crate::helper::Helper::Offers)
                                        || (tool == crate::browser::CLAUDE_NAME && opts.launch.browse)
                                        || (tool == crate::sibling::CLAUDE_NAME && opts.launch.conversations)
                                        || ((tool == crate::verify::CLAUDE_NAME || tool == crate::map::CLAUDE_NAME) && opts.launch.verify)
                                        || (crate::progress::claude_tool(tool).is_some() && opts.launch.progress)
                                    {
                                        let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id,
                                            "response": { "behavior": "allow", "updatedInput": input } } });
                                        let _ = proc::write_line(&mut *stdin, &msg).await;
                                        continue;
                                    }
                                    // Only the isolated mode answers for itself. Full access hands the request to the engine, which
                                    // decides with the mode in force at that moment (see Core::handle), so a switch to Ask is never raced.
                                    if opts.launch.permissions == Permissions::Isolated {
                                        let resolve = |name: &str| crate::cli::Cli::find_in(name, &std::env::var("PATH").unwrap_or_default(), cfg!(windows),
                                            &|key| std::env::var(key).ok(), &|path| std::path::Path::new(path).is_file(),
                                            &|path| std::fs::read_to_string(path).unwrap_or_default());
                                        let response = if let Some(updated) = crate::permissions::checked_input(&opts.cwd, tool, input, opts.sandbox.as_ref(), &resolve) {
                                            json!({ "behavior": "allow", "updatedInput": updated })
                                        } else {
                                            json!({ "behavior": "deny", "message": "This action is outside the working-folder policy, or its native sandbox runner is unavailable. Use sandboxed tools; never retry unsandboxed." })
                                        };
                                        let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id,
                                            "response": response } });
                                        if proc::write_line(&mut *stdin, &msg).await.is_err() {
                                            on_event(EngineEvent::Error { message: "error.permission.undelivered".into(), fatal: true });
                                            shutdown_confirmed |= proc::stop(&mut *stdin, &mut child, &mut job).await;
                                            break 'driver;
                                        }
                                        continue;
                                    }
                                    pending.insert(request_id.clone(), (tool.clone(), input.clone()));
                                }
                                on_event(event);
                            }
                            if msg["type"] == "result" {
                                on_event(writers.snapshot());
                            }
                            if closed && !watch.active() && !writers.busy() {
                                let confirmed = proc::stop_confirmed(&mut *stdin, &mut child, &mut job).await;
                                shutdown_confirmed |= confirmed;
                                writers.shutdown(confirmed, false);
                                on_event(writers.snapshot());
                                break;
                            }
                            if finished {
                                // A turn's own line is echoed before its result. A line the person wrote that is still unread is
                                // a turn of its own, and it starts now, after a stop too (fixtures `steer*`, `helper-told-later`).
                                unread.retain(|(tag, _)| tag.is_some());
                                if !unread.is_empty() { watch.begin(); }
                                for turn in std::mem::take(&mut queued) {
                                    watch.begin();
                                    on_event(writers.snapshot());
                                    turn_sent_ms = unix_ms();
                                    if replay { unread.push_back((None, turn.text.clone())); }
                                    let _ = proc::write_line(&mut *stdin, &user_message(&turn)).await;
                                }
                            }
                        }
                        Err(_) => {
                            writers.lost(watch.active());
                            on_event(writers.snapshot());
                            on_event(EngineEvent::Error { message: format!("Unexpected output from Claude Code: {}", line.chars().take(200).collect::<String>()), fatal: false });
                        }
                    }
                }
                cmd = rx.recv(), if !closed => match cmd {
                    Some(SessionCommand::McpStatus { reply }) => { let _ = reply.send(None); }
                    Some(SessionCommand::StopBackground { id: task, ack }) => {
                        if !commands.contains(&task) { let _ = ack.send(Delivery::Unknown); continue; }
                        if command_stops.len() >= crate::background::MAX_COMMANDS { let _ = ack.send(Delivery::WriteFailed); continue; }
                        control_id += 1;
                        let id = format!("faber-{control_id}");
                        let request = json!({"type":"control_request","request_id":id,"request":{"subtype":"stop_task","task_id":task}});
                        if proc::write_line(&mut *stdin, &request).await.is_ok() { command_stops.insert(id, ack); }
                        else { let _ = ack.send(Delivery::WriteFailed); }
                    }
                    Some(SessionCommand::Send(turn)) => {
                        if watch.stopping() { queued.push(turn); continue; }
                        watch.begin();
                        on_event(writers.snapshot());
                        request_began = None;
                        turn_sent_ms = unix_ms();
                        if replay { unread.push_back((None, turn.text.clone())); }
                        let _ = proc::write_line(&mut *stdin, &user_message(&turn)).await;
                    }
                    // What fills the context, counted from the last response and local estimates: no turn, no token-count request.
                    Some(SessionCommand::ContextUsage) => {
                        control_id += 1;
                        let id = format!("faber-{control_id}");
                        let msg = json!({ "type": "control_request", "request_id": id, "request": { "subtype": "get_context_usage", "detail": "summary" } });
                        usage_request = Some(id);
                        let _ = proc::write_line(&mut *stdin, &msg).await;
                    }
                    // The CLI's own command, as a line of its own: it compacts and answers with a result like any turn.
                    Some(SessionCommand::Compact) => {
                        if watch.stopping() { continue; }
                        let turn = UserTurn::text(format!("/compact {COMPACT_FOCUS}"));
                        watch.begin();
                        on_event(writers.snapshot());
                        request_began = None;
                        turn_sent_ms = unix_ms();
                        if replay { unread.push_back((None, turn.text.clone())); }
                        let _ = proc::write_line(&mut *stdin, &user_message(&turn)).await;
                    }
                    // A line written mid-turn is folded into a turn that uses tools at its next step, and becomes a turn of its
                    // own after a plain reply or a stop (fixtures `steer*`, `queued*`, `helper-told*`). Its echo says which turn
                    // took it in.
                    Some(SessionCommand::Steer { turn, tag }) => {
                        if replay && watch.active() && !watch.stopping() && proc::write_line(&mut *stdin, &user_message(&turn)).await.is_ok() {
                            unread.push_back((Some(tag), turn.text.clone()));
                        } else {
                            on_event(EngineEvent::SteerFailed { tag });
                        }
                    }
                    Some(SessionCommand::Answer { request_id, allow, reply, ack }) => {
                        // The request stays pending until the write worked, so a failed write can be answered again.
                        let delivery = match pending.get(&request_id) {
                            None => Delivery::Unknown,
                            Some((tool, input)) => {
                                let response = if allow { allow_response(tool, input, reply.as_ref()) } else { json!({ "behavior": "deny", "message": if tool == "ExitPlanMode" { "The person wants to keep planning." } else { DECLINED[0] } }) };
                                let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } });
                                match proc::write_line(&mut *stdin, &msg).await {
                                    Ok(()) => {
                                        pending.remove(&request_id);
                                        Delivery::Delivered
                                    }
                                    Err(e) => {
                                        // A half-written or unflushed line cannot be sent again: end the process, and the
                                        // end of its output tells the engine.
                                        if e.torn {
                                            shutdown_confirmed |= proc::stop(&mut *stdin, &mut child, &mut job).await;
                                        }
                                        Delivery::WriteFailed
                                    }
                                }
                            }
                        };
                        let _ = ack.send(delivery);
                    }
                    Some(SessionCommand::HelperReply { id, ok, text, pictures }) => {
                        let Some((request_id, rpc)) = helpers.remove(&id) else { continue };
                        let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id,
                            "response": { "mcp_response": crate::helper::mcp_result(&rpc, ok, &text, &pictures) } } });
                        let _ = proc::write_line(&mut *stdin, &msg).await;
                    }
                    Some(SessionCommand::Interrupt) => {
                        if !writers.should_interrupt(watch.active(), watch.stopping()) { continue; }
                        watch.interrupt();
                        writers.interrupt();
                        on_event(writers.snapshot());
                        control_id += 1;
                        let msg = json!({ "type": "control_request", "request_id": format!("faber-{control_id}"), "request": { "subtype": "interrupt" } });
                        let _ = proc::write_line(&mut *stdin, &msg).await;
                    }
                    Some(SessionCommand::Retire) => retiring = true,
                    Some(SessionCommand::Close) | None => {
                        closed = true;
                        if watch.active() || writers.busy() {
                            if !watch.stopping() {
                                if watch.active() { watch.interrupt(); }
                                writers.interrupt();
                                on_event(writers.snapshot());
                                control_id += 1;
                                let msg = json!({ "type": "control_request", "request_id": format!("faber-{control_id}"), "request": { "subtype": "interrupt" } });
                                let _ = proc::write_line(&mut *stdin, &msg).await;
                            }
                            close_deadline = Some(watch.deadline().unwrap_or_else(|| tokio::time::Instant::now() + std::time::Duration::from_secs(5)));
                        } else {
                            let confirmed = proc::stop_confirmed(&mut *stdin, &mut child, &mut job).await;
                            shutdown_confirmed |= confirmed;
                            writers.shutdown(confirmed, false);
                            on_event(writers.snapshot());
                            break;
                        }
                    }
                }
            }
        }
        shutdown_confirmed |= no_process && !watch.active() && !writers.busy();
        on_event(EngineEvent::WriterExit { confirmed: shutdown_confirmed });
        // An unresolved owned tree keeps its original session control, so Stop can retry the same handle.
        while !shutdown_confirmed && job.is_some() {
            match rx.recv().await {
                Some(SessionCommand::Interrupt | SessionCommand::Close) => {
                    shutdown_confirmed = proc::stop_confirmed(&mut *stdin, &mut child, &mut job).await;
                    writers.shutdown(shutdown_confirmed, watch.active());
                    on_event(writers.snapshot());
                    on_event(EngineEvent::WriterExit { confirmed: shutdown_confirmed });
                }
                None => break,
                _ => {}
            }
        }
    });
    Session::new(tx)
}

/// The words of a user line's content, plain or in blocks: what an echo is matched by.
fn words(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b["text"].as_str()).collect(),
        _ => String::new(),
    }
}

fn user_message(turn: &UserTurn) -> Value {
    let content = if turn.images.is_empty() {
        Value::String(turn.text.clone())
    } else {
        let mut blocks: Vec<Value> = turn
            .images
            .iter()
            .map(|i| json!({ "type": "image", "source": { "type": "base64", "media_type": i.media_type, "data": i.base64 } }))
            .collect();
        blocks.push(json!({ "type": "text", "text": turn.text }));
        Value::Array(blocks)
    };
    json!({ "type": "user", "message": { "role": "user", "content": content }, "parent_tool_use_id": null, "session_id": "" })
}

#[cfg(test)]
mod allow_tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn rewind_close_retains_a_failed_tree_query_until_stop_retries_the_same_owned_container() {
        use crate::proc::ProcessIdentity;
        use std::sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        };
        use tokio::io::AsyncWriteExt;
        let empty = Arc::new(AtomicBool::new(false));
        let query = empty.clone();
        let unavailable = Arc::new(AtomicBool::new(false));
        let failed_query = unavailable.clone();
        let contained = proc::Containment::scripted_descendants(
            ProcessIdentity { pid: 42, created: 1 },
            move || {
                if query.load(Ordering::Acquire) {
                    Ok(Vec::new())
                } else if failed_query.load(Ordering::Acquire) {
                    Err(std::io::Error::other("query unavailable"))
                } else {
                    Ok(vec![ProcessIdentity { pid: 99, created: 2 }])
                }
            },
            || {},
        );
        let (driver, script) = tokio::io::duplex(65536);
        let (stdout, stdin) = tokio::io::split(driver);
        let (input, mut output) = tokio::io::split(script);
        let mut input = BufReader::new(input).lines();
        let process = Arc::new(Mutex::new(Some(proc::Proc {
            stdin: Box::new(stdin),
            stdout: Box::new(stdout),
            stderr: None,
            child: None,
            contained: Some(contained),
        })));
        let spawn = Arc::new(move |_: &str, _: &[String], _: &std::path::Path| Ok(process.lock().unwrap().take().unwrap()));
        let launch = ClaudeLaunch {
            area: Area::Project,
            model: None,
            effort: None,
            resume: None,
            user_hooks: false,
            permissions: Permissions::Ask,
            instructions: None,
            research: None,
            group_instructions: None,
            user_name: None,
            flags: Default::default(),
            tuning: Default::default(),
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            tools: Default::default(),
        };
        let (sent, mut events) = tokio::sync::mpsc::unbounded_channel();
        let session = start_claude(
            ClaudeOptions { bin: "scripted".into(), cwd: PathBuf::new(), launch, spawn, sandbox: None },
            Arc::new(move |event| drop(sent.send(event))),
        );
        let exchange = async {
            session.send(UserTurn::text("command"));
            input.next_line().await.unwrap().unwrap();
            let started = json!({"type":"system", "subtype":"task_started", "task_type":"local_bash", "task_id":"command"});
            output.write_all(format!("{started}\n").as_bytes()).await.unwrap();
            while !matches!(events.recv().await.unwrap(), EngineEvent::Writers { active, unknown: true } if active.contains(&"process-tree:99".into()))
            {
            }
            session.close();
            let interrupt: Value = serde_json::from_str(&input.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(interrupt["request"]["subtype"], "interrupt");
            unavailable.store(true, Ordering::Release);
            output.shutdown().await.unwrap();
            drop(output);
            while !matches!(events.recv().await.unwrap(), EngineEvent::WriterExit { confirmed: false }) {}
            empty.store(true, Ordering::Release);
            session.interrupt();
            while !matches!(events.recv().await.unwrap(), EngineEvent::WriterExit { confirmed: true }) {}
        };
        tokio::time::timeout(std::time::Duration::from_secs(3), exchange)
            .await
            .expect("the retained Stop route must retry the same owned tree");
    }

    #[test]
    fn the_main_threads_end_turn_settles_the_reply_before_the_result_comes() {
        let delta = |parent: Value, reason: Value| {
            parse_claude_message(&json!({ "type": "stream_event", "parent_tool_use_id": parent,
                "event": { "type": "message_delta", "delta": { "stop_reason": reason } } }))
        };
        assert!(matches!(delta(Value::Null, json!("end_turn")).as_slice(), [EngineEvent::Settled]));
        assert!(delta(Value::Null, json!("tool_use")).is_empty(), "a message that calls a tool is not the last one");
        assert!(delta(json!("agent1"), json!("end_turn")).is_empty(), "a subagent ending says nothing of the main thread");
    }

    #[test]
    fn a_subagents_tool_calls_name_the_agent_call_and_its_words_never_join_the_answer() {
        let call = |parent: Value| {
            parse_claude_message(&json!({ "type": "assistant", "parent_tool_use_id": parent,
                "message": { "content": [{ "type": "tool_use", "id": "t2", "name": "Grep", "input": { "pattern": "x" } }] } }))
        };
        let parent_of = |events: Vec<EngineEvent>| match events.as_slice() {
            [EngineEvent::Tool { parent, .. }] => parent.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(parent_of(call(json!("agent1"))), Some("agent1".into()));
        assert_eq!(parent_of(call(Value::Null)), None);
        let words = |parent: Value| {
            parse_claude_message(&json!({ "type": "stream_event", "parent_tool_use_id": parent,
                "event": { "type": "content_block_delta", "delta": { "type": "text_delta", "text": "inner" } } }))
        };
        assert!(words(json!("agent1")).is_empty(), "a subagent's text is not the answer");
        assert!(matches!(words(Value::Null).as_slice(), [EngineEvent::Text(t)] if t == "inner"));
        let done = parse_claude_message(&json!({ "type": "system", "subtype": "task_notification", "tool_use_id": "agent1",
            "status": "failed", "summary": "Gave up" }));
        assert!(matches!(done.as_slice(), [EngineEvent::HelperDone { id, ok: false, text }] if id == "agent1" && text == "Gave up"));
    }

    #[test]
    fn a_question_the_person_answered_goes_back_with_the_questions_and_the_answers() {
        let input = json!({ "questions": [{ "question": "Which stack?", "header": "Stack", "multiSelect": false, "options": [{ "label": "Rust", "description": "" }] }] });
        let answers = json!({ "Which stack?": "Rust" });
        let response = allow_response("AskUserQuestion", &input, Some(&answers));
        assert_eq!(response["behavior"], "allow");
        assert_eq!(response["updatedInput"]["questions"], input["questions"]);
        assert_eq!(response["updatedInput"]["answers"], answers);
        assert_eq!(
            allow_response("Bash", &input, Some(&answers)),
            json!({ "behavior": "allow", "updatedInput": input }),
            "only a question carries answers"
        );
    }

    #[test]
    fn r160_exiting_plan_only_answers_the_tool_and_never_changes_the_session_mode() {
        let input = json!({ "plan": "do it" });
        let leaving = allow_response("ExitPlanMode", &input, None);
        assert_eq!(leaving["behavior"], "allow");
        assert_eq!(leaving["updatedInput"], input);
        assert!(leaving.get("updatedPermissions").is_none());
        for tool in ["Write", "Bash", "Edit", "WebFetch"] {
            let plain = allow_response(tool, &input, None);
            assert_eq!(plain, json!({ "behavior": "allow", "updatedInput": input }), "{tool}");
        }
    }
}
