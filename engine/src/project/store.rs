//! The mission journal. Plain records live in the repository's `.faber/missions/<id>/` folder, where people and other
//! tools can read them: `mission.json`, `plan.json`, and one file per contract, brief, decision, artifact and intent.
//! CLI sessions and leases live in Faber's data folder (`missions/<id>/runtime.json`). The authoritative effect ledger
//! lives beside them (`effects.json`); repository receipts are readable copies and cannot release safety authority.
//!
//! Every write is an atomic replacement that returns its fault, and every read tells a missing record from a damaged or
//! newer one. Nothing here falls back to empty data: a mission that cannot be read or saved stops taking effects.

use super::{Artifact, Brief, Contract, DecidedBy, Decision, Intent, Mission, Outcome, Plan, Receipt, Record, Runtime, SCHEMA, valid_id};
use crate::store::{Disk, Fault, RealDisk, read_checked, replace};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

type MissionSink = dyn Fn(&Mission) + Send + Sync;
type Observers = BTreeMap<PathBuf, std::sync::Weak<MissionSink>>;
static MISSION_OBSERVERS: std::sync::OnceLock<std::sync::Mutex<Observers>> = std::sync::OnceLock::new();

/// One shell owner observes private checkpoints; workers cannot publish lifecycle events.
pub struct MissionSubscription {
    root: PathBuf,
    sink: Arc<MissionSink>,
}

impl Drop for MissionSubscription {
    fn drop(&mut self) {
        let mut observers = MISSION_OBSERVERS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
        if observers.get(&self.root).is_some_and(|sink| sink.ptr_eq(&Arc::downgrade(&self.sink))) {
            observers.remove(&self.root);
        }
    }
}

pub fn observe_missions(data: &Path, sink: Arc<MissionSink>) -> Result<MissionSubscription, JournalError> {
    let root = super::lease::real_path(&data.join("missions")).map_err(|_| Fault::Read)?;
    let mut observers = MISSION_OBSERVERS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if observers.get(&root).is_some_and(|old| old.upgrade().is_some()) {
        return Err(JournalError::Mismatch);
    }
    observers.insert(root.clone(), Arc::downgrade(&sink));
    Ok(MissionSubscription { root, sink })
}

/// Why the journal refused, as a key the screen translates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalError {
    Store(Fault),
    /// An id that could name a file outside its folder.
    BadId,
    /// A record of another mission.
    Mismatch,
    /// A record written by a newer Faber with an incompatible schema.
    Newer,
    /// The intent is already recorded: the effect may already have happened.
    IntentExists,
    NoIntent,
    ReceiptExists,
}

impl JournalError {
    pub fn key(self) -> &'static str {
        match self {
            JournalError::Store(fault) => fault.key(),
            JournalError::BadId => "error.journal.badId",
            JournalError::Mismatch => "error.journal.mismatch",
            JournalError::Newer => "error.journal.newer",
            JournalError::IntentExists => "error.journal.intentExists",
            JournalError::NoIntent => "error.journal.noIntent",
            JournalError::ReceiptExists => "error.journal.receiptExists",
        }
    }
}

impl From<Fault> for JournalError {
    fn from(fault: Fault) -> Self {
        JournalError::Store(fault)
    }
}

/// `receipts/<intent>.json`: the intent, and its receipt once the effect is known.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Entry {
    intent: Intent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    receipt: Option<Receipt>,
    #[serde(flatten)]
    extra: super::Extra,
}

