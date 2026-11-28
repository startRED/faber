//! Short check output (R155): what a project's check printed, read into its failures with `file:line`, the tool's suggested
//! fix, the measurements and one summary line. The roles get only that; the whole output stays in the mission's private
//! log. A format no parser knows falls back to its last lines. `cargo t` reads Cargo with the same functions (R154), so
//! Faber's lanes and its missions see a check the same way.

use serde_json::Value;
use std::collections::HashMap;

/// One problem a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// `file:line` or `file:line:col` when the tool names one, relative to the folder the check ran in.
    pub place: Option<String>,
    /// What is wrong, in a few lines.
    pub message: String,
    /// The tool's own suggested fix, when it gives one.
    pub fix: Option<String>,
}

/// What the roles read of one check.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub failures: Vec<Failure>,
    pub measurements: Vec<String>,
    /// One line: passed or failed, and the tool's own tally when it prints one.
    pub summary: String,
    /// The only failure is the output's last lines, shown whole.
    pub fallback: bool,
}

/// The most failures a role reads, the most lines of each, and the longest line; the log keeps everything.
pub const SHOWN: usize = 20;
pub const FAILURE_LINES: usize = 8;
const LINE_CHARS: usize = 300;
/// How many last lines an output in an unknown format leaves.
pub const LAST_LINES: usize = 20;

/// What the parsers found in one output.
#[derive(Default)]
struct Found {
    failures: Vec<Failure>,
    measurements: Vec<String>,
    tallies: Vec<String>,
}

/// A parser reads the lines it knows, adds what it found, and says whether it recognised anything.
type Parser = fn(&str, &mut Found) -> bool;

/// Every known format but `cargo t`, whose output is already short and is read alone. A new format (pytest, jest or vitest,
/// eslint, go test) is one more function here.
const PARSERS: [Parser; 6] = [cargo_json, rustc, cargo_test, rustfmt, tsc, oxlint];

/// Reads a check's whole output. `exit` is its exit code (`None` when it did not finish) and `root` the folder it ran in,
/// which is cut from every path so places read as the project's own.
pub fn read(output: &str, exit: Option<i32>, root: &str) -> Report {
    let text = plain(output, root);
    let mut found = Found::default();
    let known = faber_test(&text, &mut found) || PARSERS.iter().fold(false, |any, parse| parse(&text, &mut found) | any);
    let mut failures: Vec<Failure> = Vec::new();
    for failure in found.failures {
        if !failures.contains(&failure) {
            failures.push(failure);
        }
    }
    let status = match exit {
        Some(0) => "passed".to_string(),
        Some(code) => format!("failed (exit {code})"),
        None => "did not finish".to_string(),
    };
    let fallback = exit != Some(0) && failures.is_empty();
    if fallback {
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let last = lines[lines.len().saturating_sub(LAST_LINES)..].join("\n");
        if !last.is_empty() {
            failures.push(Failure { place: None, message: last, fix: None });
        }
    }
    let mut summary = status;
    if !found.tallies.is_empty() {
        summary = format!("{summary}: {}", found.tallies.join("; "));
    }
    if !known && exit != Some(0) {
        summary.push_str(" (output not recognised; its last lines are shown)");
    }
    Report { fallback: fallback && !failures.is_empty(), failures, measurements: found.measurements, summary }
}

/// The output with every line the secret scanner flags replaced, so neither the log nor a role ever holds a printed secret.
/// A line is scanned as printed and without its colour codes (which could split a token), and a private key is withheld
/// from its header to its footer, or to the end when the footer never comes.
pub fn withheld(output: &str) -> String {
    let mut text = String::with_capacity(output.len());
    let mut in_key = false;
    for line in output.split_inclusive('\n') {
        let bare = plain(line, "");
        let key = |marker: &str| bare.contains(marker) && bare.contains("PRIVATE KEY-----");
        in_key = in_key || key("-----BEGIN");
        if in_key || super::scan_secrets(line).is_err() || super::scan_secrets(&bare).is_err() {
            text.push_str("[line withheld: it looks like a secret]");
            text.push_str(if line.ends_with('\n') { "\n" } else { "" });
        } else {
            text.push_str(line);
        }
        in_key = in_key && !key("-----END");
    }
    text
}

impl Failure {
    /// One failure as a role reads it: its place and first line, at most `most` lines in all (each cut to a readable
    /// length), and the tool's fix under them. Every line ends with a newline.
    pub fn short(&self, most: usize) -> String {
        let mut out = String::new();
        let mut lines = self.message.lines().map(cut);
        let first = lines.next().unwrap_or_default();
        match &self.place {
            Some(place) => out.push_str(&format!("{place}: {first}\n")),
            None => out.push_str(&format!("{first}\n")),
        }
        for line in lines.by_ref().take(most.saturating_sub(1)) {
            out.push_str(&format!("    {line}\n"));
        }
        let more = lines.count();
        if more > 0 {
            out.push_str(&format!("    ({more} more lines in the log)\n"));
        }
        if let Some(fix) = &self.fix {
            out.push_str(&format!("    fix: {}\n", cut(fix)));
        }
        out
    }
}

impl Report {
    /// The text a role reads: each failure with its place and the fix under it, then the measurements, then the summary.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for failure in self.failures.iter().take(SHOWN) {
            out.push_str(&failure.short(if self.fallback { LAST_LINES } else { FAILURE_LINES }));
        }
        if self.failures.len() > SHOWN {
            out.push_str(&format!("({} more failures in the log)\n", self.failures.len() - SHOWN));
        }
        for measurement in &self.measurements {
            out.push_str(&format!("{}\n", cut(measurement)));
        }
        out.push_str(&self.summary);
        out
    }
}

