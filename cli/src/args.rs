//! The command line's words and flags. Flags may come anywhere; `--` ends them.

use std::collections::HashMap;

/// Flags that take a value (`--id c1` or `--id=c1`).
const VALUED: &[&str] = &["id", "project", "vendor", "model", "effort", "limit", "data"];
/// Flags that are on or off.
const SWITCHES: &[&str] = &["json", "plain", "no-wait", "no-start", "archived", "help", "version", "allow", "always", "deny"];

#[derive(Debug, Default, PartialEq)]
pub struct Args {
    pub words: Vec<String>,
    values: HashMap<String, String>,
    switches: Vec<String>,
}

impl Args {
    /// The arguments after the program's name. A flag the command line does not know is an error that names it.
    pub fn parse(raw: impl IntoIterator<Item = String>) -> Result<Args, String> {
        let mut args = Args::default();
        let mut raw = raw.into_iter();
        while let Some(arg) = raw.next() {
            if arg == "--" {
                args.words.extend(raw.by_ref());
                break;
            }
            let Some(flag) = arg.strip_prefix("--") else {
                if arg == "-h" {
                    args.switches.push("help".into());
                } else {
                    args.words.push(arg);
                }
                continue;
            };
            let (name, inline) = match flag.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (flag, None),
            };
            if VALUED.contains(&name) {
                let value = inline.or_else(|| raw.next()).ok_or_else(|| format!("--{name}"))?;
                args.values.insert(name.into(), value);
            } else if SWITCHES.contains(&name) && inline.is_none() {
                args.switches.push(name.into());
            } else {
                return Err(format!("--{name}"));
            }
        }
        Ok(args)
    }

    pub fn on(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// The word at `at`, after the verb.
    pub fn word(&self, at: usize) -> Option<&str> {
        self.words.get(at).map(String::as_str)
    }

    /// Every word from `at` on, as one text.
    pub fn rest(&self, at: usize) -> String {
        self.words.get(at..).unwrap_or_default().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Args, String> {
        Args::parse(line.split_whitespace().map(String::from))
    }

    #[test]
    fn flags_mix_with_words_in_any_order() {
        let args = parse("chat --id c1 hello --json there --model=sonnet").unwrap();
        assert_eq!(args.words, ["chat", "hello", "there"]);
        assert_eq!((args.value("id"), args.value("model")), (Some("c1"), Some("sonnet")));
        assert!(args.on("json") && !args.on("plain"));
        assert_eq!(args.rest(1), "hello there");
    }

    #[test]
    fn after_two_dashes_everything_is_a_word() {
        let args = parse("chat -- --json is text").unwrap();
        assert_eq!(args.words, ["chat", "--json", "is", "text"]);
        assert!(!args.on("json"));
    }

    #[test]
    fn an_unknown_flag_or_a_missing_value_names_the_flag() {
        assert_eq!(parse("status --colour").unwrap_err(), "--colour");
        assert_eq!(parse("chat --id").unwrap_err(), "--id");
        assert_eq!(parse("status --json=yes").unwrap_err(), "--json");
    }
}
