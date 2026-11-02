//! Faber's own guide (`engine/src/guide.md`, R18) answers "how do I..." about Faber, so it may not drift from the screen. Every
//! screen word it names is checked against the five locales, every Settings row and shortcut of the product map is in it, and
//! it holds nothing private. A new setting or Settings section fails here until the guide says it.

use std::path::{Path, PathBuf};

const LANGUAGES: [&str; 5] = ["en", "pt-BR", "es", "fr", "de"];
const TABLE: &str = "## Words in each language\n";

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repo(path)).unwrap().replace("\r\n", "\n")
}

/// The plain texts of a locale file as (key, text): one per line, quoted either way; the plural objects are left out.
fn locale(language: &str) -> Vec<(String, String)> {
    let mut texts = Vec::new();
    for line in read(&format!("ui/locales/{language}.ts")).lines() {
        let Some(rest) = line.strip_prefix("  ") else { continue };
        let Some(quote) = rest.chars().next().filter(|c| *c == '\'' || *c == '"') else { continue };
        let Some((key, after)) = rest[1..].split_once(quote) else { continue };
        let Some(value) = after.strip_prefix(':').map(str::trim_start) else { continue };
        let Some(open) = value.chars().next().filter(|c| *c == '\'' || *c == '"') else { continue };
        let (mut text, mut chars) = (String::new(), value[1..].chars());
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some('n') => text.push('\n'),
                    Some(other) => text.push(other),
                    None => break,
                },
                c if c == open => break,
                c => text.push(c),
            }
        }
        texts.push((key.to_string(), text));
    }
    texts
}

/// The guide before its table of words, and the table's rows.
fn guide() -> (&'static str, Vec<Vec<&'static str>>) {
    // The text is compiled in as the checkout wrote it: a checkout with CRLF line ends reads the same.
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let text = TEXT.get_or_init(|| faber_engine::guide::TEXT.replace("\r\n", "\n")).as_str();
    let (prose, table) = text.split_once(TABLE).expect("the guide ends with its table of words");
    let mut rows = table.lines().filter(|line| !line.is_empty());
    assert_eq!(rows.next().unwrap().split(" | ").count(), 5, "the table's first line names the five languages");
    (prose, rows.map(|row| row.split(" | ").collect()).collect())
}

/// What the guide puts between double quotes: the words of the screen, and nothing else.
fn quoted(prose: &str) -> Vec<&str> {
    prose.split('"').skip(1).step_by(2).collect()
}

#[test]
fn every_row_of_the_words_table_is_one_text_written_in_each_language() {
    let locales: Vec<_> = LANGUAGES.map(locale).into();
    let (_, rows) = guide();
    assert!(rows.len() > 40, "the table has its words");
    for row in rows {
        assert_eq!(row.len(), 5, "{row:?}: one cell per language, written ` | `");
        let one_key = locales[0].iter().any(|(key, text)| {
            *text == row[0] && locales.iter().zip(&row).all(|(texts, cell)| texts.iter().any(|(k, t)| k == key && t == cell))
        });
        assert!(one_key, "{row:?} is no key of ui/locales written so in all five languages: the screen words moved, so update the row");
    }
}

#[test]
fn every_word_the_guide_quotes_is_a_screen_word_and_is_in_the_table_unless_all_languages_write_it_alike() {
    let locales: Vec<_> = LANGUAGES.map(locale).into();
    let (prose, rows) = guide();
    for word in quoted(prose) {
        assert!(locales[0].iter().any(|(_, text)| text == word), "\"{word}\" is no text of ui/locales/en.ts: quote only screen words");
        let alike = locales[0]
            .iter()
            .any(|(key, text)| text == word && locales.iter().all(|texts| texts.iter().any(|(k, t)| k == key && t == word)));
        assert!(
            alike || rows.iter().any(|row| row[0] == word),
            "\"{word}\" has no row in the table of words, so the AI would name it in English"
        );
    }
    for row in rows {
        assert!(prose.contains(&format!("\"{}\"", row[0])), "the table has {row:?} but the guide never names it");
    }
}