fn cut(line: &str) -> String {
    match line.char_indices().nth(LINE_CHARS) {
        Some((at, _)) => format!("{}...", &line[..at]),
        None => line.to_string(),
    }
}

/// The output without colour codes or carriage returns, and with the folder it ran in cut from its paths.
fn plain(output: &str, root: &str) -> String {
    let mut text = String::with_capacity(output.len());
    let mut chars = output.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => {
                // A CSI sequence ends at its first letter.
                if chars.peek() == Some(&'[') {
                    chars.next();
                    while chars.next().is_some_and(|c| !c.is_ascii_alphabetic()) {}
                }
            }
            '\r' => {}
            c => text.push(c),
        }
    }
    let text = text.replace(r"\\?\", "");
    let root = root.trim_end_matches(['/', '\\']);
    if root.is_empty() {
        return text;
    }
    let forward = root.replace('\\', "/");
    [format!("{root}\\"), format!("{root}/"), format!("{forward}/")].iter().fold(text, |text, prefix| text.replace(prefix.as_str(), ""))
}

/// `1 error`, `2 errors`.
fn count(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

/// `path:line` or `path:line:col`, when `text` is one.
fn place(text: &str) -> Option<String> {
    let mut parts = text.rsplitn(3, ':');
    let last = parts.next()?;
    let middle = parts.next()?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(last) {
        return None;
    }
    let path = if digits(middle) { parts.next()? } else { middle };
    // A drive letter alone (`C:5`) is not a path.
    (path.trim().len() > 1).then(|| text.to_string())
}

// ----- `cargo t`, Faber's own runner -----

/// `cargo t` prints only failures, measurements and one summary already (R154): each `FAIL` or not-run block becomes a
/// failure, each pass it printed is a measurement, and its summary is the tally.
fn faber_test(text: &str, found: &mut Found) -> bool {
    let is_total =
        |l: &&str| l.split_once(" passed, ").is_some_and(|(n, rest)| n.bytes().all(|b| b.is_ascii_digit()) && rest.contains(" not run, "));
    let Some(total) = text.lines().rev().find(is_total) else { return false };
    found.tallies.push(total.split(". Every line").next().unwrap_or(total).to_string());
    let lines: Vec<&str> = text.lines().collect();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        at += 1;
        if line.starts_with("pass  ") || line.starts_with("Build folder:") {
            found.measurements.push(line.to_string());
            continue;
        }
        let Some(name) = line.strip_prefix("FAIL  ").or_else(|| line.strip_prefix("----  ")) else { continue };
        let mut body = Vec::new();
        while at < lines.len() && lines[at].starts_with("          ") {
            body.push(lines[at].trim());
            at += 1;
        }
        let mut failure = Failure { place: None, message: squeeze(name), fix: None };
        if let Some(first) = body.first() {
            let (spot, rest) = located(first);
            failure.place = spot;
            let mut message = vec![failure.message.clone()];
            message.extend(rest.map(str::to_string));
            for line in &body[1..] {
                match line.strip_prefix("help: ") {
                    Some(help) if failure.fix.is_none() => failure.fix = Some(help.to_string()),
                    _ => message.push(line.to_string()),
                }
            }
            failure.message = message.join("\n");
        }
        found.failures.push(failure);
    }
    true
}

/// The suite padding of a `cargo t` line squeezed: `lint    cargo clippy` reads `lint: cargo clippy`.
fn squeeze(name: &str) -> String {
    match name.split_once("  ") {
        Some((suite, rest)) => format!("{suite}: {}", rest.trim_start()),
        None => name.to_string(),
    }
}

/// A line that starts with a place (`file:line:col: text`, or a panic `at file:line:col:`), split into the place and the
/// rest of the line.
fn located(line: &str) -> (Option<String>, Option<&str>) {
    if let Some((_, at)) = line.split_once(" panicked at ") {
        return (place(at.trim_end_matches(':')), None);
    }
    for (index, _) in line.match_indices(": ") {
        if let Some(spot) = place(&line[..index]) {
            return (Some(spot), Some(&line[index + 2..]));
        }
    }
    (None, Some(line))
}

// ----- Cargo and rustc -----

/// The result lines of `cargo test`: `test some::name ... ok`. Anything that is not a test line is ignored.
pub fn parse_cargo_tests(output: &str) -> Vec<(String, bool)> {
    output
        .lines()
        .filter_map(|l| l.strip_prefix("test "))
        .filter_map(|rest| {
            let (name, verdict) = rest.rsplit_once(" ... ")?;
            match verdict.trim() {
                "ok" => Some((name.to_string(), true)),
                "FAILED" => Some((name.to_string(), false)),
                _ => None,
            }
        })
        .collect()
}

/// What each failed test printed, from libtest's `---- name stdout ----` sections: from the panic on (its first line
/// names the file and line), without the backtrace note.
pub fn failure_sections(output: &str) -> HashMap<String, String> {
    let mut sections: HashMap<String, Vec<&str>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in output.lines() {
        if let Some(name) = line.strip_prefix("---- ").and_then(|rest| rest.strip_suffix(" stdout ----")) {
            sections.insert(name.to_string(), Vec::new());
            current = Some(name.to_string());
        } else if line == "failures:" || line.starts_with("test result:") {
            current = None;
        } else if let Some(lines) = current.as_ref().and_then(|name| sections.get_mut(name)) {
            lines.push(line);
        }
    }
    sections
        .into_iter()
        .map(|(name, lines)| {
            let from = lines.iter().position(|l| l.starts_with("thread '")).unwrap_or(0);
            let kept: Vec<&str> = lines[from..]
                .iter()
                .copied()
                .filter(|l| !l.trim().is_empty() && !l.starts_with("note: run with `RUST_BACKTRACE"))
                .collect();
            (name, kept.join("\n"))
        })
        .collect()
}

