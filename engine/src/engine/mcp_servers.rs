use super::*;
use crate::mcp_servers::{List, Reported};
use std::collections::BTreeSet;

pub(super) const CHECK_PENDING: &str = "catalog launch check pending";
type CheckedKit = (crate::tools::Delivery, crate::tools::kits::Report);
type AfterCheck = Box<dyn FnOnce(&mut Core) + Send>;

#[derive(Clone, PartialEq, Eq)]
struct CheckKey {
    vendor: Vendor,
    area: Area,
    permissions: Permissions,
    cwd: PathBuf,
    request: String,
    snapshot: Option<String>,
    skills: Option<Vec<String>>,
    servers: Option<String>,
    generation: u64,
    disabled: Vec<String>,
    native: Option<String>,
    previous: Vec<String>,
}

pub(super) struct LaunchCheck {
    key: CheckKey,
    pub(super) token: u64,
    ready: Option<CheckedKit>,
    after: Option<AfterCheck>,
    pub(super) owner: String,
    owner_sid: Option<u64>,
}

fn verify_kit(store: crate::tools::Store, key: CheckKey, (delivery, mut report): CheckedKit) -> CheckedKit {
    let CheckKey { vendor, area, permissions, disabled, previous, native, .. } = key;
    let may_retain = delivery.servers.is_empty();
    let (mut delivery, damaged, io_error) = delivery.verified_for_launch(vendor);
    report.ids.retain(|id| {
        delivery.skill_ids.as_ref().is_some_and(|ids| ids.contains(id))
            || delivery.servers.iter().any(|server| server.name == format!("faber-{id}"))
    });
    if !damaged.is_empty() {
        report.error = Some("error.tools.reinstall".into());
    } else if io_error {
        report.error = Some("error.tools.snapshot".into());
    }
    if vendor == Vendor::Codex
        && native.is_some()
        && may_retain
        && delivery.servers.is_empty()
        && store.retain_servers(&mut delivery, &previous, area, permissions, &disabled)
    {
        report.ids.retain(|id| delivery.skill_ids.as_ref().is_some_and(|ids| ids.contains(id)));
    }
    if report.error.is_none() {
        report.error = store.preparation_error().or_else(|| previous.iter().find_map(|id| store.item_status(id)));
    }
    (delivery, report)
}

pub(super) struct Reading {
    pub sid: u64,
    pub conversation: String,
    pub added: BTreeSet<String>,
    pub servers: Vec<Reported>,
    pub read_at: Option<f64>,
    pub cwd: PathBuf,
}

impl Engine {
    /// Called only while the catalog's server part is open. Never starts a CLI to get a list.
    pub async fn mcp_servers(&self, project_id: &str) -> Result<Vec<List>, String> {
        self.project(project_id)?;
        let latest = self.with(|core| {
            core.mcp_servers.get(&(project_id.into(), Vendor::Codex)).and_then(|reading| {
                let live = core.live.get(&reading.conversation).filter(|l| l.sid == reading.sid && l.vendor == Vendor::Codex)?;
                Some((reading.conversation.clone(), reading.sid, live.session.mcp_status()))
            })
        });
        if let Some((conversation, sid, receive)) = latest
            && let Ok(Ok(Some(servers))) = tokio::time::timeout(Duration::from_secs(5), receive).await
        {
            self.with(|core| core.note_mcp_servers(&conversation, sid, servers));
        }
        let project = self.project(project_id)?;
        let installed: BTreeSet<_> = self
            .with(|c| c.catalog_store())
            .installed_resources()
            .into_keys()
            .filter(|id| crate::tools::plugins::find_resource(id).is_some_and(|i| i.server.is_some()))
            .map(|id| format!("faber-{id}"))
            .collect();
        let snapshots = self.with(|core| {
            core.opts
                .clis
                .keys()
                .map(|vendor| {
                    let reading = core.mcp_servers.get(&(project_id.into(), *vendor));
                    (
                        *vendor,
                        reading.map(|r| r.cwd.clone()).unwrap_or_else(|| PathBuf::from(&project.path)),
                        reading.map(|r| (r.servers.clone(), r.added.clone(), r.read_at)),
                    )
                })
                .collect::<Vec<_>>()
        });
        Ok(snapshots
            .into_iter()
            .map(|(vendor, folder, reading)| {
                let (servers, added, read_at) = reading.unwrap_or_default();
                let project_names = crate::mcp_servers::project_names(&folder, vendor);
                List {
                    vendor,
                    read_at,
                    rows: crate::mcp_servers::rows(&servers, &added, &installed, &project.disabled_mcp_servers, &project_names),
                }
            })
            .collect())
    }

