//! Two tools of a project's agent (the "Toolbox" of docs/product/areas.md): `quota`, what Faber already knows of each connected
//! AI's limits, and `catalog`, a search of the tool catalog. Searching starts no request, process or download, and neither tool
//! shows a token, an account id or an e-mail: they answer from what Faber keeps in memory and ships. Both CLIs call them like the
//! other tools of Faber's server. `catalog` also installs, by id (`install`): that changes Faber itself, so it waits for the
//! person's card (guardrail 3, `engine/src/engine/toolbox.rs`); the words of the card and of the log live here.

use crate::engine::VendorUsage;
use crate::model::Vendor;
use crate::tools::{Item, Tier};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::OnceLock;

/// The names both drivers give the calls, whatever the CLI calls them.
pub const QUOTA_TOOL: &str = "Quota";
pub const CATALOG_TOOL: &str = "Catalog";
/// The tools as Codex knows them, and as Faber's MCP server lists them to Claude Code.
pub const QUOTA: &str = "quota";
pub const CATALOG: &str = "catalog";
/// The tools as Claude Code names them: `mcp__<server>__<tool>`.
pub const CLAUDE_QUOTA: &str = "mcp__faber__quota";
pub const CLAUDE_CATALOG: &str = "mcp__faber__catalog";

/// Results one search lists; the rest are counted.
const SHOWN: usize = 20;
/// Shorter words match everything ("a", "to") and rank nothing, unless they are a whole id or job word ("ui", "ai", "3d").
const SHORTEST_WORD: usize = 3;

const QUOTA_DESCRIPTION: &str = "Reads the limits of each connected AI subscription as Faber last measured them: share used in each \
window, when it renews, and whether it is billed per use. Starts no request. Use it before big work or to choose which AI takes it.";

const CATALOG_DESCRIPTION: &str = "Searches Faber's catalog of skills, plugins and tool servers by words, offline. With `install` set \
to a catalog id it installs that tool from its original source, but only after the person allows it on a card in Faber (or has \
turned on full trust for Faber's own setup), and only a tool Faber holds no copy of: it never updates, repairs or replaces one. A \
tool installed loads in a new conversation, not this one.";

/// The permission card of an install: its tool name in the turn's list. It is Faber's own, never a CLI's request.
pub const SELF_INSTALL: &str = "faber.tools.install";
/// Every decision about a self-change, one line each, in the data folder.
pub const SELF_CHANGES: &str = "self-changes.jsonl";
/// About 1,000 lines of about 200 bytes. At the cap the oldest half goes and the newest lines stay, so a decision is never lost
/// for being late.
const SELF_CHANGES_MAX: u64 = 200_000;

fn quota_schema() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn catalog_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": "Words for the job, e.g. \"fix a bug\". Empty lists the recommended tools." },
            "install": { "type": "string", "description": "The id of one catalog tool, from a search, to install after the person allows it." }
        },
        "additionalProperties": false
    })
}

/// The tools as Codex `dynamicTools` entries.
pub fn codex_tools() -> [Value; 2] {
    [
        json!({ "type": "function", "name": QUOTA, "description": QUOTA_DESCRIPTION, "inputSchema": quota_schema() }),
        json!({ "type": "function", "name": CATALOG, "description": CATALOG_DESCRIPTION, "inputSchema": catalog_schema() }),
    ]
}

/// The tools as Faber's MCP server lists them to Claude Code.
pub fn mcp_tools() -> [Value; 2] {
    [
        json!({ "name": QUOTA, "description": QUOTA_DESCRIPTION, "inputSchema": quota_schema() }),
        json!({ "name": CATALOG, "description": CATALOG_DESCRIPTION, "inputSchema": catalog_schema() }),
    ]
}

/// The name both drivers give a call of one of these tools, from the name Codex or the MCP server knows.
pub fn tool_of(name: &str) -> Option<&'static str> {
    match name {
        QUOTA => Some(QUOTA_TOOL),
        CATALOG => Some(CATALOG_TOOL),
        _ => None,
    }
}

/// The same, from the name Claude Code gives the call.
pub fn claude_tool(name: &str) -> Option<&'static str> {
    name.strip_prefix("mcp__faber__").and_then(tool_of)
}

// ---- quota ----

fn vendor_name(vendor: Vendor) -> &'static str {
    match vendor {
        Vendor::Claude => "Claude Code",
        Vendor::Codex => "Codex",
    }
}

