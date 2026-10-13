//! Software compilation is code. It grants no process, network or publication authority.
use super::{Extra, Plan, Task, bar, wire::*};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct Compilation {
    pub plan: Plan,
    pub receipt: CompileReceipt,
}

/// Identities come from admitted driver sessions, never from worker-provided JSON.
#[derive(Clone)]
pub struct BarDraft {
    pub vendor: crate::model::Vendor,
    pub session: String,
    pub role: super::Role,
    pub checks: Vec<BarCheck>,
}

pub fn merge_software_miss(
    root: &Path,
    lead: crate::model::Vendor,
    producer_session: &str,
    a: &BarDraft,
    b: &BarDraft,
    critic: &BarDraft,
) -> Result<Vec<BarCheck>, bar::BarError> {
    if a.vendor == b.vendor
        || critic.vendor == lead
        || a.role != super::Role::Research
        || b.role != super::Role::Research
        || critic.role != super::Role::Reviewer
        || [a, b, critic].iter().any(|d| d.session.is_empty() || d.session == producer_session)
        || a.session == b.session
        || critic.session == a.session
        || critic.session == b.session
    {
        return Err(bar::BarError::Malformed("isolated-software-drafts".into()));
    }
    for draft in [a, b, critic] {
        for check in &draft.checks {
            check.validate().map_err(|_| bar::BarError::Malformed(check.id.clone()))?;
            bar::validate_ground(root, &check.ground)?;
        }
    }
    let merged = bar::merge_drafters(&a.checks, &b.checks);
    let result = bar::apply_critic(&merged, &critic.checks)?;
    for piece in result.iter().map(|c| &c.piece).collect::<std::collections::BTreeSet<_>>() {
        bar::filter_and_validate_bar(piece, &result)?;
    }
    Ok(result)
}

fn phase(kind: CompilePhaseKind, tag: &str, started_at: u64, started: Instant) -> CompilePhase {
    CompilePhase {
        phase: kind,
        started_at,
        duration_micros: started.elapsed().as_micros() as u64,
        cost_tag: tag.into(),
        cost_micros: Some(0),
    }
}
fn timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_micros() as u64)
}

pub fn plan_digest(plan: &Plan) -> String {
    super::lease::hash(&serde_json::to_vec(plan).expect("plain plan serializes"))
}

/// Reserve immutable result identities before dispatch; outcomes arrive later.
pub fn check_receipt_id(mission: &str, revision: u64, task: &str) -> String {
    super::lease::hash(&serde_json::to_vec(&(mission, revision, task)).expect("string tuple serializes"))
}

/// No model calls are hidden in compilation. A library miss and URL grounding
/// require externally recorded evidence; they cannot be replaced by a default pass.
pub fn compile_software(
    root: &Path,
    mission: &str,
    revision: u64,
    candidate: &str,
    sheet: DeliverableSheet,
    tasks: Vec<Task>,
    additions: Vec<BarCheck>,
) -> Result<Compilation, bar::BarError> {
    compile(root, mission, revision, candidate, sheet, tasks, additions, false)
}

pub struct GroundedAddition {
    pub check: BarCheck,
    pub quote: String,
}

/// Official documents are fetched with an explicit source grant. Failed/offline or
/// unsupported quotations stop compilation rather than becoming grounded additions.
#[allow(clippy::too_many_arguments)]
pub async fn compile_software_grounded(
    root: &Path,
    journal: &super::store::Journal,
    mission: &str,
    revision: u64,
    candidate: &str,
    sheet: DeliverableSheet,
    tasks: Vec<Task>,
    additions: Vec<GroundedAddition>,
    authorized: &std::collections::BTreeSet<String>,
) -> Result<Compilation, bar::BarError> {
    let mut checks = Vec::new();
    for addition in additions {
        let ground = addition.check.ground.clone();
        let owned_root = root.to_path_buf();
        tokio::task::spawn_blocking(move || bar::validate_ground(&owned_root, &ground))
            .await
            .map_err(|_| bar::BarError::Malformed("ground-worker".into()))??;
        let mut check = addition.check;
        if matches!(check.ground.kind, GroundKind::Standard | GroundKind::Documentation) {
            if addition.quote.trim().is_empty() || addition.quote.len() > MAX_TEXT {
                return Err(bar::BarError::InvalidGround(check.id));
            }
            let response = super::proof::native::fetch_documentation(&check.ground.reference, authorized, journal)
                .await
                .map_err(|_| bar::BarError::InvalidGround(check.id.clone()))?;
            let body = response.body.ok_or_else(|| bar::BarError::InvalidGround(check.id.clone()))?;
            let text = std::str::from_utf8(&body).map_err(|_| bar::BarError::InvalidGround(check.id.clone()))?;
            if !text.contains(&addition.quote) {
                return Err(bar::BarError::InvalidGround(check.id));
            }
            check.also.push(Ground { kind: GroundKind::Evidence, reference: format!("source:{}", response.receipt.id) });
        } else if check.ground.kind != GroundKind::Repository {
            return Err(bar::BarError::InvalidGround(check.id));
        }
        checks.push(check);
    }
    let root = root.to_path_buf();
    let mission = mission.to_string();
    let candidate = candidate.to_string();
    tokio::task::spawn_blocking(move || compile(&root, &mission, revision, &candidate, sheet, tasks, checks, true))
        .await
        .map_err(|_| bar::BarError::Malformed("compile-worker".into()))?
}

