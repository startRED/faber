//! Checked composite catalog items. Their parts share the parent's store transaction and switches.

use super::{Item, Store, digest, portable_name, valid_id};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Definition {
    #[serde(flatten)]
    pub item: Item,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Part {
    pub id: String,
    pub kind: PartKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_name: Option<String>,
    pub path: String,
    /// Folder digests use the skill algorithm; a prompt file uses SHA-256 of its bytes.
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<Item>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PartKind {
    Skill,
    Command,
    Agent,
    Server,
}

pub fn catalog() -> &'static [Definition] {
    static PLUGINS: OnceLock<Vec<Definition>> = OnceLock::new();
    PLUGINS.get_or_init(|| {
        serde_json::from_str::<super::CatalogFile>(include_str!("../tools.json")).map(|file| file.plugins).unwrap_or_default()
    })
}

fn relative(path: &str) -> bool {
    !path.is_empty() && path.split('/').all(|part| portable_name(part) && !matches!(part, "." | ".."))
}

/// A catalog id with no leading or trailing dash, so `<plugin>--<part>` splits back into exactly its two ids.
fn plain_id(id: &str) -> bool {
    valid_id(id) && !id.starts_with('-') && !id.ends_with('-')
}

fn pin(pin: &str, length: usize) -> bool {
    pin.len() == length && pin.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl Definition {
    pub(super) fn validate(&self) -> Result<(), String> {
        if !plain_id(&self.item.id)
            || self.item.id.contains("--")
            || self.item.chat_safe
            || self.item.cli_name.is_some()
            || self.item.server.is_some()
            || !pin(&self.item.commit, 40)
            || !pin(&self.item.digest, 64)
            || !self.item.path.is_empty() && !relative(&self.item.path)
            || self.item.repo.split('/').count() != 2
            || !self.item.repo.split('/').all(portable_name)
            || self.parts.is_empty()
            || self.parts.len() > 64
        {
            return Err("Invalid pinned plugin definition".into());
        }
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        for part in &self.parts {
            if !plain_id(&part.id)
                || part.id.contains("--")
                || format!("{}--{}", self.item.id, part.id).len() > 64
                || !ids.insert(&part.id)
                || part.cli_name.as_ref().is_some_and(|name| !valid_id(name) || !matches!(part.kind, PartKind::Skill | PartKind::Agent))
                || matches!(part.kind, PartKind::Skill | PartKind::Agent)
                    && !aliases.insert((part.kind as u8, part.cli_name.as_deref().unwrap_or(&part.id)))
                || !relative(&part.path)
                || !pin(&part.digest, 64)
                || paths.iter().any(|previous: &&str| {
                    previous.eq_ignore_ascii_case(&part.path)
                        || part.path.to_ascii_lowercase().starts_with(&format!("{}/", previous.to_ascii_lowercase()))
                        || previous.to_ascii_lowercase().starts_with(&format!("{}/", part.path.to_ascii_lowercase()))
                })
            {
                return Err("Invalid, duplicate or overlapping plugin part".into());
            }
            paths.insert(part.path.as_str());
            match (part.kind, &part.server) {
                (PartKind::Server, Some(item)) => {
                    let server = item.server.as_ref().ok_or("A plugin server needs a pinned npm package")?;
                    if item.chat_safe
                        || item.cli_name.is_some()
                        || !pin(&item.commit, 40)
                        || !pin(&item.digest, 64)
                        || !relative(&server.entry)
                        || !server.entry.ends_with(".js") && !server.entry.ends_with(".mjs") && !server.entry.ends_with(".cjs")
                        || server.package.split('/').any(|part| !portable_name(part) || matches!(part, "." | ".."))
                        || server.package.contains([':', '\\'])
                        || server.version.split('.').count() != 3
                        || !server
                            .version
                            .split('.')
                            .all(|component| !component.is_empty() && component.bytes().all(|b| b.is_ascii_digit()))
                    {
                        return Err("A plugin server is not R253-portable".into());
                    }
                }
                (PartKind::Server, None) | (_, Some(_)) => return Err("A server package belongs only to a server part".into()),
                _ => {}
            }
        }
        Ok(())
    }
}

pub fn part_digest(path: &Path) -> io::Result<String> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() {
        digest(path)
    } else if meta.is_file() && meta.len() <= super::SKILL_LIMIT {
        Ok(crate::hex(&Sha256::digest(std::fs::read(path)?)))
    } else {
        Err(io::Error::other("Not a bounded plain plugin part"))
    }
}

/// A command or agent body can name a file with `@path`; only a path inside the project is portable.
fn leaves_project(body: &str) -> bool {
    body.match_indices('@').any(|(at, _)| {
        let path = body[at + 1..].split(char::is_whitespace).next().unwrap_or("").trim_start_matches(['"', '\'', '`', '(']);
        path.starts_with(['~', '/', '\\', '$', '%'])
            || path.as_bytes().get(1) == Some(&b':')
            || path.split(['/', '\\']).any(|part| part == "..")
    })
}

/// Native definitions may describe a task, but cannot change the session's tools, permissions or model.
fn prompt(body: &str, kind: PartKind) -> Result<(), String> {
    let body = super::yaml_newlines(body.trim_start_matches('\u{feff}'));
    let body = body.as_str();
    if let Some(key) = super::widening_key(body) {
        return Err(format!("Plugin instructions set {key}"));
    }
    if let Some(front) = super::front_matter(body) {
        let mut closed = false;
        let mut named = false;
        let mut described = false;
        let mut description_block = false;
        let mut keys = BTreeSet::new();
        for line in front.lines() {
            if super::front_line(line)? {
                closed = true;
                break;
            }
            if line.is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            if line.starts_with([' ', '\t']) {
                if description_block {
                    continue;
                }
                return Err("Indented plugin metadata is not portable".into());
            }
            let (key, value) = line.split_once(':').ok_or("Invalid plugin front matter")?;
            let key = key.trim().trim_matches(['\'', '"']);
            if !keys.insert(key) {
                return Err("Duplicate plugin instruction fields are not portable".into());
            }
            let value = value.trim().trim_matches(['\'', '"']);
            named |= key == "name" && valid_id(value);
            described |= key == "description" && !value.is_empty();
            description_block = key == "description" && matches!(value, ">" | ">-" | ">+" | "|" | "|-" | "|+");
            let allowed = match kind {
                PartKind::Skill => matches!(key, "name" | "description" | "license" | "compatibility" | "metadata"),
                PartKind::Agent => matches!(key, "name" | "description"),
                PartKind::Command => matches!(key, "description" | "argument-hint"),
                PartKind::Server => false,
            };
            if !allowed {
                return Err(format!("Unsupported plugin instruction field: {key}"));
            }
        }
        if !closed {
            return Err("Unclosed plugin front matter".into());
        }
        if kind == PartKind::Agent && (!named || !described) {
            return Err("A plugin agent needs a valid name and description".into());
        }
    } else if kind == PartKind::Agent {
        return Err("A plugin agent needs a name and description".into());
    }
    if leaves_project(body) {
        return Err("Plugin instructions read a file outside the project".into());
    }
    if body.contains("${CLAUDE_PLUGIN_ROOT}") || body.contains("${CLAUDE_PLUGIN_DATA}") || body.lines().any(shell_marker) {
        return Err("Plugin instructions depend on native execution or a different plugin root".into());
    }
    Ok(())
}

/// Claude Code runs `!` followed by a backtick span and a fenced block opened by "```!" (any fence length, indentation or
/// place on the line) when it loads a command or skill; "~~~!" is refused too, as no reader may treat it differently.
fn shell_marker(line: &str) -> bool {
    line.contains("!`") || line.contains("```!") || line.contains("~~~!")
}

fn instruction_name(body: &str) -> Option<String> {
    let body = super::yaml_newlines(body.trim_start_matches('\u{feff}'));
    let front = super::front_matter(&body)?;
    for line in front.lines().take_while(|line| super::front_line(line) == Ok(false)) {
        if line.starts_with([' ', '\t']) {
            continue;
        }
        if let Some((key, value)) = line.split_once(':')
            && key.trim().trim_matches(['\'', '"']) == "name"
        {
            let value = value.trim().trim_matches(['\'', '"']);
            return valid_id(value).then(|| value.to_owned());
        }
    }
    None
}

fn check_manifest(definition: &Definition, manifest: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    for (field, kind) in
        [("skills", PartKind::Skill), ("commands", PartKind::Command), ("agents", PartKind::Agent), ("mcpServers", PartKind::Server)]
    {
        let Some(value) = manifest.get(field) else { continue };
        let references: Vec<_> = if let Some(path) = value.as_str() {
            vec![path]
        } else if let Some(paths) = value.as_array() {
            paths.iter().map(|path| path.as_str().ok_or("A plugin manifest has an unsupported part reference")).collect::<Result<_, _>>()?
        } else {
            return Err("Embedded native plugin definitions are not portable".into());
        };
        let parts: Vec<_> = definition.parts.iter().filter(|part| part.kind == kind).collect();
        let references: Vec<_> = references.iter().map(|path| path.strip_prefix("./").unwrap_or(path).trim_end_matches('/')).collect();
        let covers = |reference: &str, part: &Part| {
            part.path == reference || kind != PartKind::Server && part.path.starts_with(&format!("{reference}/"))
        };
        if references.iter().any(|reference| !relative(reference) || !parts.iter().any(|part| covers(reference, part)))
            || parts.iter().any(|part| !references.iter().any(|reference| covers(reference, part)))
        {
            return Err(format!("Plugin manifest {field} includes parts the catalog cannot deliver"));
        }
    }
    Ok(())
}

fn check_source(definition: &Definition, source: &Path) -> Result<(), String> {
    definition.validate()?;
    if !std::fs::symlink_metadata(source).is_ok_and(|metadata| metadata.is_dir()) {
        return Err("The plugin source is not a plain folder".into());
    }
    if digest(source).map_err(|e| e.to_string())? != definition.item.digest {
        return Err("The plugin source does not match the catalog".into());
    }
    let mut files = Vec::new();
    super::collect(source, source, &mut files).map_err(|e| e.to_string())?;
    let mut seen = BTreeSet::new();
    for relative in &files {
        if !relative.split('/').all(portable_name) || !seen.insert(relative.to_ascii_lowercase()) {
            return Err("A plugin source has an unsafe or duplicate name".into());
        }
        let path = Path::new(relative);
        let native_kind = if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
            Some(PartKind::Skill)
        } else if relative.starts_with("commands/") && relative.ends_with(".md") {
            Some(PartKind::Command)
        } else if relative.starts_with("agents/") && relative.ends_with(".md") {
            Some(PartKind::Agent)
        } else if relative == ".mcp.json" {
            Some(PartKind::Server)
        } else {
            None
        };
        if let Some(kind) = native_kind {
            let declared = definition.parts.iter().any(|part| {
                part.kind == kind
                    && if kind == PartKind::Skill { relative == &format!("{}/SKILL.md", part.path) } else { relative == &part.path }
            });
            if !declared {
                return Err(format!("The plugin has an undeclared native part: {relative}"));
            }
        }
        if path.components().any(|component| component.as_os_str().to_str().is_some_and(|name| name.eq_ignore_ascii_case("hooks"))) {
            return Err("Plugins with hooks cannot be delivered safely".into());
        }
        // Parts Claude Code would load from the plugin root that Faber never delivers: refuse them instead of dropping them.
        let top = relative.split('/').next().unwrap_or_default().to_ascii_lowercase();
        if matches!(top.as_str(), "bin" | "monitors" | "output-styles" | "workflows" | "settings.json" | ".lsp.json") {
            return Err(format!("The plugin has a native part Faber cannot deliver: {relative}"));
        }
        let body = std::fs::read(source.join(relative)).map_err(|e| e.to_string())?;
        let text = std::str::from_utf8(&body).map_err(|_| "A plugin contains a binary file")?;
        let extension = path.extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase();
        if text.starts_with("#!")
            || body.contains(&0)
            || matches!(
                extension.as_str(),
                "exe" | "dll" | "so" | "dylib" | "node" | "wasm" | "sh" | "bat" | "cmd" | "ps1" | "py" | "js" | "mjs" | "cjs"
            )
        {
            return Err("Plugins with shipped executable files are not portable".into());
        }
        if matches!(extension.as_str(), "md" | "markdown")
            && let Some(key) = super::widening_key(text)
        {
            return Err(format!("Plugin instructions set {key}"));
        }
        if relative == ".claude-plugin/plugin.json" {
            let manifest: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
            let manifest = manifest.as_object().ok_or("Invalid plugin manifest")?;
            for key in manifest.keys() {
                if !matches!(
                    key.as_str(),
                    "name"
                        | "version"
                        | "description"
                        | "author"
                        | "homepage"
                        | "repository"
                        | "license"
                        | "keywords"
                        | "skills"
                        | "commands"
                        | "agents"
                        | "mcpServers"
                ) {
                    return Err(format!("Unsupported native plugin field: {key}"));
                }
            }
            check_manifest(definition, manifest)?;
        }
        if relative.ends_with("package.json") {
            let package: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
            if package.get("scripts").is_some_and(|value| value.as_object().is_none_or(|object| !object.is_empty())) {
                return Err("Plugins with package scripts are not portable".into());
            }
        }
    }
    for part in &definition.parts {
        let path = source.join(&part.path);
        if part_digest(&path).map_err(|e| e.to_string())? != part.digest {
            return Err(format!("The plugin part {} does not match the catalog", part.id));
        }
        match part.kind {
            PartKind::Skill => {
                let body = std::fs::read_to_string(path.join("SKILL.md")).map_err(|_| "A plugin skill has no SKILL.md")?;
                prompt(&body, PartKind::Skill)?;
                let name = instruction_name(&body).ok_or("A plugin skill needs a portable name")?;
                if name != part.cli_name.as_deref().unwrap_or(&part.id) {
                    return Err("The plugin skill name does not match its checked alias".into());
                }
            }
            PartKind::Command | PartKind::Agent => {
                if !part.path.ends_with(".md") || !path.is_file() {
                    return Err("Plugin commands and agents must be plain Markdown files".into());
                }
                let body = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                prompt(&body, part.kind)?;
                if part.kind == PartKind::Agent && instruction_name(&body).as_deref() != Some(part.cli_name.as_deref().unwrap_or(&part.id))
                {
                    return Err("The plugin agent name does not match its checked alias".into());
                }
            }
            PartKind::Server => {
                let value: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                let server = part.server.as_ref().and_then(|item| item.server.as_ref()).ok_or("Missing portable server")?;
                let record = value.as_object().ok_or("A server part must be its own stdio definition")?;
                let record = if record.contains_key("mcpServers") {
                    let records =
                        record.get("mcpServers").and_then(serde_json::Value::as_object).ok_or("Invalid plugin MCP definitions")?;
                    if record.len() != 1 || records.len() != 1 {
                        return Err("A server source must describe one checked portable server".into());
                    }
                    records.values().next().and_then(serde_json::Value::as_object).ok_or("Invalid plugin MCP definition")?
                } else {
                    record
                };
                if record.keys().any(|key| !matches!(key.as_str(), "command" | "args" | "env" | "type"))
                    || record.get("type").is_some_and(|value| value != "stdio")
                    || record.get("command").and_then(|value| value.as_str()) != Some("npx")
                    || record.get("args") != Some(&serde_json::json!(["-y", format!("{}@{}", server.package, server.version)]))
                    || record.get("env").is_some_and(|value| value != &serde_json::json!(server.env))
                {
                    return Err("The plugin server definition is not the pinned portable package".into());
                }
            }
        }
    }
    Ok(())
}