/// "5-hour", "weekly" and the like; a window Faber has no word for keeps its own name.
fn window_name(name: &str) -> String {
    name.replacen("five_hour", "5-hour", 1).replacen("seven_day", "weekly", 1).replace('_', " ")
}

/// How long until `secs` from now, in the two largest units that matter.
fn span(secs: f64) -> String {
    let minutes = (secs / 60.0).round() as u64;
    match minutes {
        0 => "under a minute".into(),
        1..=59 => format!("{minutes} min"),
        60..=2879 => format!("{} h {} min", minutes / 60, minutes % 60),
        _ => format!("{} d {} h", minutes / 1440, minutes % 1440 / 60),
    }
}

/// What Faber knows of each connected AI's limits, from the readings it already holds: nothing here asks a CLI.
pub fn quota(vendors: &[VendorUsage], now: f64) -> String {
    if vendors.is_empty() {
        return "No AI is connected.".into();
    }
    let mut out = vec!["Limits as Faber last read them. Shares are of the whole account, other people's use included.".to_string()];
    for v in vendors {
        // A sign-in billed per use, or one the CLI has not confirmed, reads "out" in the engine but has no quota to run out of.
        let state = match v.state {
            _ if v.billed_per_use || v.billing_unknown => "no included quota to run out of",
            "spare" => "quota to spare",
            "ok" => "on pace",
            "fast" => "being used fast",
            "low" => "running low",
            "out" => "out of included quota",
            _ => "no confirmed reading",
        };
        let billing = if v.billed_per_use {
            "billed per use, so every request costs money"
        } else if v.billing_unknown {
            "billing not confirmed"
        } else {
            "subscription"
        };
        let plan = v.plan.as_deref().map(|p| format!(", plan {p}")).unwrap_or_default();
        out.push(format!("{} ({billing}{plan}): {state}.", vendor_name(v.vendor)));
        if v.windows.is_empty() {
            out.push("  unknown: no reading yet.".into());
        }
        for w in &v.windows {
            // A window past its renewal is already dropped from the readings, so `resets_at` is ahead of `now`.
            let renews = format!("renews in {}", span((w.resets_at - now).max(0.0)));
            let age = if now > w.read_at && w.read_at > 0.0 { format!(", read {} ago", span(now - w.read_at)) } else { String::new() };
            let uncounted =
                w.uncounted.filter(|u| *u >= 0.005).map(|u| format!(", about {:.0}% more not counted yet", u * 100.0)).unwrap_or_default();
            out.push(format!(
                "  {}: {:.0}% used, {:.0}% of the time passed, {renews}{age}{uncounted}.",
                window_name(&w.name),
                w.used * 100.0,
                w.elapsed * 100.0
            ));
        }
        if v.relearning {
            out.push("  Learning this account's pace again, so the pace is rough.".into());
        } else if v.learning {
            out.push("  Still learning this account's pace, so the pace is rough.".into());
        }
        if v.counting {
            out.push("  A request is running or just ended: the figures may still rise.".into());
        }
        if v.overage_on {
            out.push("  Paid extra usage is on for this account.".into());
        }
    }
    out.join("\n")
}

// ---- catalog ----

/// One catalog entry with what the store says about it.
pub struct Entry<'a> {
    pub item: &'a Item,
    pub plugin: bool,
    pub installed: bool,
    pub on: bool,
}

/// The English words of the screen the catalog shows: names, one-line purposes and jobs. They are the same text a person
/// reads in the Catalog, so the agent describes a tool the way the screen does.
fn english() -> &'static HashMap<&'static str, String> {
    static WORDS: OnceLock<HashMap<&'static str, String>> = OnceLock::new();
    WORDS.get_or_init(|| {
        include_str!("../../ui/locales/en.ts")
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let quote = line.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
                let (key, rest) = line[1..].split_once(quote)?;
                if !["catalog.name.", "tools.", "catalog.job."].iter().any(|p| key.starts_with(p)) {
                    return None;
                }
                let rest = rest.strip_prefix(':')?.trim_start();
                let open = rest.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
                let mut chars = rest[1..].chars();
                let mut text = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => match chars.next()? {
                            'n' => text.push('\n'),
                            't' => text.push('\t'),
                            'u' => {
                                let hex: String = chars.by_ref().take(4).collect();
                                text.push(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)?);
                            }
                            c => text.push(c),
                        },
                        c if c == open => return Some((key, text)),
                        c => text.push(c),
                    }
                }
                None
            })
            .collect()
    })
}