impl Record for Entry {
    fn schema(&self) -> u32 {
        self.intent.schema.max(self.receipt.as_ref().map_or(0, |r| r.schema))
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Effects {
    entries: Vec<Entry>,
}

impl Effects {
    fn pending(self) -> Vec<Intent> {
        let mut open: Vec<Intent> = self
            .entries
            .into_iter()
            .filter(|e| e.receipt.as_ref().is_none_or(|r| r.outcome == Outcome::Unknown))
            .map(|e| e.intent)
            .collect();
        open.sort_by(|a, b| (a.at, &a.id).cmp(&(b.at, &b.id)));
        open
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedProof {
    schema: u32,
    task: String,
    candidate: super::proof::CheckCandidate,
    outcome: super::wire::CheckOutcome,
    #[serde(flatten)]
    extra: super::Extra,
}
impl Record for CachedProof {
    fn schema(&self) -> u32 {
        self.schema
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SheetRecord {
    schema: u32,
    mission: String,
    sheet: super::wire::DeliverableSheet,
    #[serde(flatten)]
    extra: super::Extra,
}
impl Record for SheetRecord {
    fn schema(&self) -> u32 {
        self.schema
    }
}

pub(crate) fn verify_artifacts<'a>(
    root: &Path,
    artifacts: impl IntoIterator<Item = &'a super::wire::ArtifactRef>,
) -> Result<(), JournalError> {
    for art in artifacts {
        art.validate().map_err(|_| JournalError::Mismatch)?;
        super::kit::delivery::scoped_path(&art.path).map_err(|_| JournalError::Mismatch)?;
        let path = super::lease::normalized(&art.path).map_err(|_| JournalError::Mismatch)?;
        let file = super::files::ScopedFile::new(&root.join(path), false).map_err(|_| JournalError::Mismatch)?;
        if file.hash().map_err(|_| JournalError::Mismatch)?.as_deref() != Some(&art.sha256) {
            return Err(JournalError::Mismatch);
        }
    }
    Ok(())
}
impl Record for Effects {
    fn schema(&self) -> u32 {
        self.entries.iter().map(Record::schema).max().unwrap_or(SCHEMA)
    }
}

#[derive(Clone)]
pub struct Journal {
    mission: String,
    records: PathBuf,
    private: PathBuf,
    disk: Arc<dyn Disk>,
}

#[cfg(test)]
mod pending_path_tests {
    use super::*;
    #[test]
    fn public_pending_preserves_owner_relative_paths_while_private_guard_refuses_them() {
        let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let data = PathBuf::from(root.path().file_name().unwrap()).join("data");
        let journal = Journal::open(&root.path().join("repo"), &data, "m1").unwrap();
        let intent = Intent::new("dispatch", "m1", super::super::IntentKind::Dispatch, 1, 100);
        journal.record_intent(&intent).unwrap();
        assert_eq!(journal.pending().unwrap(), vec![intent]);
        assert!(Journal::pending_private(journal.private()).is_err());
    }

    /// Lessons row 5: what a role left running is a defect every saved report carries, recorded before or after it.
    #[test]
    fn a_report_names_what_the_roles_left_running() {
        use super::super::report::{LeftoverReport, MAX_LEFTOVERS, MissionReport};
        let root = tempfile::tempdir().unwrap();
        let journal = Journal::open(&root.path().join("repo"), &root.path().join("data"), "m1").unwrap();
        let many: Vec<String> = (0..MAX_LEFTOVERS + 3).map(|n| format!("p{n}.exe")).collect();
        journal.record_leftovers(&LeftoverReport::new("t1-1", &["ping.exe".into()])).unwrap();
        journal.save_report(&MissionReport::new("m1", "goal", "done")).unwrap();
        assert_eq!(journal.report().unwrap().unwrap().leftovers, [LeftoverReport::new("t1-1", &["ping.exe".into()])]);
        journal.record_leftovers(&LeftoverReport::new("t2-1", &many)).unwrap();
        let report = journal.report().unwrap().unwrap();
        journal.save_report(&report).unwrap();
        let saved = journal.report().unwrap().unwrap().leftovers;
        assert_eq!((saved.len(), saved[1].count, saved[1].processes.len()), (2, MAX_LEFTOVERS + 3, MAX_LEFTOVERS));
        assert!(journal.record_leftovers(&LeftoverReport::new("../x", &["a.exe".into()])).is_err(), "an id is never a path");
        // A report with nothing left running keeps its old shape.
        let clean = Journal::open(&root.path().join("repo"), &root.path().join("data"), "m2").unwrap();
        clean.save_report(&MissionReport::new("m2", "goal", "done")).unwrap();
        assert!(!std::fs::read_to_string(clean.records().join("report.json")).unwrap().contains("leftovers"));
    }
}

impl Journal {
    pub(crate) fn mission_checkpointed(&self, mission: &Mission) {
        let sink = self.private.parent().and_then(|root| super::lease::real_path(root).ok()).and_then(|root| {
            MISSION_OBSERVERS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner()).get(&root).and_then(|s| s.upgrade())
        });
        if let Some(sink) = sink {
            sink(mission);
        }
    }
    /// The journal of mission `mission` for the repository whose working folder is `repo`, with its private sidecar
    /// under `data_dir`. Creates both folders.
    pub fn open(repo: &Path, data_dir: &Path, mission: &str) -> Result<Journal, JournalError> {
        Journal::open_with(repo, data_dir, mission, Arc::new(RealDisk))
    }

    /// The same, writing through `disk`, so tests can make writes fail.
    pub fn open_with(repo: &Path, data_dir: &Path, mission: &str, disk: Arc<dyn Disk>) -> Result<Journal, JournalError> {
        if !valid_id(mission) {
            return Err(JournalError::BadId);
        }
        let records = repo.join(".faber").join("missions").join(mission);
        let private = data_dir.join("missions").join(mission);
        let fresh = !private.exists();
        for dir in [&records, &private] {
            fs::create_dir_all(dir).map_err(|e| Fault::of_write(&e))?;
        }
        let journal = Journal { mission: mission.into(), records, private, disk };
        if fresh {
            journal.put(&journal.private.join("effects.json"), &Effects::default())?;
        }
        Ok(journal)
    }

    pub fn records(&self) -> &Path {
        &self.records
    }
    pub fn mission_id(&self) -> &str {
        &self.mission
    }

    pub fn private(&self) -> &Path {
        &self.private
    }

    pub(crate) fn put<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), JournalError> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| Fault::of_write(&e))?;
        }
        let json = serde_json::to_string_pretty(value).expect("plain data always serializes");
        Ok(replace(&*self.disk, path, json.as_bytes())?)
    }

