//! A conversation's readable record, without CLI metadata or the work log.

use crate::model::{Conversation, Message, Role, Status, Vendor};
use crate::pieces::{PieceData, SavedValue, detect, evaluate};
use chrono::{DateTime, SecondsFormat, TimeZone};
use serde::Deserialize;
use std::fmt::Write;

mod message;
mod paths;
pub use paths::Paths;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Words {
    pub you: String,
    pub auto: String,
    pub attachments: String,
    pub files: String,
}

/// The caller supplies the time zone and known roots, keeping conversion pure and reproducible.
pub fn markdown(conversation: &Conversation, words: &Words, zone: &impl TimeZone, paths: &Paths) -> Result<String, &'static str> {
    let mut out = format!("# {}\n", heading(&paths.redact(&conversation.title)));
    for message in &conversation.messages {
        let who = if message.wake.is_some() {
            "Faber".into()
        } else if message.role == Role::User {
            conversation.user_name.as_deref().filter(|name| !name.trim().is_empty()).unwrap_or(&words.you).into()
        } else {
            model_name(message.model.as_deref(), message.vendor, &words.auto)
        };
        let at = i64::try_from(message.at).ok().and_then(DateTime::from_timestamp_millis).ok_or("error.export.read")?;
        let at = at.with_timezone(zone).to_rfc3339_opts(SecondsFormat::Secs, false);
        let _ = write!(out, "\n## {} · {at}\n\n", heading(&paths.redact(&who)));
        // activity_end is a UTF-16 boundary, just like the screen's final-answer boundary.
        let mut text = message.text.as_str();
        if message.role == Role::Assistant
            && message.status != Some(Status::Streaming)
            && message.tools.as_ref().is_some_and(|tools| !tools.is_empty())
            && let Some(end) = message.activity_end.filter(|end| *end > 0)
        {
            let mut units = 0;
            if let Some((byte, _)) = text.char_indices().find(|(_, ch)| {
                let found = units == end;
                units += ch.len_utf16();
                found
            }) && !text[byte..].trim().is_empty()
            {
                text = &text[byte..];
            }
        }
        let text = piece_snapshots(message, &conversation.piece_values, text, paths)?;
        out.push_str(&message::markdown(&text, paths)?);
        out.push('\n');
        let mut names: Vec<String> = message.attachments.iter().flatten().map(|name| heading(file_name(name))).collect();
        for (i, image) in message.images.iter().flatten().enumerate() {
            // Inline images never kept an original filename. Give each a name, never a data URL or bytes.
            let extension = match image.media_type.as_str() {
                "image/jpeg" => "jpg",
                "image/gif" => "gif",
                "image/webp" => "webp",
                _ => "png",
            };
            names.push(format!("image-{}.{extension}", i + 1));
        }
        if !names.is_empty() {
            let _ = writeln!(out, "\n{}: {}", heading(&words.attachments), names.join(", "));
        }
        let files: Vec<_> = message.files.iter().flatten().map(|file| heading(file_name(&file.path))).collect();
        if !files.is_empty() {
            let _ = writeln!(out, "\n{}: {}", heading(&words.files), files.join(", "));
        }
    }
    Ok(out)
}

fn piece_snapshots(message: &Message, saved: &[SavedValue], text: &str, paths: &Paths) -> Result<String, &'static str> {
    let offset = message.text.len() - text.len();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for piece in detect(message, saved) {
        let Some(start) = utf16_byte(&message.text, piece.start).and_then(|byte| byte.checked_sub(offset)) else { continue };
        let Some(end) = utf16_byte(&message.text, piece.end).and_then(|byte| byte.checked_sub(offset)) else { continue };
        let mut snapshot = serde_json::json!({"piece": piece.data});
        if !piece.value.is_null() {
            snapshot["value"] = piece.value.clone();
        }
        if let PieceData::Calculation { expression, .. } = &piece.data {
            snapshot["result"] = serde_json::json!(piece.value.as_f64().and_then(|value| evaluate(expression, value)));
        }
        redact_piece(&mut snapshot, paths);
        let snapshot = serde_json::to_string_pretty(&snapshot).map_err(|_| "error.export.read")?;
        out.push_str(&text[copied..start]);
        let _ = write!(out, "```json\n{snapshot}\n```\n");
        copied = end;
    }
    out.push_str(&text[copied..]);
    Ok(out)
}

fn utf16_byte(text: &str, boundary: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units == boundary {
            return Some(byte);
        }
        units += ch.len_utf16();
    }
    (units == boundary).then_some(text.len())
}

fn redact_piece(value: &mut serde_json::Value, paths: &Paths) {
    match value {
        serde_json::Value::String(text) => *text = paths.redact(text),
        serde_json::Value::Array(items) => items.iter_mut().for_each(|item| redact_piece(item, paths)),
        serde_json::Value::Object(fields) => fields.values_mut().for_each(|value| redact_piece(value, paths)),
        _ => {}
    }
}

fn heading(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            ch if ch.is_ascii_punctuation() => {
                out.push('\\');
                out.push(ch);
            }
            ch => out.push(ch),
        }
    }
    out
}

fn file_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("")
}

/// Matches prettyModel and the missing-model fallback in ui/Thread.tsx.
fn model_name(model: Option<&str>, vendor: Option<Vendor>, auto: &str) -> String {
    let Some(vendor) = vendor else { return auto.into() };
    let Some(model) = model.filter(|model| !model.is_empty() && *model != "codex") else { return capitalize(vendor.name()) };
    if let Some(rest) = model.strip_prefix("claude-") {
        let parts: Vec<_> = rest.split('-').collect();
        if parts.len() >= 3 && parts[1].parse::<u32>().is_ok() && parts[2].parse::<u32>().is_ok() {
            return format!("{} {}.{}", capitalize(parts[0]), parts[1], parts[2]);
        }
    }
    if let Some(rest) = model.strip_prefix("gpt-") {
        let (version, suffix) = rest.split_once('-').unwrap_or((rest, ""));
        if version.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
            return format!(
                "GPT-{version}{}",
                if suffix.is_empty() {
                    String::new()
                } else {
                    format!(" {}", suffix.split('-').map(capitalize).collect::<Vec<_>>().join(" "))
                }
            );
        }
    }
    capitalize(model)
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |ch| ch.to_uppercase().collect::<String>() + chars.as_str())
}