/// What a catalog item is called to the agent: a tool server, a plugin or a skill.
fn kind_of(item: &Item, plugin: bool) -> &'static str {
    if item.server.is_some() {
        "tool server"
    } else if plugin {
        "plugin"
    } else {
        "skill"
    }
}

/// The entries that match the words of `input["query"]`, best first: a tool matches by how many of the words its id, name,
/// purpose, job or source holds; with no words the recommended ones are listed. At most 20 are shown.
pub fn catalog(entries: &[Entry], input: &Value) -> (bool, String) {
    let words = english();
    let say = |key: String| words.get(key.as_str()).cloned();
    let query = input["query"].as_str().unwrap_or_default().to_lowercase();
    let wanted: Vec<&str> = query.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let mut found: Vec<(usize, &Entry, String, String)> = entries
        .iter()
        .filter_map(|e| {
            let name = say(format!("catalog.name.{}", e.item.id)).unwrap_or_else(|| e.item.id.clone());
            let job = say(format!("catalog.job.{}", e.item.job)).unwrap_or_else(|| e.item.job.clone());
            let what = say(format!("tools.{}", e.item.id)).unwrap_or_else(|| job.clone());
            let hay = format!("{} {name} {what} {job} {} {}", e.item.id, e.item.job, e.item.repo).to_lowercase();
            // A short word counts only as a whole word of the id or job, never inside another word.
            let token = |w: &str| e.item.id.split('-').chain(e.item.job.split(['-', '_'])).any(|t| t == w);
            let long = |w: &str| w.chars().count() >= SHORTEST_WORD;
            let hits = wanted.iter().filter(|w| if long(w) { hay.contains(**w) } else { token(w) }).count();
            (wanted.is_empty() && e.item.tier == Tier::Recommended || hits > 0).then_some((hits, e, name, what))
        })
        .collect();
    if found.is_empty() {
        return (
            true,
            "Nothing in the catalog matches. Try other words for the job, or call it with no words for the recommended tools.".into(),
        );
    }
    found.sort_by_key(|(hits, e, ..)| (std::cmp::Reverse(*hits), e.item.tier != Tier::Recommended));
    let mut out: Vec<String> = found
        .iter()
        .take(SHOWN)
        .map(|(_, e, name, what)| {
            let kind = kind_of(e.item, e.plugin);
            let state = match (e.installed, e.on) {
                (false, _) => "not installed",
                (true, true) => "installed, on",
                (true, false) => "installed, off",
            };
            format!("{} — {name} ({kind}): {what}. Source: github.com/{}. Licence: {}. {state}.", e.item.id, e.item.repo, e.item.license)
        })
        .collect();
    if found.len() > SHOWN {
        out.push(format!("({} more: add words to narrow it)", found.len() - SHOWN));
    }
    (true, out.join("\n"))
}

// ---- install ----

/// What the card shows, built by the engine from the shipped catalog: the model never writes a word of it. The screen names
/// the tool from its own `catalog.name.<id>` and `tools.<id>` words.
/// `runs_code` is true for a tool server, or a plugin that brings one: the card says it runs code on this computer.
pub fn install_card(item: &Item, plugin: bool, runs_code: bool) -> Value {
    let kind = if item.server.is_some() {
        "server"
    } else if plugin {
        "plugin"
    } else {
        "skill"
    };
    json!({
        "kind": kind, "id": item.id, "repo": item.repo, "license": item.license, "version": version_of(item),
        "server": item.server.is_some(), "code": runs_code
    })
}

/// The pinned version an install brings: a server's release, or a skill's commit.
pub fn version_of(item: &Item) -> String {
    item.server.as_ref().map_or_else(|| item.commit.chars().take(7).collect(), |s| s.version.clone())
}

pub const NOT_HERE: &str = "Faber installs tools only for a project's own agent.";
pub const NOT_ALLOWED: &str = "The person said no. Do not retry; they can install it from Catalog.";
pub const ALREADY: &str = "Already installed.";
/// While updating and repairing from the agent are not built, it installs only what Faber holds nothing of: these are the
/// refusals, each with no card, that leave a copy, its Undo copy and its switch as the person left them.
pub const REINSTALL: &str = "It needs reinstalling; the person does that in Catalog.";
pub const SWITCHED_OFF: &str = "It is installed but switched off; the person turns it on in Catalog.";
pub const REMOVED: &str = "The person removed it; they can bring it back in Catalog.";
pub const BUSY: &str = "Another install of it is already running; nothing was changed.";
pub const NO_RECORD: &str = "Nothing was changed: Faber could not write its record of the install.";
pub const UPDATING: &str = "Faber is about to restart for an update, so nothing was installed. Ask again afterwards.";
pub const NO_NODE: &str = "Needs Node.js to run, which is not installed on this computer. Nothing was changed.";
pub const FAILED: &str = "Download failed; nothing changed.";
pub const UNKNOWN: &str = "No tool with that id is in the catalog. Search it first and use an id from the results.";
pub const WAITING: &str = "Another install is already waiting for the person's answer. Wait for it, then ask again.";
pub const PLAN: &str = "Faber is in Plan or Isolated mode, which changes nothing here. The person can install it from Catalog.";

