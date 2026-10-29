//! The open conversation as lines: the person's messages marked, the AI's steps one line each, its answer as markdown.
//! Each message is drawn once and kept until it changes or the width does, so a streaming answer redraws only itself.

use super::markdown::{self, spans, truncate, wrap_text};
use super::palette::{Palette, Tone};
use crate::thread::{self, StepState, Thread};
use crate::words::Words;
use ratatui::text::{Line, Span};
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Steps shown above an answer; the ones before them fold into one line.
const STEPS_SHOWN: usize = 8;

#[derive(Default)]
pub struct Transcript {
    width: usize,
    cache: HashMap<String, (u64, Vec<Line<'static>>)>,
    order: Vec<String>,
    total: usize,
}

impl Transcript {
    pub fn clear(&mut self) {
        self.cache.clear();
        self.order.clear();
        self.total = 0;
    }

    /// Draws what changed since the last call.
    pub fn sync(&mut self, thread: Option<&Thread>, width: usize, p: &Palette, words: &Words) {
        if width != self.width {
            self.cache.clear();
            self.width = width;
        }
        self.order.clear();
        let Some(thread) = thread else {
            self.cache.clear();
            self.total = 0;
            return;
        };
        for id in &thread.order {
            let Some(message) = thread.get(id) else { continue };
            let sig = signature(message, width);
            if self.cache.get(id).is_none_or(|(old, _)| *old != sig) {
                self.cache.insert(id.clone(), (sig, message_lines(message, width, p, words)));
            }
            self.order.push(id.clone());
        }
        if self.cache.len() > self.order.len() {
            let keep: std::collections::HashSet<&String> = self.order.iter().collect();
            self.cache.retain(|id, _| keep.contains(id));
        }
        self.total = self.order.iter().filter_map(|id| self.cache.get(id)).map(|(_, lines)| lines.len()).sum();
    }

    pub fn total(&self) -> usize {
        self.total
    }

    /// `count` lines from line `start`.
    pub fn window(&self, start: usize, count: usize) -> Vec<Line<'static>> {
        let mut out = Vec::with_capacity(count);
        let mut at = 0;
        for id in &self.order {
            let Some((_, lines)) = self.cache.get(id) else { continue };
            let end = at + lines.len();
            if end > start && at < start + count {
                let from = start.saturating_sub(at);
                let to = (start + count - at).min(lines.len());
                out.extend_from_slice(&lines[from..to]);
            }
            at = end;
            if at >= start + count {
                break;
            }
        }
        out
    }
}

fn signature(message: &Value, width: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    width.hash(&mut hasher);
    message["role"].as_str().hash(&mut hasher);
    message["text"].as_str().map(str::len).hash(&mut hasher);
    message["status"].as_str().hash(&mut hasher);
    message["error"].as_str().hash(&mut hasher);
    message["model"].as_str().hash(&mut hasher);
    message["took"].as_u64().hash(&mut hasher);
    message["outputTokens"].as_u64().hash(&mut hasher);
    for tool in top_steps(message) {
        (thread::step_state(tool) as u8).hash(&mut hasher);
        thread::step_line(tool).hash(&mut hasher);
    }
    hasher.finish()
}

/// The AI's own steps; a subagent's are folded into the one that started it.
fn top_steps(message: &Value) -> impl Iterator<Item = &Value> {
    message["tools"].as_array().into_iter().flatten().filter(|t| t["parent"].is_null())
}

fn message_lines(message: &Value, width: usize, p: &Palette, words: &Words) -> Vec<Line<'static>> {
    let text = message["text"].as_str().unwrap_or("");
    let mut lines = Vec::new();
    if message["role"].as_str() == Some("user") {
        let mark = Span::styled("› ", p.tone(Tone::Accent).add_modifier(ratatui::style::Modifier::BOLD));
        lines.extend(wrap_text(text, p.tone(Tone::Text), width, mark, "  "));
        lines.push(Line::default());
        return lines;
    }
    let steps: Vec<&Value> = top_steps(message).collect();
    let hidden = steps.len().saturating_sub(STEPS_SHOWN);
    if hidden > 0 {
        lines.push(Line::from(Span::styled(format!("  {}", words.count("cli.screen.steps", hidden)), p.tone(Tone::Quiet))));
    }
    for tool in &steps[hidden..] {
        lines.push(step(tool, width, p));
    }
    if !steps.is_empty() && !text.trim().is_empty() {
        lines.push(Line::default());
    }
    lines.extend(markdown::render(text, width, p));
    if let Some(error) = message["error"].as_str() {
        lines.push(Line::from(Span::styled(truncate(&words.get(error), width), p.tone(Tone::Bad))));
    } else if message["status"].as_str() == Some("stopped") {
        lines.push(Line::from(Span::styled(words.get("cli.chat.stopped"), p.tone(Tone::Quiet))));
    }
    if message["status"].as_str().is_some_and(|s| s != "streaming")
        && let Some(meta) = meta(message, width, p, words)
    {
        lines.push(meta);
    }
    lines.push(Line::default());
    lines
}