fn check_package(item: &Item, path: &Path) -> Result<(), String> {
    let server = item.server.as_ref().ok_or("Missing plugin server package")?;
    if digest(path).map_err(|e| e.to_string())? != item.digest || !path.join(&server.entry).is_file() {
        return Err("The plugin server package does not match the catalog".into());
    }
    let mut files = Vec::new();
    super::collect(path, path, &mut files).map_err(|error| error.to_string())?;
    for file in files {
        let extension = Path::new(&file).extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase();
        let bytes = std::fs::read(path.join(&file)).map_err(|error| error.to_string())?;
        if bytes.contains(&0)
            || std::str::from_utf8(&bytes).is_err()
            || matches!(extension.as_str(), "exe" | "dll" | "so" | "dylib" | "node" | "wasm" | "bat" | "cmd" | "ps1" | "sh")
        {
            return Err("A plugin server contains a binary or native executable".into());
        }
    }
    let package: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("package.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if !package.is_object()
        || ["dependencies", "optionalDependencies", "peerDependencies", "bundledDependencies", "bundleDependencies", "scripts"]
            .iter()
            .any(|key| package.get(key).is_some_and(|value| !value.as_object().is_some_and(|object| object.is_empty())))
    {
        return Err("A plugin server must have no dependencies or lifecycle scripts".into());
    }
    Ok(())
}

pub(super) fn extract(definition: &Definition, archive: &Path, packages: &BTreeMap<String, PathBuf>, staging: &Path) -> Result<(), String> {
    definition.validate()?;
    let expected: BTreeSet<_> = definition.parts.iter().filter(|part| part.kind == PartKind::Server).map(|part| part.id.clone()).collect();
    if packages.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err("Each plugin server needs its pinned package".into());
    }
    let source = staging.join("source");
    super::extract_skill(archive, &definition.item.path, &source)?;
    check_source(definition, &source)?;
    for part in definition.parts.iter().filter(|part| part.kind == PartKind::Server) {
        let dest = staging.join("servers").join(&part.id);
        super::extract_package(&packages[&part.id], &dest)?;
        check_package(part.server.as_ref().ok_or("Missing server package")?, &dest)?;
    }
    Ok(())
}