/// Cargo's lines for test programs that ended without a verdict (a crash); libtest's own exit code for failed tests is 101.
pub fn died(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("process didn't exit successfully") && !l.ends_with("(exit code: 101)"))
        .collect()
}

/// One compiler error from Cargo's JSON: its place, `error[code]: message (label)`, and the compiler's fix. Warnings and
/// the closing "aborting due to" give `None`.
pub fn compiler_error(message: &Value) -> Option<Failure> {
    let text = message["message"].as_str()?;
    if message["level"] != "error" || text.starts_with("aborting due to") {
        return None;
    }
    let code = message["code"]["code"].as_str().map(|c| format!("[{c}]")).unwrap_or_default();
    let primary = message["spans"].as_array().and_then(|spans| spans.iter().find(|s| s["is_primary"] == true));
    // An error inside a macro of the standard library is placed where the project's code called the macro.
    let mut called = primary;
    while let Some(span) = called.filter(|s| s["file_name"].as_str().is_some_and(|f| f.starts_with("/rustc/"))) {
        called = span["expansion"]["span"].is_object().then(|| &span["expansion"]["span"]);
    }
    let place =
        called.or(primary).map(|s| format!("{}:{}:{}", s["file_name"].as_str().unwrap_or_default(), s["line_start"], s["column_start"]));
    let label = primary.and_then(|s| s["label"].as_str()).map(|l| format!(" ({l})")).unwrap_or_default();
    Some(Failure { place, message: format!("error{code}: {text}{label}"), fix: json_fix(message) })
}

/// One compiler error from Cargo's JSON as `file:line:col: error[code]: message (label)`, with the compiler's fix on a
/// `help:` line under it when it has one.
pub fn diagnostic(message: &Value) -> Option<String> {
    let failure = compiler_error(message)?;
    let place = failure.place.map(|p| format!("{p}: ")).unwrap_or_default();
    let fix = failure.fix.map(|help| format!("\n    help: {help}")).unwrap_or_default();
    Some(format!("{place}{}{fix}", failure.message))
}

/// Help that is only a link or the way to silence the lint is no fix.
fn advice(text: &str) -> bool {
    !text.starts_with("for further information") && !text.starts_with("to override")
}

/// The compiler's fix: a help that carries an edit first, else the first help that is advice.
fn json_fix(message: &Value) -> Option<String> {
    let helps: Vec<&Value> = message["children"].as_array()?.iter().filter(|c| c["level"] == "help").collect();
    let edit = |help: &&Value| {
        let replacement = help["spans"].as_array()?.iter().find_map(|s| s["suggested_replacement"].as_str())?;
        let text = help["message"].as_str()?;
        Some(match replacement.lines().next().map(str::trim) {
            Some(first) if !first.is_empty() => format!("{text}: `{first}`"),
            _ => text.to_string(),
        })
    };
    helps.iter().find_map(edit).or_else(|| helps.iter().filter_map(|h| h["message"].as_str()).find(|t| advice(t)).map(str::to_string))
}

/// The messages Cargo prints with `--message-format=json`. Any other line, JSON or not, is what the tests printed.
const CARGO_REASONS: [&str; 4] = ["compiler-message", "compiler-artifact", "build-script-executed", "build-finished"];

fn cargo_message(line: &str) -> Option<Value> {
    line.starts_with('{')
        .then(|| serde_json::from_str::<Value>(line).ok())
        .flatten()
        .filter(|v| CARGO_REASONS.iter().any(|r| v["reason"] == *r))
}

