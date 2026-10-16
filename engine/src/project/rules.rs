//! What a project declares about itself for its missions (R146): the areas of its map, the paths whose change needs a
//! security review, and the files whose entries are numbered in sequence. The file is read at the mission base, as the
//! engine showed it to the roles, never from a candidate: a change cannot lower its own review.
//!
//! From the rules and the paths a change really touched, the engine sizes the work. One area and no security path is a
//! light change: the Writer, the checks and one review. More than one area, a security path or a path no declared area
//! holds is a full change: two reviews from different sessions. A model may widen the size by naming more areas in its
//! contract, never narrow it.
use super::{overlaps, wire::MAX_TEXT};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The project's own rules file, at the root of its tree.
pub const FILE: &str = "faber-rules.json";

/// Words that mark a path as security work in any project: processes and shells, permissions, credentials and secrets,
/// and updates. A path matches when one of its words (split on `/ . _ -`) is one of these.
const SECURITY_WORDS: [&str; 16] = [
    "auth",
    "credential",
    "credentials",
    "crypto",
    "password",
    "passwords",
    "permission",
    "permissions",
    "secret",
    "secrets",
    "shell",
    "spawn",
    "token",
    "tokens",
    "update",
    "updater",
];

/// One area of the project's map and the paths it holds. A path claims itself and everything under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Area {
    pub name: String,
    pub paths: Vec<String>,
}

/// Entries numbered in sequence, such as decision numbers or release notes. A number sits between `before` and
/// `after` in the files under `files`; a Writer leaves a placeholder (see `sequence`) and the engine numbers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sequence {
    pub name: String,
    pub files: Vec<String>,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rules {
    #[serde(default)]
    pub areas: Vec<Area>,
    /// Paths that always need the full chain, besides the words every project shares.
    #[serde(default)]
    pub security: Vec<String>,
    #[serde(default)]
    pub sequences: Vec<Sequence>,
}

impl Rules {
    /// The rules in the shown files of the base, or none declared. A rules file that cannot be read stops the mission:
    /// guessing would size every change as light.
    pub fn from_sources(sources: &[(String, String)]) -> Result<Rules, &'static str> {
        let Some((_, text)) = sources.iter().find(|(path, _)| path == FILE) else { return Ok(Rules::default()) };
        let rules: Rules = serde_json::from_str(text).map_err(|_| "reason.rules.invalid")?;
        rules.validate()?;
        Ok(rules)
    }

    /// The rules of a mission's base: `sources` are the shown texts and `tree` every tracked path. A rules file that is
    /// tracked but was not shown is refused, never read as no rules at all.
    pub fn from_setup(sources: &[(String, String)], tree: &[String]) -> Result<Rules, &'static str> {
        if tree.iter().any(|p| p == FILE) && !sources.iter().any(|(p, _)| p == FILE) {
            return Err("reason.rules.invalid");
        }
        Rules::from_sources(sources)
    }

    fn validate(&self) -> Result<(), &'static str> {
        let short = |s: &str| !s.trim().is_empty() && s.len() <= 120;
        let clean = |p: &String| super::clean_path(p.trim_end_matches('/')).is_some_and(|c| !c.is_empty());
        let names: BTreeSet<String> = self.areas.iter().map(|a| a.name.to_lowercase()).collect();
        let ok = self.areas.len() <= 64
            && names.len() == self.areas.len()
            && self.areas.iter().all(|a| short(&a.name) && !a.paths.is_empty() && a.paths.len() <= 64 && a.paths.iter().all(clean))
            && self.security.len() <= 256
            && self.security.iter().all(clean)
            && self.sequences.len() <= 8
            && self.sequences.iter().all(|s| {
                super::valid_id(&s.name)
                    && !s.files.is_empty()
                    && s.files.len() <= 16
                    && s.files.iter().all(clean)
                    && !s.before.is_empty()
                    && s.before.len() <= 32
                    && s.after.len() <= 32
                    && !s.before.ends_with(|c: char| c.is_ascii_digit())
                    && !s.after.starts_with(|c: char| c.is_ascii_digit())
            });
        if ok { Ok(()) } else { Err("reason.rules.invalid") }
    }

    /// The area that holds `path`: the declared area with the longest matching path, or, when the project declares no
    /// areas, the path's top folder (`.` for a file at the root). `None` is a path no declared area holds.
    pub fn area(&self, path: &str) -> Option<String> {
        if self.areas.is_empty() {
            return Some(path.split_once('/').map_or(".", |(top, _)| top).to_string());
        }
        self.areas
            .iter()
            .flat_map(|a| a.paths.iter().map(move |p| (a, p.trim_end_matches('/'))))
            .filter(|(_, p)| holds(p, path))
            .max_by_key(|(_, p)| p.len())
            .map(|(a, _)| a.name.clone())
    }

    /// Whether a change to `path` needs the full chain whatever its size.
    pub fn security(&self, path: &str) -> bool {
        let lower = path.to_lowercase();
        lower == FILE
            || lower.starts_with(".github/")
            || lower.split(['/', '.', '_', '-']).any(|word| SECURITY_WORDS.contains(&word))
            || self.security.iter().any(|p| holds(p.trim_end_matches('/'), path))
    }

    /// The size of a change to `paths`, measured by the engine.
    pub fn size(&self, paths: &[String]) -> Sizing {
        let mut sizing = Sizing::default();
        for path in paths {
            match self.area(path) {
                Some(area) => {
                    sizing.areas.insert(area);
                }
                None => sizing.unknown.push(path.clone()),
            }
            if self.security(path) {
                sizing.security.push(path.clone());
            }
        }
        sizing
    }
}