    pub fn set_mcp_server(&self, project_id: &str, name: &str, enabled: bool) -> Result<(), String> {
        if !crate::tools::plugins::resources().iter().any(|i| i.server.is_some() && name == format!("faber-{}", i.id)) {
            return Err("error.tools.unknown".into());
        }
        let before = self.project(project_id)?;
        let mut project = before.clone();
        if enabled {
            project.disabled_mcp_servers.remove(name);
        } else {
            project.disabled_mcp_servers.insert(name.into());
        }
        self.update_project(&before, &project)
    }
}

impl Core {
    pub(super) fn session_tools(
        &self,
        vendor: Vendor,
        area: Area,
        permissions: Permissions,
        project_id: Option<&str>,
        request: &str,
        cwd: &Path,
    ) -> (crate::tools::Delivery, crate::tools::kits::Report) {
        let disabled = self.project_disabled_servers(project_id);
        let store = self.catalog_store();
        // Chat consumes bounded instructions directly; it never discovers files through a plugin.
        let selection_vendor = if vendor == Vendor::Claude && area == Area::Chat { Vendor::Codex } else { vendor };
        let (mut delivery, report) = store.ready_kit_for(selection_vendor, request, Some(cwd), area, permissions, &disabled);
        if vendor == Vendor::Claude && area == Area::Chat {
            delivery.plugin = None;
        }
        let (request, cwd) = (request.to_string(), cwd.to_path_buf());
        let prepare = move || store.prepare_kit_for(selection_vendor, &request, Some(&cwd), area, permissions, &disabled);
        // A detached worker races the next try_lock in ready_kit_for and empties its kit; tests prepare in line.
        #[cfg(test)]
        if self.synchronous_kit_checks {
            prepare();
            return (delivery, report);
        }
        self.opts.runtime.spawn_blocking(prepare);
        (delivery, report)
    }

    pub(super) fn after_kit_check(&mut self, key: &str, after: impl FnOnce(&mut Core) + Send + 'static) {
        if let Some(check) = self.kit_checks.get_mut(key) {
            check.after = Some(Box::new(after));
        }
    }