/// One step on one line: how it ended, its tool and what it worked on.
pub fn step(tool: &Value, width: usize, p: &Palette) -> Line<'static> {
    let (mark, tone) = match thread::step_state(tool) {
        StepState::Done => ("✓", Tone::Quiet),
        StepState::Failed => ("✗", Tone::Bad),
        StepState::Declined => ("–", Tone::Warn),
        StepState::Running => ("…", Tone::Accent),
    };
    let line = markdown::clean(&thread::step_line(tool)).replace('\n', " ");
    Line::from(vec![
        Span::raw("  "),
        Span::styled(mark, p.tone(tone)),
        Span::raw(" "),
        Span::styled(truncate(&line, width.saturating_sub(4)), p.tone(Tone::Quiet)),
    ])
}

/// Who answered, how long it took and what it used, on one quiet line.
fn meta(message: &Value, width: usize, p: &Palette, words: &Words) -> Option<Line<'static>> {
    let mut parts: Vec<Span<'static>> = Vec::new();
    let mut add = |span: Span<'static>| {
        if !parts.is_empty() {
            parts.push(Span::styled(" · ", p.tone(Tone::Quiet)));
        }
        parts.push(span);
    };
    if let Some(vendor) = message["vendor"].as_str() {
        add(Span::styled(markdown::clean(&words.get(&format!("cli.vendor.{vendor}"))), p.vendor(vendor)));
    }
    if let Some(model) = message["model"].as_str() {
        add(Span::styled(markdown::clean(model), p.tone(Tone::Quiet)));
    }
    if let Some(took) = message["took"].as_u64() {
        add(Span::styled(format!("{:.1} s", took as f64 / 1000.0), p.tone(Tone::Quiet)));
    }
    let tokens = message["inputTokens"].as_u64().unwrap_or(0) + message["outputTokens"].as_u64().unwrap_or(0);
    if tokens > 0 {
        add(Span::styled(words.fill("cli.tokens", &[("n", &tokens.to_string())]), p.tone(Tone::Quiet)));
    }
    if parts.is_empty() {
        return None;
    }
    let all: Vec<_> = parts.iter().flat_map(|s| s.content.chars().map(|c| (c, s.style))).collect();
    Some(Line::from(spans(&markdown::truncate_cells(&all, width))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn thread() -> Thread {
        Thread::from_conversation(&json!({ "messages": [
            { "id": "m1", "role": "user", "text": "Fix the bug in the parser please" },
            { "id": "m2", "role": "assistant", "status": "done", "vendor": "claude", "model": "sonnet", "took": 2500,
              "tools": [{ "id": "t1", "name": "Read", "input": { "file_path": "src/a.rs" }, "ok": true }, { "id": "t2", "name": "Bash", "input": { "command": "cargo test" } }],
              "text": "# Done\n\n- one\n- two" },
        ] }))
    }

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>()).collect()
    }

    #[test]
    fn messages_read_as_the_person_then_the_steps_then_the_answer() {
        let mut transcript = Transcript::default();
        transcript.sync(Some(&thread()), 40, &Palette::plain(), &Words::new("en"));
        let lines = text(&transcript.window(0, 50));
        assert_eq!(
            lines,
            [
                "› Fix the bug in the parser please",
                "",
                "  ✓ Read src/a.rs",
                "  … Bash cargo test",
                "",
                "Done",
                "",
                "• one",
                "• two",
                "Claude Code · sonnet · 2.5 s",
                ""
            ]
        );
        assert_eq!(transcript.total(), lines.len());
        assert_eq!(text(&transcript.window(4, 3)), lines[4..7]);
    }

    #[test]
    fn a_message_is_drawn_again_only_when_it_changes() {
        let mut transcript = Transcript::default();
        let mut thread = thread();
        let words = Words::new("en");
        let p = Palette::plain();
        transcript.sync(Some(&thread), 40, &p, &words);
        let before = transcript.total();
        thread.take(json!({ "id": "m2", "role": "assistant", "status": "done", "text": "short" })).unwrap();
        transcript.sync(Some(&thread), 40, &p, &words);
        assert!(transcript.total() < before);
        transcript.sync(None, 40, &p, &words);
        assert_eq!(transcript.total(), 0);
    }

    #[test]
    fn old_steps_fold_and_texts_from_the_ai_lose_their_escapes() {
        let tools: Vec<Value> = (0..12)
            .map(|n| json!({ "id": format!("t{n}"), "name": "Read", "input": { "file_path": format!("f{n}") }, "ok": true }))
            .collect();
        let thread = Thread::from_conversation(
            &json!({ "messages": [{ "id": "m", "role": "assistant", "status": "done", "tools": tools, "text": "ok\u{1b}[2J" }] }),
        );
        let mut transcript = Transcript::default();
        transcript.sync(Some(&thread), 40, &Palette::plain(), &Words::new("en"));
        let lines = text(&transcript.window(0, 50));
        assert_eq!(lines[0], "  4 earlier steps");
        assert_eq!(lines.iter().filter(|l| l.contains("Read f")).count(), STEPS_SHOWN);
        assert!(lines.iter().all(|l| !l.contains('\u{1b}')));
    }
}
