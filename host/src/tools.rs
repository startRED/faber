//! The catalog's skills and servers on screen (`faber_engine::tools`): what ships, what is installed, and installing or removing one.

use crate::{Host, Res};
use faber_engine::tools::{InstallError, Store, catalog, find};
use serde::Serialize;

/// Reads only known skill directories and the plugin installation manifest; no CLI is started.
pub(crate) async fn tools_inventory(h: &Host, project_id: Option<String>) -> Res<Vec<faber_engine::tool_inventory::List>> {
    let engine = h.engine.clone();
    let data = h.data_dir.clone();
    faber_engine::tool_inventory::read_with(project_id.is_some(), move || {
        let project = project_id.as_deref().map(|id| engine.project(id)).transpose()?;
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(std::path::PathBuf::from);
        Ok(faber_engine::tool_inventory::inventory(&data, home.as_deref(), project.as_ref().map(|p| std::path::Path::new(&p.path))))
    })
    .await
}

pub(crate) async fn mcp_servers(h: &Host, project_id: String) -> Res<Vec<faber_engine::mcp_servers::List>> {
    h.engine.mcp_servers(&project_id).await
}

pub(crate) async fn set_mcp_server(h: &Host, project_id: String, name: String, enabled: bool) -> Res<()> {
    let engine = h.engine.clone();
    tokio::task::spawn_blocking(move || engine.set_mcp_server(&project_id, &name, enabled))
        .await
        .map_err(|_| "error.tools.toggle".to_string())?
}

/// One row of the list: the catalog's entry and whether this computer has it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolRow {
    id: String,
    cli_name: Option<String>,
    job: String,
    tier: faber_engine::tools::Tier,
    license: String,
    repo: String,
    home: String,
    installed: bool,
    has_copy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    removed_enabled: Option<bool>,
    /// An engine key explaining why a present item needs reinstalling.
    status: Option<String>,
    enabled: bool,
    version: String,
    node_available: bool,
    /// A tool server (an npm package Node runs) rather than a skill.
    server: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin_parts: Option<Vec<PluginPartRow>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    claude_plugins_available: Option<bool>,
    requires_node: bool,
}

#[derive(Serialize)]
struct PluginPartRow {
    id: String,
    kind: faber_engine::tools::plugins::PartKind,
}

pub(crate) async fn tools_list(h: &Host) -> Res<Vec<ToolRow>> {
    let data = h.data_dir.clone();
    // Unknown until the startup probe has read the CLI: the default flags mean "not read yet" (or no Claude Code).
    let flags = h.engine.claude_flags();
    let claude_plugins_available = (flags != Default::default()).then_some(flags.plugin_dir);
    tokio::task::spawn_blocking(move || {
        let store = Store::new(&data);
        let statuses = store.checked_statuses();
        let installed = store.installed();
        let node_available = faber_engine::cli::find_cli("node").is_some();
        catalog()
            .iter()
            .map(|i| ToolRow {
                id: i.id.clone(),
                cli_name: i.cli_name.clone(),
                job: i.job.clone(),
                tier: i.tier,
                license: i.license.clone(),
                repo: i.repo.clone(),
                home: i.home(),
                installed: installed.contains_key(&i.id),
                has_copy: store.has_copy(&i.id).unwrap_or(true),
                removed_enabled: store.agent_removed_enabled(&i.id),
                status: statuses.get(&i.id).cloned(),
                enabled: installed.contains_key(&i.id) && store.enabled(&i.id),
                version: i.server.as_ref().map_or_else(|| i.commit.clone(), |s| s.version.clone()),
                node_available,
                server: i.server.is_some(),
                plugin_parts: store
                    .plugin_definition(&i.id)
                    .map(|plugin| plugin.parts.iter().map(|part| PluginPartRow { id: part.id.clone(), kind: part.kind }).collect()),
                claude_plugins_available: store.plugin_definition(&i.id).and(claude_plugins_available),
                requires_node: store.requires_node(i),
            })
            .collect()
    })
    .await
    .map_err(|_| "error.tools.read".to_string())
}

