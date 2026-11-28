//! Small typed visual answers. Invalid source remains an ordinary Markdown code block.

use crate::model::{Conversation, Message, Role};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const ITEMS: usize = 12;
const SOURCE_BYTES: usize = 8192;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumberItem {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartItem {
    pub label: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextItem {
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum PieceData {
    Number { title: String, items: Vec<NumberItem> },
    Table { title: String, columns: Vec<String>, rows: Vec<Vec<String>> },
    Chart { title: String, items: Vec<ChartItem> },
    Calculation { title: String, label: String, min: f64, max: f64, step: f64, value: f64, expression: String },
    Checklist { title: String, items: Vec<String> },
    Tabs { title: String, items: Vec<TextItem> },
    Steps { title: String, items: Vec<String> },
    Form { title: String, options: Vec<String> },
    Notice { title: String, text: String },
    Timeline { title: String, items: Vec<TextItem> },
}

fn text(s: &str, limit: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= limit && !s.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
}

fn count(n: usize) -> bool {
    (1..=ITEMS).contains(&n)
}

fn bounded(n: f64, limit: f64) -> bool {
    n.is_finite() && n.abs() <= limit
}

impl PieceData {
    pub fn parse(source: &str) -> Option<Self> {
        if source.len() > SOURCE_BYTES {
            return None;
        }
        let data: Self = serde_json::from_str(source).ok()?;
        data.valid().then_some(data)
    }

    fn valid(&self) -> bool {
        let title = match self {
            Self::Number { title, items } => {
                if !count(items.len()) || !items.iter().all(|item| text(&item.label, 120) && text(&item.value, 500)) {
                    return false;
                }
                title
            }
            Self::Table { title, columns, rows } => {
                if !(1..=6).contains(&columns.len())
                    || !columns.iter().all(|column| text(column, 120))
                    || !count(rows.len())
                    || !rows.iter().all(|row| row.len() == columns.len() && row.iter().all(|cell| text(cell, 500)))
                {
                    return false;
                }
                title
            }
            Self::Chart { title, items } => {
                if !count(items.len()) || !items.iter().all(|item| text(&item.label, 120) && bounded(item.value, 1e12)) {
                    return false;
                }
                title
            }
            Self::Calculation { title, label, min, max, step, value, expression } => {
                if !text(label, 120)
                    || !bounded(*min, 1e6)
                    || !bounded(*max, 1e6)
                    || min >= max
                    || !step.is_finite()
                    || *step <= 0.0
                    || *step > max - min
                    || (max - min) / step > 10000.0
                    || !self.accepts(&Value::from(*value))
                    || [*min, *value, *max].iter().any(|x| evaluate(expression, *x).is_none())
                {
                    return false;
                }
                title
            }
            Self::Checklist { title, items } | Self::Steps { title, items } => {
                if !count(items.len()) || !items.iter().all(|item| text(item, 500)) {
                    return false;
                }
                title
            }
            Self::Tabs { title, items } | Self::Timeline { title, items } => {
                if !count(items.len()) || !items.iter().all(|item| text(&item.label, 120) && text(&item.text, 500)) {
                    return false;
                }
                title
            }
            Self::Form { title, options } => {
                if !count(options.len()) || !options.iter().all(|option| text(option, 500)) {
                    return false;
                }
                title
            }
            Self::Notice { title, text: body } => {
                if !text(body, 500) {
                    return false;
                }
                title
            }
        };
        text(title, 120)
    }

    pub fn default_value(&self) -> Value {
        match self {
            Self::Checklist { items, .. } => Value::from(vec![false; items.len()]),
            Self::Calculation { value, .. } => Value::from(*value),
            Self::Tabs { .. } | Self::Steps { .. } | Self::Form { .. } => Value::from(0),
            _ => Value::Null,
        }
    }

    pub fn accepts(&self, value: &Value) -> bool {
        match self {
            Self::Checklist { items, .. } => {
                value.as_array().is_some_and(|values| values.len() == items.len() && values.iter().all(Value::is_boolean))
            }
            Self::Calculation { min, max, step, .. } => value.as_f64().is_some_and(|n| {
                if !n.is_finite() || n < *min || n > *max {
                    return false;
                }
                let increments = (n - min) / step;
                (increments - increments.round()).abs() <= 1e-7
            }),
            Self::Tabs { items, .. } => index(value, items.len()),
            Self::Steps { items, .. } => index(value, items.len()),
            Self::Form { options, .. } => index(value, options.len()),
            _ => false,
        }
    }
}

fn index(value: &Value, len: usize) -> bool {
    value.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n < len as f64)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualPiece {
    pub message_id: String,
    /// Ordinal among this message's faber-piece fences, including invalid ones.
    pub block: usize,
    /// UTF-16 offsets spanning the complete Markdown fence.
    pub start: usize,
    pub end: usize,
    pub data: PieceData,
    pub value: Value,
    #[serde(skip)]
    source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedValue {
    pub message_id: String,
    pub block: usize,
    /// Digest of the exact bounded JSON source: changed pieces cannot inherit an old choice.
    pub source: String,
    pub value: Value,
}

/// Detection is independent of which CLI supplied the assistant message.
pub fn detect(message: &Message, saved: &[SavedValue]) -> Vec<VisualPiece> {
    if message.role != Role::Assistant {
        return vec![];
    }
    let mut result = Vec::new();
    let mut block = 0;
    let mut depth = 0usize;
    let mut pending = None;
    for (event, range) in Parser::new(&message.text).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Tag::CodeBlock(CodeBlockKind::Fenced(language)) = tag
                    && language.as_ref() == "faber-piece"
                {
                    if depth == 0 {
                        let raw = &message.text[range.clone()];
                        let marker = raw.as_bytes()[0];
                        let count = raw.bytes().take_while(|byte| *byte == marker).count();
                        let content_at = range.start + raw.find('\n').map_or(raw.len(), |at| at + 1);
                        pending = Some((block, marker, count, content_at, range.clone(), String::new()));
                    }
                    block += 1;
                }
                depth += 1;
            }
            Event::Text(part) => {
                if let Some((_, _, _, end, _, source)) = &mut pending {
                    if source.len() <= SOURCE_BYTES {
                        // Keep an oversized sentinel instead of copying unbounded AI output.
                        if part.len() <= SOURCE_BYTES - source.len() {
                            source.push_str(&part);
                        } else {
                            *source = " ".repeat(SOURCE_BYTES + 1);
                        }
                    }
                    *end = range.end;
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                depth = depth.saturating_sub(1);
                let Some((ordinal, marker, count, content_end, range, source)) = pending.take() else { continue };
                let suffix = message.text[content_end..range.end].trim_end();
                if suffix.bytes().rev().take_while(|byte| *byte == marker).count() < count {
                    continue;
                }
                let Some(data) = PieceData::parse(&source) else { continue };
                let source = crate::hex(&Sha256::digest(source.as_bytes()));
                let value = saved
                    .iter()
                    .find(|entry| {
                        entry.message_id == message.id && entry.block == ordinal && entry.source == source && data.accepts(&entry.value)
                    })
                    .map_or_else(|| data.default_value(), |entry| entry.value.clone());
                let start = message.text[..range.start].encode_utf16().count();
                let end = start + message.text[range].encode_utf16().count();
                result.push(VisualPiece { message_id: message.id.clone(), block: ordinal, start, end, data, value, source });
                if result.len() == ITEMS {
                    break;
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    result
}

/// Prepares a bounded, validated update. The caller persists it or rolls back the saved values.
pub fn save_value(conversation: &mut Conversation, message_id: &str, block: usize, value: Value) -> Result<(), &'static str> {
    let message = conversation.messages.iter().find(|message| message.id == message_id).ok_or("error.messageGone")?;
    let pieces = detect(message, &[]);
    let piece = pieces.iter().find(|piece| piece.block == block).ok_or("error.messageGone")?;
    if !piece.data.accepts(&value) {
        return Err("error.messageGone");
    }
    conversation.piece_values.retain(|entry| {
        conversation.messages.iter().any(|message| message.id == entry.message_id)
            && (entry.message_id != message_id || pieces.iter().any(|piece| piece.block == entry.block && piece.source == entry.source))
            && !(entry.message_id == message_id && entry.block == block)
    });
    conversation.piece_values.push(SavedValue { message_id: message_id.into(), block, source: piece.source.clone(), value });
    Ok(())
}

/// Decimal literals, x, unary +/- and arithmetic only; no names, calls or runtime execution.
pub fn evaluate(expression: &str, x: f64) -> Option<f64> {
    if expression.len() > 128 || !bounded(x, 1e6) {
        return None;
    }
    let mut parser = Expression { bytes: expression.as_bytes(), at: 0, tokens: 0, x };
    let value = parser.sum(0)?;
    parser.whitespace();
    (parser.at == parser.bytes.len()).then_some(value)
}

struct Expression<'a> {
    bytes: &'a [u8],
    at: usize,
    tokens: usize,
    x: f64,
}

impl Expression<'_> {
    fn whitespace(&mut self) {
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn token(&mut self) -> Option<()> {
        self.tokens += 1;
        (self.tokens <= 64).then_some(())
    }

    fn sum(&mut self, depth: usize) -> Option<f64> {
        let mut value = self.product(depth)?;
        loop {
            self.whitespace();
            let Some(op @ (b'+' | b'-')) = self.bytes.get(self.at).copied() else { break };
            self.at += 1;
            self.token()?;
            let right = self.product(depth)?;
            value = if op == b'+' { value + right } else { value - right };
            if !bounded(value, 1e12) {
                return None;
            }
        }
        Some(value)
    }

    fn product(&mut self, depth: usize) -> Option<f64> {
        let mut value = self.atom(depth)?;
        loop {
            self.whitespace();
            let Some(op @ (b'*' | b'/')) = self.bytes.get(self.at).copied() else { break };
            self.at += 1;
            self.token()?;
            let right = self.atom(depth)?;
            value = if op == b'*' { value * right } else { value / right };
            if !bounded(value, 1e12) {
                return None;
            }
        }
        Some(value)
    }

    fn atom(&mut self, depth: usize) -> Option<f64> {
        if depth > 16 {
            return None;
        }
        self.whitespace();
        self.token()?;
        let first = *self.bytes.get(self.at)?;
        self.at += 1;
        let value = match first {
            b'+' => self.atom(depth + 1)?,
            b'-' => -self.atom(depth + 1)?,
            b'x' => self.x,
            b'(' => {
                let value = self.sum(depth + 1)?;
                self.whitespace();
                if self.bytes.get(self.at) != Some(&b')') {
                    return None;
                }
                self.at += 1;
                self.token()?;
                value
            }
            b'0'..=b'9' | b'.' => {
                let start = self.at - 1;
                while self.bytes.get(self.at).is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.') {
                    self.at += 1;
                }
                std::str::from_utf8(&self.bytes[start..self.at]).ok()?.parse::<f64>().ok()?
            }
            _ => return None,
        };
        bounded(value, 1e12).then_some(value)
    }
}
