//! Native project-only parts of the same verified catalog snapshot.

use super::plugins::{Definition, PartKind, part_digest};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Agent {
    pub name: String,
    pub path: PathBuf,
    pub description: String,
}

fn file_name(plugin: &Definition, part: &super::plugins::Part) -> String {
    format!("{}--{}", plugin.item.id, part.id)
}

fn directory(kind: PartKind) -> Option<&'static str> {
    match kind {
        PartKind::Command => Some("commands"),
        PartKind::Agent => Some("agents"),
        _ => None,
    }
}

/// Only instructions enter the Codex role layer; no tool, model, permission or sandbox override is generated.
fn agent_config(markdown: &str) -> io::Result<String> {
    // The same opening and line breaks as the install check, so what installed always has a closing line here.
    let front = super::front_matter(markdown).ok_or_else(|| io::Error::other("Invalid checked agent front matter"))?;
    let mut consumed = 0;
    let mut instructions = None;
    for line in front.split_inclusive('\n') {
        consumed += line.len();
        if line.trim() == "---" {
            instructions = Some(&front[consumed..]);
            break;
        }
    }
    let instructions = instructions.ok_or_else(|| io::Error::other("Unclosed checked agent front matter"))?;
    #[derive(Serialize)]
    struct Config<'a> {
        developer_instructions: &'a str,
    }
    toml::to_string(&Config { developer_instructions: instructions }).map_err(io::Error::other)
}

pub(super) fn has_native_parts(plugins: &[Definition]) -> bool {
    plugins.iter().any(|plugin| plugin.parts.iter().any(|part| directory(part.kind).is_some()))
}

/// Only an actual digest mismatch marks an install damaged; unreadable or linked paths remain snapshot I/O failures.
pub(super) fn source_changed(store: &Path, plugin: &Definition) -> bool {
    let parent = store.join("plugins").join(&plugin.item.id);
    if [store.to_path_buf(), store.join("plugins"), parent.clone(), parent.join("source")]
        .iter()
        .any(|path| !std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()))
    {
        return false;
    }
    if super::digest(&parent.join("source")).is_ok_and(|actual| actual != plugin.item.digest) {
        return true;
    }
    plugin.parts.iter().filter_map(|part| part.server.as_ref().map(|item| (part, item))).any(|(part, item)| {
        let packages = parent.join("servers");
        let path = packages.join(&part.id);
        std::fs::symlink_metadata(&packages).is_ok_and(|meta| meta.is_dir())
            && std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir())
            && super::digest(&path).is_ok_and(|actual| actual != item.digest)
    })
}

pub(super) fn names(plugins: &[Definition], kind: PartKind) -> BTreeSet<std::ffi::OsString> {
    let mut names = BTreeSet::new();
    for plugin in plugins {
        for part in plugin.parts.iter().filter(|part| part.kind == kind) {
            let name = file_name(plugin, part);
            names.insert(format!("{name}.md").into());
            if kind == PartKind::Agent {
                names.insert(format!("{name}.toml").into());
            }
        }
    }
    names
}

pub(super) fn copy_parts(store: &Path, target: &Path, plugins: &[Definition], remaining: &mut u64) -> io::Result<()> {
    for plugin in plugins {
        for part in &plugin.parts {
            let Some(directory) = directory(part.kind) else { continue };
            let source = store.join("plugins").join(&plugin.item.id).join("source").join(&part.path);
            if part_digest(&source)? != part.digest {
                return Err(io::Error::other("The plugin part changed during snapshot preparation"));
            }
            let length = std::fs::symlink_metadata(&source)?.len();
            *remaining = remaining.checked_sub(length).ok_or_else(|| io::Error::other("Catalog snapshot exceeds its size limit"))?;
            let parent = target.join(directory);
            std::fs::create_dir_all(&parent)?;
            if !std::fs::symlink_metadata(&parent)?.is_dir() {
                return Err(io::Error::other("Not a plain native part folder"));
            }
            let name = file_name(plugin, part);
            let dest = parent.join(format!("{name}.md"));
            if std::fs::hard_link(&source, &dest).is_err() {
                let mut input = std::fs::File::open(&source)?;
                let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&dest)?;
                std::io::copy(&mut input, &mut output)?;
            }
            if part.kind == PartKind::Agent {
                let config = agent_config(&std::fs::read_to_string(&dest)?)?;
                *remaining = remaining
                    .checked_sub(config.len() as u64)
                    .ok_or_else(|| io::Error::other("Catalog snapshot exceeds its size limit"))?;
                let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(parent.join(format!("{name}.toml")))?;
                std::io::Write::write_all(&mut output, config.as_bytes())?;
            }
        }
    }
    Ok(())
}

