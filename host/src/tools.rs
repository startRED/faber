//! The catalog's skills and servers on screen (`faber_engine::tools`): what ships, what is installed, and installing or removing one.

use crate::{Host, Res, check_revoked, request_flag};
use faber_engine::tools::{InstallError, Store, catalog, find};
use serde::Serialize;

fn install_error(why: InstallError) -> String {
    match why {
        InstallError::Node => "error.tools.node".into(),
        InstallError::Failed(detail)
            if matches!(
                detail.as_str(),
                "error.tools.integrity"
                    | "error.tools.scripts"
                    | "error.tools.native"
                    | "error.tools.host"
                    | "error.tools.size"
                    | "error.tools.tree"
                    | "error.tools.digest"
                    | "error.tools.uvMissing"
                    | "error.tools.space"
                    | "error.tools.vram"
                    | "error.storage.unreachable"
            ) =>
        {
            detail
        }
        _ => "error.tools.install".into(),
    }
}

#[cfg(test)]
mod install_errors {
    use super::*;

    #[test]
    fn checked_package_refusals_reach_the_screen_as_localized_keys() {
        for key in ["integrity", "scripts", "native", "host", "size", "tree", "digest", "uvMissing", "space", "vram"] {
            let key = format!("error.tools.{key}");
            assert_eq!(install_error(InstallError::Failed(key.clone())), key);
        }
        assert_eq!(install_error(InstallError::Failed("untrusted archive detail".into())), "error.tools.install");
        assert_eq!(install_error(InstallError::Failed("error.storage.unreachable".into())), "error.storage.unreachable");
    }
}

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
    runtime: Option<faber_engine::tools::runtime::Runtime>,
    uv_available: bool,
    runtime_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime_fits: Option<bool>,
    runtime_download_size: u64,
    /// Who installed the copy Faber holds, when the install log still says so; none means unknown and shows nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    installed_by: Option<InstallOrigin>,
}

/// The conversation whose agent installed an item (none: the person, from this screen), and whether Full access let the
/// agent skip the card.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstallOrigin {
    #[serde(skip_serializing_if = "Option::is_none")]
    conv: Option<String>,
    full_access: bool,
}

/// The newest part of the install log read at most: the log keeps itself under 200 KB, so this holds all of it.
const LOG_READ_MAX: u64 = 256 * 1024;

/// Who installed each item, from the install log (`self-changes.jsonl`): the agent's lines carry its conversation, the
/// screen's carry none. The newest successful install wins unless a successful removal came after it. Only a plain file
/// is read, and only its newest `LOG_READ_MAX` bytes; a line that does not parse is skipped. An item the log no longer
/// mentions (never logged, or trimmed away) has no origin rather than a guessed one.
fn install_origins(data: &std::path::Path) -> std::collections::HashMap<String, InstallOrigin> {
    use std::io::{Read, Seek, SeekFrom};
    let mut by = std::collections::HashMap::new();
    let path = data.join(faber_engine::toolbox::SELF_CHANGES);
    let Ok(meta) = std::fs::symlink_metadata(&path) else { return by };
    if !meta.file_type().is_file() {
        return by;
    }
    let Ok(mut file) = std::fs::File::open(&path) else { return by };
    let skip = meta.len().saturating_sub(LOG_READ_MAX);
    if skip > 0 && file.seek(SeekFrom::Start(skip)).is_err() {
        return by;
    }
    let mut bytes = Vec::new();
    if file.take(LOG_READ_MAX).read_to_end(&mut bytes).is_err() {
        return by;
    }
    // Reading from the middle starts inside a line: drop that piece.
    let start = if skip > 0 { bytes.iter().position(|b| *b == b'\n').map_or(bytes.len(), |n| n + 1) } else { 0 };
    for line in String::from_utf8_lossy(&bytes[start..]).lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let (Some(id), Some(kind), Some(result)) = (entry["id"].as_str(), entry["kind"].as_str(), entry["result"].as_str()) else {
            continue;
        };
        match (kind, result) {
            ("tools.install", "installed") => {
                let conv = entry["conv"].as_str().filter(|conv| !conv.is_empty()).map(str::to_string);
                by.insert(id.to_string(), InstallOrigin { conv, full_access: entry["how"] == "full_access" });
            }
            ("tools.remove", "removed") => {
                by.remove(id);
            }
            _ => {}
        }
    }
    by
}

