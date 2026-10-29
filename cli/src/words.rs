//! The screen's words (`ui/locales/`, built in by `build.rs`), in the language Faber shows.

use std::collections::HashMap;

include!(concat!(env!("OUT_DIR"), "/words.rs"));

pub struct Words {
    chosen: HashMap<&'static str, &'static str>,
    english: HashMap<&'static str, &'static str>,
}

fn table(language: &str) -> Option<HashMap<&'static str, &'static str>> {
    WORDS.iter().find(|(name, _)| *name == language).map(|(_, entries)| entries.iter().copied().collect())
}

impl Words {
    /// `language` as Settings keeps it ("pt-BR", "de"); a region the screen has no file for falls to its language, then English.
    pub fn new(language: &str) -> Words {
        let english = table("en").unwrap_or_default();
        let base = language.split(['-', '_']).next().unwrap_or("");
        let chosen = table(language)
            .or_else(|| WORDS.iter().find(|(name, _)| name.split('-').next() == Some(base)).and_then(|(name, _)| table(name)))
            .unwrap_or_default();
        Words { chosen, english }
    }

    /// The text of `key`; the key itself when no language has it, so a missing word is visible and never empty.
    pub fn get(&self, key: &str) -> String {
        self.chosen.get(key).or_else(|| self.english.get(key)).map(|text| text.to_string()).unwrap_or_else(|| key.to_string())
    }

    /// `key` with each `{name}` filled.
    pub fn fill(&self, key: &str, values: &[(&str, &str)]) -> String {
        values.iter().fold(self.get(key), |text, (name, value)| text.replace(&format!("{{{name}}}"), value))
    }

    /// The plural form of `key` for `n`, with `{n}` filled. English and the four other languages need only one and other.
    pub fn count(&self, key: &str, n: usize) -> String {
        let form = if n == 1 { "one" } else { "other" };
        let text = self.get(&format!("{key}#{form}"));
        text.replace("{n}", &n.to_string())
    }
}

/// The language to speak: what Settings says, else the system's, else English.
pub fn language(settings_language: Option<&str>) -> String {
    if let Some(language) = settings_language.filter(|l| !l.is_empty()) {
        return language.to_string();
    }
    system_language().unwrap_or_else(|| "en".into())
}

fn system_language() -> Option<String> {
    #[cfg(windows)]
    if let Some(name) = faber_winproc::user_language() {
        return Some(name);
    }
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
        .map(|value| value.split('.').next().unwrap_or("").replace('_', "-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_of_the_screen_is_built_in_with_the_command_lines_words() {
        let names: Vec<&str> = WORDS.iter().map(|(name, _)| *name).collect();
        for name in ["en", "pt-BR", "es", "fr", "de"] {
            assert!(names.contains(&name), "{name} in {names:?}");
        }
        let english = table("en").unwrap();
        for (name, entries) in WORDS {
            for key in english.keys().filter(|k| k.starts_with("cli.")) {
                assert!(entries.iter().any(|(k, _)| k == key), "{name} has no {key}");
            }
        }
    }

    #[test]
    fn a_region_falls_to_its_language_and_a_missing_key_shows_itself() {
        let words = Words::new("pt-PT");
        assert_eq!(words.get("cli.word.yes"), table("pt-BR").unwrap()["cli.word.yes"]);
        assert_eq!(Words::new("xx").get("cli.word.yes"), "Yes");
        assert_eq!(Words::new("en").get("cli.nothing.here"), "cli.nothing.here");
    }

    #[test]
    fn plural_forms_and_values_fill_in() {
        let words = Words::new("en");
        assert_eq!(words.count("cli.list.count", 1), "1 conversation");
        assert_eq!(words.count("cli.list.count", 3), "3 conversations");
        assert_eq!(words.fill("cli.error.noFaber", &[]), words.get("cli.error.noFaber"));
    }
}