    pub(crate) fn get<T: DeserializeOwned + Record>(&self, path: &Path) -> Result<Option<T>, JournalError> {
        match read_checked::<T>(path)? {
            Some(record) if record.schema() > SCHEMA => Err(JournalError::Newer),
            found => Ok(found),
        }
    }

    fn named(&self, folder: &str, id: &str) -> Result<PathBuf, JournalError> {
        if !valid_id(id) {
            return Err(JournalError::BadId);
        }
        Ok(self.records.join(folder).join(format!("{id}.json")))
    }

    /// Every record in `folder`, by file name. One damaged record fails the whole list.
    fn list<T: DeserializeOwned + Record>(&self, folder: &str) -> Result<Vec<T>, JournalError> {
        self.list_at(&self.records.join(folder))
    }

    fn list_at<T: DeserializeOwned + Record>(&self, folder: &Path) -> Result<Vec<T>, JournalError> {
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
            Err(_) => return Err(Fault::Read.into()),
        };
        let mut paths = vec![];
        for entry in entries {
            let path = entry.map_err(|_| Fault::Read)?.path();
            if path.extension().is_some_and(|x| x == "json") {
                paths.push(path);
            }
        }
        paths.sort();
        let mut out = vec![];
        for path in paths {
            out.extend(self.get(&path)?);
        }
        Ok(out)
    }

    fn same_mission(&self, mission: &str) -> Result<(), JournalError> {
        if mission == self.mission { Ok(()) } else { Err(JournalError::Mismatch) }
    }

    pub fn save_mission(&self, mission: &Mission) -> Result<(), JournalError> {
        self.same_mission(&mission.id)?;
        self.put(&self.records.join("mission.json"), mission)
    }

    pub fn mission(&self) -> Result<Option<Mission>, JournalError> {
        self.get(&self.records.join("mission.json"))
    }

    pub fn save_plan(&self, plan: &Plan) -> Result<(), JournalError> {
        self.same_mission(&plan.mission)?;
        plan.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
        self.put(&self.records.join("plan.json"), plan)
    }

    pub fn plan(&self) -> Result<Option<Plan>, JournalError> {
        let plan: Option<Plan> = self.get(&self.records.join("plan.json"))?;
        if let Some(plan) = &plan {
            self.same_mission(&plan.mission)?;
            // Read legacy software metadata verbatim; current acceptance validates it separately.
            let mut graph = plan.clone();
            graph.sheet = None;
            graph.bar.clear();
            graph.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
        }
        Ok(plan)
    }

    pub fn save_contract(&self, contract: &Contract) -> Result<(), JournalError> {
        self.put(&self.named("contracts", &contract.id)?, contract)
    }

    pub fn contract(&self, id: &str) -> Result<Option<Contract>, JournalError> {
        self.get(&self.named("contracts", id)?)
    }

    pub fn contracts(&self) -> Result<Vec<Contract>, JournalError> {
        self.list("contracts")
    }

    pub fn save_brief(&self, brief: &Brief) -> Result<(), JournalError> {
        self.put(&self.named("briefs", &brief.task)?, brief)
    }

    pub fn brief(&self, task: &str) -> Result<Option<Brief>, JournalError> {
        self.get(&self.named("briefs", task)?)
    }

    pub fn save_decision(&self, decision: &Decision) -> Result<(), JournalError> {
        self.put(&self.named("decisions", &decision.id)?, decision)
    }

    pub fn decision(&self, id: &str) -> Result<Option<Decision>, JournalError> {
        self.get(&self.named("decisions", id)?)
    }

    pub fn decisions(&self) -> Result<Vec<Decision>, JournalError> {
        self.list("decisions")
    }

    pub fn save_artifact(&self, artifact: &Artifact) -> Result<(), JournalError> {
        self.put(&self.named("artifacts", &artifact.id)?, artifact)
    }

    pub fn artifact(&self, id: &str) -> Result<Option<Artifact>, JournalError> {
        self.get(&self.named("artifacts", id)?)
    }

    pub fn artifacts(&self) -> Result<Vec<Artifact>, JournalError> {
        self.list("artifacts")
    }

    /// Records that an effect is about to happen. Refused when the intent is already there: it may have happened.
    pub fn record_intent(&self, intent: &Intent) -> Result<(), JournalError> {
        self.same_mission(&intent.mission)?;
        self.named("receipts", &intent.id)?;
        if self.private.join("closure.json").exists() {
            return Err(JournalError::Mismatch);
        }
        let mut effects = self.effects()?;
        if effects.entries.iter().any(|e| e.intent.id == intent.id) {
            return Err(JournalError::IntentExists);
        }
        let entry = Entry { intent: intent.clone(), receipt: None, extra: Default::default() };
        effects.entries.push(entry.clone());
        self.put(&self.private.join("effects.json"), &effects)?;
        self.put(&self.named("receipts", &intent.id)?, &entry)
    }

    /// Records how an intended effect ended, once.
    pub fn record_receipt(&self, receipt: &Receipt) -> Result<(), JournalError> {
        self.save_receipt(receipt, None)
    }

    /// Only trusted person input may resolve an ambiguous outcome; the prior receipt and decision remain in the ledger.
    pub fn reconcile(&self, receipt: &Receipt, decision: &Decision) -> Result<(), JournalError> {
        if decision.answered_by != Some(DecidedBy::Person)
            || decision.answer.as_ref().is_none_or(|s| s.trim().is_empty())
            || decision.answered_at.is_none()
            || receipt.outcome == Outcome::Unknown
            || receipt.evidence.is_empty()
        {
            return Err(JournalError::Mismatch);
        }
        self.save_receipt(receipt, Some(decision))
    }

    fn save_receipt(&self, receipt: &Receipt, decision: Option<&Decision>) -> Result<(), JournalError> {
        self.named("receipts", &receipt.intent)?;
        let mut effects = self.effects()?;
        let entry = effects.entries.iter_mut().find(|e| e.intent.id == receipt.intent).ok_or(JournalError::NoIntent)?;
        if entry.receipt.is_some() && (decision.is_none() || entry.receipt.as_ref().is_some_and(|r| r.outcome != Outcome::Unknown)) {
            return Err(JournalError::ReceiptExists);
        }
        if let Some(decision) = decision {
            entry.extra.insert("reconciliation".into(), serde_json::json!({"decision": decision, "previous": entry.receipt}));
        }
        entry.receipt = Some(receipt.clone());
        let entry = entry.clone();
        self.put(&self.private.join("effects.json"), &effects)?;
        self.put(&self.named("receipts", &receipt.intent)?, &entry)
    }

    fn effects(&self) -> Result<Effects, JournalError> {
        match self.get(&self.private.join("effects.json"))? {
            Some(effects) => Ok(effects),
            None => Err(Fault::Corrupt.into()),
        }
    }

    pub(crate) fn initialize_effects(&self) -> Result<(), JournalError> {
        let effects = self.effects()?;
        self.put(&self.private.join("effects.json"), &effects)
    }

    pub(crate) fn close(&self, evidence: &serde_json::Value) -> Result<(), JournalError> {
        self.put(&self.private.join("closure.json"), evidence)
    }

    /// Intents with no receipt or an uncertain outcome: recovery must reconcile before anything resumes.
    pub fn pending(&self) -> Result<Vec<Intent>, JournalError> {
        Ok(self.effects()?.pending())
    }

    pub(crate) fn pending_private(private: &Path) -> Result<Vec<Intent>, JournalError> {
        let bytes = super::files::ScopedFile::new(&private.join("effects.json"), false)
            .map_err(|_| JournalError::Store(Fault::Read))?
            .read_limited(16 * 1024 * 1024)
            .map_err(|_| JournalError::Store(Fault::Read))?
            .ok_or(Fault::Corrupt)?
            .0;
        let effects: Effects = serde_json::from_slice(&bytes).map_err(|_| Fault::Corrupt)?;
        if effects.schema() > SCHEMA {
            return Err(JournalError::Newer);
        }
        Ok(effects.pending())
    }

    pub fn record_check(&self, receipt: &super::wire::CheckReceipt) -> Result<(), JournalError> {
        if !valid_id(&receipt.id) || receipt.schema != SCHEMA {
            return Err(JournalError::Mismatch);
        }
        let path = self.private.join("checks").join(format!("{}.json", receipt.id));
        if self.get::<super::wire::CheckReceipt>(&path)?.is_some() {
            return Err(JournalError::ReceiptExists);
        }
        self.put(&path, receipt)?;
        self.put(&self.named("checks", &receipt.id)?, receipt)
    }
    pub fn check(&self, id: &str) -> Result<Option<super::wire::CheckReceipt>, JournalError> {
        if !valid_id(id) {
            return Err(JournalError::BadId);
        }
        self.get(&self.private.join("checks").join(format!("{id}.json")))
    }
    /// Read the authoritative private effect ledger, never its editable repository copy.
    pub fn effect_receipt(&self, id: &str) -> Result<Option<Receipt>, JournalError> {
        if !valid_id(id) {
            return Err(JournalError::BadId);
        }
        Ok(self.effects()?.entries.into_iter().find(|e| e.intent.id == id).and_then(|e| e.receipt))
    }
    pub fn save_runtime(&self, runtime: &Runtime) -> Result<(), JournalError> {
        self.put(&self.private.join("runtime.json"), runtime)
    }

    /// The private sidecar; none yet is an empty one, a damaged one is an error (it holds leases).
    pub fn runtime(&self) -> Result<Runtime, JournalError> {
        Ok(self.get(&self.private.join("runtime.json"))?.unwrap_or_default())
    }
}