/// `area` holds `path`: the same path, or one under it, compared without case as the leases do.
fn holds(area: &str, path: &str) -> bool {
    !area.is_empty() && overlaps(area, path) && path.len() >= area.len()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    /// One area, no security path: the Writer, the checks and one review.
    Light,
    /// The whole chain: two reviews from different sessions.
    Full,
}

impl Tier {
    pub fn reviews(self) -> u8 {
        match self {
            Tier::Light => 1,
            Tier::Full => 2,
        }
    }
}

/// What the engine measured about a change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sizing {
    /// The areas the paths fall in, and those the contract adds.
    pub areas: BTreeSet<String>,
    /// Paths on a security list.
    pub security: Vec<String>,
    /// Paths no declared area holds: unknown reach, so the full chain.
    pub unknown: Vec<String>,
}

impl Sizing {
    pub fn tier(&self) -> Tier {
        if self.areas.len() <= 1 && self.security.is_empty() && self.unknown.is_empty() { Tier::Light } else { Tier::Full }
    }

    /// A contract that names more areas widens the change; one that names fewer changes nothing.
    pub fn widen(&mut self, coherence: &Coherence) {
        for named in &coherence.areas {
            if !self.areas.iter().any(|a| a.eq_ignore_ascii_case(named)) {
                self.areas.insert(named.clone());
            }
        }
    }
}

/// The three coherence fields of a contract (R146): the map areas the change touches, the closest thing that already
/// exists, and whether the change extends it or is new, with why. A Writer starts only once they are set and cover what
/// the change really touches; the reviewer reads them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Coherence {
    pub areas: Vec<String>,
    pub closest: String,
    /// The map entry or existing thing it extends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    /// Why the closest existing thing does not fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
}

