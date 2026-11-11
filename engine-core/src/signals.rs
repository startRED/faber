//! What a CLI says about the account besides window readings, reduced to the few facts the router needs. Everything else in
//! those messages is dropped unread.

use crate::budget::{Billing, Reach, vendor_wide};
use serde_json::Value;

/// What a CLI said about the account besides window readings. Each field is `Some` only when the message says something about
/// it; the engine merges them into what it knows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Signals {
    pub reach: Option<Reach>,
    pub overage_active: Option<bool>,
    pub paid_fallback: Option<bool>,
    pub plan: Option<String>,
    pub billing: Option<Billing>,
    pub credit_balance: Option<f64>,
}

impl Signals {
    /// True when the message said nothing about the account, so there is nothing to pass on.
    pub fn is_empty(&self) -> bool {
        *self == Signals::default()
    }
}

/// Claude Code's `rate_limit_event`. A status other than `allowed` counts only when it is about a vendor-wide window; the
/// extra-usage switch is "possible" unless the CLI says it is rejected or off.
pub fn claude_signals(msg: &Value) -> Signals {
    let info = &msg["rate_limit_info"];
    let wide = info["rateLimitType"].as_str().is_none_or(vendor_wide);
    let status = info["status"].as_str().unwrap_or_default();
    let reach = (wide && !status.is_empty())
        .then(|| if status.starts_with("allowed") { Reach::Clear } else { Reach::Out(info["resetsAt"].as_f64()) });
    Signals {
        reach,
        overage_active: info["isUsingOverage"].as_bool(),
        paid_fallback: info["overageStatus"].as_str().map(|s| !matches!(s, "rejected" | "disabled" | "off" | "none")),
        ..Signals::default()
    }
}

/// Codex's `account/rateLimits/updated`: the plan, credits, and whether a limit was reached.
pub fn codex_signals(msg: &Value) -> Signals {
    let r = &msg["params"]["rateLimits"];
    let named = r["rateLimitReachedType"].as_str();
    let reset = ["primary", "secondary"]
        .into_iter()
        .filter(|name| named == Some(*name) || r[*name]["usedPercent"].as_f64().is_some_and(|used| used >= 100.0))
        .filter_map(|name| r[name]["resetsAt"].as_f64())
        .reduce(f64::max);
    let reached = !r["rateLimitReachedType"].is_null() || r["spendControlReached"] == true;
    let reach = if reached {
        Some(Reach::Out(reset))
    } else {
        (r.get("rateLimitReachedType").is_some() && r.get("spendControlReached").is_some()).then_some(Reach::Clear)
    };
    let paid_fallback = match (r["credits"]["hasCredits"].as_bool(), r["credits"]["unlimited"].as_bool()) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    };
    Signals {
        reach,
        paid_fallback,
        credit_balance: r["credits"]["balance"]
            .as_f64()
            .or_else(|| r["credits"]["balance"].as_str()?.parse().ok())
            .filter(|v| v.is_finite()),
        plan: r["planType"].as_str().map(String::from),
        billing: r["planType"].as_str().filter(|p| usage_plan(p)).map(|_| Billing::UsageBilled),
        ..Signals::default()
    }
}

/// Plan names that bill by use rather than by subscription (Codex lists some as `..._usage_based`).
fn usage_plan(plan: &str) -> bool {
    plan.to_ascii_lowercase().contains("usage_based")
}

/// Codex's `account/updated`: how the account signs in and its plan. The mode names are not credentials; nothing else in
/// the message is kept. Only a ChatGPT sign-in on a plan that is not billed by use counts as a subscription: any other
/// way in is kept out of Auto unless its subscription billing is confirmed.
pub fn codex_account(msg: &Value) -> Signals {
    let p = &msg["params"];
    let plan = p["planType"].as_str();
    let by_use = plan.is_some_and(usage_plan);
    let billing = match p["authMode"].as_str().map(str::to_ascii_lowercase).as_deref() {
        _ if by_use => Billing::UsageBilled,
        Some("chatgpt" | "chatgptauthtokens") => Billing::Subscription,
        Some("apikey" | "api_key" | "amazonbedrock" | "googlevertexai") => Billing::UsageBilled,
        _ => Billing::Unknown,
    };
    Signals { plan: plan.map(String::from), billing: Some(billing), ..Signals::default() }
}

