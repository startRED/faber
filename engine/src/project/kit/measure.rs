//! G9 measurement aggregation. Missing production evidence is not a passing fixture measurement.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeasurementSource {
    Synthetic,
    Fixture,
    Live,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LabelPair {
    pub old_micros: u64,
    pub new_micros: u64,
    pub old_output_tokens: u64,
    pub new_output_tokens: u64,
    pub old_cost_micros: u64,
    pub new_cost_micros: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurements {
    pub source: MeasurementSource,
    pub cards: usize,
    pub samples: usize,
    pub assembly_p50_micros: Option<u64>,
    pub assembly_p99_micros: Option<u64>,
    pub assembly_target: Option<bool>,
    pub label_p50_extra_micros: Option<i128>,
    pub label_p90_extra_micros: Option<i128>,
    pub label_max_extra_tokens: Option<u64>,
    pub label_max_extra_cost_micros: Option<u64>,
    pub label_target: Option<bool>,
    pub plan_extra_tokens_per_node: Option<f64>,
    pub plan_target: Option<bool>,
    pub attestation_complete: bool,
    pub outcomes_complete: bool,
    pub best_effort_sessions: usize,
    pub selection_share: Option<f64>,
    pub selection_target: Option<bool>,
    pub gap_count: usize,
    pub resident_max_relative_error: Option<f64>,
    pub resident_target: Option<bool>,
    pub card_writing_cost_micros: Option<u64>,
    pub pass_cost_micros: Option<u64>,
    pub real_goal_proven: bool,
    pub gap_tiers: Vec<GapStats>,
    pub covered_start_delay_max_micros: Option<u64>,
    pub covered_start_target: Option<bool>,
    pub card_writing_target: Option<bool>,
    pub pass_cost_target: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GapSample {
    pub tier: GapTier,
    pub elapsed_micros: u64,
    pub cost_micros: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GapStats {
    pub tier: GapTier,
    pub samples: usize,
    pub p90_micros: Option<u64>,
    pub max_cost_micros: Option<u64>,
    pub target: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardSample {
    pub calls: u32,
    pub cost_micros: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassSample {
    pub piece_cost_micros: u64,
    pub pass_cost_micros: u64,
    pub kept: bool,
    pub won_both_orders: bool,
}
fn quantile<T: Ord + Copy>(samples: &[T], percentile: usize) -> Option<T> {
    if samples.is_empty() {
        return None;
    }
    let mut ordered = samples.to_vec();
    ordered.sort();
    Some(ordered[(ordered.len() * percentile).div_ceil(100).saturating_sub(1)])
}
pub struct MeasureInput<'a> {
    pub source: MeasurementSource,
    pub cards: usize,
    pub assembly_micros: &'a [u64],
    pub labels: &'a [LabelPair],
    pub plan_extra_tokens: Option<u64>,
    pub plan_nodes: usize,
    pub receipts: &'a [KitReceipt],
    pub mission_cost_micros: Option<u64>,
    pub observed_resident_tokens: &'a [(u32, u32)],
    pub gaps: &'a [GapSample],
    pub covered_start_delays_micros: &'a [u64],
    pub card_writes: &'a [CardSample],
    pub passes: &'a [PassSample],
}
pub fn measure(input: &MeasureInput<'_>) -> Measurements {
    let p50 = quantile(input.assembly_micros, 50);
    let p99 = quantile(input.assembly_micros, 99);
    let assembly_target =
        if input.cards == 2000 && input.assembly_micros.len() >= 100 { p50.zip(p99).map(|(a, b)| a <= 20000 && b <= 200000) } else { None };
    let label_times: Vec<i128> = input.labels.iter().map(|p| i128::from(p.new_micros) - i128::from(p.old_micros)).collect();
    let label50 = quantile(&label_times, 50);
    let label90 = quantile(&label_times, 90);
    let tokens = input.labels.iter().map(|p| p.new_output_tokens.saturating_sub(p.old_output_tokens)).max();
    let cost = input.labels.iter().map(|p| p.new_cost_micros.saturating_sub(p.old_cost_micros)).max();
    let label_target = if input.labels.len() == 22 {
        label50.zip(label90).zip(tokens.zip(cost)).map(|((a, b), (t, c))| a <= 300000 && b <= 500000 && t <= 150 && c <= 1000)
    } else {
        None
    };
    let plan_tokens = input.plan_extra_tokens.filter(|_| input.plan_nodes > 0).map(|t| t as f64 / input.plan_nodes as f64);
    let sum_tag = |tag: &str| -> Option<u64> {
        if input.receipts.is_empty() {
            return None;
        }
        input.receipts.iter().try_fold(0_u64, |sum, r| sum.checked_add(*r.costs.get(tag)?.as_ref()?))
    };
    let labels_cost = sum_tag("kit.label");
    let card_cost = sum_tag("kit.card");
    let gap_cost = sum_tag("kit.gap");
    let selection_cost = labels_cost.zip(card_cost).zip(gap_cost).and_then(|((a, b), c)| a.checked_add(b)?.checked_add(c));
    let selection_share =
        selection_cost.zip(input.mission_cost_micros).filter(|(_, total)| *total > 0).map(|(c, total)| c as f64 / total as f64);
    let gap_count = input.receipts.iter().map(|r| r.kit.gaps.len()).sum();
    let error = input
        .observed_resident_tokens
        .iter()
        .filter(|(_, actual)| *actual > 0)
        .map(|(estimate, actual)| (f64::from(*estimate) - f64::from(*actual)).abs() / f64::from(*actual))
        .reduce(f64::max);
    let attestation_complete = !input.receipts.is_empty()
        && input.receipts.iter().all(|r| {
            r.attestation.as_ref().is_some_and(|a| matches!(a.status, AttestationStatus::Matched | AttestationStatus::BestEffort))
        });
    let outcomes_complete = !input.receipts.is_empty() && input.receipts.iter().all(|r| r.outcome.is_some() && r.completed_at.is_some());
    let gap_tiers = [(GapTier::G0, 100000, 0), (GapTier::G1, 120000000, 100000), (GapTier::G2, 180000000, 150000)]
        .into_iter()
        .map(|(tier, limit, cost_limit)| {
            let samples: Vec<_> = input.gaps.iter().filter(|g| g.tier == tier).collect();
            let times: Vec<_> = samples.iter().map(|s| s.elapsed_micros).collect();
            let p90 = quantile(&times, 90);
            let known = samples.iter().all(|s| s.cost_micros.is_some());
            let cost = if known { samples.iter().filter_map(|s| s.cost_micros).max() } else { None };
            GapStats {
                tier,
                samples: samples.len(),
                p90_micros: p90,
                max_cost_micros: cost,
                target: p90.zip(cost).map(|(t, c)| t <= limit && c <= cost_limit),
            }
        })
        .collect();
    let covered_delay = input.covered_start_delays_micros.iter().copied().max();
    let card_target = if input.card_writes.is_empty() || input.card_writes.iter().any(|s| s.cost_micros.is_none()) {
        None
    } else {
        Some(
            input.card_writes.iter().all(|s| s.calls <= 1 && s.cost_micros.is_some_and(|c| c <= 3000))
                && (input.card_writes.len() != 100 || input.card_writes.iter().filter_map(|s| s.cost_micros).sum::<u64>() <= 300000),
        )
    };
    let pass_target = if input.passes.is_empty() {
        None
    } else {
        Some(
            input
                .passes
                .iter()
                .all(|p| (!p.kept || p.won_both_orders) && u128::from(p.pass_cost_micros) * 100 <= u128::from(p.piece_cost_micros) * 30),
        )
    };
    Measurements {
        source: input.source,
        cards: input.cards,
        samples: input.assembly_micros.len(),
        assembly_p50_micros: p50,
        assembly_p99_micros: p99,
        assembly_target,
        label_p50_extra_micros: label50,
        label_p90_extra_micros: label90,
        label_max_extra_tokens: tokens,
        label_max_extra_cost_micros: cost,
        label_target,
        plan_extra_tokens_per_node: plan_tokens,
        plan_target: plan_tokens.map(|t| t <= 40.0),
        attestation_complete,
        outcomes_complete,
        best_effort_sessions: input
            .receipts
            .iter()
            .filter(|r| r.attestation.as_ref().is_some_and(|a| a.status == AttestationStatus::BestEffort))
            .count(),
        selection_share,
        selection_target: selection_share.map(|s| s <= if gap_count > 0 { 0.03 } else { 0.01 }),
        gap_count,
        resident_max_relative_error: error,
        resident_target: if input.observed_resident_tokens.is_empty() {
            None
        } else {
            Some(input.observed_resident_tokens.iter().all(|(estimate, actual)| {
                if *actual == 0 { *estimate == 0 } else { (f64::from(*estimate) - f64::from(*actual)).abs() / f64::from(*actual) <= 0.2 }
            }))
        },
        card_writing_cost_micros: card_cost,
        pass_cost_micros: sum_tag("kit.pass"),
        real_goal_proven: false,
        gap_tiers,
        covered_start_delay_max_micros: covered_delay,
        covered_start_target: covered_delay.map(|d| d == 0),
        card_writing_target: card_target,
        pass_cost_target: pass_target,
    }
}
