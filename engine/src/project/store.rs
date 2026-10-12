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
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
impl Record for Effects {
    fn schema(&self) -> u32 {
        self.entries.iter().map(Record::schema).max().unwrap_or(SCHEMA)
    }
}

pub struct Journal {
    mission: String,
    records: PathBuf,
    private: PathBuf,
    disk: Arc<dyn Disk>,
}

impl Journal {
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

    pub fn private(&self) -> &Path {
        &self.private
    }

    fn put<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), JournalError> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| Fault::of_write(&e))?;
        }
        let json = serde_json::to_string_pretty(value).expect("plain data always serializes");
        Ok(replace(&*self.disk, path, json.as_bytes())?)
    }

    fn get<T: DeserializeOwned + Record>(&self, path: &Path) -> Result<Option<T>, JournalError> {
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
        let entries = match fs::read_dir(self.records.join(folder)) {
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
        self.put(&self.records.join("plan.json"), plan)
    }

    pub fn plan(&self) -> Result<Option<Plan>, JournalError> {
        self.get(&self.records.join("plan.json"))
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
        let mut open: Vec<Intent> = self
            .effects()?
            .entries
            .into_iter()
            .filter(|e| e.receipt.as_ref().is_none_or(|r| r.outcome == Outcome::Unknown))
            .map(|e| e.intent)
            .collect();
        open.sort_by(|a, b| (a.at, &a.id).cmp(&(b.at, &b.id)));
        Ok(open)
    }

    pub fn save_runtime(&self, runtime: &Runtime) -> Result<(), JournalError> {
        self.put(&self.private.join("runtime.json"), runtime)
    }

    /// The private sidecar; none yet is an empty one, a damaged one is an error (it holds leases).
    pub fn runtime(&self) -> Result<Runtime, JournalError> {
        Ok(self.get(&self.private.join("runtime.json"))?.unwrap_or_default())
    }
}