#[allow(clippy::too_many_arguments)]
fn compile(
    root: &Path,
    mission: &str,
    revision: u64,
    candidate: &str,
    sheet: DeliverableSheet,
    tasks: Vec<Task>,
    additions: Vec<BarCheck>,
    urls_fetched: bool,
) -> Result<Compilation, bar::BarError> {
    let mut phases = Vec::new();
    let at = timestamp();
    let start = Instant::now();
    sheet.validate().map_err(|_| bar::BarError::Malformed("sheet".into()))?;
    if !super::valid_id(mission) || revision == 0 || !super::git::valid_oid(candidate) {
        return Err(bar::BarError::Malformed("candidate".into()));
    }
    phases.push(phase(CompilePhaseKind::Sheet, "plan.sheet", at, start));
    let at = timestamp();
    let start = Instant::now();
    let library = bar::software_bar(&sheet);
    phases.push(phase(CompilePhaseKind::Bar, "plan.bar", at, start));
    let at = timestamp();
    let start = Instant::now();
    for addition in &additions {
        if addition.ground.kind != GroundKind::Repository && !urls_fetched {
            return Err(bar::BarError::InvalidGround(addition.id.clone()));
        }
        bar::validate_ground(root, &addition.ground)?;
    }
    let mut checks = bar::apply_critic(&library, &additions)?;
    phases.push(phase(CompilePhaseKind::Critic, "plan.critic", at, start));
    let at = timestamp();
    let start = Instant::now();
    for check in &mut checks {
        let owners = tasks.iter().filter(|t| t.checks.contains(&check.id)).collect::<Vec<_>>();
        if owners.len() != 1 {
            return Err(bar::BarError::Malformed(check.id.clone()));
        }
        check.owner = owners[0].id.clone();
    }
    let pieces = sheet.pieces.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    let (kept, dropped) = bar::back_chain_and_prune(tasks, &checks, &pieces);
    bar::validate_compiler_graph(&kept, &checks, &pieces).map_err(|e| bar::BarError::Malformed(e.key().into()))?;
    let ordered = bar::order_kill_early(&kept, &checks).map_err(|e| bar::BarError::Malformed(e.key().into()))?;
    phases.push(phase(CompilePhaseKind::Steps, "plan.compile", at, start));
    let at = timestamp();
    let start = Instant::now();
    let id = format!("compile-{revision}");
    let mut plan = Plan::new(mission, revision, ordered);
    plan.sheet = Some(sheet);
    plan.bar = checks;
    plan.compile_receipt = Some(id.clone());
    plan.validate().map_err(|e| bar::BarError::Malformed(e.key().into()))?;
    phases.push(phase(CompilePhaseKind::Code, "plan.compile", at, start));
    let mut extra = Extra::new();
    extra.insert("planSha256".into(), serde_json::json!(plan_digest(&plan)));
    extra.insert(
        "notApplicable".into(),
        serde_json::json!(
            plan.bar
                .iter()
                .filter(|c| c.from == "library:software@2:not-applicable")
                .map(|c| serde_json::json!({"check":c.id,"piece":c.piece,"paths":c.paths,"reason":"reason.proof.notApplicable"}))
                .collect::<Vec<_>>()
        ),
    );
    extra.insert("criticStatus".into(), serde_json::json!(if additions.is_empty() { "not-run-library-hit" } else { "recorded-additions" }));
    let receipt = CompileReceipt {
        schema: super::SCHEMA,
        id,
        mission: mission.into(),
        plan_revision: revision,
        candidate: candidate.into(),
        phases,
        check_receipts: plan
            .bar
            .iter()
            .map(|c| &c.owner)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|task| check_receipt_id(mission, revision, task))
            .collect(),
        dropped,
        extra,
    };
    Ok(Compilation { plan, receipt })
}

pub fn faber_facts(mission: &str, sheet: &DeliverableSheet) -> FactSheet {
    let readers = sheet.pieces.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    FactSheet {
        schema: super::SCHEMA,
        mission: mission.into(),
        facts: vec![
            Fact {
                key: "users.public".into(),
                value: "true".into(),
                source: "generated-pipelines §15.1: Faber public users".into(),
                readers: readers.clone(),
            },
            Fact {
                key: "users.others-data".into(),
                value: "false".into(),
                source: "generated-pipelines §15.1: no other people's data".into(),
                readers: readers.clone(),
            },
            Fact {
                key: "device.platforms".into(),
                value: "Windows, macOS, Linux".into(),
                source: "generated-pipelines §15.1: desktop platforms".into(),
                readers,
            },
        ],
        extra: Extra::new(),
    }
}

use super::{Contract, SCHEMA, lease::normalized, wire::check_id};
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
