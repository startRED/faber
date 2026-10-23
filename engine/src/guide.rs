//! Asking Faber about Faber (R18): Faber's own guide, written for the AI, is a skill of Faber's own that every session has from
//! its start, so a question about Faber is answered from it and not from the AI's guess. It is not a catalog item (no card, no
//! switch, nothing to install or remove, no line in a kit's report) and never reaches an `Isolated` request.
//!
//! The guide is one file Faber writes under its data folder and checks at each launch (`plugin`), in the screen's language. How
//! a session is told about it is what its CLI can do, decided once when the session starts, so a conversation's system prompt or
//! developer instructions never change midway (rule 3 of models.md):
//! - Claude Code in Projects loads the folder as a plugin of its own (`--plugin-dir`, next to the catalog's), so only the
//!   skill descriptions ride on each turn and the CLI reads a body when the model decides to. Two original frontend
//!   skills share this first-party delivery; downloaded catalog skills remain external.
//! - Every other session that can read a file (Claude Code's Chat and Codex, in both areas) gets one short line that points to
//!   the help and frontend files (`Guide::Index`), the way the catalog's index points to its skills, read only when relevant.
//! - Only where the folder cannot be written does the text itself go (`Guide::Text`): the prose, the English words and the
//!   words of the screen's language, plus a short frontend fallback.
//!
//! The line and the text are tracked by their own digest, apart from the catalog kit's, so a kit change never sends them again.
use std::path::{Path, PathBuf};

/// The guide: plain English for the AI, the screen words of each language in a table at its end. Its checks are
/// `engine/tests/guide.rs`.
pub const TEXT: &str = include_str!("guide.md");

/// What the model reads to decide to load it: the only part that costs tokens on every turn where the CLI loads skills.
pub const DESCRIPTION: &str = "Use when the person asks how Faber itself works, such as its screens, settings, shortcuts, Chat, Projects, Catalog, Keep going, accounts and limits";

/// The plugin's name and its one skill's, so Claude Code lists it as `faber-guide:guide`. The catalog's own plugin is `faber`.
pub const SKILL_RULE: &str = "Skill(faber-guide:guide)";
/// Original Faber instructions, lazily loaded alongside the guide; third-party catalog skills stay external.
pub const FRONTEND_SKILLS: [(&str, &str); 2] = [
    ("frontend-craft", include_str!("frontend-skills/frontend-craft/SKILL.md")),
    ("frontend-proof", include_str!("frontend-skills/frontend-proof/SKILL.md")),
];
const SKILL: &str = "guide";
const MANIFEST: &str = r#"{"name":"faber-guide","description":"Faber's own guide to itself.","version":"1.0.0"}"#;
const FILE: &str = "skills/guide/SKILL.md";
const TABLE: &str = "## Words in each language\n";
/// What every index line starts with, so the test runners can tell it from the person's own words.
const NOTE: &str = "Faber's own guide: ";

/// The languages of the screen, in the order of the guide's table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    En,
    PtBr,
    Es,
    Fr,
    De,
}

impl Language {
    pub fn name(self) -> &'static str {
        ["English", "Portuguese (Brazil)", "Spanish", "French", "German"][self as usize]
    }

    /// The language the screen shows: the person's choice in Settings, else the first system language Faber has (what the
    /// screen's own `systemLanguage` does), else English.
    pub fn resolve(chosen: Option<&str>, system: &[String]) -> Language {
        let by_code = |code: &str| match code {
            "en" => Some(Language::En),
            "pt-BR" => Some(Language::PtBr),
            "es" => Some(Language::Es),
            "fr" => Some(Language::Fr),
            "de" => Some(Language::De),
            _ => None,
        };
        chosen.and_then(by_code).unwrap_or_else(|| {
            system
                .iter()
                .find_map(|tag| {
                    let base = tag.to_lowercase();
                    let base = base.split(['-', '_']).next().unwrap_or_default().to_string();
                    by_code(if base == "pt" { "pt-BR" } else { &base })
                })
                .unwrap_or_default()
        })
    }
}

/// The languages the person asked the system for, first choice first, as the screen's own `navigator.languages` lists them.
pub fn system_languages() -> Vec<String> {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let user = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(list) =
            user.open_subkey(r"Control Panel\International\User Profile").and_then(|k| k.get_value::<Vec<String>, _>("Languages"))
        {
            return list;
        }
        user.open_subkey(r"Control Panel\International").and_then(|k| k.get_value::<String, _>("LocaleName")).into_iter().collect()
    }
    #[cfg(not(windows))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|name| std::env::var(name).ok()).filter(|v| !v.is_empty()).collect()
    }
}