/// Cargo's output for the screen and the log: its compiler errors as [`diagnostic`] lines, each once, and the text a
/// person would have read, with every JSON message rendered and the build artifacts left out.
pub fn cargo_output(stdout: &str, stderr: &str) -> (Vec<String>, String) {
    let (mut errors, mut text) = (Vec::new(), String::new());
    for line in stdout.lines() {
        match cargo_message(line) {
            Some(v) if v["reason"] == "compiler-message" => {
                text.push_str(v["message"]["rendered"].as_str().unwrap_or_default());
                if let Some(error) = diagnostic(&v["message"]).filter(|e| !errors.contains(e)) {
                    errors.push(error);
                }
            }
            Some(_) => {}
            None => {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    text.push_str(stderr);
    (errors, text)
}

/// Compiler errors in Cargo's JSON messages, when the check asked for them.
fn cargo_json(text: &str, found: &mut Found) -> bool {
    let messages: Vec<Value> = text.lines().filter_map(cargo_message).collect();
    let errors = messages.iter().filter(|v| v["reason"] == "compiler-message").filter_map(|v| compiler_error(&v["message"]));
    found.failures.extend(errors);
    !messages.is_empty()
}

/// Cargo's own closing lines, which repeat what the compiler or the tests already said.
fn cargo_note(message: &str) -> bool {
    ["could not compile", "test failed", "build failed", "aborting due to"].iter().any(|start| message.starts_with(start))
        || message.ends_with(" target failed:")
        || message.ends_with(" targets failed:")
}

/// The compiler's errors as a person reads them: `error[E0277]: message`, its ` --> file:line:col`, the label under the
/// carets, and its fix (an inline `help:` label or a suggested line first, else its first advice).
fn rustc(text: &str, found: &mut Found) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    let mut errors = 0;
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        at += 1;
        let Some(rest) = line.strip_prefix("error") else { continue };
        let (code, message) = match rest.strip_prefix('[').and_then(|r| r.split_once("]: ")) {
            Some((code, message)) => (format!("[{code}]"), message),
            None => match rest.strip_prefix(": ") {
                Some(message) => (String::new(), message),
                None => continue,
            },
        };
        if cargo_note(message) {
            continue;
        }
        let mut block = Vec::new();
        while at < lines.len() && !lines[at].trim().is_empty() {
            block.push(lines[at]);
            at += 1;
        }
        // An error of Cargo itself (a build script, a missing package) has no place; its cause follows in sections.
        if !block.iter().any(|l| l.trim_start().starts_with("--> ")) {
            let mut said: Vec<&str> = block.iter().map(|l| l.trim()).collect();
            while at < lines.len() && !lines[at].starts_with("error") && !lines[at].starts_with("warning:") {
                said.push(lines[at].trim());
                at += 1;
            }
            let said = said.into_iter().filter(|l| !l.is_empty());
            let message = std::iter::once(format!("error{code}: {message}")).chain(said.map(str::to_string)).collect::<Vec<_>>();
            found.failures.push(Failure { place: None, message: message.join("\n"), fix: None });
            errors += 1;
            continue;
        }
        let spot = block.iter().find_map(|l| l.trim_start().strip_prefix("--> ")).and_then(place);
        let (mut label, mut edit, mut first_advice) = (None, None, None);
        for (index, line) in block.iter().enumerate() {
            if let Some(under) = carets(line) {
                match under.strip_prefix("help: ") {
                    Some(help) => edit = edit.or(Some(help.to_string())),
                    None if label.is_none() => label = Some(under.to_string()),
                    None => {}
                }
            } else if let Some(help) = line.strip_prefix("help: ") {
                let suggested = block[index + 1..].iter().take_while(|l| !l.starts_with("help: ")).find_map(|l| added(l));
                match suggested {
                    Some(code) => edit = edit.or(Some(format!("{help}: `{code}`"))),
                    None => first_advice = first_advice.or(advice(help).then(|| help.to_string())),
                }
            } else if let Some(help) = line.trim_start().strip_prefix("= help: ") {
                first_advice = first_advice.or(advice(help).then(|| help.to_string()));
            }
        }
        let label = label.map(|l| format!(" ({l})")).unwrap_or_default();
        found.failures.push(Failure { place: spot, message: format!("error{code}: {message}{label}"), fix: edit.or(first_advice) });
        errors += 1;
    }
    if errors > 0 {
        found.tallies.push(count(errors, "compiler error"));
    }
    errors > 0
}

/// The text after the primary carets of a snippet line (`  |     ^^^ label`), when there is any.
fn carets(line: &str) -> Option<&str> {
    let (gutter, rest) = line.split_once('|')?;
    if !gutter.trim().bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let rest = rest.trim_start();
    let after = rest.strip_prefix('^')?.trim_start_matches('^').trim();
    (!after.is_empty()).then_some(after)
}

/// The new text of a suggested line (`5 + pub fn empty(v: &[u32]) -> bool {`).
fn added(line: &str) -> Option<&str> {
    let (number, rest) = line.trim_start().split_once(' ')?;
    (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())).then_some(())?;
    rest.strip_prefix("+ ").map(str::trim).filter(|code| !code.is_empty())
}

/// `cargo test`: each failed test with the place it panicked at and what it said, test programs that crashed, and the
/// totals of every `test result:` line.
fn cargo_test(text: &str, found: &mut Found) -> bool {
    let results: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix("test result: ")).collect();
    let tests = parse_cargo_tests(text);
    if results.is_empty() && tests.is_empty() {
        return false;
    }
    let sections = failure_sections(text);
    for (name, _) in tests.iter().filter(|(_, ok)| !ok) {
        let said = sections.get(name).map_or("", String::as_str);
        let mut lines = said.lines();
        let (spot, message) = match said.lines().next().map(located) {
            Some((Some(spot), None)) => {
                lines.next();
                (Some(spot), lines.collect::<Vec<_>>().join("\n"))
            }
            _ => (None, said.to_string()),
        };
        let message = if message.is_empty() { format!("test {name} failed") } else { format!("test {name}: {message}") };
        found.failures.push(Failure { place: spot, message, fix: None });
    }
    for crash in died(text) {
        found.failures.push(Failure { place: None, message: crash.to_string(), fix: None });
    }
    let (mut counts, mut seconds) = ([0u64; 3], 0.0);
    for result in &results {
        for part in result.split(';') {
            let words: Vec<&str> = part.split_whitespace().collect();
            match words.as_slice() {
                [.., n, "passed"] => counts[0] += n.parse::<u64>().unwrap_or(0),
                [n, "failed"] => counts[1] += n.parse::<u64>().unwrap_or(0),
                [n, "ignored"] => counts[2] += n.parse::<u64>().unwrap_or(0),
                ["finished", "in", time] => seconds += time.trim_end_matches('s').parse::<f64>().unwrap_or(0.0),
                _ => {}
            }
        }
    }
    let ignored = if counts[2] > 0 { format!(", {} ignored", counts[2]) } else { String::new() };
    found.tallies.push(format!("{} passed, {} failed{ignored} in {seconds:.1} s", counts[0], counts[1]));
    true
}