fn verify(definition: &Definition, dir: &Path) -> Result<(), String> {
    if !std::fs::symlink_metadata(dir).is_ok_and(|metadata| metadata.is_dir()) {
        return Err("The plugin is not a plain directory".into());
    }
    let servers: BTreeSet<_> = definition.parts.iter().filter(|part| part.kind == PartKind::Server).map(|part| part.id.clone()).collect();
    let expected: BTreeSet<_> =
        if servers.is_empty() { ["source"].into_iter().collect() } else { ["source", "servers"].into_iter().collect() };
    let entries = super::entries(dir).ok_or("The plugin cannot be listed")?;
    if entries.iter().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect::<BTreeSet<_>>()
        != expected.into_iter().map(str::to_owned).collect()
    {
        return Err("Unexpected files beside the checked plugin source".into());
    }
    check_source(definition, &dir.join("source"))?;
    if !servers.is_empty() {
        let parent = dir.join("servers");
        if !std::fs::symlink_metadata(&parent).is_ok_and(|metadata| metadata.is_dir())
            || super::entries(&parent)
                .ok_or("The plugin servers cannot be listed")?
                .iter()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect::<BTreeSet<_>>()
                != servers
        {
            return Err("Unexpected plugin server files".into());
        }
        for part in definition.parts.iter().filter(|part| part.kind == PartKind::Server) {
            let path = parent.join(&part.id);
            if !std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_dir()) {
                return Err("A plugin server is not a plain folder".into());
            }
            check_package(part.server.as_ref().ok_or("Missing server package")?, &path)?;
        }
    }
    Ok(())
}