/// What the agent reads once the tool is installed: what it is, where it came from (the pinned source) and when it loads.
pub fn installed(item: &Item, plugin: bool) -> String {
    format!(
        "Installed {} ({}, licence {}) from github.com/{} at {}. It loads in a new conversation, not this one.",
        item.id,
        kind_of(item, plugin),
        item.license,
        item.repo,
        version_of(item)
    )
}

/// One decision about changing Faber itself, as a line of `self-changes.jsonl`: when, what, from which pinned version to which,
/// how it was allowed and what came of it. No path, no prompt and no token: it stays on this computer.
pub struct SelfChange<'a> {
    pub at: u64,
    pub id: &'a str,
    pub from: Option<&'a str>,
    pub to: &'a str,
    pub how: &'a str,
    pub result: &'a str,
    pub conv: &'a str,
}

/// Adds one line and says whether it is on the disk. Installs can overlap, so one write at a time.
pub fn log_self_change(dir: &std::path::Path, change: &SelfChange) -> bool {
    use std::io::{Read, Seek, SeekFrom, Write};
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let SelfChange { at, id, from, to, how, result, conv } = change;
    let line = json!({ "at": at, "kind": "tools.install", "id": id, "from": from, "to": to, "how": how, "result": result, "conv": conv });
    let path = dir.join(SELF_CHANGES);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() >= SELF_CHANGES_MAX) {
        // The newest half, from the start of a line, replaces the file in one rename.
        let Ok(old) = std::fs::read(&path) else { return false };
        let from_byte = old.len().saturating_sub(SELF_CHANGES_MAX as usize / 2);
        let start = old[from_byte..].iter().position(|b| *b == b'\n').map_or(old.len(), |n| from_byte + n + 1);
        let part = path.with_extension("part");
        if std::fs::write(&part, &old[start..]).and_then(|()| crate::tools::retry_busy(|| std::fs::rename(&part, &path))).is_err() {
            return false;
        }
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(&path)
        .and_then(|mut f| {
            // A line a crash cut short gets its newline first, so it never swallows this decision.
            let torn = f.metadata()?.len() > 0 && {
                f.seek(SeekFrom::End(-1))?;
                let mut last = [0u8];
                f.read_exact(&mut last)?;
                last[0] != b'\n'
            };
            f.write_all(format!("{}{line}\n", if torn { "\n" } else { "" }).as_bytes())
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(installed: &[&str]) -> Vec<Entry<'static>> {
        let on = |id: &str| installed.contains(&id);
        crate::tools::catalog().iter().map(|item| Entry { item, plugin: false, installed: on(&item.id), on: on(&item.id) }).collect()
    }

    #[test]
    fn the_screens_english_words_are_found_with_their_quotes_unescaped() {
        let words = english();
        assert_eq!(words["catalog.name.systematic-debugging"], "Systematic debugging");
        assert_eq!(words["tools.ponytail"], "Solves the task with the least new code");
        assert_eq!(words["catalog.job.debugging"], "Fixing bugs");
        assert!(words["tools.improve-codebase-architecture"].contains("codebase's structure"), "an escaped quote reads as itself");
    }

    #[test]
    fn every_catalog_item_has_a_real_english_name_and_job() {
        let words = english();
        for item in crate::tools::catalog() {
            assert!(words.contains_key(format!("catalog.name.{}", item.id).as_str()), "name of {}", item.id);
            assert!(words.contains_key(format!("catalog.job.{}", item.job).as_str()), "job {} of {}", item.job, item.id);
        }
    }

    #[test]
    fn a_two_letter_word_matches_only_a_whole_id_word() {
        let (_, text) = catalog(&entries(&[]), &json!({ "query": "ui" }));
        assert!(text.lines().any(|l| l.starts_with("break-ui ")), "{text}");
        let (_, none) = catalog(&entries(&[]), &json!({ "query": "zz" }));
        assert!(none.starts_with("Nothing in the catalog matches"));
    }

    #[test]
    fn a_search_lists_what_matches_with_source_licence_and_state_and_is_offline() {
        let (ok, text) = catalog(&entries(&["systematic-debugging"]), &json!({ "query": "debugging a bug" }));
        assert!(ok);
        let first = text.lines().next().unwrap();
        assert!(first.starts_with("systematic-debugging — Systematic debugging (skill): Finds the cause of a bug"), "{first}");
        assert!(
            first.contains("Source: github.com/obra/superpowers.") && first.contains("Licence: MIT.") && first.ends_with("installed, on."),
            "{first}"
        );
        assert!(text.lines().count() <= SHOWN + 1);
        let (_, none) = catalog(&entries(&[]), &json!({ "query": "zzzz qqqq" }));
        assert!(none.starts_with("Nothing in the catalog matches"));
    }

    #[test]
    fn no_words_lists_the_recommended_tools_and_counts_what_did_not_fit() {
        let (_, text) = catalog(&entries(&[]), &json!({}));
        assert!(text.lines().next().unwrap().ends_with("not installed."), "{text}");
        let recommended = crate::tools::catalog().iter().filter(|i| i.tier == Tier::Recommended).count();
        assert_eq!(text.lines().count(), SHOWN.min(recommended) + usize::from(recommended > SHOWN));
        let (_, many) = catalog(&entries(&[]), &json!({ "query": "the design performance data" }));
        assert!(many.lines().count() <= SHOWN + 1);
    }

    fn usage(state: &'static str, windows: Vec<crate::engine::WindowUsage>) -> VendorUsage {
        VendorUsage {
            vendor: Vendor::Codex,
            plan: Some("Plus".into()),
            state,
            learning: false,
            relearning: false,
            outside_share: None,
            foreign_use: false,
            overage_on: false,
            billed_per_use: false,
            billing_unknown: false,
            ask_for_plan: false,
            paid_possible: false,
            own_spend_usd: 0.0,
            unmetered: 0,
            counting: true,
            windows,
        }
    }

    #[test]
    fn the_quota_says_the_share_used_when_it_renews_and_never_probes() {
        let window = crate::engine::WindowUsage {
            name: "five_hour".into(),
            used: 0.31,
            elapsed: 0.4,
            expected: 0.4,
            resets_at: 10_000.0 + 7_800.0,
            read_at: 10_000.0 - 180.0,
            uncounted: None,
        };
        let text = quota(&[usage("ok", vec![window]), VendorUsage { vendor: Vendor::Claude, ..usage("unknown", vec![]) }], 10_000.0);
        assert!(text.contains("Codex (subscription, plan Plus): on pace."), "{text}");
        assert!(text.contains("5-hour: 31% used, 40% of the time passed, renews in 2 h 10 min, read 3 min ago."), "{text}");
        assert!(text.contains("Claude Code (subscription, plan Plus): no confirmed reading.\n  unknown: no reading yet."), "{text}");
        assert!(text.contains("may still rise"));
        assert_eq!(quota(&[], 0.0), "No AI is connected.");
    }

    #[test]
    fn a_billed_or_unconfirmed_sign_in_has_no_quota_to_run_out_of_and_a_learning_one_says_so() {
        let billed = quota(&[VendorUsage { billed_per_use: true, ..usage("out", vec![]) }], 0.0);
        assert!(billed.contains("(billed per use, so every request costs money, plan Plus): no included quota to run out of."), "{billed}");
        let unconfirmed = quota(&[VendorUsage { billing_unknown: true, ..usage("out", vec![]) }], 0.0);
        assert!(unconfirmed.contains("(billing not confirmed, plan Plus): no included quota to run out of."), "{unconfirmed}");
        assert!(!billed.contains("out of included") && !unconfirmed.contains("out of included"));
        let learning = quota(&[VendorUsage { learning: true, ..usage("ok", vec![]) }], 0.0);
        assert!(learning.contains("Still learning this account's pace, so the pace is rough."), "{learning}");
    }

    #[test]
    fn an_install_card_and_its_log_line_hold_only_catalog_facts_and_the_description_names_the_card() {
        let skill = crate::tools::find("ponytail").unwrap();
        let card = install_card(skill, false, false);
        assert_eq!(
            (card["kind"].as_str(), card["id"].as_str(), card["version"].as_str()),
            (Some("skill"), Some("ponytail"), Some(&skill.commit[..7]))
        );
        assert_eq!((card["repo"].as_str(), card["license"].as_str()), (Some(skill.repo.as_str()), Some(skill.license.as_str())));
        assert_eq!(
            (install_card(skill, true, true)["kind"].as_str(), install_card(skill, true, true)["code"].as_bool()),
            (Some("plugin"), Some(true))
        );
        let server = crate::tools::catalog().iter().find(|i| i.server.is_some()).unwrap();
        assert_eq!(
            (install_card(server, false, true)["kind"].as_str(), install_card(server, false, true)["server"].as_bool()),
            (Some("server"), Some(true))
        );
        assert_eq!(version_of(server), server.server.as_ref().unwrap().version);
        assert!(installed(skill, false).contains("loads in a new conversation, not this one"));
        assert!(NOT_ALLOWED.contains("Do not retry") && NOT_ALLOWED.contains("Catalog"));

        let dir = tempfile::tempdir().unwrap();
        let change = SelfChange { at: 5, id: "ponytail", from: None, to: "abc", how: "once", result: "declined", conv: "c1" };
        assert!(log_self_change(dir.path(), &change));
        assert!(log_self_change(dir.path(), &SelfChange { from: Some("old"), how: "always", result: "installed", ..change }));
        let text = std::fs::read_to_string(dir.path().join(SELF_CHANGES)).unwrap();
        let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            json!({ "at": 5, "kind": "tools.install", "id": "ponytail", "from": null, "to": "abc", "how": "once", "result": "declined", "conv": "c1" })
        );
        assert_eq!((lines[1]["from"].as_str(), lines[1]["how"].as_str()), (Some("old"), Some("always")));

        let listed = mcp_tools();
        assert!(listed[1]["inputSchema"]["properties"]["install"].is_object(), "the model can name an id to install");
        assert!(CATALOG_DESCRIPTION.contains("only after the person allows it on a card"));
        assert!(!CATALOG_DESCRIPTION.contains("installs nothing"));
    }

    #[test]
    fn a_log_at_its_cap_drops_the_oldest_lines_and_still_records_the_new_decision() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SELF_CHANGES);
        let old = format!("{}\n", json!({ "at": 1, "pad": "x".repeat(180) }));
        let full: String = old.repeat(SELF_CHANGES_MAX as usize / old.len() + 1);
        std::fs::write(&path, &full).unwrap();
        let change = SelfChange { at: 9, id: "ponytail", from: None, to: "abc", how: "once", result: "allowed", conv: "c1" };
        assert!(log_self_change(dir.path(), &change));
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).expect("every line is whole")).collect();
        assert!(lines.len() > 100 && (text.len() as u64) < SELF_CHANGES_MAX, "the newest half stays: {} lines", lines.len());
        assert_eq!(lines.last().unwrap()["result"], "allowed");
        assert!(log_self_change(dir.path(), &SelfChange { result: "installed", ..change }));
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().last().map(|l| l.contains("installed")), Some(true));
        let blocked = tempfile::tempdir().unwrap();
        std::fs::create_dir(blocked.path().join(SELF_CHANGES)).unwrap();
        assert!(!log_self_change(blocked.path(), &change), "a log that cannot be written says so");
    }

    #[test]
    fn the_tools_are_named_for_both_clis() {
        assert_eq!((tool_of("quota"), tool_of("catalog"), tool_of("map")), (Some("Quota"), Some("Catalog"), None));
        assert_eq!(
            (claude_tool(CLAUDE_QUOTA), claude_tool(CLAUDE_CATALOG), claude_tool("mcp__other__quota")),
            (Some("Quota"), Some("Catalog"), None)
        );
    }

    #[test]
    fn a_line_cut_by_a_crash_never_swallows_the_next_decision_and_a_short_log_is_never_cut() {
        let dir = tempfile::tempdir().unwrap();
        let change = |id| SelfChange { at: 1, id, from: None, to: "0123", how: "once", result: "allowed", conv: "c" };
        std::fs::write(dir.path().join(SELF_CHANGES), b"{\"at\":1,\"kind\":\"tools.in").unwrap();
        assert!(log_self_change(dir.path(), &change("careful")));
        assert!(log_self_change(dir.path(), &change("other")));
        let text = std::fs::read_to_string(dir.path().join(SELF_CHANGES)).unwrap();
        let ids: Vec<_> =
            text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).map(|v| v["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids, ["careful", "other"], "{text}");
        assert_eq!(text.lines().count(), 3, "the cut line stays on a line of its own");
    }
}
