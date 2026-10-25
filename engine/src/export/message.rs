//! Normalize each message on its own so Markdown scopes and references cannot consume later turns.

use super::Paths;
use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};
use std::cell::RefCell;

pub(super) fn markdown(text: &str, paths: &Paths) -> Result<String, &'static str> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut parser = Parser::new_ext(text, options).peekable();
    let mut events = Vec::new();
    let mut image_depth = 0;
    let mut image_number = 0;
    let mut in_code_block = false;
    let mut links = Vec::new();
    while let Some(event) = parser.next() {
        if let Event::Start(Tag::Image { dest_url, .. }) = &event {
            if image_depth == 0 {
                image_number += 1;
                events.push(Event::Text(picture_name(dest_url, image_number, paths).into()));
            }
            image_depth += 1;
            continue;
        }
        if event == Event::End(TagEnd::Image) {
            image_depth -= 1;
            continue;
        }
        if image_depth > 0 {
            continue;
        }
        events.push(match event {
            Event::Start(Tag::Link { dest_url, title, .. }) => {
                let destination = target(&dest_url, paths);
                if !clickable(&destination) {
                    let literal = data_name(&decode_path(&destination)).unwrap_or(destination);
                    links.push(Some(literal));
                    continue;
                }
                links.push(None);
                // Inline links cannot leave reference definitions for a later message to pick up.
                Event::Start(Tag::Link {
                    link_type: LinkType::Inline,
                    dest_url: destination.into(),
                    title: paths.redact(&title).into(),
                    id: "".into(),
                })
            }
            Event::End(TagEnd::Link) => match links.pop().flatten() {
                Some(destination) => Event::Text(format!(" ({destination})").into()),
                None => Event::End(TagEnd::Link),
            },
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                let kind = match kind {
                    CodeBlockKind::Fenced(info) => CodeBlockKind::Fenced(paths.redact(&info).into()),
                    CodeBlockKind::Indented => CodeBlockKind::Indented,
                };
                Event::Start(Tag::CodeBlock(kind))
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                Event::End(TagEnd::CodeBlock)
            }
            Event::Start(Tag::HtmlBlock) => Event::Start(Tag::Paragraph),
            Event::End(TagEnd::HtmlBlock) => Event::End(TagEnd::Paragraph),
            Event::Html(html) | Event::InlineHtml(html) => Event::Text(literal_html(&html, paths).into()),
            Event::Text(text) => {
                // The parser splits escapes and entities into adjacent text events; paths may span those splits.
                let mut joined = text.into_string();
                while let Some(Event::Text(part)) = parser.peek() {
                    joined.push_str(part);
                    parser.next();
                }
                let text = if in_code_block {
                    let mut code = paths.redact(&joined);
                    // An unfinished fence has no trailing newline; normalize it so serialization emits its closing fence.
                    if !code.ends_with('\n') {
                        code.push('\n');
                    }
                    code
                } else {
                    paths.redact(&joined)
                };
                Event::Text(text.into())
            }
            Event::Code(code) => Event::Code(paths.redact(&code).into()),
            event => event,
        });
    }
    let mut out = String::new();
    let formatting = pulldown_cmark_to_cmark::Options { list_token: '-', code_block_token_count: 3, ..Default::default() };
    pulldown_cmark_to_cmark::cmark_with_options(events, &mut out, options, formatting).map_err(|_| "error.export.read")?;
    Ok(out.trim_end().to_string())
}

pub(super) fn external_link(text: &str) -> bool {
    ["https://", "http://", "mailto:"].iter().any(|prefix| text.get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix)))
}

/// URL parsing removes surrounding C0/space and embedded ASCII tabs/newlines before recognizing the scheme.
fn browser_destination(text: &str) -> String {
    text.trim_matches(|ch: char| ch <= '\u{20}').replace(['\t', '\n', '\r'], "")
}