/// Downloads a skill from its own repository, or a server's package from npm, and installs it. New sessions receive the changed store; moving files may also affect an existing session.
pub(crate) async fn tools_install(h: &Host, id: String) -> Res<()> {
    // An update never ends the process in the middle of an install: the shell holds it off until this is dropped.
    let _running = h.shell.hold_install()?;
    let item = find(&id).ok_or("error.tools.unknown")?.clone();
    let data_dir = h.data_dir.clone();
    let store = tokio::task::spawn_blocking(move || Store::new(&data_dir)).await.map_err(|_| "error.tools.install".to_string())?;
    // FABER_SETUP_SOURCE (tests): the archive comes from that loopback server, as the CLI installs do.
    let source = std::env::var("FABER_SETUP_SOURCE").ok().map(|s| s.trim_end_matches('/').to_string());
    // The project agent's install comes through the same function: the Node check, host list, digest and caps are one.
    faber_engine::tools::install_checked(&store, &item, source.as_deref()).await.map_err(|why| match why {
        InstallError::Node => "error.tools.node".to_string(),
        InstallError::Held(_) => "error.tools.install".to_string(),
        InstallError::Failed(detail) => {
            eprintln!("tools: install of {id} failed: {detail}");
            "error.tools.install".to_string()
        }
    })
}

#[derive(Serialize)]
pub(crate) struct PreparedReplacement {
    plan: faber_engine::tools::ReplacementPlan,
    from: String,
    to: String,
    enabled: bool,
}

/// Captures the exact existing copy for the person's detail confirmation; it downloads nothing.
pub(crate) async fn tools_prepare_replacement(h: &Host, id: String) -> Res<PreparedReplacement> {
    find(&id).ok_or("error.tools.unknown")?;
    let data = h.data_dir.clone();
    tokio::task::spawn_blocking(move || {
        let plan = Store::new(&data).capture_replacement(&id).map_err(|_| "error.tools.install".to_string())?;
        Ok(PreparedReplacement { from: plan.old_commit().into(), to: plan.item().commit.clone(), enabled: plan.enabled(), plan })
    })
    .await
    .map_err(|_| "error.tools.install".to_string())?
}

/// Revalidates the confirmed snapshot before downloading and again under the publication lock.
pub(crate) async fn tools_replace(h: &Host, plan: faber_engine::tools::ReplacementPlan) -> Res<()> {
    let _running = h.shell.hold_install()?;
    let data = h.data_dir.clone();
    let store = tokio::task::spawn_blocking(move || Store::new(&data)).await.map_err(|_| "error.tools.install".to_string())?;
    let source = std::env::var("FABER_SETUP_SOURCE").ok().map(|s| s.trim_end_matches('/').to_string());
    faber_engine::tools::replace_checked(&store, &plan, source.as_deref()).await.map_err(|why| match why {
        InstallError::Node => "error.tools.node".to_string(),
        InstallError::Held(_) | InstallError::Failed(_) => "error.tools.install".to_string(),
    })
}

pub(crate) async fn tools_remove(h: &Host, id: String) -> Res<()> {
    find(&id).ok_or("error.tools.unknown")?;
    let data = h.data_dir.clone();
    tokio::task::spawn_blocking(move || Store::new(&data).remove(&id))
        .await
        .map_err(|_| "error.tools.remove".to_string())?
        .map_err(|_| "error.tools.remove".to_string())
}

pub(crate) async fn tools_set_enabled(h: &Host, id: String, enabled: bool) -> Res<()> {
    find(&id).ok_or("error.tools.unknown")?;
    let data = h.data_dir.clone();
    tokio::task::spawn_blocking(move || Store::new(&data).set_enabled(&id, enabled))
        .await
        .map_err(|_| "error.tools.toggle".to_string())?
        .map_err(|_| "error.tools.toggle".to_string())
}

pub(crate) async fn tools_restore(h: &Host, id: String) -> Res<()> {
    find(&id).ok_or("error.tools.unknown")?;
    let data = h.data_dir.clone();
    tokio::task::spawn_blocking(move || Store::new(&data).restore(&id))
        .await
        .map_err(|_| "error.tools.restore".to_string())?
        .map_err(|_| "error.tools.restore".to_string())
}

/// The catalog tools a conversation's agent has now, each with its job group, for the context meter's panel: read from the
/// engine's memory when the panel opens; nothing is prepared, polled or started.
pub(crate) async fn tools_in_use(h: &Host, id: String) -> Res<Vec<(String, String)>> {
    Ok(h.engine.tools_in_use(&id))
}

/// Called on entering Catalog, before any removal in this visit can offer Undo.
pub(crate) async fn tools_clear_removed(h: &Host) -> Res<()> {
    let data = h.data_dir.clone();
    tokio::task::spawn_blocking(move || Store::new(&data).clear_removed_for_catalog())
        .await
        .map_err(|_| "error.tools.remove".to_string())?
        .map_err(|_| "error.tools.remove".to_string())
}