/// Rechecks the complete original plugin and packages before a session receives any of its parts.
pub(crate) fn verify_definition(store_root: &Path, definition: &Definition) -> bool {
    [store_root.to_path_buf(), store_root.join("plugins")]
        .iter()
        .all(|path| std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir()))
        && verify(definition, &store_root.join("plugins").join(&definition.item.id)).is_ok()
}

/// Static launch metadata, without opening the store or creating a background cache.
pub fn expand(items: &[Item], definitions: &[Definition]) -> Vec<Item> {
    let mut items: Vec<_> =
        items.iter().filter(|item| !definitions.iter().any(|definition| definition.item.id == item.id)).cloned().collect();
    for definition in definitions {
        for part in &definition.parts {
            if !matches!(part.kind, PartKind::Skill | PartKind::Server) {
                continue;
            }
            let mut item = part.server.clone().unwrap_or_else(|| definition.item.clone());
            item.id = format!("{}--{}", definition.item.id, part.id);
            item.job = definition.item.job.clone();
            item.tier = definition.item.tier;
            if part.kind == PartKind::Skill {
                // Claude names a plugin skill by its own front-matter name, so the index must say the same.
                item.cli_name = Some(part.cli_name.clone().unwrap_or_else(|| part.id.clone()));
                item.digest = part.digest.clone();
                item.path = format!("plugins/{}/source/{}", definition.item.id, part.path);
            } else {
                item.path = format!("plugins/{}/servers/{}", definition.item.id, part.id);
            }
            items.push(item);
        }
    }
    items
}