/// `cargo fmt --check`: one failure per file it would change, at the first place it differs.
fn rustfmt(text: &str, found: &mut Found) -> bool {
    let mut files: Vec<(String, Vec<String>)> = Vec::new();
    for spot in text.lines().filter_map(|l| l.strip_prefix("Diff in ")?.strip_suffix(':')).filter_map(place) {
        let (file, line) = spot.rsplit_once(':').unwrap_or((&spot, ""));
        match files.iter_mut().find(|(f, _)| f == file) {
            Some((_, lines)) => lines.push(line.to_string()),
            None => files.push((file.to_string(), vec![line.to_string()])),
        }
    }
    for (file, lines) in &files {
        let message = if lines.len() == 1 {
            "not formatted the way rustfmt formats it".to_string()
        } else {
            format!("not formatted the way rustfmt formats it, also at lines {}", lines[1..].join(", "))
        };
        found.failures.push(Failure { place: Some(format!("{file}:{}", lines[0])), message, fix: Some("run `cargo fmt`".into()) });
    }
    if !files.is_empty() {
        found.tallies.push(format!("{} not formatted", count(files.len(), "file")));
    }
    !files.is_empty()
}

// ----- TypeScript and JavaScript -----

/// TypeScript's errors, plain (`src/a.ts(4,3): error TS2322: ...`, with its indented explanation) or pretty
/// (`src/a.ts:4:3 - error TS2322: ...`).
fn tsc(text: &str, found: &mut Found) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    let mut errors = 0;
    for (index, line) in lines.iter().enumerate() {
        let parsed = if let Some((left, message)) = line.split_once("): error TS") {
            left.rsplit_once('(').and_then(|(file, at)| at.split_once(',').map(|(l, c)| (format!("{file}:{l}:{c}"), message)))
        } else if let Some((left, message)) = line.split_once(" - error TS") {
            Some((left.to_string(), message))
        } else {
            None
        };
        let Some((spot, message)) = parsed else { continue };
        let Some(spot) = place(&spot) else { continue };
        let explained: Vec<&str> =
            lines[index + 1..].iter().take_while(|l| l.starts_with("  ") && !l.trim().is_empty()).take(3).map(|l| l.trim()).collect();
        let mut message = format!("error TS{message}");
        // The pretty form follows with the code it points at, not an explanation.
        if line.contains("): error TS") && !explained.is_empty() {
            message = format!("{message}\n{}", explained.join("\n"));
        }
        found.failures.push(Failure { place: Some(spot), message, fix: None });
        errors += 1;
    }
    if errors > 0 {
        found.tallies.push(count(errors, "type error"));
    }
    errors > 0
}