/// How the guide reaches one session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Guide {
    /// Faber's own internal requests, and anything that is not a person's session.
    #[default]
    Off,
    /// The words themselves go with the session: only where the folder cannot be written.
    Text(Language),
    /// One line that points to the guide's file in `dir`.
    Index { dir: PathBuf, language: Language },
    /// Claude Code loads the folder as a plugin.
    Plugin { dir: PathBuf, language: Language },
}

impl Guide {
    /// An older Claude Code takes no plugin folder: it is pointed to the file instead.
    pub fn without_plugin(self) -> Guide {
        match self {
            Guide::Plugin { dir, language } => Guide::Index { dir, language },
            other => other,
        }
    }

    /// The folder Faber wrote, which the agent may read but never change.
    pub fn folder(&self) -> Option<&Path> {
        match self {
            Guide::Index { dir, .. } | Guide::Plugin { dir, .. } => Some(dir),
            Guide::Off | Guide::Text(_) => None,
        }
    }

    /// The guide's file, where a session is pointed to it.
    pub fn file(&self) -> Option<PathBuf> {
        self.folder().map(|dir| dir.join(FILE))
    }

    /// What goes into the session beside its messages: the line, or the text. A loaded plugin needs none.
    pub fn note(&self) -> Option<String> {
        match self {
            Guide::Text(language) => Some(format!(
                "{}\nFrontend fallback (frontend-craft and frontend-proof): for website or application UI work, extend the project's existing design system; establish the main task and a coherent visual direction; implement relevant empty, loading, success and recoverable error states. Preserve input and reading position. Use semantic controls, keyboard/focus behavior, responsive layout and reduced motion. Inspect the running result at supported sizes, test the affected flow and report evidence and anything unverified. This does not authorize new tools or external actions.",
                text_for(*language)
            )),
            Guide::Index { language, .. } => {
                let file = crate::project::git::native_path(&self.file()?);
                let skills = crate::project::git::native_path(&self.folder()?.join("skills"));
                Some(format!(
                    "{NOTE}when the person asks how Faber itself works (its screens, settings, shortcuts, Keep going, accounts and limits), read \"{file}\" and answer from it. For website or application UI work, read \"{skills}/frontend-craft/SKILL.md\" to build and \"{skills}/frontend-proof/SKILL.md\" to verify; follow the project's own design system and current permissions. The screen is shown in {}.",
                    language.name()
                ))
            }
            Guide::Off | Guide::Plugin { .. } => None,
        }
    }
}

/// The guide for one screen language: the prose, then the table with the English words and the words of that language.
pub fn text_for(language: Language) -> String {
    let text = TEXT.replace("\r\n", "\n");
    let (prose, table) = text.split_once(TABLE).expect("the guide ends with its table of words");
    let column = language as usize;
    let rows = table.lines().filter(|line| !line.is_empty()).map(|row| {
        let cells: Vec<_> = row.split(" | ").collect();
        if column == 0 { cells[0].to_string() } else { format!("{} | {}", cells[0], cells[column]) }
    });
    let rows: Vec<_> = rows.collect();
    format!(
        "{prose}{TABLE}{}\n\nThe screen is shown in {}: name its words, the right column, when you say where to click. Answer in the language the person writes in.\n",
        rows.join("\n"),
        language.name()
    )
}

/// The skill file the plugin holds: the guide in the screen's language under a front matter the catalog's own checks admit.
pub fn skill_file(language: Language) -> String {
    format!("---\nname: {SKILL}\ndescription: {DESCRIPTION}\n---\n\n{}", text_for(language))
}

/// What the person wrote, with the line that points to the guide taken off the front of a first input. Only that line, up to
/// the blank line after it: anything else before the words stays, so a test cannot pass on a wrong first input.
pub fn without_note(content: &str) -> &str {
    content.strip_prefix(NOTE).and_then(|rest| rest.split_once("\n\n")).map_or(content, |(_, rest)| rest)
}