/// What `claude auth status` printed, reduced to the plan and the billing: everything else in the output is dropped
/// unread.
pub fn claude_plan(output: &str) -> Signals {
    let Ok(v) = serde_json::from_str::<Value>(output) else { return Signals::default() };
    let plan = v["subscriptionType"].as_str().filter(|s| !s.is_empty() && v["loggedIn"] != false).map(String::from);
    let method = ["authMethod", "authMode", "loginMethod"].into_iter().find_map(|k| v[k].as_str()).unwrap_or_default().to_ascii_lowercase();
    let billing = if v["loggedIn"] == false {
        Billing::Unknown
    } else if method.contains("api")
        || method.contains("console")
        || v["apiKeySource"].as_str().is_some_and(|s| !matches!(s, "none" | ""))
        || v["apiProvider"].as_str().is_some_and(|s| !matches!(s, "firstParty" | "firstparty" | "anthropic" | ""))
    {
        Billing::UsageBilled
    } else if plan.is_some() {
        Billing::Subscription
    } else {
        Billing::Unknown
    };
    Signals { plan, billing: Some(billing), ..Signals::default() }
}

/// Presence alone is enough; credential values are never inspected or kept.
pub fn claude_env_billed(names: impl IntoIterator<Item = std::ffi::OsString>) -> bool {
    const OVERRIDES: [&str; 6] = [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_USE_BEDROCK",
        "CLAUDE_CODE_USE_VERTEX",
        "CLAUDE_CODE_USE_FOUNDRY",
        "ANTHROPIC_BASE_URL",
    ];
    names.into_iter().any(|name| OVERRIDES.iter().any(|expected| name.to_string_lossy().eq_ignore_ascii_case(expected)))
}

/// What `claude -p /usage` printed: a local command that asks no model anything. Its text has one line per window ("Current
/// session: 10% used · resets Oct 14, 11:29pm (America/Fortaleza)"); only the two vendor-wide windows are read, as
/// (window, used 0 to 1, reset in Unix seconds). The reset time is printed in the PC's own time, so the screen's offset reads
/// it; a line without a percentage or a reset time is skipped, and so is anything else in the text.
pub fn claude_usage_windows(text: &str, now: f64, offset: i64) -> Vec<(&'static str, f64, f64)> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(": ")?;
            let window = match name.trim() {
                "Current session" => "five_hour",
                "Current week (all models)" => "seven_day",
                _ => return None,
            };
            let used = rest.split('%').next()?.trim().parse::<f64>().ok().filter(|u| u.is_finite() && *u >= 0.0)? / 100.0;
            let when = rest.split_once("resets ")?.1;
            let resets = printed_time(when.split(" (").next()?.trim(), now, offset)?;
            Some((window, used, resets))
        })
        .collect()
}

/// "Oct 20, 4:59am" or "11:29pm" (today, or tomorrow when that has passed) in local time, as Unix seconds. The text comes from
/// a program Faber does not control, so nothing here may panic: only ASCII am/pm suffixes are read, the minute (0 to 59), the
/// hour (1 to 12) and the day (1 to 31) are range-checked before any arithmetic, and the arithmetic is checked.
fn printed_time(text: &str, now: f64, offset: i64) -> Option<f64> {
    let (date, clock) = match text.rsplit_once(", ") {
        Some((date, clock)) => (Some(date), clock),
        None => (None, text),
    };
    let clock = clock.trim().to_ascii_lowercase();
    let (hm, pm) = match (clock.strip_suffix("am"), clock.strip_suffix("pm")) {
        (Some(hm), _) => (hm, false),
        (_, Some(hm)) => (hm, true),
        _ => return None,
    };
    let (h, m) = hm.trim().split_once(':').map_or((hm.trim(), "0"), |(h, m)| (h, m));
    let (h, m): (i64, i64) = (h.parse().ok()?, m.parse().ok()?);
    if !(1..=12).contains(&h) || !(0..=59).contains(&m) {
        return None;
    }
    let hour = h % 12 + if pm { 12 } else { 0 };
    let local_now = (now as i64).checked_add(offset)?;
    let (year, month, day) = civil(local_now.div_euclid(86_400));
    let at = |y: i64, mo: i64, d: i64| -> Option<f64> {
        let seconds = days(y, mo, d).checked_mul(86_400)?.checked_add(hour * 3600 + m * 60)?.checked_sub(offset)?;
        Some(seconds as f64)
    };
    match date {
        Some(date) => {
            let (name, d) = date.split_once(' ')?;
            let mo = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
                .iter()
                .position(|n| name.to_ascii_lowercase().starts_with(n))? as i64
                + 1;
            let d: i64 = d.trim().parse().ok()?;
            if !(1..=31).contains(&d) {
                return None;
            }
            let this = at(year, mo, d)?;
            if this < now - 86_400.0 { at(year.checked_add(1)?, mo, d) } else { Some(this) }
        }
        None => {
            let today = at(year, month, day)?;
            Some(if today < now { today + 86_400.0 } else { today })
        }
    }
}

/// Days since 1970-01-01 of a civil date, and back (proleptic Gregorian; Howard Hinnant's algorithms).
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
