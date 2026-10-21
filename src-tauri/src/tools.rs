//! The catalog's skills and servers on screen (`faber_engine::tools`): what ships, what is installed, and installing or removing one.

use crate::Faber;
use faber_engine::tools::{Store, catalog, find};
use serde::Serialize;
use tauri::State;

type Res<T> = Result<T, String>;

#[tauri::command]
pub async fn mcp_servers(faber: State<'_, Faber>, project_id: String) -> Res<Vec<faber_engine::mcp_servers::List>> {
    faber.engine.mcp_servers(&project_id).await
}

#[tauri::command]
pub fn set_mcp_server(faber: State<'_, Faber>, project_id: String, name: String, enabled: bool) -> Res<()> {
    faber.engine.set_mcp_server(&project_id, &name, enabled)
}

/// One row of the list: the catalog's entry and whether this computer has it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRow {
    id: String,
    job: String,
    license: String,
    repo: String,
    home: String,
    installed: bool,
    /// A tool server (an npm package Node runs) rather than a skill.
    server: bool,
}

#[tauri::command]
pub fn tools_list(faber: State<'_, Faber>) -> Vec<ToolRow> {
    let installed = Store::new(&faber.data_dir).installed();
    catalog()
        .iter()
        .map(|i| ToolRow {
            id: i.id.clone(),
            job: i.job.clone(),
            license: i.license.clone(),
            repo: i.repo.clone(),
            home: i.home(),
            installed: installed.contains_key(&i.id),
            server: i.server.is_some(),
        })
        .collect()
}

/// Downloads a skill from its own repository, or a server's package from npm, and installs it. A new session gets it; one under way keeps what it started with.
#[tauri::command]
pub async fn tools_install(faber: State<'_, Faber>, id: String) -> Res<()> {
    // An update never ends the process in the middle of an install (update.rs).
    let _running = crate::update::SetupRunning::begin()?;
    let item = find(&id).ok_or("error.tools.unknown")?.clone();
    // A server runs under Node, which neither CLI brings: without it the switch would install a tool nothing can start.
    if item.server.is_some() && faber_engine::cli::find_cli("node").is_none() {
        return Err("error.tools.node".into());
    }
    let store = Store::new(&faber.data_dir);
    // FABER_SETUP_SOURCE (tests): the archive comes from that loopback server, as the CLI installs do.
    let source = std::env::var("FABER_SETUP_SOURCE").ok().map(|s| s.trim_end_matches('/').to_string());
    faber_engine::tools::install(&store, &item, source.as_deref()).await.map_err(|detail| {
        eprintln!("tools: install of {id} failed: {detail}");
        "error.tools.install".to_string()
    })
}

#[tauri::command]
pub fn tools_remove(faber: State<'_, Faber>, id: String) -> Res<()> {
    find(&id).ok_or("error.tools.unknown")?;
    Store::new(&faber.data_dir).remove(&id).map_err(|_| "error.tools.remove".to_string())
}