fn clickable(destination: &str) -> bool {
    let normalized = browser_destination(destination);
    let Some((scheme, _)) = normalized.split_once(':') else { return true };
    let mut chars = scheme.chars();
    let has_scheme = chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'));
    !has_scheme || ["http", "https", "mailto"].iter().any(|allowed| scheme.eq_ignore_ascii_case(allowed))
}

/// HTML's tokenizer resolves attribute entities before URI classification, including semicolonless numeric entities.
fn literal_html(text: &str, paths: &Paths) -> String {
    use html5ever::tokenizer::{BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer};
    use std::fmt::Write;
    struct Literal<'a>(RefCell<String>, &'a Paths);
    impl TokenSink for Literal<'_> {
        type Handle = ();
        fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
            let mut out = self.0.borrow_mut();
            match token {
                Token::CharacterTokens(text) => out.push_str(&self.1.redact(&text)),
                Token::TagToken(tag) if tag.kind == TagKind::StartTag && &*tag.name == "img" => {
                    let source = tag.attrs.iter().find(|attr| &*attr.name.local == "src").map(|attr| attr.value.as_ref()).unwrap_or("");
                    let source = source.replace(['\t', '\n', '\r'], "");
                    out.push_str(&picture_name(source.trim(), 1, self.1));
                }
                Token::TagToken(tag) => {
                    out.push('<');
                    if tag.kind == TagKind::EndTag {
                        out.push('/');
                    }
                    out.push_str(&tag.name);
                    for attr in tag.attrs {
                        let uri = attr.value.replace(['\t', '\n', '\r'], "");
                        let value = data_name(uri.trim()).unwrap_or_else(|| self.1.redact(&attr.value));
                        let _ = write!(out, " {}=\"{}\"", attr.name.local, value);
                    }
                    out.push('>');
                }
                Token::CommentToken(comment) => {
                    let _ = write!(out, "<!--{}-->", self.1.redact(&comment));
                }
                Token::DoctypeToken(doctype) => {
                    let _ = write!(out, "<!DOCTYPE {}>", doctype.name.as_deref().unwrap_or("html"));
                }
                Token::NullCharacterToken => out.push('\u{fffd}'),
                _ => {}
            }
            TokenSinkResult::Continue
        }
    }
    let input = BufferQueue::default();
    input.push_back(text.into());
    let parser = Tokenizer::new(Literal(RefCell::new(String::new()), paths), Default::default());
    let _ = parser.feed(&input);
    parser.end();
    parser.sink.0.into_inner()
}

fn data_name(text: &str) -> Option<String> {
    let text = browser_destination(text);
    if !text.get(..5).is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:")) {
        return None;
    }
    Some(if text.get(..11).is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:image/")) {
        embedded_picture_name(&text, 1)
    } else {
        "attachment-1.bin".into()
    })
}

fn target(destination: &str, paths: &Paths) -> String {
    if external_link(destination) {
        return destination.into();
    }
    let decoded = decode_path(destination);
    let redacted = paths.redact(&decoded);
    if redacted == decoded { destination.into() } else { redacted }
}

fn picture_name(destination: &str, number: usize, paths: &Paths) -> String {
    let decoded = browser_destination(&decode_path(destination));
    if decoded.get(..5).is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:")) {
        embedded_picture_name(&decoded, number)
    } else {
        target(destination, paths)
    }
}

fn embedded_picture_name(destination: &str, number: usize) -> String {
    let lower = destination.to_ascii_lowercase();
    let extension = if lower.starts_with("data:image/jpeg") {
        "jpg"
    } else if lower.starts_with("data:image/gif") {
        "gif"
    } else if lower.starts_with("data:image/webp") {
        "webp"
    } else {
        "png"
    };
    format!("image-{number}.{extension}")
}

fn decode_path(text: &str) -> String {
    let mut bytes = Vec::with_capacity(text.len());
    let raw = text.as_bytes();
    let mut at = 0;
    while at < raw.len() {
        if raw[at] == b'%'
            && at + 2 < raw.len()
            && let Ok(hex) = std::str::from_utf8(&raw[at + 1..at + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            bytes.push(byte);
            at += 3;
        } else {
            bytes.push(raw[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