/// The screen's own install or removal, in the same log as the agent's, with no conversation: the next list says "by you".
/// A line that cannot be written changes nothing done; that item then shows no origin.
fn log_screen_change(data: &std::path::Path, id: &str, to: &str, remove: bool) {
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let result = if remove { "removed" } else { "installed" };
    let change = faber_engine::toolbox::SelfChange { at, id, from: None, to, how: "catalog", result, conv: "" };
    if remove {
        faber_engine::toolbox::log_remove(data, &change);
    } else {
        faber_engine::toolbox::log_self_change(data, &change);
    }
}

/// The public directories, only when the person submits a search and live web search is on; the same plain-word filter and
/// cache as the agent's catalog tool (`open_search`). Hits are data: nothing here can install one.
pub(crate) async fn tools_search(h: &Host, query: String) -> Res<Vec<OpenHit>> {
    if !faber_engine::visual::network(h.engine.settings().web_search) {
        return Err("error.tools.searchOff".into());
    }
    let hits = faber_engine::open_search::search(&query).await.map_err(|_| "error.tools.searchUnavailable".to_string())?;
    Ok(hits
        .iter()
        .map(faber_engine::toolbox::open_hit)
        .map(|(id, source, installs, server)| OpenHit { id: id.into(), source: source.into(), installs, server })
        .collect())
}

/// One unchecked public hit for the open shelf: no install path, link or instruction comes with it.
#[derive(Serialize)]
pub(crate) struct OpenHit {
    id: String,
    source: String,
    installs: u64,
    server: bool,
}

#[cfg(test)]
mod origin {
    use super::*;

    fn log(dir: &std::path::Path, lines: &[&str]) {
        std::fs::write(dir.join(faber_engine::toolbox::SELF_CHANGES), lines.join("\n")).unwrap();
    }
    fn ai(conv: &str, full_access: bool) -> Option<InstallOrigin> {
        Some(InstallOrigin { conv: Some(conv.into()), full_access })
    }

    #[test]
    fn the_log_names_the_conversation_and_full_access_and_a_later_removal_clears_it() {
        let dir = tempfile::tempdir().unwrap();
        assert!(install_origins(dir.path()).is_empty(), "no log: no origin is guessed");
        log(
            dir.path(),
            &[
                r#"{"at":1,"kind":"tools.install","id":"ponytail","to":"a","how":"once","result":"allowed","conv":"c1"}"#,
                r#"{"at":2,"kind":"tools.install","id":"ponytail","to":"a","how":"once","result":"installed","conv":"c1"}"#,
                r#"{"at":3,"kind":"tools.install","id":"humanizer","to":"b","how":"full_access","result":"installed","conv":"c2"}"#,
                r#"{"at":4,"kind":"tools.install","id":"grill-me","to":"c","how":"once","result":"failed","conv":"c3"}"#,
                r#"{"at":5,"kind":"tools.install","id":"break-ui","to":"d","how":"always","result":"installed","conv":"c4"}"#,
                r#"{"at":6,"kind":"tools.remove","id":"break-ui","to":null,"how":"always","result":"removed","conv":"c4"}"#,
                "not json",
                r#"{"at":7,"kind":"tools.install","id":"#,
            ],
        );
        let by = install_origins(dir.path());
        assert_eq!(by.get("ponytail").cloned(), ai("c1", false));
        assert_eq!(by.get("humanizer").cloned(), ai("c2", true));
        assert!(!by.contains_key("grill-me"), "a failed install is not an origin");
        assert!(!by.contains_key("break-ui"), "removed afterwards, and never reinstalled: no origin");
    }