/// Covers the complete native folders, including generated role layers and unexpected files.
pub(super) fn intact(root: &Path, plugins: &[Definition]) -> bool {
    for (kind, directory) in [(PartKind::Command, "commands"), (PartKind::Agent, "agents")] {
        let expected = names(plugins, kind);
        let parent = root.join(directory);
        if expected.is_empty() {
            if std::fs::symlink_metadata(&parent).is_ok() {
                return false;
            }
            continue;
        }
        if !std::fs::symlink_metadata(&parent).is_ok_and(|meta| meta.is_dir())
            || super::entries(&parent).map(|entries| entries.into_iter().map(|entry| entry.file_name()).collect::<BTreeSet<_>>())
                != Some(expected)
        {
            return false;
        }
        for plugin in plugins {
            for part in plugin.parts.iter().filter(|part| part.kind == kind) {
                let name = file_name(plugin, part);
                let path = parent.join(format!("{name}.md"));
                let Some(bytes) = super::small_file(&path) else { return false };
                if crate::hex(&Sha256::digest(&bytes)) != part.digest {
                    return false;
                }
                if kind == PartKind::Agent {
                    let expected = std::str::from_utf8(&bytes).map_err(io::Error::other).and_then(agent_config);
                    if !expected.is_ok_and(|expected| {
                        super::small_file(&parent.join(format!("{name}.toml"))).is_some_and(|actual| actual == expected.as_bytes())
                    }) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Built off the engine lock with the snapshot; ordinary thread creation only clones this bounded metadata.
pub(super) fn agents(root: &Path, plugins: &[Definition]) -> io::Result<Vec<Agent>> {
    let mut agents = Vec::new();
    for plugin in plugins {
        for part in plugin.parts.iter().filter(|part| part.kind == PartKind::Agent) {
            let name = file_name(plugin, part);
            let bytes = super::small_file(&root.join("agents").join(format!("{name}.md")))
                .ok_or_else(|| io::Error::other("Unreadable checked agent"))?;
            if crate::hex(&Sha256::digest(&bytes)) != part.digest {
                return Err(io::Error::other("The agent changed during metadata preparation"));
            }
            let markdown = std::str::from_utf8(&bytes).map_err(io::Error::other)?;
            agents.push(Agent {
                name: format!("faber-{name}"),
                path: root.join("agents").join(format!("{name}.toml")),
                description: super::kits::checked_description(markdown, &part.id),
            });
        }
    }
    Ok(agents)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_role_layer_preserves_instructions_without_permission_overrides() {
        let input = "---\r\nname: reader\r\ndescription: Read a change\r\n---\r\nRead \"quoted\" code.\nKeep paths like C:\\work intact.\n";
        let config: toml::Value = toml::from_str(&super::agent_config(input).unwrap()).unwrap();
        let table = config.as_table().unwrap();
        assert_eq!(table.len(), 1);
        assert_eq!(table["developer_instructions"].as_str().unwrap(), "Read \"quoted\" code.\nKeep paths like C:\\work intact.\n");
        for input in [
            "\u{feff}---\nname: reader\ndescription: Read\n---\nBody",
            "---\nname: reader\ndescription: Read\n  ---  ",
            "---\nname: reader\ndescription: Read\n---",
        ] {
            assert!(super::agent_config(input).is_ok());
        }
        // Every line break the install check honours splits the same lines here.
        let config = super::agent_config("---\rname: reader\rdescription: Read\r---\rBody\rmore").unwrap();
        assert_eq!(toml::from_str::<toml::Value>(&config).unwrap()["developer_instructions"].as_str(), Some("Body\nmore"));
        assert!(super::agent_config("missing front matter").is_err());
    }
}