impl Journal {
    fn kit_path(&self, task: &str, attempt: u32) -> Result<PathBuf, JournalError> {
        if !valid_id(task) || attempt == 0 {
            return Err(JournalError::BadId);
        }
        Ok(self.records.join("kits").join(format!("{task}-{attempt}.json")))
    }

    /// Persist before dispatch; an existing selection cannot be replaced by a retry.
    pub fn record_kit(&self, receipt: &super::kit::KitReceipt) -> Result<(), JournalError> {
        self.same_mission(&receipt.mission)?;
        if receipt.schema > SCHEMA {
            return Err(JournalError::Newer);
        }
        if receipt.outcome.is_some() || receipt.completed_at.is_some() || receipt.attestation.is_some() {
            return Err(JournalError::Mismatch);
        }
        receipt.kit.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
        let path = self.kit_path(&receipt.task, receipt.attempt)?;
        if self.get::<super::kit::KitReceipt>(&path)?.is_some() {
            return Err(JournalError::ReceiptExists);
        }
        self.put(&path, receipt)
    }

    pub fn kit(&self, task: &str, attempt: u32) -> Result<Option<super::kit::KitReceipt>, JournalError> {
        let found: Option<super::kit::KitReceipt> = self.get(&self.kit_path(task, attempt)?)?;
        if let Some(receipt) = &found {
            self.same_mission(&receipt.mission)?;
            if receipt.task != task || receipt.attempt != attempt {
                return Err(JournalError::Mismatch);
            }
            receipt.kit.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
        }
        Ok(found)
    }