    pub(super) fn checked_session_tools(
        &mut self,
        conv: &str,
        vendor: Vendor,
        permissions: Permissions,
        cwd: &Path,
    ) -> Result<CheckedKit, String> {
        let c = self.convs.get(conv).ok_or("error.conversationGone")?;
        let area = c.area;
        let request = c.messages.iter().rev().find(|m| m.role == Role::User).map(|m| m.text.clone()).unwrap_or_default();
        let project = c.project_id.clone();
        let native = c.resumable(vendor).cloned();
        let previous = c
            .session_kits
            .get(&vendor)
            .filter(|(id, _, _)| Some(id) == native.as_ref())
            .map(|(_, ids, _)| ids.clone())
            .or_else(|| {
                native.as_ref().and_then(|_| {
                    c.messages
                        .iter()
                        .rev()
                        .filter(|m| m.vendor == Some(vendor))
                        .find_map(|m| m.tool_kit.as_ref().map(|kit| kit.ids.clone()))
                })
            })
            .unwrap_or_default();
        let mut raw = self.session_tools(vendor, area, permissions, project.as_deref(), &request, cwd);
        raw.0.guide = self.guide_for(vendor, area, permissions);
        let disabled = self.project_disabled_servers(project.as_deref());
        self.check_kit(conv, conv, vendor, area, permissions, cwd, request, native, previous, disabled, raw)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_kit(
        &mut self,
        name: &str,
        owner: &str,
        vendor: Vendor,
        area: Area,
        permissions: Permissions,
        cwd: &Path,
        request: String,
        native: Option<String>,
        previous: Vec<String>,
        disabled: Vec<String>,
        mut raw: CheckedKit,
    ) -> Result<CheckedKit, String> {
        let store = self.catalog_store();
        let key = CheckKey {
            vendor,
            area,
            permissions,
            cwd: cwd.into(),
            request,
            snapshot: raw.0.identity(),
            skills: raw.0.skill_ids.clone(),
            servers: raw.0.server_identity(),
            generation: store.generation(),
            disabled,
            native,
            previous,
        };
        // Excluding damaged resources changes the pick; keep this launch's diagnosis across that retry only.
        if let Some(check) = self.kit_checks.get(name) {
            let mut previous = check.key.clone();
            previous.skills = key.skills.clone();
            previous.servers = key.servers.clone();
            if key.snapshot.is_none() {
                previous.snapshot = None;
            }
            if previous == key && raw.1.error.is_none() {
                raw.1.error = check.ready.as_ref().and_then(|(_, report)| report.error.clone());
            }
        }
        if raw.0.lease.is_none() && raw.0.servers.is_empty() {
            self.kit_checks.remove(name);
            return Ok(raw);
        }
        #[cfg(test)]
        if self.synchronous_kit_checks {
            return Ok(verify_kit(store, key, raw));
        }
        if let Some(check) = self.kit_checks.get_mut(name).filter(|check| check.key == key) {
            if let Some(ready) = check.ready.take() {
                self.kit_checks.remove(name);
                return Ok(ready);
            }
            return Err(CHECK_PENDING.into());
        }
        self.tick += 1;
        let token = self.tick;
        self.kit_checks.insert(
            name.into(),
            LaunchCheck {
                key: key.clone(),
                token,
                ready: None,
                after: None,
                owner: owner.into(),
                owner_sid: self.live.get(owner).filter(|_| name.starts_with("helper:")).map(|live| live.sid),
            },
        );
        let (me, name, gate) = (self.me.clone(), name.to_string(), self.kit_check_gate.clone());
        #[cfg(test)]
        let hook = self.kit_check_hook.take();
        self.opts.runtime.spawn(async move {
            let Ok(permit) = gate.acquire_owned().await else { return };
            let Some(core) = me.upgrade() else { return };
            {
                let core = core.lock().unwrap_or_else(|e| e.into_inner());
                if core.kit_checks.get(&name).is_none_or(|check| check.token != token) {
                    return;
                }
            }
            let checked = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                #[cfg(test)]
                if let Some((started, gate, _)) = &hook {
                    let _ = started.send(());
                    gate.wait();
                }
                let checked = verify_kit(store, key, raw);
                #[cfg(test)]
                if let Some((_, _, finished)) = hook {
                    let _ = finished.send(());
                }
                checked
            })
            .await;
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            if checked.is_err() {
                core.failed_kit_check(&name, token);
                return;
            }
            let Some(check) = core.kit_checks.get_mut(&name).filter(|check| check.token == token) else { return };
            check.ready = checked.ok();
            if let Some(after) = check.after.take() {
                after(&mut core);
            }
            // A stopped/deleted owner or a failed gate must not retain the checked snapshot indefinitely.
            if core.kit_checks.get(&name).is_some_and(|check| check.token == token && check.ready.is_some()) {
                core.kit_checks.remove(&name);
            }
        });
        Err(CHECK_PENDING.into())
    }

    pub(super) fn failed_kit_check(&mut self, name: &str, token: u64) {
        let Some(check) = self.kit_checks.get(name).filter(|check| check.token == token) else { return };
        let owner = check.owner.clone();
        let owner_sid = check.owner_sid;
        self.kit_checks.remove(name);
        if let Some(call) = name.strip_prefix("helper:") {
            if let Some(live) = self.live.get(&owner).filter(|live| Some(live.sid) == owner_sid) {
                let (_, text) = crate::helper::report(false, "", &[], Some("the catalog files could not be checked"));
                live.session.helper_reply(call, false, text);
            }
        } else if self.compacts.remove(&owner).is_some() {
            if let Some(conversation) = self.convs.get_mut(&owner) {
                conversation.compacting = false;
            }
            self.list_changed();
            self.drain(&owner);
        } else if let Some(reply) = self
            .convs
            .get(&owner)
            .and_then(|conversation| conversation.messages.last())
            .filter(|message| message.status == Some(Status::Streaming))
            .map(|message| message.id.clone())
        {
            self.fail_reply(&owner, &reply, "error.tools.snapshot".into());
        }
    }

    pub(super) fn project_disabled_servers(&self, project_id: Option<&str>) -> Vec<String> {
        project_id
            .and_then(|id| self.store.projects().into_iter().find(|p| p.id == id))
            .map(|p| p.disabled_mcp_servers.into_iter().collect())
            .unwrap_or_default()
    }

    pub(super) fn note_mcp_servers(&mut self, conversation: &str, sid: u64, servers: Vec<Reported>) {
        let Some(live) = self.live.get(conversation).filter(|live| live.sid == sid) else { return };
        let Some(project) = self.convs.get(conversation).and_then(|c| c.project_id.as_ref()) else { return };
        let key = (project.clone(), live.vendor);
        let at = self.secs();
        if let Some(reading) = self.mcp_servers.get_mut(&key).filter(|r| r.sid == sid && r.conversation == conversation) {
            reading.servers = servers;
            reading.read_at = Some(at);
        }
    }
}