/// oxlint's lines (`src/a.ts:3:17: error eslint(no-debugger): message help: fix`). Warnings are counted, not failures.
fn oxlint(text: &str, found: &mut Found) -> bool {
    let (mut errors, mut warnings) = (0, 0);
    for line in text.lines() {
        let Some((index, level)) =
            [": error ", ": warning "].iter().find_map(|level| line.find(level).map(|index| (index, level.trim_matches([':', ' ']))))
        else {
            continue;
        };
        let Some(spot) = place(&line[..index]) else { continue };
        let rest = &line[index + level.len() + 3..];
        if !rest.contains("): ") {
            continue;
        }
        if level == "warning" {
            warnings += 1;
            continue;
        }
        let (message, fix) = match rest.split_once(" help: ") {
            Some((message, fix)) => (message, Some(fix.to_string())),
            None => (rest, None),
        };
        found.failures.push(Failure { place: Some(spot), message: format!("error {message}"), fix });
        errors += 1;
    }
    if errors + warnings > 0 {
        found.tallies.push(format!("{}, {}", count(errors, "lint error"), count(warnings, "warning")));
    }
    errors + warnings > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/check-output").join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn read_fixture(name: &str, exit: i32) -> Report {
        read(&fixture(name), Some(exit), r"C:\work\demo")
    }

    fn at(spot: &str) -> Option<String> {
        Some(spot.to_string())
    }

    #[test]
    fn a_failed_cargo_test_keeps_the_place_it_panicked_at_and_what_it_said() {
        let report = read_fixture("cargo-test-fail.txt", 101);
        assert_eq!(
            report.failures,
            [Failure {
                place: at(r"src\lib.rs:14:9"),
                message: "test tests::adds: assertion `left == right` failed: two and two\n  left: 4\n right: 5".into(),
                fix: None
            }]
        );
        assert_eq!(report.summary, "failed (exit 101): 1 passed, 1 failed in 0.0 s");
        assert!(!report.text().contains("noise from the test"));
        let no_fail_fast = format!("{}error: 2 targets failed:\n    `-p demo --lib`\n", fixture("cargo-test-fail.txt"));
        assert_eq!(read(&no_fail_fast, Some(101), "").failures.len(), 1, "Cargo's closing line is no compiler error");
    }

    #[test]
    fn a_passing_cargo_test_is_one_line() {
        let report = read_fixture("cargo-test-pass.txt", 0);
        assert!(report.failures.is_empty());
        assert_eq!(report.text(), "passed: 2 passed, 0 failed in 0.0 s");
    }

    #[test]
    fn a_compiler_error_keeps_its_place_label_and_first_advice_and_cargo_closing_lines_go() {
        let report = read_fixture("rustc-fail.txt", 101);
        assert_eq!(
            report.failures,
            [Failure {
                place: at(r"src\lib.rs:2:7"),
                message: "error[E0277]: cannot add `&str` to `u32` (no implementation for `u32 + &str`)".into(),
                fix: Some("the trait `Add<&str>` is not implemented for `u32`".into())
            }]
        );
        assert_eq!(report.summary, "failed (exit 101): 1 compiler error");
    }

    #[test]
    fn a_clippy_error_carries_the_suggested_line_or_the_inline_help_and_never_the_link_or_the_silencer() {
        let report = read_fixture("clippy-fail.txt", 101);
        assert_eq!(
            report.failures,
            [
                Failure {
                    place: at(r"src\lib.rs:5:17"),
                    message: "error: writing `&Vec` instead of `&[_]` involves a new object where a slice will do".into(),
                    fix: Some("change this to: `pub fn empty(v: &[u32]) -> bool {`".into())
                },
                Failure {
                    place: at(r"src\lib.rs:6:5"),
                    message: "error: length comparison to zero".into(),
                    fix: Some("using `is_empty` is clearer and more explicit: `v.is_empty()`".into())
                }
            ]
        );
    }

    #[test]
    fn rustfmt_names_each_file_at_its_first_difference_and_the_command_that_fixes_it() {
        let report = read_fixture("rustfmt-fail.txt", 1);
        assert_eq!(
            report.failures,
            [Failure {
                place: at(r"src\lib.rs:19"),
                message: "not formatted the way rustfmt formats it".into(),
                fix: Some("run `cargo fmt`".into())
            }]
        );
        assert_eq!(report.summary, "failed (exit 1): 1 file not formatted");
    }

    #[test]
    fn tsc_errors_keep_their_place_in_the_plain_and_the_pretty_form() {
        let plain = read_fixture("tsc-fail.txt", 2);
        let error = Failure {
            place: at("src/a.ts:4:3"),
            message: "error TS2322: Type 'number' is not assignable to type 'string'.".into(),
            fix: None,
        };
        assert_eq!(plain.failures, read_fixture("tsc-pretty-fail.txt", 2).failures);
        assert_eq!(plain.failures, [error]);
        assert_eq!(plain.summary, "failed (exit 2): 1 type error");
        let explained = read("src/b.ts(1,1): error TS2345: Argument is wrong.\n  Type 'a' is not 'b'.\n", Some(2), "");
        assert_eq!(explained.failures[0].message, "error TS2345: Argument is wrong.\nType 'a' is not 'b'.");
    }

    #[test]
    fn oxlint_errors_carry_their_help_and_warnings_are_only_counted() {
        let report = read_fixture("oxlint-fail.txt", 1);
        assert_eq!(
            report.failures,
            [Failure {
                place: at("src/a.ts:3:17"),
                message: "error eslint(no-debugger): `debugger` statement is not allowed".into(),
                fix: Some("Remove the debugger statement".into())
            }]
        );
        assert_eq!(report.summary, "failed (exit 1): 1 lint error, 1 warning");
        assert_eq!(read_fixture("oxlint-pass.txt", 0).text(), "passed");
    }

    #[test]
    fn faber_s_own_runner_is_read_alone_with_its_failures_measurements_and_summary() {
        let report = read_fixture("faber-test-fail.txt", 1);
        assert_eq!(
            report.failures,
            [
                Failure {
                    place: at("engine/src/project/run.rs:460:13"),
                    message: "lint: cargo clippy --workspace --all-targets -- -D warnings (48.3 s)\nerror: length comparison to zero".into(),
                    fix: Some("using `is_empty` is clearer and more explicit: `log.is_empty()`".into())
                },
                Failure {
                    place: at(r"engine\src\project\proof\output.rs:512:9"),
                    message: "rust: project::proof::output::tests::a_tsc_error_keeps_its_place\nassertion `left == right` failed\nleft: None\nright: Some(\"src/a.ts:4:3\")".into(),
                    fix: None
                }
            ]
        );
        assert_eq!(report.measurements.len(), 2, "the build folder line and the memory pass");
        assert_eq!(report.summary, "failed (exit 1): 484 passed, 2 failed, 0 not run, 0 skipped");
        let pass = read_fixture("faber-test-pass.txt", 0);
        assert!(pass.failures.is_empty());
        assert!(pass.text().ends_with("[0.4%]\npassed: 486 passed, 0 failed, 0 not run, 0 skipped"), "{}", pass.text());
    }

    #[test]
    fn an_unknown_format_falls_back_to_its_last_lines_and_says_so() {
        let output = (1..=30).map(|n| format!("step {n}")).collect::<Vec<_>>().join("\n");
        let report = read(&output, Some(3), "");
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].message.starts_with("step 11\n") && report.failures[0].message.ends_with("step 30"));
        assert_eq!(report.summary, "failed (exit 3) (output not recognised; its last lines are shown)");
        assert_eq!(read("all good\n", Some(0), "").text(), "passed");
        assert_eq!(read("", None, "").text(), "did not finish (output not recognised; its last lines are shown)");
    }

    #[test]
    fn the_folder_the_check_ran_in_and_colours_are_cut_from_what_roles_read() {
        let output = "\u{1b}[31mDiff in \\\\?\\C:\\work\\demo\\src\\x.rs:3:\u{1b}[0m\nsrc/a.ts(1,2): error TS1: x\nC:/work/demo/b.ts(2,3): error TS2: y\n";
        let report = read(output, Some(1), "C:\\work\\demo\\");
        let places: Vec<_> = report.failures.iter().filter_map(|f| f.place.as_deref()).collect();
        assert_eq!(places, [r"src\x.rs:3", "src/a.ts:1:2", "b.ts:2:3"]);
    }

    #[test]
    fn a_long_report_shows_twenty_failures_and_counts_the_rest() {
        let output = (1..=25).map(|n| format!("src/a.ts({n},1): error TS1: bad {n}")).collect::<Vec<_>>().join("\n");
        let text = read(&output, Some(2), "").text();
        assert!(text.contains("src/a.ts:20:1: error TS1: bad 20") && !text.contains("bad 21"));
        assert!(text.ends_with("(5 more failures in the log)\nfailed (exit 2): 25 type errors"), "{text}");
        let long = format!("src/a.ts(1,1): error TS1: {}", "x".repeat(1000));
        assert!(read(&long, Some(2), "").text().lines().next().unwrap().ends_with("x..."));
    }

    #[test]
    fn a_line_that_looks_like_a_secret_is_withheld_and_the_rest_is_kept_as_it_was() {
        let token = format!("ghp_{}", "a".repeat(36));
        let output = format!("src/a.ts(1,1): error TS1: leaked {token}\r\nok line\nlast");
        assert_eq!(withheld(&output), "[line withheld: it looks like a secret]\nok line\nlast");
        assert!(!read(&withheld(&output), Some(2), "").text().contains(&token));
        let split = format!("error TS1: gh\u{1b}[0mp_{}\n", "a".repeat(36));
        assert_eq!(withheld(&split), "[line withheld: it looks like a secret]\n", "a colour code cannot hide a token");
        let key = "a\n-----BEGIN PRIVATE KEY-----\nMC4CAQAw\n-----END PRIVATE KEY-----\nb\n-----BEGIN RSA PRIVATE KEY-----\nMIIE\n";
        let shown = withheld(key);
        let kept: Vec<&str> = shown.lines().filter(|l| !l.starts_with("[line withheld")).collect();
        assert_eq!(kept, ["a", "b"], "a key is withheld whole, and to the end without its footer");
    }

    #[test]
    fn an_unknown_failure_shows_all_its_last_lines_and_a_long_failure_counts_what_it_cut() {
        let output = (1..=19).map(|n| format!("step {n}")).chain(["fatal: missing config.toml".to_string()]).collect::<Vec<_>>();
        let text = read(&output.join("\n"), Some(1), "").text();
        assert!(text.contains("fatal: missing config.toml") && text.contains("step 1\n"), "{text}");
        let message = (0..12).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
        let long = Report { failures: vec![Failure { place: None, message, fix: None }], ..Report::default() };
        assert!(long.text().contains("    line 7\n    (4 more lines in the log)"), "{}", long.text());
    }

    #[test]
    fn a_path_with_a_space_keeps_its_error() {
        let output = "src/my folder/a.ts(1,7): error TS2322: x\nsrc/b.ts(2,7): error TS2322: y\n";
        let places: Vec<_> = read(output, Some(2), "").failures.into_iter().filter_map(|f| f.place).collect();
        assert_eq!(places, ["src/my folder/a.ts:1:7", "src/b.ts:2:7"]);
    }

    #[test]
    fn an_error_of_cargo_itself_keeps_its_cause() {
        let output = "error: failed to run custom build command for `demo v0.1.0`\n\nCaused by:\n  process didn't exit successfully: \
                      `build-script-build` (exit code: 1)\n  --- stderr\n  pkg-config could not find openssl\n";
        let report = read(output, Some(101), "");
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].message.ends_with("pkg-config could not find openssl"), "{:?}", report.failures);
    }

    #[test]
    fn the_same_error_twice_is_shown_once() {
        let twice = "error: e\n --> src/a.rs:1:1\n\nerror: e\n --> src/a.rs:1:1\n";
        assert_eq!(read(twice, Some(101), "").failures.len(), 1);
    }

    // ----- Cargo's JSON and test output, shared with `cargo t` -----

    #[test]
    fn test_lines_become_pass_or_fail_and_everything_else_is_ignored() {
        let output = "running 3 tests\ntest plan::tests::a ... ok\ntest plan::tests::b ... FAILED\ntest plan::tests::c ... ignored\n\
                      test result: FAILED. 1 passed; 1 failed; 1 ignored\ntest sandbox::tests::long_name_with ... ok\n";
        assert_eq!(
            parse_cargo_tests(output),
            [
                ("plan::tests::a".to_string(), true),
                ("plan::tests::b".to_string(), false),
                ("sandbox::tests::long_name_with".to_string(), true)
            ]
        );
    }

    #[test]
    fn a_failed_test_keeps_its_panic_with_file_and_line_and_drops_its_prints_and_the_backtrace_note() {
        let output = "failures:\n\n---- recents::tests::keeps stdout ----\nlots of logging\n\nthread 'recents::tests::keeps' (32224) \
                      panicked at engine\\src\\recents.rs:180:23:\ncalled `Option::unwrap()` on a `None` value\nnote: run with \
                      `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n---- b stdout ----\nError: \"no\"\n\n\nfailures:\n    \
                      recents::tests::keeps\n    b\n\ntest result: FAILED. 1 passed; 2 failed\n";
        let sections = failure_sections(output);
        assert_eq!(
            sections["recents::tests::keeps"],
            "thread 'recents::tests::keeps' (32224) panicked at engine\\src\\recents.rs:180:23:\ncalled `Option::unwrap()` on a `None` value"
        );
        assert_eq!(sections["b"], "Error: \"no\"", "a test that returned an error keeps all it printed");
        assert_eq!(sections.len(), 2);
    }

    fn message(level: &str, text: &str, children: Value) -> Value {
        json!({
            "level": level, "message": text, "code": {"code": "E0308"}, "rendered": format!("{level}: {text}\n --> engine/src/nudge.rs:126:23\n"),
            "spans": [
                {"file_name": "engine/src/nudge.rs", "line_start": 120, "column_start": 9, "is_primary": false, "label": "expected due to this"},
                {"file_name": "engine/src/nudge.rs", "line_start": 126, "column_start": 23, "is_primary": true, "label": "expected `u32`, found `&str`"}
            ],
            "children": children
        })
    }

    #[test]
    fn a_compiler_error_is_one_line_with_its_place_and_the_fix_under_it() {
        let lint = json!([
            {"level": "note", "message": "`-D clippy::len-zero` implied by `-D warnings`", "spans": []},
            {"level": "help", "message": "for further information visit https://rust-lang.github.io/rust-clippy/", "spans": []},
            {"level": "help", "message": "using `is_empty` is clearer and more explicit", "spans": [{"suggested_replacement": "word.is_empty()"}]}
        ]);
        assert_eq!(
            diagnostic(&message("error", "length comparison to zero", lint)).unwrap(),
            "engine/src/nudge.rs:126:23: error[E0308]: length comparison to zero (expected `u32`, found `&str`)\n    \
             help: using `is_empty` is clearer and more explicit: `word.is_empty()`"
        );
        let advice = json!([{"level": "help", "message": "to override `-D warnings` add `#[allow(x)]`", "spans": []},
                            {"level": "help", "message": "consider removing the borrow", "spans": [{"suggested_replacement": ""}]}]);
        assert!(diagnostic(&message("error", "x", advice)).unwrap().ends_with("\n    help: consider removing the borrow"));
        assert!(!diagnostic(&message("error", "x", json!([]))).unwrap().contains("help"));
        assert_eq!(diagnostic(&message("warning", "unused variable", json!([]))), None);
        assert_eq!(diagnostic(&message("error", "aborting due to 1 previous error", json!([]))), None);
        let in_macro = json!({"level": "error", "message": "unnecessary use of `clone`", "children": [], "spans": [{
            "file_name": "/rustc/b94/library\\core\\src\\macros\\mod.rs", "line_start": 45, "column_start": 24, "is_primary": true,
            "expansion": {"span": {"file_name": "engine\\src\\a.rs", "line_start": 692, "column_start": 9, "is_primary": false}}}]});
        assert_eq!(diagnostic(&in_macro).unwrap(), "engine\\src\\a.rs:692:9: error: unnecessary use of `clone`");
        let linker = json!({"level": "error", "message": "linking with `rust-lld.exe` failed: exit code: 1", "spans": [], "children": []});
        assert_eq!(diagnostic(&linker).unwrap(), "error: linking with `rust-lld.exe` failed: exit code: 1");
    }

    #[test]
    fn cargo_json_keeps_each_error_once_renders_messages_for_the_log_and_drops_artifacts() {
        let error = json!({"reason": "compiler-message", "message": message("error", "mismatched types", json!([]))}).to_string();
        let warning = json!({"reason": "compiler-message", "message": message("warning", "unused", json!([]))}).to_string();
        let artifact = json!({"reason": "compiler-artifact", "filenames": ["x.rlib"]}).to_string();
        let printed = r#"{"reason":"mismatch","expected":1}"#;
        let stdout = format!("{artifact}\n{error}\n{warning}\n{error}\ntest a ... ok\n{{not json\n{printed}\n");
        let (errors, text) = cargo_output(&stdout, "   Compiling faber-engine\n");
        assert_eq!(errors, ["engine/src/nudge.rs:126:23: error[E0308]: mismatched types (expected `u32`, found `&str`)"]);
        assert!(text.contains("error: mismatched types\n --> engine/src/nudge.rs:126:23") && text.contains("warning: unused"));
        assert!(
            text.contains(&format!("test a ... ok\n{{not json\n{printed}\n   Compiling faber-engine")),
            "a test's own JSON stays: {text}"
        );
        assert!(!text.contains("x.rlib"));
        let report = read(&stdout, Some(101), "");
        assert_eq!(report.failures.len(), 1, "a mission that asked for JSON reads the same error once: {report:?}");
        assert_eq!(report.failures[0].place.as_deref(), Some("engine/src/nudge.rs:126:23"));
    }

    #[test]
    fn a_test_program_that_crashed_is_told_apart_from_one_whose_tests_failed() {
        let stderr = "error: test failed, to rerun pass `-p faber-engine --lib`\n\nCaused by:\n  process didn't exit successfully: \
                      `deps\\faber_engine.exe --test-threads 8` (exit code: 101)\nerror: test failed, to rerun pass `-p faber-engine --test \
                      engine`\n\nCaused by:\n  process didn't exit successfully: `deps\\engine.exe` (exit code: 0xc00000fd, STATUS_STACK_OVERFLOW)\n";
        assert_eq!(died(stderr), ["process didn't exit successfully: `deps\\engine.exe` (exit code: 0xc00000fd, STATUS_STACK_OVERFLOW)"]);
    }
}