    /// Record init before work. It remains evidence, never an admission grant.
    pub fn attest_kit(&self, task: &str, attempt: u32, attestation: &super::kit::Attestation) -> Result<(), JournalError> {
        let mut receipt = self.kit(task, attempt)?.ok_or(JournalError::NoIntent)?;
        if receipt.attestation.is_some() || receipt.outcome.is_some() {
            return Err(JournalError::ReceiptExists);
        }
        if attestation.vendor != receipt.kit.vendor || attestation.containment_proven {
            return Err(JournalError::Mismatch);
        }
        receipt.attestation = Some(attestation.clone());
        self.put(&self.kit_path(task, attempt)?, &receipt)
    }

    /// Completion can add measurements and outcomes, never change the approved selection.
    pub fn complete_kit(&self, receipt: &super::kit::KitReceipt) -> Result<(), JournalError> {
        let saved = self.kit(&receipt.task, receipt.attempt)?.ok_or(JournalError::NoIntent)?;
        self.same_mission(&receipt.mission)?;
        if saved.outcome.is_some() {
            return Err(JournalError::ReceiptExists);
        }
        if receipt.schema != saved.schema
            || receipt.kit != saved.kit
            || receipt.selected_at != saved.selected_at
            || receipt.completed_at.is_none_or(|at| at < saved.selected_at)
            || receipt.outcome.is_none()
            || receipt.attestation.as_ref().is_none_or(|a| a.vendor != receipt.kit.vendor || a.containment_proven)
            || saved.attestation.as_ref().is_none_or(|a| receipt.attestation.as_ref() != Some(a))
        {
            return Err(JournalError::Mismatch);
        }
        let outcome = receipt.outcome.as_ref().ok_or(JournalError::Mismatch)?;
        if outcome.kit_used.len() > 64 || !outcome.kit_used.iter().all(|id| receipt.kit.items.iter().any(|item| &item.id == id)) {
            return Err(JournalError::Mismatch);
        }
        let mut completed = receipt.clone();
        for (key, value) in saved.extra {
            if completed.extra.get(&key).is_some_and(|v| v != &value) {
                return Err(JournalError::Mismatch);
            }
            completed.extra.insert(key, value);
        }
        self.put(&self.kit_path(&receipt.task, receipt.attempt)?, &completed)
    }

    pub fn save_card_cache(&self, cache: &super::kit::CardCache) -> Result<(), JournalError> {
        if cache.schema() > SCHEMA {
            return Err(JournalError::Newer);
        }
        self.put(&self.records.join("cards.json"), cache)
    }
    pub fn card_cache(&self) -> Result<super::kit::CardCache, JournalError> {
        Ok(self.get(&self.records.join("cards.json"))?.unwrap_or_default())
    }
    /// Derive versioned Beta counts from immutable completed receipts, without double-settling a replay.
    pub fn kit_evidence(&self) -> Result<Vec<super::kit::Evidence>, JournalError> {
        let mut counts = BTreeMap::new();
        for receipt in self.list::<super::kit::KitReceipt>("kits")? {
            self.same_mission(&receipt.mission)?;
            receipt.kit.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
            let Some(outcome) = receipt.outcome else { continue };
            let (Some(first), Some(blocking)) = (outcome.proof_first_try, outcome.review_blocking) else { continue };
            for item in receipt.kit.items.iter().filter(|item| outcome.kit_used.contains(&item.id)) {
                let entry = counts.entry((item.id.clone(), item.version.clone())).or_insert(super::kit::Evidence {
                    id: item.id.clone(),
                    version: item.version.clone(),
                    kept: 0,
                    discarded: 0,
                });
                if first && blocking == 0 {
                    entry.kept += 1;
                } else {
                    entry.discarded += 1;
                }
            }
        }
        Ok(counts.into_values().collect())
    }
    fn field_brief_path(&self, need: &str) -> Result<PathBuf, JournalError> {
        let need = super::wire::normalize_need(need).map_err(|_| JournalError::BadId)?;
        let id = super::kit::digest(need.trim_end_matches('!').as_bytes());
        let faber = self.records.parent().and_then(Path::parent).ok_or(JournalError::Mismatch)?;
        Ok(faber.join("briefs").join(format!("{id}.json")))
    }
    pub fn save_field_brief(&self, brief: &super::kit::FieldBrief) -> Result<(), JournalError> {
        if brief.schema > SCHEMA {
            return Err(JournalError::Newer);
        }
        brief.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
        self.put(&self.field_brief_path(&brief.need)?, brief)
    }
    pub fn field_brief(&self, need: &str, now: u64, version: &str) -> Result<Option<super::kit::FieldBrief>, JournalError> {
        let brief: Option<super::kit::FieldBrief> = self.get(&self.field_brief_path(need)?)?;
        if let Some(value) = &brief {
            value.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
        }
        Ok(brief.filter(|b| !b.stale(now, version)))
    }

