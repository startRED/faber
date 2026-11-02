//! The catalog's project server list. CLI reports supply names and status; project files supply names only.

use crate::model::Vendor;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reported {
    pub name: String,
    pub status: Option<String>,
}

pub fn reported(rows: &[Value], status: &str) -> Vec<Reported> {
    rows.iter()
        .take(256)
        .filter_map(|row| {
            let name = row["name"].as_str().filter(|n| !n.is_empty() && n.len() <= 256 && !n.chars().any(char::is_control))?;
            if name == "faber" {
                return None;
            }
            let status = row[status].as_str().filter(|s| s.len() <= 64 && !s.chars().any(char::is_control)).map(String::from);
            Some(Reported { name: name.into(), status })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub name: String,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct List {
    pub vendor: Vendor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<f64>,
    pub rows: Vec<Row>,
}

/// Mark this project's disabled catalog deliveries. Codex still needs their transport definition with enabled=false.
pub fn choose(delivery: &mut crate::tools::Delivery, disabled: &BTreeSet<String>) {
    delivery.disabled_servers = delivery
        .servers
        .iter()
        .filter(|s| {
            disabled.contains(&s.name) && crate::tools::catalog().iter().any(|i| i.server.is_some() && s.name == format!("faber-{}", i.id))
        })
        .map(|s| s.name.clone())
        .collect();
}

/// The only places this feature may read. Resolution must stay in the project, including through junctions.
fn project_file(root: &Path, relative: &str, home: Option<&Path>) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    if home.and_then(|h| h.canonicalize().ok()).is_some_and(|h| h == root) || crate::permissions::forbidden_folder(&root) {
        return None;
    }
    let written = root.join(relative);
    // A project may be a drive root containing the home. A link into that home's setup still stays inside the project.
    if !std::fs::symlink_metadata(&written).is_ok_and(|m| m.is_file())
        || (relative.starts_with(".codex/") && !std::fs::symlink_metadata(root.join(".codex")).is_ok_and(|m| m.is_dir()))
    {
        return None;
    }
    let path = written.canonicalize().ok()?;
    let personal = home
        .and_then(|h| h.canonicalize().ok())
        .is_some_and(|h| [".codex", ".claude", ".gemini"].iter().any(|dir| path.starts_with(h.join(dir))));
    let relocated = crate::permissions::relocated_credentials(&|name| std::env::var(name).ok())
        .iter()
        .any(|(dir, file)| Path::new(dir).canonicalize().is_ok_and(|p| if *file { path == p } else { path.starts_with(p) }));
    (path.starts_with(&root) && !personal && !relocated).then_some(path)
}

/// `read` is injected so the no-home-read guarantee can be checked without opening anybody's CLI setup.
pub fn project_names_with(root: &Path, vendor: Vendor, home: Option<&Path>, read: &dyn Fn(&Path) -> Option<String>) -> BTreeSet<String> {
    let relative = match vendor {
        Vendor::Claude => ".mcp.json",
        Vendor::Codex => ".codex/config.toml",
        // Not yet: its server file is read once its CLI runs.
        Vendor::Gemini => return BTreeSet::new(),
    };
    let Some(path) = project_file(root, relative, home) else { return BTreeSet::new() };
    let Some(text) = read(&path) else { return BTreeSet::new() };
    match vendor {
        Vendor::Claude => {
            #[derive(serde::Deserialize)]
            struct Names {
                #[serde(rename = "mcpServers", default)]
                names: std::collections::BTreeMap<String, serde::de::IgnoredAny>,
            }
            serde_json::from_str::<Names>(&text).map(|n| n.names.into_keys().take(256).collect()).unwrap_or_default()
        }
        Vendor::Codex => {
            #[derive(serde::Deserialize)]
            struct Names {
                #[serde(default)]
                mcp_servers: std::collections::BTreeMap<String, serde::de::IgnoredAny>,
            }
            toml::from_str::<Names>(&text).map(|n| n.mcp_servers.into_keys().take(256).collect()).unwrap_or_default()
        }
        Vendor::Gemini => BTreeSet::new(),
    }
}

pub fn project_names(root: &Path, vendor: Vendor) -> BTreeSet<String> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    project_names_with(root, vendor, home.as_deref(), &|path| {
        let mut text = String::new();
        std::fs::File::open(path).ok()?.take(1024 * 1024 + 1).read_to_string(&mut text).ok()?;
        (text.len() <= 1024 * 1024).then_some(text)
    })
}

pub fn rows(
    reported: &[Reported],
    added: &BTreeSet<String>,
    installed: &BTreeSet<String>,
    disabled: &BTreeSet<String>,
    project: &BTreeSet<String>,
) -> Vec<Row> {
    let mut rows = std::collections::BTreeMap::new();
    for server in reported {
        let catalog = added.contains(&server.name);
        rows.insert(
            server.name.clone(),
            Row {
                name: server.name.clone(),
                source: if catalog {
                    "catalog"
                } else if project.contains(&server.name) {
                    "project"
                } else {
                    "user"
                },
                status: server.status.clone(),
                enabled: catalog.then(|| !disabled.contains(&server.name)),
            },
        );
    }
    for name in installed {
        rows.entry(name.clone()).or_insert_with(|| Row {
            name: name.clone(),
            source: "catalog",
            status: None,
            enabled: Some(!disabled.contains(name)),
        });
    }
    rows.into_values().collect()
}