/// The settings of the map that a person changes in Settings, with the key of the label each shows in the screen. A new one
/// stops here: name it in the guide and add it.
const SETTINGS: [(&str, &str); 16] = [
    ("appearance", "set.appearance"),
    ("language", "set.language"),
    ("userName", "set.name"),
    ("permissions", "set.permissions"),
    ("webSearch", "set.webSearch"),
    ("reasoningSummary", "set.summary"),
    ("replyStyle", "set.replyStyle"),
    ("userHooks", "set.hooksBody"),
    ("faberTrust", "set.faberTrust"),
    ("notifications", "set.notifyMe"),
    ("notificationSound", "set.playSound"),
    ("previews", "set.previews"),
    ("keepAwake", "set.keepAwake"),
    ("resumeInterrupted", "set.resumeInterrupted"),
    ("autostart", "set.autostart"),
    ("experience", "first.xp.title"),
];

#[test]
fn every_settings_row_and_section_of_the_product_map_is_in_the_guide() {
    let en = locale("en");
    let text = |key: &str| en.iter().find(|(k, _)| k == key).unwrap_or_else(|| panic!("no {key} in en.ts")).1.clone();
    let (prose, _) = guide();
    let named = |key: &str| {
        let word = text(key);
        assert!(prose.contains(&format!("\"{word}\"")), "the guide does not name \"{word}\" ({key})");
    };
    let map = read("docs/product-map.md");
    let every = map.split("\n## Every setting\n").nth(1).and_then(|s| s.split("\n## ").next()).unwrap();
    let mut seen = Vec::new();
    for row in every.lines().filter(|line| line.starts_with("| `")) {
        let cells: Vec<_> = row.split('|').map(str::trim).collect();
        let key = cells[1].trim_matches('`');
        // A key a person changes in Settings says so where it lives; the rest is internal or the first run's.
        if cells[3].contains("Settings >") && key != "tutorialDone" {
            let label = SETTINGS.iter().find(|(k, _)| *k == key).unwrap_or_else(|| {
                panic!("the map's setting `{key}` lives in Settings: say it in engine/src/guide.md and add its label key to SETTINGS here")
            });
            named(label.1);
            seen.push(key);
        }
    }
    for (key, _) in SETTINGS {
        assert!(seen.contains(&key), "`{key}` is no longer a Settings row of the map: take it out of SETTINGS");
    }
    named("set.tourAgain");
    for key in ["area.chat", "area.projects", "area.catalog", "set.label", "set.projects.checks", "set.projects.copies", "set.keys"] {
        named(key);
    }
    // The five sections are the map's own list, so a sixth stops here.
    let sections = map.split("\n## Settings screen\n").nth(1).and_then(|s| s.split("\n## ").next()).unwrap();
    let found: Vec<_> = sections.lines().filter_map(|l| l.strip_prefix("- **")?.split("**").next()).collect();
    assert_eq!(
        found,
        ["General", "Projects", "Appearance", "Accounts and limits", "Privacy"],
        "Settings has new sections: update the guide"
    );
    for key in ["set.general", "set.projects", "set.appearance", "set.subscriptions", "set.privacy"] {
        named(key);
    }
}

#[test]
fn every_shortcut_of_the_screen_is_in_the_guide() {
    let (prose, _) = guide();
    let keys = read("ui/keys.ts");
    let mut chords = 0;
    for line in keys.lines().filter(|l| l.contains("caps: [")) {
        // The caps are the quoted words of the array: one of them is a comma.
        let caps: Vec<_> = line.split("caps: [").nth(1).unwrap().split(']').next().unwrap().split('\'').skip(1).step_by(2).collect();
        if !caps.contains(&"ctrl") && caps != ["shift", "f10"] {
            continue;
        }
        let chord: Vec<_> = caps
            .iter()
            .map(|cap| match *cap {
                "ctrl" => "Ctrl".to_string(),
                "shift" => "Shift".to_string(),
                "f10" => "F10".to_string(),
                "tab" => "Tab".to_string(),
                other => other.to_string(),
            })
            .collect();
        let chord = chord.join("+");
        assert!(prose.contains(&chord), "{chord} works in the screen (ui/keys.ts) and the guide does not say it");
        chords += 1;
    }
    assert!(chords >= 11, "the shortcut list was read");
}

#[test]
fn the_guide_is_short_enough_to_load_and_holds_nothing_private() {
    let text = faber_engine::guide::TEXT;
    assert!(text.len() < 16 * 1024, "{} bytes is more than a kit's own 16 KiB budget", text.len());
    for private in [":\\", "/Users/", "/home/", "~/", ".claude", ".codex", "auth.json", "sk-", "password", "@gmail", "C:/"] {
        assert!(!text.contains(private), "the guide names {private}");
    }
    assert!(text.starts_with("Faber guide."), "the AI is told what the text is before anything else");
}
