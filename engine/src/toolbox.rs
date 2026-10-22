//! Two read-only tools of a project's agent (the "Toolbox" of docs/product/areas.md): `quota`, what Faber already knows of
//! each connected AI's limits, and `catalog`, a search of the tool catalog. Neither starts a request, a process or a download,
//! and neither shows a token, an account id or an e-mail: they answer from what Faber keeps in memory and ships. Both CLIs
//! call them like the other tools of Faber's server; the engine answers at once. Installing from the catalog is not here: it
//! changes Faber itself and waits for the approval gate.

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

const CATALOG_DESCRIPTION: &str = "Searches Faber's catalog of skills, plugins and tool servers by words, offline. It installs \
nothing: the person does that in the Catalog.";

fn quota_schema() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn catalog_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "query": { "type": "string", "description": "Words for the job, e.g. \"fix a bug\". Empty lists the recommended tools." } },
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
            let kind = if e.item.server.is_some() {
                "tool server"
            } else if e.plugin {
                "plugin"
            } else {
                "skill"
            };
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
    fn the_tools_are_named_for_both_clis() {
        assert_eq!((tool_of("quota"), tool_of("catalog"), tool_of("map")), (Some("Quota"), Some("Catalog"), None));
        assert_eq!(
            (claude_tool(CLAUDE_QUOTA), claude_tool(CLAUDE_CATALOG), claude_tool("mcp__other__quota")),
            (Some("Quota"), Some("Catalog"), None)
        );
    }
}