pub fn resources() -> &'static [Item] {
    static RESOURCES: OnceLock<Vec<Item>> = OnceLock::new();
    RESOURCES.get_or_init(|| expand(super::catalog(), catalog()))
}

pub fn find_resource(id: &str) -> Option<&'static Item> {
    resources().iter().find(|item| item.id == id)
}

impl Store {
    pub(super) fn resource_catalog(&self) -> &[Item] {
        if self.plugins.is_empty() { &self.items } else { &self.resources }
    }

    pub fn plugin_definitions(&self) -> &[Definition] {
        &self.plugins
    }

    pub fn plugin_definition(&self, id: &str) -> Option<&Definition> {
        self.plugins.iter().find(|definition| definition.item.id == id)
    }

    pub(super) fn validate_plugin_aliases(&self, definition: &Definition) -> Result<(), String> {
        let prefix = format!("{}--", definition.item.id);
        for part in definition.parts.iter().filter(|part| matches!(part.kind, PartKind::Skill | PartKind::Agent)) {
            let alias = part.cli_name.as_deref().unwrap_or(&part.id);
            let duplicate = if part.kind == PartKind::Skill {
                self.resources.iter().any(|item| {
                    item.server.is_none() && !item.id.starts_with(&prefix) && item.cli_name.as_deref().unwrap_or(&item.id) == alias
                })
            } else {
                self.plugins
                    .iter()
                    .filter(|other| other.item.id != definition.item.id)
                    .flat_map(|other| &other.parts)
                    .any(|other| other.kind == PartKind::Agent && other.cli_name.as_deref().unwrap_or(&other.id) == alias)
            };
            if duplicate {
                return Err(format!("The plugin instruction name {alias} is already used in the catalog"));
            }
        }
        Ok(())
    }

