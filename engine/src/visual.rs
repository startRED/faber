//! Visual answers are derived from reply text; the conversation already keeps their history.

use crate::model::{Message, Role};
use html5ever::tokenizer::{BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VisualKind {
    Html,
    Svg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualVersion {
    pub message_id: String,
    /// Zero-based index among the message's fenced code blocks, including nonvisual ones.
    pub block: usize,
    /// UTF-16 offsets spanning the complete fenced block in the message's text.
    pub start: usize,
    pub end: usize,
    pub kind: VisualKind,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisualAnswer {
    pub id: String,
    /// None lets the screen name an untitled answer in its own language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub kind: VisualKind,
    pub versions: Vec<VisualVersion>,
}

/// Pure detection shared by both CLI drivers, in the conversation's message order.
pub fn detect(messages: &[Message]) -> Vec<VisualAnswer> {
    let mut answers: Vec<VisualAnswer> = Vec::new();
    let mut named = HashMap::new();
    for message in messages.iter().filter(|message| message.role == Role::Assistant) {
        let mut block = 0;
        let mut pending = None;
        for (event, range) in Parser::new(&message.text).into_offset_iter() {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language))) => {
                    let kind = match language.split_whitespace().next() {
                        Some("html") => Some(VisualKind::Html),
                        Some("svg") => Some(VisualKind::Svg),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        let raw = &message.text[range.clone()];
                        let marker = raw.as_bytes()[0];
                        let count = raw.bytes().take_while(|byte| *byte == marker).count();
                        let content_at = range.start + raw.find('\n').map_or(raw.len(), |at| at + 1);
                        pending = Some((kind, block, marker, count, content_at, range.clone(), String::new()));
                    }
                    block += 1;
                }
                Event::Text(text) => {
                    if let Some((_, _, _, _, content_end, _, source)) = &mut pending {
                        source.push_str(&text);
                        *content_end = range.end;
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    let Some((kind, block, marker, count, content_end, range, source)) = pending.take() else { continue };
                    // CommonMark accepts a fence that reaches EOF. A visual must wait for its actual closing fence.
                    let suffix = message.text[content_end..range.end].trim_end();
                    if suffix.bytes().rev().take_while(|byte| *byte == marker).count() < count {
                        continue;
                    }
                    let name = title(&source);
                    let start = message.text[..range.start].encode_utf16().count();
                    let end = start + message.text[range].encode_utf16().count();
                    let version = VisualVersion { message_id: message.id.clone(), block, start, end, kind, source };
                    if let Some(index) = name.as_ref().and_then(|name| named.get(name)).copied() {
                        let answer: &mut VisualAnswer = &mut answers[index];
                        answer.versions.push(version);
                    } else {
                        if let Some(name) = &name {
                            named.insert(name.clone(), answers.len());
                        }
                        answers.push(VisualAnswer { id: format!("{}:{block}", message.id), name, kind, versions: vec![version] });
                    }
                }
                _ => {}
            }
        }
    }
    answers
}

fn title(source: &str) -> Option<String> {
    #[derive(Default)]
    struct Title {
        text: String,
        inside: bool,
        ended: bool,
    }
    struct Sink(RefCell<Title>);
    impl TokenSink for Sink {
        type Handle = ();
        fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
            let mut title = self.0.borrow_mut();
            match token {
                Token::TagToken(tag) if tag.name.as_ref() == "title" => {
                    if tag.kind == TagKind::StartTag && !title.ended {
                        title.inside = true;
                        return TokenSinkResult::RawData(RawKind::Rcdata);
                    }
                    if tag.kind == TagKind::EndTag && title.inside {
                        title.inside = false;
                        title.ended = true;
                    }
                }
                Token::TagToken(tag) if tag.kind == TagKind::StartTag => match tag.name.as_ref() {
                    "script" => return TokenSinkResult::RawData(RawKind::ScriptData),
                    "style" | "textarea" | "xmp" | "iframe" | "noembed" | "noframes" => {
                        return TokenSinkResult::RawData(RawKind::Rawtext);
                    }
                    _ => {}
                },
                Token::CharacterTokens(text) if title.inside => title.text.push_str(&text),
                _ => {}
            }
            TokenSinkResult::Continue
        }
    }
    let input = BufferQueue::default();
    input.push_back(source.into());
    let parser = Tokenizer::new(Sink(RefCell::new(Title::default())), Default::default());
    let _ = parser.feed(&input);
    parser.end();
    let title = parser.sink.0.into_inner();
    let text = title.text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}
