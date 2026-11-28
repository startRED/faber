//! A project copy's declared app command and the confirmation kept on its project.
use crate::model::Project;
use faber_engine_core::map::package_scripts;
use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::path::Path;

const MAX_FILE: u64 = 256 * 1024;
const CONFIGS: [&str; 5] = [".npmrc", ".yarnrc", ".yarnrc.yml", "bunfig.toml", "pnpm-workspace.yaml"];

pub struct Declaration {
    pub manager: &'static str,
    pub script: &'static str,
    pub scripts: Vec<(String, String)>,
}

impl Declaration {
    pub fn command(&self) -> String {
        format!("{} run {}", self.manager, self.script)
    }

    pub fn stamp(&self, folder: &Path) -> io::Result<String> {
        let input = serde_json::json!([self.manager, self.script, self.scripts]);
        let mut digest = Sha256::new();
        digest.update(input.to_string());
        let mut bytes = Vec::new();
        for name in CONFIGS {
            let path = folder.join(name);
            digest.update(name);
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    digest.update([0]);
                    continue;
                }
                Err(err) => return Err(err),
            };
            let path = if metadata.is_symlink() { path.canonicalize()? } else { path };
            if !super::git::path_within(&path, folder) || !path.is_file() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "app config must be a file in the project folder"));
            }
            bytes.clear();
            std::fs::File::open(path)?.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_FILE {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "app config is too large"));
            }
            digest.update([1]);
            digest.update(Sha256::digest(&bytes));
        }
        Ok(crate::hex(&digest.finalize()))
    }
}

pub fn seen(stamp: &str, folder: &Path) -> String {
    let mut digest = Sha256::new();
    digest.update(stamp);
    digest.update(folder.as_os_str().as_encoded_bytes());
    crate::hex(&digest.finalize())
}

pub fn confirmed(project: &Project, stamp: &str) -> bool {
    project.extra.get("runApp").and_then(|v| v.as_str()) == Some(stamp)
}

pub fn confirm(project: &mut Project, stamp: String) {
    project.extra.insert("runApp".into(), stamp.into());
}

pub fn declared(folder: &Path) -> Option<Declaration> {
    let path = folder.join("package.json");
    if !path.is_file() {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(path).ok()?.take(MAX_FILE + 1).read_to_string(&mut text).ok()?;
    if text.len() as u64 > MAX_FILE {
        return None;
    }
    let scripts = package_scripts(&text)?;
    let body = |key: &str| scripts.get(key)?.as_str().filter(|line| !line.trim().is_empty());
    let script = ["dev", "start", "serve"].into_iter().find(|name| body(name).is_some())?;
    let manager = [("pnpm-lock.yaml", "pnpm"), ("yarn.lock", "yarn"), ("bun.lock", "bun"), ("bun.lockb", "bun")]
        .into_iter()
        .find_map(|(lock, manager)| folder.join(lock).is_file().then_some(manager))
        .unwrap_or("npm");
    // The package manager can also run lifecycle scripts around the selected script.
    let scripts = [format!("pre{script}"), script.into(), format!("post{script}")]
        .into_iter()
        .filter_map(|name| Some((name.clone(), body(&name)?.to_owned())))
        .collect();
    Some(Declaration { manager, script, scripts })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_preferred_script_and_lockfile_and_asks_again_when_the_declaration_changes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let package = root.join("package.json");
        let mut project = Project::new("p".into(), "P".into(), root.to_string_lossy().into_owned());
        std::fs::write(root.join("index.html"), "<title>App</title>").unwrap();
        assert!(declared(root).is_none());
        std::fs::write(&package, r#"{"scripts":{"serve":"serve","start":"start","dev":"vite"}}"#).unwrap();
        assert_eq!(declared(root).unwrap().command(), "npm run dev");
        std::fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
        let first = declared(root).unwrap();
        assert_eq!(first.command(), "pnpm run dev");
        let first = first.stamp(root).unwrap();
        assert!(!confirmed(&project, &first));
        confirm(&mut project, first.clone());
        assert!(confirmed(&project, &declared(root).unwrap().stamp(root).unwrap()));
        std::fs::write(&package, r#"{"scripts":{"dev":"vite","predev":"prepare"}}"#).unwrap();
        let changed = declared(root).unwrap().stamp(root).unwrap();
        assert_ne!(seen(&first, root), seen(&changed, root));
        assert!(!confirmed(&project, &changed));
        std::fs::write(&package, r#"{"scripts":{"dev":" ","start":"start","serve":"serve"}}"#).unwrap();
        assert_eq!(declared(root).unwrap().command(), "pnpm run start");
        std::fs::write(&package, r#"{"scripts":{"serve":"serve"}}"#).unwrap();
        assert_eq!(declared(root).unwrap().command(), "pnpm run serve");
    }

    #[test]
    fn config_creation_edit_and_removal_each_invalidate_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        let mut project = Project::new("p".into(), "P".into(), root.to_string_lossy().into_owned());
        let stamp = || declared(root).unwrap().stamp(root).unwrap();
        for name in [".npmrc", ".yarnrc", ".yarnrc.yml", "bunfig.toml", "pnpm-workspace.yaml"] {
            let path = root.join(name);
            confirm(&mut project, stamp());
            std::fs::write(&path, []).unwrap();
            assert!(!confirmed(&project, &stamp()), "adding {name}");
            confirm(&mut project, stamp());
            std::fs::write(&path, [0xff]).unwrap();
            assert!(!confirmed(&project, &stamp()), "editing {name}");
            confirm(&mut project, stamp());
            std::fs::remove_file(&path).unwrap();
            assert!(!confirmed(&project, &stamp()), "removing {name}");
        }
    }
}
