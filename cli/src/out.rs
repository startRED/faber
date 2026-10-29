//! What the command line prints: words for a person, JSON for a program, and what each exit code means.

use crate::words::Words;
use serde_json::{Value, json};
use std::io::{IsTerminal, Write};

/// Done.
pub const DONE: i32 = 0;
/// Faber refused or the work failed; the key says why.
pub const REFUSED: i32 = 1;
/// The command line was used wrongly.
pub const USAGE: i32 = 2;
/// Faber could not be reached.
pub const UNREACHED: i32 = 3;
/// A card waits for a person (`--no-wait`, or nobody at this terminal to answer it).
pub const WAITING: i32 = 4;
/// Stopped with Ctrl+C.
pub const STOPPED: i32 = 130;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Strong,
    Quiet,
    Accent,
    Good,
    Warn,
    Bad,
}

pub struct Out {
    pub json: bool,
    pub color: bool,
    /// A person can answer here: both ends are a terminal.
    pub interactive: bool,
    pub words: Words,
}

impl Out {
    pub fn new(json: bool, plain: bool, words: Words) -> Out {
        let terminal = std::io::stdout().is_terminal();
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) || std::env::var("TERM").is_ok_and(|t| t == "dumb");
        let color = terminal && !plain && !json && !no_color && crossterm::ansi_support::supports_ansi();
        Out { json, color, interactive: terminal && std::io::stdin().is_terminal() && !json, words }
    }

    pub fn t(&self, key: &str) -> String {
        self.words.get(key)
    }

    pub fn paint(&self, tone: Tone, text: &str) -> String {
        use crossterm::style::Stylize;
        if !self.color {
            return text.to_string();
        }
        match tone {
            Tone::Strong => text.bold().to_string(),
            Tone::Quiet => text.dim().to_string(),
            Tone::Accent => text.cyan().to_string(),
            Tone::Good => text.green().to_string(),
            Tone::Warn => text.yellow().to_string(),
            Tone::Bad => text.red().to_string(),
        }
    }

    pub fn line(&self, text: &str) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{text}");
    }

    /// Part of a line, shown at once (a streaming answer).
    pub fn part(&self, text: &str) {
        let mut out = std::io::stdout().lock();
        let _ = write!(out, "{text}");
        let _ = out.flush();
    }

    /// One JSON document: indented for a person at a terminal, one line for a program.
    pub fn json(&self, value: &Value) {
        let text = if std::io::stdout().is_terminal() { serde_json::to_string_pretty(value) } else { serde_json::to_string(value) };
        self.line(&text.unwrap_or_default());
    }

    /// A refusal or failure, by its key: `{"error": key, "message": words}` for a program, the words on stderr for a person.
    pub fn fail(&self, key: &str, code: i32) -> i32 {
        if self.json {
            self.json(&json!({ "error": key, "message": self.t(key) }));
        } else {
            let mut err = std::io::stderr().lock();
            let _ = writeln!(err, "{}", self.paint(Tone::Bad, &self.t(key)));
        }
        code
    }
}