/// The folder `<data>/guide-plugin`, written when it is missing or differs from the guide of this build in this language.
/// `None` when something else sits in it (a link, a file the guide does not have) or it cannot be written: the guide then goes
/// as text, since a folder Faber did not write is not one to hand a CLI.
pub fn plugin(data_dir: &Path, language: Language) -> Option<PathBuf> {
    let root = data_dir.join("guide-plugin");
    let skill = format!("skills/{SKILL}");
    let mut folders: Vec<(PathBuf, Vec<&str>)> = vec![
        (root.clone(), vec![".claude-plugin", "skills"]),
        (root.join(".claude-plugin"), vec!["plugin.json"]),
        (root.join("skills"), vec![SKILL, "frontend-craft", "frontend-proof"]),
        (root.join(&skill), vec!["SKILL.md"]),
    ];
    folders.extend(FRONTEND_SKILLS.iter().map(|(name, _)| (root.join("skills").join(name), vec!["SKILL.md"])));
    if !folders.iter().all(|(folder, names)| holds_only(folder, names)) {
        return None;
    }
    let files = [(root.join(".claude-plugin/plugin.json"), MANIFEST.to_string()), (root.join(FILE), skill_file(language))];
    let frontend = FRONTEND_SKILLS.iter().map(|(name, text)| (root.join("skills").join(name).join("SKILL.md"), text.replace("\r\n", "\n")));
    for (file, text) in files.into_iter().chain(frontend) {
        if std::fs::symlink_metadata(&file).is_ok_and(|m| m.is_file()) && std::fs::read(&file).is_ok_and(|bytes| bytes == text.as_bytes()) {
            continue;
        }
        if std::fs::symlink_metadata(&file).is_ok_and(|m| !m.is_file()) {
            return None;
        }
        std::fs::create_dir_all(file.parent()?).ok()?;
        std::fs::write(&file, text).ok()?;
    }
    Some(root)
}

