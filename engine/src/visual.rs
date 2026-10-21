//! Visual answers are derived from reply text; the conversation already keeps their history.

use crate::model::{Message, Role, WebSearch};
use faber_winproc::rewind::Root;
use html5ever::tokenizer::{BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

/// Cached search belongs to the CLI; only the person's live-web choice grants a page network access.
pub fn network(web: Option<WebSearch>) -> bool {
    web.unwrap_or(WebSearch::Live) == WebSearch::Live
}

/// URL-level filtering, not a DNS firewall: the WebViews do not expose the resolved destination before connecting.
pub fn public_https(url: &reqwest::Url) -> bool {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(host) = url.host_str() else { return false };
    let host = host.trim_end_matches('.');
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(ip) => {
                !ip.is_private()
                    && !ip.is_loopback()
                    && !ip.is_link_local()
                    && !ip.is_unspecified()
                    && !ip.is_broadcast()
                    && !ip.is_multicast()
                    && ip.octets()[0] != 0
                    && ip.octets()[0] < 240
                    && !(ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
            }
            std::net::IpAddr::V6(ip) => {
                if let Some(ip) = ip.to_ipv4_mapped() {
                    return public_https(&reqwest::Url::parse(&format!("https://{ip}/")).expect("an IPv4 address is a URL host"));
                }
                let segments = ip.segments();
                !ip.is_loopback()
                    && !ip.is_unspecified()
                    && !ip.is_unique_local()
                    && !ip.is_unicast_link_local()
                    && !ip.is_multicast()
                    && segments[0] & 0xffc0 != 0xfec0
                    && segments[..6] != [0; 6]
                    && segments[..6] != [0x64, 0xff9b, 0, 0, 0, 0]
            }
        };
    }
    host.contains('.')
        && !["localhost", "local", "internal", "lan", "home", "tauri.localhost", "ipc.localhost"]
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VisualKind {
    Html,
    Svg,
    Mermaid,
}

/// Saved only after a successful editing tool, never reconstructed from the project on opening history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualFile {
    pub path: String,
    pub kind: VisualKind,
    pub source: Arc<str>,
}

pub const FILE_BYTES: usize = 256 * 1024;
pub const TURN_BYTES: usize = 1024 * 1024;
pub const TURN_FILES: usize = 12;
pub const CONVERSATION_BYTES: usize = 8 * 1024 * 1024;

pub fn pin(root: &std::path::Path) -> Option<Root> {
    let pinned = Root::open(root).ok()?;
    // Launch removes Windows' canonical prefix for CLI compatibility. Normalize only that spelling, never resolve root again.
    let same = if cfg!(windows) {
        crate::project::git::native_path(pinned.path()) == crate::project::git::native_path(root)
    } else {
        pinned.path() == root
    };
    same.then_some(pinned)
}

/// Read only the editing call's named outputs through the existing pinned, no-link file reader.
pub fn capture(messages: &mut [Message], root: &Root, id: &str) {
    let remaining = CONVERSATION_BYTES.saturating_sub(messages.iter().flat_map(|m| &m.visual_files).map(|f| f.source.len()).sum());
    let Some(message) = messages.iter_mut().rev().find(|m| m.tools.iter().flatten().any(|tool| tool.id == id)) else { return };
    capture_message(message, root, id, remaining);
}

fn capture_message(message: &mut Message, root: &Root, id: &str, mut remaining: usize) {
    let Some(tool) = message.tools.iter().flatten().find(|tool| tool.id == id) else { return };
    if message.role != Role::Assistant || tool.ok != Some(true) || tool.declined || tool.parent.is_some() {
        return;
    }
    let paths: Vec<String> = crate::nudge::edited(tool).into_iter().take(TURN_FILES).map(str::to_string).collect();
    let mut used: usize = message.visual_files.iter().map(|file| file.source.len()).sum();
    for raw in paths {
        if message.visual_files.len() >= TURN_FILES || used >= TURN_BYTES || remaining == 0 {
            break;
        }
        let Some(file) = crate::fileview::literal(root.path(), &raw) else { continue };
        let kind = match std::path::Path::new(&file.path).extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).as_deref() {
            Some("html" | "htm") => VisualKind::Html,
            Some("svg") => VisualKind::Svg,
            Some("mmd" | "mermaid") => VisualKind::Mermaid,
            _ => continue,
        };
        let limit = FILE_BYTES.min(TURN_BYTES - used).min(remaining);
        let Ok(input) = root.open_file(&file.path) else { continue };
        let mut bytes = Vec::new();
        if input.take((limit + 1) as u64).read_to_end(&mut bytes).is_err() {
            continue;
        }
        if bytes.is_empty() || bytes.len() > limit || bytes.contains(&0) {
            continue;
        }
        let Ok(source) = String::from_utf8(bytes) else { continue };
        if message.visual_files.iter().rev().find(|old| old.path == file.path).is_some_and(|old| old.source.as_ref() == source) {
            continue;
        }
        used += source.len();
        remaining -= source.len();
        message.visual_files.push(VisualFile { path: file.path, kind, source: source.into() });
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
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
    project(messages, true, None)
}

/// Discovery sends locators, not captured bytes. Only the requested version supplies its saved source.
pub fn list(messages: &[Message], selected: Option<(&str, usize)>) -> Vec<VisualAnswer> {
    project(messages, false, selected)
}

fn project(messages: &[Message], all: bool, selected: Option<(&str, usize)>) -> Vec<VisualAnswer> {
    let mut answers: Vec<VisualAnswer> = Vec::new();
    let mut named = HashMap::new();
    let mut files: HashMap<String, usize> = HashMap::new();
    for message in messages.iter().filter(|message| message.role == Role::Assistant) {
        let mut block = 0;
        let mut pending = None;
        for (event, range) in Parser::new(&message.text).into_offset_iter() {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language))) => {
                    let kind = match language.split_whitespace().next() {
                        Some("html") => Some(VisualKind::Html),
                        Some("svg") => Some(VisualKind::Svg),
                        Some("mermaid") => Some(VisualKind::Mermaid),
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
                    let name = if kind == VisualKind::Mermaid { None } else { title(&source) };
                    let start = message.text[..range.start].encode_utf16().count();
                    let end = start + message.text[range].encode_utf16().count();
                    let version = VisualVersion { message_id: message.id.clone(), block, start, end, kind, source, file: None };
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
        for (index, file) in message.visual_files.iter().enumerate() {
            let version = VisualVersion {
                message_id: message.id.clone(),
                block: index,
                start: 0,
                end: 0,
                kind: file.kind,
                source: if all || selected == Some((message.id.as_str(), index)) { file.source.to_string() } else { String::new() },
                file: Some(file.path.clone()),
            };
            if let Some(index) = files.get(&file.path).copied() {
                answers[index].versions.push(version);
            } else {
                files.insert(file.path.clone(), answers.len());
                answers.push(VisualAnswer {
                    id: format!("{}:file:{index}", message.id),
                    name: Some(file.path.clone()),
                    kind: file.kind,
                    versions: vec![version],
                });
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