    pub fn save_sheet(&self, sheet: &super::wire::DeliverableSheet) -> Result<(), JournalError> {
        sheet.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
        let path = self.records.join("sheet-record.json");
        let previous: Option<SheetRecord> = self.get(&path)?;
        if let Some(previous) = &previous {
            self.same_mission(&previous.mission)?;
        }
        let record = SheetRecord {
            schema: SCHEMA,
            mission: self.mission.clone(),
            sheet: sheet.clone(),
            extra: previous.map(|r| r.extra).unwrap_or_default(),
        };
        self.put(&path, &record)?;
        // Keep the pre-versioning display file readable by last-good builds.
        self.put(&self.records.join("sheet.json"), sheet)
    }

    pub fn sheet(&self) -> Result<Option<super::wire::DeliverableSheet>, JournalError> {
        if let Some(record) = self.get::<SheetRecord>(&self.records.join("sheet-record.json"))? {
            self.same_mission(&record.mission)?;
            record.sheet.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
            return Ok(Some(record.sheet));
        }
        // Legacy display data is readable; compilation validates it before acceptance.
        self.get(&self.records.join("sheet.json"))
    }

    pub fn save_facts(&self, sheet: &super::wire::FactSheet) -> Result<(), JournalError> {
        self.same_mission(&sheet.mission)?;
        if sheet.schema > SCHEMA {
            return Err(JournalError::Newer);
        }
        for fact in &sheet.facts {
            fact.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
        }
        self.put(&self.records.join("facts.json"), sheet)
    }

    pub fn facts(&self) -> Result<Option<super::wire::FactSheet>, JournalError> {
        let found: Option<super::wire::FactSheet> = self.get(&self.records.join("facts.json"))?;
        if let Some(sheet) = &found {
            self.same_mission(&sheet.mission)?;
            for fact in &sheet.facts {
                fact.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
            }
        }
        Ok(found)
    }

    /// Saves the report with every leftover recorded so far (`record_leftovers`), whatever the caller put there.
    pub fn save_report(&self, report: &super::report::MissionReport) -> Result<(), JournalError> {
        self.same_mission(&report.mission)?;
        if report.schema > SCHEMA {
            return Err(JournalError::Newer);
        }
        let mut report = report.clone();
        report.leftovers = self.leftovers()?;
        self.put(&self.records.join("report.json"), &report)
    }

    /// What a role left running when its lease ended, kept privately until the next report is saved. One record per
    /// invocation; a repeated retirement writes the same record again.
    pub fn record_leftovers(&self, leftovers: &super::report::LeftoverReport) -> Result<(), JournalError> {
        if !valid_id(&leftovers.invocation) || leftovers.schema != SCHEMA || leftovers.processes.len() > super::report::MAX_LEFTOVERS {
            return Err(JournalError::Mismatch);
        }
        self.put(&self.private.join("leftovers").join(format!("{}.json", leftovers.invocation)), leftovers)
    }

    pub fn leftovers(&self) -> Result<Vec<super::report::LeftoverReport>, JournalError> {
        self.list_at(&self.private.join("leftovers"))
    }

    pub fn report(&self) -> Result<Option<super::report::MissionReport>, JournalError> {
        let found: Option<super::report::MissionReport> = self.get(&self.records.join("report.json"))?;
        if let Some(report) = &found {
            self.same_mission(&report.mission)?;
        }
        Ok(found)
    }

    pub fn list_checks(&self) -> Result<Vec<super::wire::CheckReceipt>, JournalError> {
        self.list_at(&self.private.join("checks"))
    }

    pub fn save_compile_receipt(&self, receipt: &super::wire::CompileReceipt) -> Result<(), JournalError> {
        self.same_mission(&receipt.mission)?;
        if receipt.schema > SCHEMA {
            return Err(JournalError::Newer);
        }
        super::proof::validate_compile_receipt(receipt, &receipt.candidate).map_err(|_| JournalError::Store(Fault::Corrupt))?;
        let path = self.named("compiles", &format!("rev-{}", receipt.plan_revision))?;
        if self.get::<super::wire::CompileReceipt>(&path)?.is_some()
            || self
                .get::<super::wire::CompileReceipt>(&self.private.join("compiles").join(format!("rev-{}.json", receipt.plan_revision)))?
                .is_some()
        {
            return Err(JournalError::ReceiptExists);
        }
        self.put(&self.private.join("compiles").join(format!("rev-{}.json", receipt.plan_revision)), receipt)?;
        self.put(&path, receipt)
    }

