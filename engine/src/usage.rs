//! The cost of one turn, from the figures a CLI reports.
//!
//! Claude Code's `total_cost_usd` is cumulative for the conversation, even across a resume, while `usage` is for the turn.
//! The engine keeps the last total it trusted and a turn costs the difference.

use serde::{Deserialize, Serialize};

/// The last cumulative cost a CLI session reported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostBase {
    /// The CLI session id the total belongs to.
    pub session: String,
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settled {
    /// What this turn cost, when it can be known.
    pub cost: Option<f64>,
    /// The base to keep from now on; None leaves the old one alone.
    pub base: Option<CostBase>,
}

/// `total` is None when the result carried no counters (an aborted turn reports 0 and an empty usage): that is "no
/// information", never a reset. `fresh` is true when the CLI session started in this conversation without a resume.
pub fn settle(base: Option<&CostBase>, session: &str, fresh: bool, total: Option<f64>) -> Settled {
    let Some(total) = total.filter(|t| t.is_finite() && *t >= 0.0) else { return Settled { cost: None, base: None } };
    let cost = match base {
        Some(b) if b.session == session => (total >= b.total).then_some(total - b.total),
        // A CLI session of its own starts again from zero.
        Some(_) => Some(total),
        None if fresh => Some(total),
        // A resumed session this engine has no base for (a conversation saved before the base existed): the total is
        // not this turn's, but it is the base from now on.
        None => None,
    };
    Settled { cost, base: Some(CostBase { session: session.to_string(), total }) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(got: Option<f64>, want: f64) {
        assert!((got.expect("a cost") - want).abs() < 1e-9, "{got:?} is not {want}");
    }

    fn base(session: &str, total: f64) -> CostBase {
        CostBase { session: session.into(), total }
    }

    #[test]
    fn the_first_turn_of_a_new_session_costs_its_whole_total() {
        let s = settle(None, "s1", true, Some(0.00018326));
        near(s.cost, 0.00018326);
        assert_eq!(s.base, Some(base("s1", 0.00018326)));
    }

    #[test]
    fn a_resumed_turn_costs_the_difference() {
        // claude/resume.jsonl after claude/interrupt.jsonl: 0.00023211 in all, 0.00018326 of it from the earlier process.
        let s = settle(Some(&base("s1", 0.00018326)), "s1", false, Some(0.00023211));
        near(s.cost, 0.00004885);
        assert_eq!(s.base, Some(base("s1", 0.00023211)));
    }

    #[test]
    fn an_aborted_result_changes_nothing() {
        let old = base("s1", 0.5);
        assert_eq!(settle(Some(&old), "s1", false, None), Settled { cost: None, base: None });
    }

    #[test]
    fn a_new_session_id_starts_again_from_zero() {
        let s = settle(Some(&base("s1", 0.5)), "s2", false, Some(0.01));
        near(s.cost, 0.01);
        assert_eq!(s.base, Some(base("s2", 0.01)));
    }

    #[test]
    fn a_total_that_goes_down_inside_one_session_is_not_a_cost() {
        let s = settle(Some(&base("s1", 0.5)), "s1", false, Some(0.2));
        assert_eq!(s.cost, None);
        assert_eq!(s.base, Some(base("s1", 0.2)), "the lower total is the base from now on");
    }

    #[test]
    fn a_resumed_session_without_a_base_costs_nothing_known_but_sets_the_base() {
        let s = settle(None, "s1", false, Some(0.3));
        assert_eq!(s.cost, None);
        assert_eq!(s.base, Some(base("s1", 0.3)));
    }

    #[test]
    fn nonsense_totals_are_ignored() {
        assert_eq!(settle(None, "s", true, Some(f64::NAN)).base, None);
        assert_eq!(settle(None, "s", true, Some(-1.0)).base, None);
    }
}

/// API-equivalent USD from the reported token kinds, at published standard prices checked on 2026-10-12.
/// Sources: https://developers.openai.com/api/docs/pricing and
/// https://platform.claude.com/docs/en/about-claude/pricing. Input includes both cache categories.
pub fn api_price(model: &str, input: u64, output: u64, read: u64, write: u64, write_1h: u64) -> Option<f64> {
    let (base, hit, creation, hour, answer) = match model {
        m if m.starts_with("gpt-6-astra") => (10.0, 1.0, 12.5, 12.5, 50.0),
        m if m.starts_with("gpt-6.1-sol") => (2.0, 0.1, 2.5, 2.5, 10.0),
        m if m.starts_with("gpt-6-luna") => (0.1, 0.01, 0.125, 0.125, 0.5),
        m if m.starts_with("claude-opus-5-5") => (4.0, 0.2, 5.0, 8.0, 20.0),
        m if m.starts_with("claude-sonnet-5-5") => (2.0, 0.1, 2.5, 4.0, 10.0),
        m if m.starts_with("claude-haiku-5-5") && input > 100_000 => (0.5, 0.05, 0.625, 1.0, 2.5),
        m if m.starts_with("claude-haiku-5-5") => (0.1, 0.01, 0.125, 0.2, 0.5),
        _ => return None,
    };
    let fresh = input.checked_sub(read.checked_add(write)?)?;
    let short = write.checked_sub(write_1h)?;
    if input == 0 && output == 0 {
        return None;
    }
    Some((fresh as f64 * base + read as f64 * hit + short as f64 * creation + write_1h as f64 * hour + output as f64 * answer) / 1e6)
}

/// A fresh auxiliary session has only one request: its native API-rated total is that request's own cost.
/// Claude's native total includes sub-model work and its cache lifetimes; Codex is priced from its token deltas.
pub fn fresh_price(event: &crate::events::EngineEvent, model: &str) -> (Option<f64>, bool) {
    use crate::events::EngineEvent;
    let EngineEvent::Done {
        cost_total_usd,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        cache_write_1h_tokens,
        incomplete,
        ..
    } = event
    else {
        return (None, true);
    };
    let native = cost_total_usd.filter(|cost| cost.is_finite() && *cost >= 0.0);
    let cost =
        native.or_else(|| api_price(model, *input_tokens, *output_tokens, *cache_read_tokens, *cache_write_tokens, *cache_write_1h_tokens));
    (cost, cost.is_none() || (native.is_none() && *incomplete))
}

#[cfg(test)]
mod price_tests {
    use super::*;
    #[test]
    fn all_cache_kinds_are_priced_without_counting_them_twice() {
        let sol = api_price("gpt-6.1-sol", 1_000_000, 100_000, 600_000, 200_000, 0).unwrap();
        assert!((sol - 1.96).abs() < 1e-9);
        let haiku = api_price("claude-haiku-5-5", 3120, 73, 535, 2583, 2583).unwrap();
        assert!((haiku - 0.00055865).abs() < 1e-9);
        assert_eq!(api_price("unknown", 100, 10, 0, 0, 0), None);
        assert_eq!(api_price("gpt-6.1-sol", 100, 10, 101, 0, 0), None);
    }
}