impl Coherence {
    /// The fields are present and short, exactly one of `extends` and `new` is set, every named area is one the rules
    /// know, and together they name every area the engine measured.
    pub fn check(&self, rules: &Rules, measured: &Sizing) -> Result<(), &'static str> {
        const KEY: &str = "reason.contract.coherence";
        let text = |s: &str| !s.trim().is_empty() && s.len() <= MAX_TEXT;
        let link = match (&self.extends, &self.new) {
            (Some(e), None) => text(e),
            (None, Some(n)) => text(n),
            _ => false,
        };
        if !link || !text(&self.closest) || self.areas.is_empty() || self.areas.len() > 16 {
            return Err(KEY);
        }
        if self.areas.iter().any(|a| a.trim().is_empty() || a.len() > 120) {
            return Err(KEY);
        }
        let known = |name: &str| rules.areas.is_empty() || rules.areas.iter().any(|a| a.name.eq_ignore_ascii_case(name));
        if !self.areas.iter().all(|a| known(a)) {
            return Err(KEY);
        }
        if !measured.areas.iter().all(|m| self.areas.iter().any(|a| a.eq_ignore_ascii_case(m))) {
            return Err(KEY);
        }
        Ok(())
    }

    /// The fields as the Writer and the reviewer read them.
    pub fn text(&self) -> String {
        let link = match (&self.extends, &self.new) {
            (Some(e), _) => format!("Extends: {e}"),
            (_, Some(n)) => format!("New: {n}"),
            _ => String::new(),
        };
        format!("Areas: {}\nClosest existing thing: {}\n{link}", self.areas.join(", "), self.closest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn faber() -> Rules {
        Rules {
            areas: vec![
                Area { name: "Missions".into(), paths: vec!["engine/src/project/".into()] },
                Area { name: "Chat".into(), paths: vec!["engine/src/".into(), "ui/Chat.tsx".into()] },
            ],
            security: vec!["engine/src/proc.rs".into(), "winproc".into()],
            sequences: vec![],
        }
    }

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn one_area_without_a_security_path_is_light_and_anything_wider_is_full() {
        let rules = faber();
        let light = rules.size(&paths(&["engine/src/project/queue.rs", "engine/src/project/coordinator.rs"]));
        assert_eq!((light.areas.iter().collect::<Vec<_>>(), light.tier()), (vec![&"Missions".to_string()], Tier::Light));
        assert_eq!(light.tier().reviews(), 1);
        let two = rules.size(&paths(&["engine/src/project/queue.rs", "engine/src/chat.rs"]));
        assert_eq!(two.tier(), Tier::Full, "two areas");
        let spawn = rules.size(&paths(&["engine/src/proc.rs"]));
        assert_eq!((spawn.security.len(), spawn.tier()), (1, Tier::Full), "a declared security path");
        for path in
            ["winproc/src/lib.rs", "engine/src/permissions.rs", "engine/src/project/update.rs", "faber-rules.json", ".github/ci.yml"]
        {
            assert_eq!(rules.size(&paths(&[path])).tier(), Tier::Full, "{path}");
        }
        let unknown = rules.size(&paths(&["docs/roadmap.md"]));
        assert_eq!((unknown.unknown.len(), unknown.tier(), unknown.tier().reviews()), (1, Tier::Full, 2), "unknown reach");
        assert_eq!(rules.size(&paths(&["engine/src/project/processing.rs"])).tier(), Tier::Light, "a word, not a substring");
    }

    #[test]
    fn without_declared_areas_each_top_folder_is_an_area() {
        let rules = Rules::default();
        assert_eq!(rules.area("src/a.rs").as_deref(), Some("src"));
        assert_eq!(rules.area("README.md").as_deref(), Some("."));
        assert_eq!(rules.size(&paths(&["src/a.rs", "src/b/c.rs"])).tier(), Tier::Light);
        assert_eq!(rules.size(&paths(&["src/a.rs", "lib/b.rs"])).tier(), Tier::Full);
    }

    #[test]
    fn the_longest_declared_path_names_the_area_and_case_does_not_hide_it() {
        let rules = faber();
        assert_eq!(rules.area("engine/src/project/lead.rs").as_deref(), Some("Missions"));
        assert_eq!(rules.area("Engine/Src/chat.rs").as_deref(), Some("Chat"));
        assert_eq!(rules.area("ui/Chat.tsx").as_deref(), Some("Chat"));
        assert_eq!(rules.area("ui/Chat.tsx.bak"), None, "a path claims itself and what is under it, not a longer name");
        assert!(rules.security("WinProc/src/lib.rs"));
    }

    fn coherence(areas: &[&str]) -> Coherence {
        Coherence { areas: paths(areas), closest: "The serial queue".into(), extends: Some("Missions: the queue".into()), new: None }
    }

    #[test]
    fn a_contract_names_every_measured_area_one_closest_thing_and_one_link() {
        let rules = faber();
        let measured = rules.size(&paths(&["engine/src/project/queue.rs"]));
        assert_eq!(coherence(&["missions"]).check(&rules, &measured), Ok(()));
        assert_eq!(coherence(&["Chat"]).check(&rules, &measured), Err("reason.contract.coherence"), "misses Missions");
        assert_eq!(coherence(&["Missions", "Nowhere"]).check(&rules, &measured), Err("reason.contract.coherence"), "unknown area");
        assert_eq!(coherence(&[]).check(&rules, &measured), Err("reason.contract.coherence"));
        let both = Coherence { new: Some("why".into()), ..coherence(&["Missions"]) };
        assert_eq!(both.check(&rules, &measured), Err("reason.contract.coherence"), "extends and new at once");
        let neither = Coherence { extends: None, ..coherence(&["Missions"]) };
        assert_eq!(neither.check(&rules, &measured), Err("reason.contract.coherence"));
        let blank = Coherence { closest: " ".into(), ..coherence(&["Missions"]) };
        assert_eq!(blank.check(&rules, &measured), Err("reason.contract.coherence"));
        let new = Coherence { extends: None, new: Some("No queue held two reviews".into()), ..coherence(&["Missions"]) };
        assert_eq!(new.check(&rules, &measured), Ok(()));
        assert!(new.text().ends_with("New: No queue held two reviews"));
    }

    #[test]
    fn a_contract_may_widen_the_size_but_never_narrow_it() {
        let rules = faber();
        let mut measured = rules.size(&paths(&["engine/src/project/queue.rs"]));
        measured.widen(&coherence(&["missions"]));
        assert_eq!(measured.tier(), Tier::Light, "the same area named again");
        measured.widen(&coherence(&["Missions", "Chat"]));
        assert_eq!(measured.tier(), Tier::Full);
    }

    #[test]
    fn the_rules_come_from_the_base_and_a_bad_file_is_refused() {
        assert_eq!(Rules::from_sources(&[]), Ok(Rules::default()));
        assert_eq!(Rules::from_setup(&[], &[FILE.into()]), Err("reason.rules.invalid"), "tracked but not shown");
        let text = serde_json::to_string(&faber()).unwrap();
        assert_eq!(Rules::from_sources(&[(FILE.into(), text)]), Ok(faber()));
        for bad in [
            "not json",
            r#"{"areas": [{"name": "A", "paths": ["../out"]}]}"#,
            r#"{"areas": [{"name": "A", "paths": ["a"]}, {"name": "a", "paths": ["b"]}]}"#,
            r#"{"sequences": [{"name": "d", "files": ["docs/decisions.md"], "before": "R1", "after": " |"}]}"#,
            r#"{"unknown": []}"#,
        ] {
            assert_eq!(Rules::from_sources(&[(FILE.into(), bad.into())]), Err("reason.rules.invalid"), "{bad}");
        }
    }
}