    pub fn compile_receipt(&self, plan_revision: u64) -> Result<Option<super::wire::CompileReceipt>, JournalError> {
        let found: Option<super::wire::CompileReceipt> = self.get(&self.named("compiles", &format!("rev-{plan_revision}"))?)?;
        if let Some(receipt) = &found {
            self.same_mission(&receipt.mission)?;
            if receipt.plan_revision != plan_revision {
                return Err(JournalError::Mismatch);
            }
            // A legacy display copy is not acceptance evidence; private records are validated at the gate.
        }
        Ok(found)
    }

    pub fn save_check_receipt(&self, receipt: &super::wire::CheckReceipt) -> Result<(), JournalError> {
        if !valid_id(&receipt.id) || !valid_id(&receipt.task) {
            return Err(JournalError::BadId);
        }
        if receipt.schema > SCHEMA {
            return Err(JournalError::Newer);
        }
        super::proof::validate_check_receipt(receipt, &receipt.candidate).map_err(|_| JournalError::Store(Fault::Corrupt))?;
        let path = self.named("checks", &receipt.id)?;
        if self.get::<super::wire::CheckReceipt>(&path)?.is_some()
            || self.get::<super::wire::CheckReceipt>(&self.private.join("checks").join(format!("{}.json", receipt.id)))?.is_some()
        {
            return Err(JournalError::ReceiptExists);
        }
        self.put(&self.private.join("checks").join(format!("{}.json", receipt.id)), receipt)?;
        self.put(&path, receipt)
    }

    pub fn check_receipt(&self, id: &str) -> Result<Option<super::wire::CheckReceipt>, JournalError> {
        let found: Option<super::wire::CheckReceipt> = self.get(&self.named("checks", id)?)?;
        if let Some(receipt) = &found
            && receipt.id != id
        {
            return Err(JournalError::Mismatch);
        }
        Ok(found)
    }

    /// Save both source intent and outcome before returning evidence to the planner.
    pub fn record_source(&self, receipt: &super::proof::native::SourceReceipt) -> Result<(), JournalError> {
        receipt.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
        let path = self.named("sources", &receipt.id)?;
        if self.get::<super::proof::native::SourceReceipt>(&path)?.is_some() {
            return Err(JournalError::ReceiptExists);
        }
        self.put(&path, receipt)
    }

    pub fn record_compilation(&self, plan: &Plan, receipt: &super::wire::CompileReceipt) -> Result<(), JournalError> {
        self.same_mission(&plan.mission)?;
        if receipt.mission != plan.mission
            || receipt.plan_revision != plan.revision
            || plan.compile_receipt.as_deref() != Some(&receipt.id)
            || receipt.extra.get("planSha256").and_then(serde_json::Value::as_str) != Some(&super::lead::plan_digest(plan))
        {
            return Err(JournalError::Mismatch);
        }
        plan.validate().map_err(|_| JournalError::Store(Fault::Corrupt))?;
        let sheet = plan.sheet.as_ref().ok_or(JournalError::Mismatch)?;
        self.save_compile_receipt(receipt)?;
        self.save_sheet(sheet)?;
        self.save_facts(&super::lead::faber_facts(&plan.mission, sheet))?;
        self.save_plan(plan)
    }

    /// Public receipts are display copies. Only private engine records authorize acceptance.
    pub fn verify_acceptance(&self, root: &Path, candidate: &str) -> Result<(), JournalError> {
        let plan = self.plan()?.ok_or(JournalError::Mismatch)?;
        plan.validate().map_err(|_| JournalError::Mismatch)?;
        let compile: super::wire::CompileReceipt =
            self.get(&self.private.join("compiles").join(format!("rev-{}.json", plan.revision)))?.ok_or(JournalError::Mismatch)?;
        super::proof::validate_compile_receipt(&compile, candidate).map_err(|_| JournalError::Mismatch)?;
        if plan.compile_receipt.as_deref() != Some(&compile.id)
            || compile.mission != self.mission
            || compile.extra.get("planSha256").and_then(serde_json::Value::as_str) != Some(&super::lead::plan_digest(&plan))
        {
            return Err(JournalError::Mismatch);
        }
        let mut receipts = Vec::new();
        for id in &compile.check_receipts {
            if !valid_id(id) {
                return Err(JournalError::BadId);
            }
            let receipt: super::wire::CheckReceipt =
                self.get(&self.private.join("checks").join(format!("{id}.json")))?.ok_or(JournalError::Mismatch)?;
            if receipt.id != *id {
                return Err(JournalError::Mismatch);
            }
            for art in receipt.artifacts.iter().chain(receipt.checks.iter().flat_map(|c| &c.evidence)) {
                verify_artifacts(root, std::iter::once(art))?;
            }
            receipts.push(receipt);
        }
        super::proof::validate_acceptance(&plan.bar, &receipts, candidate).map_err(|_| JournalError::Mismatch)
    }