    pub(super) fn verify_item(&self, dir: &Path, item: &Item) -> bool {
        if let Some(definition) = self.plugin_definition(&item.id) {
            definition.item == *item && verify(definition, dir).is_ok()
        } else {
            digest(dir).is_ok_and(|actual| actual == item.digest)
                && item.server.as_ref().is_none_or(|server| dir.join(&server.entry).is_file())
        }
    }

    /// Children are virtual resources: only their parent ever downloads, installs or enters the ledger.
    pub fn resource_items(&self) -> Vec<Item> {
        self.resource_catalog().to_vec()
    }

    /// Cache identity includes delivery metadata as well as each original source digest.
    pub(super) fn trust_items(&self) -> Vec<Item> {
        let mut items = self.resource_items();
        items.extend(self.plugins.iter().map(|definition| {
            let mut item = definition.item.clone();
            item.digest = crate::hex(&Sha256::digest(serde_json::to_vec(definition).unwrap_or_default()));
            item
        }));
        items
    }

    pub fn resource_source(&self, item: &Item) -> PathBuf {
        resource_source(&self.root, item).unwrap_or_else(|| self.dir(item))
    }

    pub fn installed_resources(&self) -> BTreeMap<String, String> {
        let mut installed = self.installed();
        for item in self.resource_catalog() {
            if resource_source(&self.root, item).is_some() {
                let parent = item.id.split_once("--").map_or(item.id.as_str(), |(parent, _)| parent);
                if installed.contains_key(parent) {
                    installed.insert(item.id.clone(), item.commit.clone());
                }
            }
        }
        installed
    }

    pub fn resource_selection(&self) -> Option<super::kits::Selection> {
        let selection = self.read_selection()?;
        let mut resources = selection.clone();
        for item in self.resource_catalog() {
            if resource_source(&self.root, item).is_none() {
                continue;
            }
            let parent = item.id.split_once("--").map_or(item.id.as_str(), |(parent, _)| parent);
            if selection.disabled.contains(parent) {
                resources.disabled.insert(item.id.clone());
            }
            if selection.always.contains(parent) {
                resources.always.insert(item.id.clone());
            }
        }
        Some(resources)
    }

