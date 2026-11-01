//! Output modes and exit codes.
use crate::words::Words;
use serde_json::{Value, json};
use std::io::IsTerminal;
pub const DONE: i32 = 0;
pub const REFUSED: i32 = 1;
pub const USAGE: i32 = 2;
pub const UNREACHED: i32 = 3;
pub const PERSON: i32 = 4;
pub const TIMEOUT: i32 = 5;
pub const STOPPED: i32 = 130;
pub struct Out {
    pub json: bool,
    pub words: Words,
}
impl Out {
    pub fn new(json: bool, human: bool, words: Words) -> Self {
        Self { json: !human && (json || !std::io::stdout().is_terminal()), words }
    }
    pub fn line(&self, value: &Value) {
        println!("{}", value);
    }
    pub fn result(&self, value: &Value) {
        if self.json {
            self.line(value);
        } else if value.is_null() {
            println!("{}", self.words.get("cli.done"));
        } else {
            human(value, 0, &self.words);
        }
    }
    pub fn fail(&self, key: &str, code: i32) -> i32 {
        self.fail_detail(key, None, code)
    }
    pub fn fail_detail(&self, key: &str, detail: Option<&str>, code: i32) -> i32 {
        if self.json {
            let english = Words::new("en");
            let message = english.get(key);
            let message = if message == key { english.get("cli.error.refused") } else { message };
            let mut output = json!({"error":key,"message":message});
            if let Some(detail) = detail {
                output["detail"] = json!(detail);
            }
            self.line(&output);
        } else {
            let message = self.words.get(key);
            eprintln!("{}", if message == key { self.words.get("cli.error.refused") } else { message });
            if let Some(detail) = detail {
                eprintln!("{detail}");
            }
        }
        code
    }
}

fn human(value: &Value, depth: usize, words: &Words) {
    let indent = "  ".repeat(depth);
    match value {
        Value::Object(entries) => {
            for (key, value) in entries {
                let state = if key == "state" { value.as_str().map(|s| words.get(&format!("cli.state.{s}"))) } else { None };
                let word = words.get(&format!("cli.field.{key}"));
                let key = if word.starts_with("cli.field.") { key } else { &word };
                if value.is_object() || value.is_array() {
                    println!("{indent}{key}:");
                    human(value, depth + 1, words);
                } else {
                    let text = state.filter(|s| !s.starts_with("cli.state.")).unwrap_or_else(|| display(value, words));
                    println!("{indent}{key}: {text}");
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                human(item, depth, words);
            }
        }
        _ => println!("{indent}{}", display(value, words)),
    }
}
fn display(value: &Value, words: &Words) -> String {
    if let Some(v) = value.as_bool() {
        return words.get(if v { "cli.yes" } else { "cli.no" });
    }
    value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())
}
