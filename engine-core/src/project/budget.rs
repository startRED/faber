//! The count of requests in flight whose cost is not known yet, kept on the shared ledger. No processes or clock.
//!
//! Older versions kept a mission's grants and spending in the same record. They are left in the file exactly as they were
//! written (`extra`), never read and never dropped.
use super::Extra;
use crate::ledger::Ledger;
use crate::model::Vendor;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &str = "missionBudget";
const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    pub schema: u32,
    /// Active Chat/auxiliary requests per vendor.
    pub unbounded: BTreeMap<Vendor, u64>,
    #[serde(default)]
    pub unbounded_at: BTreeMap<Vendor, f64>,
    pub last_at: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Budget {
    fn default() -> Self {
        Self { schema: SCHEMA, unbounded: BTreeMap::new(), unbounded_at: BTreeMap::new(), last_at: 0.0, extra: Extra::new() }
    }
}

impl Budget {
    /// The record on the ledger, or an empty one; an unreadable one is `error.store.corrupt`.
    pub fn load(ledger: &Ledger) -> Result<Self, &'static str> {
        let Some(value) = ledger.extra.get(KEY) else { return Ok(Self::default()) };
        let b: Self = serde_json::from_value(value.clone()).map_err(|_| "error.store.corrupt")?;
        if b.schema != SCHEMA || !b.last_at.is_finite() || b.last_at < 0.0 || b.unbounded_at.values().any(|at| !at.is_finite() || *at < 0.0)
        {
            return Err("error.store.corrupt");
        }
        Ok(b)
    }
}

/// Counts one more request in flight for `vendor`, on Chat's hot path.
pub fn start_unbounded(ledger: &mut Ledger, vendor: Vendor, now: f64) -> Result<(), &'static str> {
    if !now.is_finite() || now < 0.0 {
        return Err("reason.budget.clock");
    }
    if !ledger.extra.contains_key(KEY) {
        ledger.extra.insert(KEY.into(), serde_json::to_value(Budget::default()).map_err(|_| "error.store.corrupt")?);
    }
    let b = ledger.extra.get_mut(KEY).and_then(serde_json::Value::as_object_mut).ok_or("error.store.corrupt")?;
    let active = b.get("unbounded").and_then(serde_json::Value::as_object).ok_or("error.store.corrupt")?;
    let n = active.get(vendor.name()).map_or(Ok(0), |v| v.as_u64().ok_or("error.store.corrupt"))?;
    let count = n.checked_add(1).ok_or("reason.budget.cost")?;
    let dates = b.entry("unboundedAt").or_insert_with(|| serde_json::json!({})).as_object_mut().ok_or("error.store.corrupt")?;
    dates.insert(vendor.name().into(), serde_json::json!(now));
    b["unbounded"][vendor.name()] = serde_json::json!(count);
    Ok(())
}

pub fn finish_unbounded(ledger: &mut Ledger, vendor: Vendor) -> Result<(), &'static str> {
    let Some(b) = ledger.extra.get_mut(KEY) else { return Ok(()) };
    let active = b.get_mut("unbounded").and_then(serde_json::Value::as_object_mut).ok_or("error.store.corrupt")?;
    let n = active.get(vendor.name()).map_or(Ok(0), |v| v.as_u64().ok_or("error.store.corrupt"))?;
    active.insert(vendor.name().into(), serde_json::json!(n.saturating_sub(1)));
    Ok(())
}

/// A persisted request from a dead engine is uncertain spend, not a permanently running process.
pub fn recover_unbounded(ledger: &mut Ledger) -> Result<bool, &'static str> {
    let b = Budget::load(ledger)?;
    let mut changed = false;
    for (vendor, n) in b.unbounded {
        if n == 0 {
            continue;
        }
        let at = b
            .unbounded_at
            .get(&vendor)
            .copied()
            .unwrap_or_else(|| ledger.of(vendor).readings.values().map(|r| r.read_at).reduce(f64::max).unwrap_or(b.last_at));
        ledger.of_mut(vendor).spend(None, true, at, 0);
        let count = ledger.extra.get_mut(KEY).ok_or("error.store.corrupt")?;
        count["unbounded"][vendor.name()] = serde_json::json!(0);
        changed = true;
    }
    Ok(changed)
}
