use super::*;
use crate::mcp_servers::{List, Reported};
use std::collections::BTreeSet;

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
        let installed: BTreeSet<_> = crate::tools::Store::new(&self.with(|c| c.opts.data_dir.clone()))
            .installed()
            .into_keys()
            .filter(|id| crate::tools::find(id).is_some_and(|i| i.server.is_some()))
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
        if !crate::tools::catalog().iter().any(|i| i.server.is_some() && name == format!("faber-{}", i.id)) {
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
    pub(super) fn tools_for_project(&self, area: Area, permissions: Permissions, project_id: Option<&str>) -> crate::tools::Delivery {
        let mut tools = self.tools_for(area, permissions);
        if let Some(project) = project_id.and_then(|id| self.store.projects().into_iter().find(|p| p.id == id)) {
            crate::mcp_servers::choose(&mut tools, &project.disabled_mcp_servers);
        }
        tools
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
