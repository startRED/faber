//! T5B owns compilation. The runner accepts only its validated, grounded graph; no fallback compiler.
use super::{Contract, Plan, SCHEMA, lease::normalized, wire::check_id};

/// The callback revalidates T5B's durable compile receipt and complete bar. A missing compiler is an error, never success.
pub fn accept(plan: Plan, contracts: &[Contract], compile: impl FnOnce(&Plan) -> Result<(), &'static str>) -> Result<Plan, &'static str> {
    plan.validate().map_err(|e| e.key())?;
    if plan.schema != SCHEMA || plan.tasks.is_empty() || plan.compile_receipt.as_ref().is_none_or(|id| !super::valid_id(id)) {
        return Err("reason.project.compile");
    }
    for task in &plan.tasks {
        if task.checks.is_empty() || task.paths.is_empty() || task.checks.len() > 64 || task.paths.len() > 64 {
            return Err("reason.project.generic");
        }
        for check in &task.checks {
            check_id(check).map_err(|e| e.key())?;
        }
        for path in &task.paths {
            normalized(path).map_err(|e| e.key())?;
        }
        for pin in &task.contracts {
            if !contracts.iter().any(|c| c.id == pin.id && c.digest == pin.digest && c.verify()) {
                return Err("reason.contract.revised");
            }
        }
    }
    compile(&plan)?;
    Ok(plan)
}
