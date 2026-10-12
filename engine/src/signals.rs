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
    let wide = info["rateLimitType"].as_str().is_some_and(vendor_wide);
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