    #[test]
    fn a_reinstall_from_the_screen_replaces_the_ai_and_an_unlogged_item_has_no_origin() {
        let dir = tempfile::tempdir().unwrap();
        log(dir.path(), &[r#"{"at":1,"kind":"tools.install","id":"ponytail","to":"a","how":"once","result":"installed","conv":"c1"}"#]);
        assert_eq!(install_origins(dir.path()).get("ponytail").cloned(), ai("c1", false));
        log_screen_change(dir.path(), "ponytail", "a", true);
        assert!(!install_origins(dir.path()).contains_key("ponytail"), "removed from the screen");
        log_screen_change(dir.path(), "ponytail", "a", false);
        let by = install_origins(dir.path());
        assert_eq!(by.get("ponytail").cloned(), Some(InstallOrigin { conv: None, full_access: false }), "by you");
        assert!(!by.contains_key("humanizer"), "never logged: no origin at all");
    }

    #[test]
    fn only_a_plain_file_is_read_and_only_its_newest_part() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(faber_engine::toolbox::SELF_CHANGES)).unwrap();
        assert!(install_origins(dir.path()).is_empty(), "a folder in its place is not read");
        let other = tempfile::tempdir().unwrap();
        let old = r#"{"at":1,"kind":"tools.install","id":"humanizer","to":"a","how":"once","result":"installed","conv":"old"}"#;
        let filler = format!("{}\n", "x".repeat(1023)).repeat(LOG_READ_MAX as usize / 1024 + 4);
        let new = r#"{"at":2,"kind":"tools.install","id":"ponytail","to":"a","how":"once","result":"installed","conv":"new"}"#;
        std::fs::write(other.path().join(faber_engine::toolbox::SELF_CHANGES), format!("{old}\n{filler}{new}\n")).unwrap();
        let by = install_origins(other.path());
        assert_eq!(by.get("ponytail").cloned(), ai("new", false));
        assert!(!by.contains_key("humanizer"), "beyond the read limit");
    }
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
        let by = install_origins(&data);
        let node_available = faber_engine::cli::find_cli("node").is_some();
        let uv_available = faber_engine::tools::runtime::uv().is_some();
        let vram = if catalog().iter().any(|item| item.runtime.as_ref().is_some_and(|runtime| runtime.min_vram_mb.is_some())) {
            if cfg!(test) { None } else { faber_winproc::available_vram_mb() }
        } else {
            None
        };
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
                runtime: i.runtime.clone(),
                uv_available,
                runtime_available: i.runtime.is_none() || faber_engine::tools::runtime::available(i),
                runtime_fits: i.runtime.as_ref().map(|runtime| runtime.fits(vram)),
                runtime_download_size: faber_engine::tools::runtime::download_size(i),
                installed_by: installed.contains_key(&i.id).then(|| by.get(&i.id).cloned()).flatten(),
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
    let store = h.engine.tool_store();
    // FABER_SETUP_SOURCE (tests): the archive comes from that loopback server, as the CLI installs do.
    let source = std::env::var("FABER_SETUP_SOURCE").ok().map(|s| s.trim_end_matches('/').to_string());
    // The project agent's install comes through the same function: the Node check, host list, digest and caps are one.
    check_revoked(request_flag().as_deref())?;
    faber_engine::tools::install_checked(&store, &item, source.as_deref()).await.map_err(|why| {
        eprintln!("tools: install of {id} failed: {why:?}");
        install_error(why)
    })?;
    let data = h.data_dir.clone();
    let _ = tokio::task::spawn_blocking(move || log_screen_change(&data, &item.id, &item.commit, false)).await;
    Ok(())
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
        let store = Store::new(&data);
        let plan = store
            .capture_replacement(&id)
            .map_err(|_| if store.location_unavailable(&id) { "error.storage.unreachable" } else { "error.tools.install" }.to_string())?;
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
    check_revoked(request_flag().as_deref())?;
    faber_engine::tools::replace_checked(&store, &plan, source.as_deref()).await.map_err(install_error)
}

pub(crate) async fn tools_remove(h: &Host, id: String) -> Res<()> {
    find(&id).ok_or("error.tools.unknown")?;
    let data = h.data_dir.clone();
    let flag = request_flag();
    tokio::task::spawn_blocking(move || {
        check_revoked(flag.as_deref())?;
        Store::new(&data).remove(&id).map_err(|_| "error.tools.remove".to_string())?;
        log_screen_change(&data, &id, "", true);
        Ok(())
    })
    .await
    .map_err(|_| "error.tools.remove".to_string())?
}

pub(crate) async fn tools_set_enabled(h: &Host, id: String, enabled: bool) -> Res<()> {
    find(&id).ok_or("error.tools.unknown")?;
    let data = h.data_dir.clone();
    let flag = request_flag();
    tokio::task::spawn_blocking(move || {
        check_revoked(flag.as_deref())?;
        Store::new(&data).set_enabled(&id, enabled).map_err(|_| "error.tools.toggle".to_string())
    })
    .await
    .map_err(|_| "error.tools.toggle".to_string())?
}

pub(crate) async fn tools_restore(h: &Host, id: String) -> Res<()> {
    find(&id).ok_or("error.tools.unknown")?;
    let data = h.data_dir.clone();
    let flag = request_flag();
    tokio::task::spawn_blocking(move || {
        check_revoked(flag.as_deref())?;
        Store::new(&data).restore(&id).map_err(|_| "error.tools.restore".to_string())
    })
    .await
    .map_err(|_| "error.tools.restore".to_string())?
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
