//! The command line's words and flags. Flags may come anywhere; `--` ends them.

use std::collections::HashMap;

/// Flags that take a value (`--id c1` or `--id=c1`).
const VALUED: &[&str] =
    &["id", "to", "area", "project", "family", "state", "query", "last", "timeout", "vendor", "model", "effort", "limit", "data"];
/// Flags that are on or off.
const SWITCHES: &[&str] = &["json", "human", "no-start", "archived", "all", "wait", "help"];

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
                let value =
                    inline.or_else(|| raw.next()).filter(|v| !v.is_empty() && !v.starts_with("--")).ok_or_else(|| format!("--{name}"))?;
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

    pub fn only(&self, allowed: &[&str]) -> bool {
        self.values.keys().all(|k| k == "data" || allowed.contains(&k.as_str()))
            && self.switches.iter().all(|k| ["json", "human", "no-start", "help"].contains(&k.as_str()) || allowed.contains(&k.as_str()))
    }

    /// The word at `at`, after the verb.
    pub fn word(&self, at: usize) -> Option<&str> {
        self.words.get(at).map(String::as_str)
    }
}
