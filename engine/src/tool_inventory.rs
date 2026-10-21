//! Bounded, read-only discovery of skill folders and plugin installation manifests, never CLI configuration or credentials.

use crate::model::Vendor;
use crate::tools::Store;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Semaphore;

const LIMIT: usize = 256;
const MANIFEST_LIMIT: u64 = 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub name: String,
    pub kind: &'static str,
    pub source: &'static str,
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Issue {
    pub source: &'static str,
    pub kind: &'static str,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct List {
    pub vendor: Vendor,
    pub rows: Vec<Row>,
    pub issues: Vec<Issue>,
}

impl List {
    fn issue(&mut self, source: &'static str, kind: &'static str, reason: &'static str) {
        let issue = Issue { source, kind, reason };
        if !self.issues.contains(&issue) {
            self.issues.push(issue);
        }
    }
}

fn unavailable(project: bool) -> Vec<List> {
    Vendor::ALL
        .into_iter()
        .map(|vendor| {
            let mut list = List { vendor, rows: Vec::new(), issues: Vec::new() };
            for source in [Some("user"), project.then_some("project")].into_iter().flatten() {
                for kind in ["skill", "plugin"] {
                    list.issue(source, kind, "unavailable");
                }
            }
            list
        })
        .collect()
}

/// A slow disk must not block the window or queue one stuck scan per retry. The worker owns its permit after timeout.
pub async fn read_with(project: bool, read: impl FnOnce() -> Result<Vec<List>, String> + Send + 'static) -> Result<Vec<List>, String> {
    static READING: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let Ok(permit) = READING.get_or_init(|| Arc::new(Semaphore::new(1))).clone().try_acquire_owned() else {
        return Ok(unavailable(project));
    };
    let work = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        read()
    });
    match tokio::time::timeout(READ_TIMEOUT, work).await {
        Ok(Ok(result)) => result,
        _ => Ok(unavailable(project)),
    }
}

fn display_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(&path).to_string()
    }
}

fn plain_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control)
}

fn linked(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Check every component: a junction inside an otherwise safe root must never reach another CLI's setup.
fn scoped(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    let root = root.canonicalize()?;
    let mut path = root.clone();
    for component in relative.components() {
        let Component::Normal(name) = component else { return Err(io::ErrorKind::PermissionDenied.into()) };
        path.push(name);
        if linked(&std::fs::symlink_metadata(&path)?) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
    }
    let resolved = path.canonicalize()?;
    if !resolved.starts_with(root) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(resolved)
}

fn skills(list: &mut List, root: &Path, relative: &str, source: &'static str) {
    let path = match scoped(root, Path::new(relative)) {
        Ok(path) => path,
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                list.issue(source, "skill", "unreadable");
            }
            return;
        }
    };
    let mut pending = vec![(path, 0)];
    let mut visited = 0;
    while let Some((folder, depth)) = pending.pop() {
        let entries = match std::fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(_) => {
                list.issue(source, "skill", "unreadable");
                continue;
            }
        };
        for entry in entries {
            visited += 1;
            if visited > LIMIT {
                list.issue(source, "skill", "limited");
                return;
            }
            let Ok(entry) = entry else {
                list.issue(source, "skill", "unreadable");
                continue;
            };
            let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
                list.issue(source, "skill", "unreadable");
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if !plain_name(&name) || name.starts_with('.') {
                continue;
            }
            if linked(&metadata) {
                if source == "user" {
                    list.rows.push(Row {
                        name,
                        kind: "skill",
                        source,
                        location: display_path(&entry.path()),
                        enabled: None,
                        catalog_id: None,
                        linked: Some(true),
                    });
                } else {
                    list.issue(source, "skill", "unreadable");
                }
                continue;
            }
            if !metadata.is_dir() {
                continue;
            }
            let folder = entry.path();
            let manifest = scoped(&folder, Path::new("SKILL.md"));
            let plugin =
                list.vendor == Vendor::Claude && scoped(&folder, Path::new(".claude-plugin/plugin.json")).is_ok_and(|p| p.is_file());
            if manifest.as_ref().is_ok_and(|p| p.is_file()) || plugin {
                list.rows.push(Row {
                    name,
                    kind: if plugin { "plugin" } else { "skill" },
                    source,
                    location: display_path(&folder),
                    enabled: None,
                    catalog_id: None,
                    linked: None,
                });
            } else if manifest.is_err_and(|e| e.kind() != io::ErrorKind::NotFound) {
                list.issue(source, "skill", "unreadable");
            } else if list.vendor == Vendor::Codex {
                if depth < 3 {
                    pending.push((folder, depth + 1));
                } else {
                    list.issue(source, "skill", "limited");
                }
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginInstall {
    scope: String,
    project_path: Option<PathBuf>,
}

#[derive(Deserialize)]
struct PluginManifest {
    version: u64,
    plugins: BTreeMap<String, Vec<PluginInstall>>,
}

fn same_project(root: &Path, installed: &Path) -> bool {
    // A manifest path is untrusted: resolving an arbitrary UNC path could contact a server. Compare spelling only.
    fn spelling(path: &Path) -> String {
        let value = path.to_string_lossy().replace('\\', "/");
        let value = value.strip_prefix("//?/").unwrap_or(&value).trim_end_matches('/');
        if cfg!(windows) { value.to_lowercase() } else { value.to_string() }
    }
    let installed = spelling(installed);
    installed == spelling(root) || root.canonicalize().is_ok_and(|root| installed == spelling(&root))
}

fn plugins(list: &mut List, home: &Path, project: Option<&Path>) {
    let path = match scoped(home, Path::new(".claude/plugins/installed_plugins.json")) {
        Ok(path) => path,
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                list.issue("user", "plugin", "unreadable");
            }
            return;
        }
    };
    let read = || -> Option<PluginManifest> {
        if !std::fs::symlink_metadata(&path).ok()?.is_file() {
            return None;
        }
        let mut text = String::new();
        std::fs::File::open(&path).ok()?.take(MANIFEST_LIMIT + 1).read_to_string(&mut text).ok()?;
        if text.len() as u64 > MANIFEST_LIMIT {
            return None;
        }
        serde_json::from_str(&text).ok()
    };
    let Some(manifest) = read().filter(|m| m.version == 2) else {
        list.issue("user", "plugin", "unreadable");
        return;
    };
    let mut seen = 0;
    for (name, installs) in manifest.plugins {
        if seen == LIMIT || installs.len() > LIMIT - seen {
            list.issue("user", "plugin", "limited");
            break;
        }
        seen += installs.len().max(1);
        if !plain_name(&name) {
            continue;
        }
        for install in installs {
            let source = match install.scope.as_str() {
                "user" => "user",
                "project" | "local"
                    if project.is_some_and(|p| install.project_path.as_deref().is_some_and(|installed| same_project(p, installed))) =>
                {
                    "project"
                }
                _ => continue,
            };
            list.rows.push(Row {
                name: name.clone(),
                kind: "plugin",
                source,
                location: "pluginRecord".into(),
                enabled: None,
                catalog_id: None,
                linked: None,
            });
        }
    }
}