    pub fn cache_check(
        &self,
        root: &Path,
        task: &str,
        candidate: &super::proof::CheckCandidate,
        outcome: &super::wire::CheckOutcome,
    ) -> Result<(), JournalError> {
        let mut candidate = candidate.clone();
        candidate.capture_environment();
        if !valid_id(task) || outcome.id != candidate.check_id {
            return Err(JournalError::Mismatch);
        }
        outcome.validate().map_err(|_| JournalError::Mismatch)?;
        verify_artifacts(root, &outcome.evidence)?;
        let inputs = candidate
            .inputs
            .iter()
            .map(|input| serde_json::from_str::<super::wire::ArtifactRef>(input).map_err(|_| JournalError::Mismatch))
            .collect::<Result<Vec<_>, _>>()?;
        if inputs.is_empty() {
            return Err(JournalError::Mismatch);
        }
        verify_artifacts(root, &inputs)?;
        let digest = super::proof::compute_check_digest(&candidate);
        let path = self.private.join("cache").join(task).join(format!("{digest}.json"));
        let extra = self.get::<CachedProof>(&path)?.map(|r| r.extra).unwrap_or_default();
        let record = CachedProof { schema: SCHEMA, task: task.into(), candidate: candidate.clone(), outcome: outcome.clone(), extra };
        self.put(&path, &record)?;
        if !outcome.is_pass() {
            return Err(JournalError::Mismatch);
        }
        Ok(())
    }

    pub fn reuse_check(
        &self,
        root: &Path,
        task: &str,
        candidate: &super::proof::CheckCandidate,
    ) -> Result<Option<super::wire::CheckOutcome>, JournalError> {
        let mut candidate = candidate.clone();
        candidate.capture_environment();
        if !valid_id(task) {
            return Err(JournalError::BadId);
        }
        let inputs = candidate
            .inputs
            .iter()
            .map(|input| serde_json::from_str::<super::wire::ArtifactRef>(input).map_err(|_| JournalError::Mismatch))
            .collect::<Result<Vec<_>, _>>()?;
        if inputs.is_empty() || verify_artifacts(root, &inputs).is_err() {
            return Ok(None);
        }
        let digest = super::proof::compute_check_digest(&candidate);
        let Some(record): Option<CachedProof> = self.get(&self.private.join("cache").join(task).join(format!("{digest}.json")))? else {
            return Ok(None);
        };
        if record.task != task || record.candidate != candidate || record.outcome.id != candidate.check_id {
            return Err(JournalError::Mismatch);
        }
        record.outcome.validate().map_err(|_| JournalError::Mismatch)?;
        if !record.outcome.is_pass() {
            return Ok(None);
        }
        verify_artifacts(root, &record.outcome.evidence)?;
        Ok(Some(record.outcome))
    }
}
#[cfg(test)]
mod kit_path_tests {
    use super::*;
    #[test]
    fn review_shallow_repo_still_has_two_parents_above_mission_records() {
        for repo in ["", ".", "repo"] {
            let repo = Path::new(repo);
            let journal = Journal {
                mission: "m1".into(),
                records: repo.join(".faber").join("missions").join("m1"),
                private: PathBuf::new(),
                disk: Arc::new(RealDisk),
            };
            assert_eq!(
                journal.field_brief_path("framework:fixture").unwrap(),
                repo.join(".faber").join("briefs").join(format!("{}.json", super::super::kit::digest(b"framework:fixture")))
            );
        }
    }
}

/// The envelope a role's turn was accepted with, kept so the coordinator that acts on it (read, patch, proof,
/// complete) can come back for it. Written once per invocation; the raw model text is never stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyRecord {
    pub schema: u32,
    pub invocation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<super::wire::Envelope>,
    /// The key of a reply the worker contract refused or stalled, kept so the coordinator can see why none was accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
}
impl Record for ReplyRecord {
    fn schema(&self) -> u32 {
        self.schema
    }
}

impl Journal {
    /// Keeps the accepted envelope of one invocation. A second write for the same invocation is refused.
    pub fn record_reply(&self, invocation: &str, envelope: &super::wire::Envelope) -> Result<(), JournalError> {
        self.keep_reply(invocation, Some(envelope.clone()), None)
    }

    /// Keeps why a turn ended with no accepted envelope (a bounce, a stall or a block), by key only.
    pub fn record_refusal(&self, invocation: &str, key: &str) -> Result<(), JournalError> {
        self.keep_reply(invocation, None, Some(key.into()))
    }

    fn keep_reply(&self, invocation: &str, envelope: Option<super::wire::Envelope>, refused: Option<String>) -> Result<(), JournalError> {
        let path = self.named("replies", invocation)?;
        if self.get::<ReplyRecord>(&path)?.is_some() {
            return Err(JournalError::ReceiptExists);
        }
        self.put(&path, &ReplyRecord { schema: SCHEMA, invocation: invocation.into(), envelope, refused })
    }

    pub fn reply(&self, invocation: &str) -> Result<Option<super::wire::Envelope>, JournalError> {
        Ok(self.get::<ReplyRecord>(&self.named("replies", invocation)?)?.and_then(|r| r.envelope))
    }

    pub fn refusal(&self, invocation: &str) -> Result<Option<String>, JournalError> {
        Ok(self.get::<ReplyRecord>(&self.named("replies", invocation)?)?.and_then(|r| r.refused))
    }
}