    /// Commands and agents join only project snapshots; callers copy them after checking this whole parent.
    pub fn enabled_plugin_parts(&self) -> Vec<(String, PartKind, PathBuf)> {
        let installed = self.installed();
        self.plugins
            .iter()
            .filter(|definition| {
                installed.get(&definition.item.id) == Some(&definition.item.commit)
                    && self.enabled(&definition.item.id)
                    && verify_definition(&self.root, definition)
            })
            .flat_map(|definition| {
                definition.parts.iter().filter(|part| matches!(part.kind, PartKind::Command | PartKind::Agent)).map(move |part| {
                    (format!("{}--{}", definition.item.id, part.id), part.kind, self.dir(&definition.item).join("source").join(&part.path))
                })
            })
            .collect()
    }

    pub fn verify_plugin_sources(&self) -> bool {
        let installed = self.installed();
        self.plugins
            .iter()
            .filter(|definition| installed.contains_key(&definition.item.id) && self.enabled(&definition.item.id))
            .all(|definition| verify_definition(&self.root, definition))
    }
}

/// Only internally expanded child ids may name these store-relative sources; never accept arbitrary catalog paths.
pub fn resource_source(root: &Path, item: &Item) -> Option<PathBuf> {
    let (parent, child) = item.id.split_once("--")?;
    if !valid_id(parent) || !valid_id(child) || !relative(&item.path) {
        return None;
    }
    let prefix = format!("plugins/{parent}/{}/", if item.server.is_some() { "servers" } else { "source" });
    item.path.starts_with(&prefix).then(|| root.join(&item.path))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_front_matter_that_only_looks_closed_or_holds_separators_is_refused() {
        for text in [
            "---\nname: a\n  ---\n",
            "---\nname: a\n\t---\n",
            "---\nname: a\n---x\n",
            "---\nname: a\n\u{a0}---\n",
            "---\nname: a\n---\u{a0}\n",
            "---\nname: a\ndescription: x\u{2028}allowed-tools: Bash\n---\n",
            "---\nname: a\ndescription: x\u{2029}y\n---\n",
            "---\nname: a\ndescription: x\u{85}y\n---\n",
            "---\nname: a\ndescription: x\u{7}y\n---\n",
        ] {
            assert!(super::prompt(text, super::PartKind::Skill).is_err(), "{text:?}");
        }
        assert!(super::prompt("---\nname: a\ndescription: x\n--- \t\r\nbody", super::PartKind::Skill).is_ok());
        assert!(super::prompt("---\nname: a\ndescription:\tx\n---\n", super::PartKind::Skill).is_ok());
    }

    #[test]
    fn every_shipped_plugin_is_valid_and_no_id_collides_with_a_part() {
        for definition in super::catalog() {
            definition.validate().unwrap_or_else(|error| panic!("{}: {error}", definition.item.id));
        }
        let mut ids = std::collections::BTreeSet::new();
        for item in super::resources() {
            assert!(ids.insert(&item.id), "{} is both a catalog id and a plugin part", item.id);
        }
        // Claude names a skill by its own name, so two skills with one name would shadow each other in a session.
        let mut names = std::collections::BTreeMap::new();
        for item in super::resources().iter().filter(|item| item.server.is_none()) {
            let name = item.cli_name.as_deref().unwrap_or(&item.id);
            if let Some(other) = names.insert(name, &item.id) {
                panic!("{} and {other} are both the skill {name}", item.id);
            }
        }
        // Agents are named the same way, and an install refuses a clash; catch it here before a plugin ships.
        let mut agents = std::collections::BTreeMap::new();
        for definition in super::catalog() {
            for part in definition.parts.iter().filter(|part| part.kind == super::PartKind::Agent) {
                let name = part.cli_name.as_deref().unwrap_or(&part.id);
                if let Some(other) = agents.insert(name, &definition.item.id) {
                    panic!("{} and {other} both ship the agent {name}", definition.item.id);
                }
            }
        }
    }
}