/// File discovery describes installations, not a live CLI report. User enablement/config overrides remain unknown.
/// Only the known skill roots and installation manifest are inspected; cached plugin folders are not proof of installation.
pub fn inventory(data: &Path, home: Option<&Path>, project: Option<&Path>) -> Vec<List> {
    let store = Store::new(data);
    let installed = store.installed();
    Vendor::ALL
        .into_iter()
        .map(|vendor| {
            let mut list = List { vendor, rows: Vec::new(), issues: Vec::new() };
            for item in crate::tools::catalog().iter().filter(|item| installed.contains_key(&item.id)) {
                list.rows.push(Row {
                    name: item.id.clone(),
                    kind: if item.server.is_some() { "server" } else { "skill" },
                    source: "catalog",
                    location: item.home(),
                    enabled: Some(store.enabled(&item.id)),
                    catalog_id: Some(item.id.clone()),
                    linked: None,
                });
            }
            let roots: &[&str] = match vendor {
                Vendor::Claude => &[".claude/skills"],
                Vendor::Codex => &[".agents/skills", ".codex/skills"],
            };
            if let Some(home) = home {
                for root in roots {
                    skills(&mut list, home, root, "user");
                }
                if vendor == Vendor::Claude {
                    plugins(&mut list, home, project);
                }
            } else {
                list.issue("user", "skill", "unavailable");
                list.issue("user", "plugin", "unavailable");
            }
            if let Some(project) = project {
                if home.and_then(|h| h.canonicalize().ok()) == project.canonicalize().ok() || crate::permissions::forbidden_folder(project)
                {
                    list.issue("project", "skill", "unreadable");
                } else {
                    for root in roots.iter().filter(|root| **root != ".codex/skills") {
                        skills(&mut list, project, root, "project");
                    }
                }
            }
            // Overrides, ancestor roots and transient --plugin-dir arguments need a CLI report, not a broad setup read.
            list.issue("user", "skill", "unsupported");
            if project.is_some() {
                list.issue("project", "skill", "unsupported");
            }
            list.issue("user", "plugin", "unsupported");
            if vendor == Vendor::Codex && project.is_some() {
                list.issue("project", "plugin", "unsupported");
            }
            list.rows.sort_by(|a, b| (a.source, a.kind, &a.name).cmp(&(b.source, b.kind, &b.name)));
            list.rows.dedup_by(|a, b| a.source == b.source && a.kind == b.kind && a.name == b.name && a.location == b.location);
            list
        })
        .collect()
}