/// A folder that is missing, or a plain one (never a link) with only these names in it.
fn holds_only(folder: &Path, names: &[&str]) -> bool {
    match std::fs::symlink_metadata(folder) {
        Err(_) => true,
        Ok(m) if m.is_dir() => std::fs::read_dir(folder)
            .is_ok_and(|entries| entries.into_iter().all(|e| e.is_ok_and(|e| names.iter().any(|name| e.file_name() == *name)))),
        Ok(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_skills_reach_native_and_index_sessions_without_catalog_installs() {
        let dir = tempfile::tempdir().unwrap();
        let root = plugin(dir.path(), Language::En).unwrap();
        for name in ["frontend-craft", "frontend-proof"] {
            let file = root.join(format!("skills/{name}/SKILL.md"));
            let text = std::fs::read_to_string(&file).expect("first-party frontend skill is available");
            assert!(text.contains(&format!("name: {name}")));
            let header = crate::tools::front_matter(&text).expect("a native skill has valid front matter");
            let description = header.lines().find_map(|line| line.strip_prefix("description: ")).unwrap();
            assert!(description.len() <= 160, "discovery stays as small as catalog descriptions");
            assert!(text.len() < 10_000, "load only the relevant frontend workflow");
            std::fs::write(&file, "tampered").unwrap();
            assert_eq!(plugin(dir.path(), Language::En), Some(root.clone()));
            assert_eq!(std::fs::read_to_string(file).unwrap(), text);
            let note = Guide::Index { dir: root.clone(), language: Language::En }.note().unwrap();
            assert!(note.contains(name), "both CLIs can discover {name}");
        }
        assert!(Guide::Off.note().is_none(), "internal requests receive no skills");
        let fallback = Guide::Text(Language::En).note().unwrap();
        assert!(fallback.contains("frontend-craft") && fallback.contains("frontend-proof"));
    }

    #[test]
    fn the_plugin_is_written_whole_and_repaired_and_never_trusts_a_folder_it_did_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let root = plugin(dir.path(), Language::En).unwrap();
        let skill = root.join("skills/guide/SKILL.md");
        assert_eq!(std::fs::read_to_string(&skill).unwrap(), skill_file(Language::En));
        assert_eq!(std::fs::read_to_string(root.join(".claude-plugin/plugin.json")).unwrap(), MANIFEST);
        std::fs::write(&skill, "tampered").unwrap();
        assert_eq!(plugin(dir.path(), Language::En), Some(root.clone()));
        assert_eq!(std::fs::read_to_string(&skill).unwrap(), skill_file(Language::En), "a changed guide is written again");
        assert_eq!(plugin(dir.path(), Language::De), Some(root.clone()));
        assert_eq!(std::fs::read_to_string(&skill).unwrap(), skill_file(Language::De), "a new screen language is written at the launch");
        std::fs::create_dir(root.join("hooks")).unwrap();
        assert_eq!(plugin(dir.path(), Language::En), None, "a part the guide does not have is never loaded");
        std::fs::remove_dir(root.join("hooks")).unwrap();
        std::fs::remove_file(&skill).unwrap();
        std::fs::create_dir(&skill).unwrap();
        assert_eq!(plugin(dir.path(), Language::En), None, "a folder where its file belongs is not repaired by deleting it");
    }

    /// A link (a junction on Windows) at any level, or in place of the file, is never followed or written through.
    #[test]
    fn a_link_in_the_plugin_folder_is_refused_at_every_level() {
        let link = |target: &Path, at: &Path| {
            #[cfg(windows)]
            junction::create(target, at).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, at).unwrap();
        };
        let elsewhere = tempfile::tempdir().unwrap();
        for level in ["", ".claude-plugin", "skills", "skills/guide", "skills/frontend-craft", "skills/frontend-proof"] {
            let dir = tempfile::tempdir().unwrap();
            let root = plugin(dir.path(), Language::En).unwrap();
            let at = if level.is_empty() { root.clone() } else { root.join(level) };
            std::fs::remove_dir_all(&at).unwrap();
            link(elsewhere.path(), &at);
            assert_eq!(plugin(dir.path(), Language::En), None, "a link at `{level}` is refused");
            assert!(std::fs::read_dir(elsewhere.path()).unwrap().next().is_none(), "nothing was written through the link at `{level}`");
        }
        #[cfg(unix)]
        {
            let dir = tempfile::tempdir().unwrap();
            let root = plugin(dir.path(), Language::En).unwrap();
            let file = root.join(FILE);
            std::fs::remove_file(&file).unwrap();
            std::fs::write(elsewhere.path().join("x"), "x").unwrap();
            std::os::unix::fs::symlink(elsewhere.path().join("x"), &file).unwrap();
            assert_eq!(plugin(dir.path(), Language::En), None, "a link where the file belongs is refused");
            assert_eq!(std::fs::read_to_string(elsewhere.path().join("x")).unwrap(), "x");
        }
    }

    #[test]
    fn the_screen_language_is_the_choice_else_the_first_system_language_faber_has() {
        let system = |tags: &[&str]| tags.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        assert_eq!(Language::resolve(Some("de"), &system(&["pt-BR"])), Language::De);
        assert_eq!(Language::resolve(None, &system(&["pt-PT", "en"])), Language::PtBr);
        assert_eq!(Language::resolve(None, &system(&["ja", "fr-CA", "en"])), Language::Fr);
        assert_eq!(Language::resolve(None, &system(&["es_ES.UTF-8"])), Language::Es);
        assert_eq!(Language::resolve(Some("klingon"), &system(&["ja"])), Language::En);
        assert_eq!(Language::resolve(None, &[]), Language::En);
    }

    #[test]
    fn a_session_is_told_the_screen_language_and_text_keeps_only_its_two_columns() {
        let index = Guide::Index { dir: "/data/guide-plugin".into(), language: Language::PtBr };
        let line = index.note().unwrap();
        assert!(line.starts_with(NOTE) && line.contains("Portuguese (Brazil)") && line.contains("SKILL.md"), "{line}");
        assert!(!line.contains("\n") && line.len() < 1000, "a bounded index, not frontend instruction bodies: {line}");
        assert_eq!(without_note(&format!("{line}\n\nhello")), "hello");
        assert_eq!(
            without_note(&format!("Current catalog resources\n\n{line}\n\nhello")),
            format!("Current catalog resources\n\n{line}\n\nhello")
        );
        let text = Guide::Text(Language::Fr).note().unwrap();
        assert!(
            text.contains("Continuer seul")
                && text.contains("Keep going")
                && !text.contains("Seguir solo")
                && !text.contains("Selbst weitermachen")
        );
        assert!(text.contains("The screen is shown in French"), "{}", &text[text.len() - 200..]);
        let english = Guide::Text(Language::En).note().unwrap();
        assert!(english.len() < TEXT.len() * 3 / 4 && !english.contains("Continuer seul"));
        assert_eq!(Guide::Plugin { dir: "d".into(), language: Language::En }.note(), None, "a loaded plugin needs no line");
    }
}
