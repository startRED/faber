//! The command line speaks with the screen's own words: the entries of `ui/locales/*.ts` it uses are read here and built
//! into the program, so a text exists once, in the files people translate by hand.

use std::fmt::Write as _;
use std::path::Path;

/// The parts of the screen's words the command line shows.
const PREFIXES: &[&str] = &["cli.", "error.", "reason.", "mode.", "note.", "quota."];

fn main() {
    let locales = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("ui").join("locales");
    println!("cargo:rerun-if-changed={}", locales.display());
    let mut out = String::from("pub static WORDS: &[(&str, &[(&str, &str)])] = &[\n");
    let mut names: Vec<_> = std::fs::read_dir(&locales).expect("ui/locales").flatten().map(|e| e.path()).collect();
    names.sort();
    for path in names.iter().filter(|p| p.extension().is_some_and(|e| e == "ts")) {
        println!("cargo:rerun-if-changed={}", path.display());
        let language = path.file_stem().unwrap().to_string_lossy().to_string();
        let source = std::fs::read_to_string(path).expect("a locale file");
        let _ = writeln!(out, "    ({language:?}, &[");
        for (key, value) in entries(&source) {
            let _ = writeln!(out, "        ({key:?}, {value:?}),");
        }
        out.push_str("    ]),\n");
    }
    out.push_str("];\n");
    std::fs::write(Path::new(&std::env::var("OUT_DIR").unwrap()).join("words.rs"), out).unwrap();
}

/// `'key': 'text',` and `'key': { one: 'text', other: 'text' },`; the plural forms come out as `key#one` and `key#other`.
fn entries(source: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for line in source.lines() {
        let rest = line.trim_start();
        let Some((key, rest)) = literal(rest) else { continue };
        if !PREFIXES.iter().any(|p| key.starts_with(p)) {
            continue;
        }
        let Some(rest) = rest.trim_start().strip_prefix(':') else { continue };
        let rest = rest.trim_start();
        if let Some(mut forms) = rest.strip_prefix('{') {
            while let Some((name, after)) = forms.trim_start().split_once(':') {
                let name = name.trim();
                let Some((text, after)) = literal(after.trim_start()) else { break };
                found.push((format!("{key}#{name}"), text));
                forms = after.trim_start().trim_start_matches(',');
                if forms.trim_start().starts_with('}') {
                    break;
                }
            }
        } else if let Some((text, _)) = literal(rest) {
            found.push((key, text));
        }
    }
    found
}

/// One JavaScript string in single or double quotes at the start of `text`, and what follows it.
fn literal(text: &str) -> Option<(String, &str)> {
    let quote = text.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let mut value = String::new();
    let mut chars = text[1..].char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' => match chars.next()?.1 {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).map(|(_, c)| c).collect();
                    value.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                other => value.push(other),
            },
            c if c == quote => return Some((value, &text[1 + at + 1..])),
            c => value.push(c),
        }
    }
    None
}
