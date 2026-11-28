//! Selected messages and the small amount of metadata needed to describe the whole conversation.

use crate::model::{Area, Conversation, Message, Role, Status, WaitingOn, WrapUp};
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::borrow::Cow;
use std::fmt;
use std::io::Read;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosingBox {
    pub recap: String,
    #[serde(default)]
    pub done: bool,
    pub ask: Option<String>,
}

impl From<&WrapUp> for ClosingBox {
    fn from(w: &WrapUp) -> Self {
        Self { recap: w.recap.clone(), done: w.done, ask: w.ask.clone() }
    }
}

#[derive(Debug)]
pub struct Conclusion {
    pub id: String,
    pub at: u64,
    pub closing: ClosingBox,
}

#[derive(Debug)]
pub struct ReplyEnd {
    pub id: String,
    pub status: Option<Status>,
    pub closed: bool,
    pub error: bool,
    pub done: bool,
}

pub struct LatestReply {
    pub title: String,
    pub id: String,
    pub words: String,
    pub state: crate::conversation_state::State,
}

pub enum ProjectHistory {
    Latest(LatestReply),
    Complete(Box<Conversation>),
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ReplyIndex {
    position: usize,
    count: usize,
    id: String,
}

impl ReplyIndex {
    pub(crate) fn from_saved(c: &Conversation) -> Option<Self> {
        let position = c.messages.iter().rposition(|m| m.role == Role::Assistant)?;
        let reply = &c.messages[position];
        // Opening removes stale permissions and ends streaming from the saved process.
        if reply.status == Some(Status::Streaming) || reply.permissions.as_ref().is_some_and(|p| !p.is_empty()) {
            return None;
        }
        Some(Self { position, count: c.messages.len(), id: reply.id.clone() })
    }

    pub(crate) fn read(&self, text: &str, id: &str, project: &str, state: crate::conversation_state::State) -> Option<LatestReply> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Record<'a> {
            id: String,
            title: String,
            project_id: Option<String>,
            #[serde(default)]
            temporary: bool,
            #[serde(borrow)]
            messages: Vec<&'a RawValue>,
        }
        #[derive(Deserialize)]
        struct MessageRole {
            role: Role,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Words {
            id: String,
            role: Role,
            text: String,
            wrap_up: Option<WrapUp>,
            status: Option<Status>,
            permissions: Option<Vec<IgnoredAny>>,
        }

        let saved: Record<'_> = serde_json::from_str(text).ok()?;
        if saved.id != id || saved.project_id.as_deref() != Some(project) || saved.temporary || saved.messages.len() != self.count {
            return None;
        }
        let reply = Words::deserialize(*saved.messages.get(self.position)?).ok()?;
        if reply.id != self.id
            || reply.role != Role::Assistant
            || reply.status == Some(Status::Streaming)
            || reply.permissions.is_some_and(|p| !p.is_empty())
            || saved.messages[self.position + 1..]
                .iter()
                .any(|raw| MessageRole::deserialize(*raw).map_or(true, |message| message.role == Role::Assistant))
        {
            return None;
        }
        let words = match reply.wrap_up {
            Some(wrap) => wrap.with_reply(&reply.text),
            None => reply.text,
        };
        Some(LatestReply { title: saved.title, id: reply.id, words, state })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Facts {
    id: String,
    at: u64,
    role: Role,
    status: Option<Status>,
    #[serde(default)]
    closed: bool,
    // Only the presence matters here. The text is read for a selected message.
    error: Option<IgnoredAny>,
    wrap_up: Option<ClosingBox>,
}

impl From<&Message> for Facts {
    fn from(m: &Message) -> Self {
        Self {
            id: m.id.clone(),
            at: m.at,
            role: m.role,
            status: m.status,
            closed: m.closed,
            error: m.error.as_ref().map(|_| IgnoredAny),
            wrap_up: m.wrap_up.as_ref().map(ClosingBox::from),
        }
    }
}

/// A bounded map from a role's message positions to the conversation's global numbers.
#[derive(Debug, Default, Serialize)]
pub struct Headers {
    id: String,
    total: usize,
    matching: usize,
    headers: Vec<NumberedHeader>,
}

#[derive(Debug, Serialize)]
struct NumberedHeader {
    number: usize,
    #[serde(flatten)]
    header: Header,
}

#[derive(Debug, Deserialize, Serialize)]
struct Header {
    id: String,
    role: Role,
    at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<Status>,
}

impl Headers {
    pub fn of(c: &Conversation, start: usize, limit: usize, role: Option<Role>) -> Self {
        let mut page = Self { id: c.id.clone(), ..Self::default() };
        let range = start..start.saturating_add(limit);
        for m in &c.messages {
            page.consider(&range, role, m.role, || Header { id: m.id.clone(), role: m.role, at: m.at, status: m.status });
        }
        page
    }

    fn consider(&mut self, range: &std::ops::Range<usize>, role: Option<Role>, found: Role, header: impl FnOnce() -> Header) {
        self.total += 1;
        if role.is_some_and(|role| role != found) {
            return;
        }
        if range.contains(&self.matching) {
            self.headers.push(NumberedHeader { number: self.total, header: header() });
        }
        self.matching += 1;
    }
}

#[derive(Debug, Default)]
pub struct Page<'a> {
    pub id: String,
    pub title: String,
    pub temporary: bool,
    pub project_id: Option<String>,
    pub started_by: Option<String>,
    pub waiting_on: Option<WaitingOn>,
    pub count: usize,
    pub messages: Cow<'a, [Message]>,
    pub last_reply: Option<ReplyEnd>,
    pub conclusion: Option<Conclusion>,
    pub(crate) saved_version: Option<(u64, std::time::SystemTime)>,
    headers: Headers,
}

impl<'a> Page<'a> {
    pub fn in_project(&self, project: &str) -> bool {
        !self.temporary && self.project_id.as_deref() == Some(project)
    }

    pub(crate) fn into_headers(mut self) -> Headers {
        self.headers.id = self.id;
        self.headers
    }

    pub fn borrowed(c: &'a Conversation, start: usize, limit: usize) -> Self {
        let end = start.saturating_add(limit).min(c.messages.len());
        let mut page = Self {
            id: c.id.clone(),
            title: c.title.clone(),
            temporary: c.temporary,
            project_id: c.project_id.clone(),
            started_by: c.started_by.clone(),
            waiting_on: c.waiting_on.clone(),
            count: c.messages.len(),
            messages: Cow::Borrowed(c.messages.get(start..end).unwrap_or_default()),
            ..Self::default()
        };
        if let Some(m) = c.messages.last() {
            page.record(Facts::from(m));
        }
        if page.conclusion.is_none() {
            page.conclusion = c
                .messages
                .iter()
                .rev()
                .filter(|m| m.role == Role::Assistant)
                .find_map(|m| m.wrap_up.as_ref().map(|closing| Conclusion { id: m.id.clone(), at: m.at, closing: closing.into() }));
        }
        page
    }

    fn record(&mut self, facts: Facts) {
        self.last_reply = (facts.role == Role::Assistant).then(|| ReplyEnd {
            id: facts.id.clone(),
            status: facts.status,
            closed: facts.closed,
            error: facts.error.is_some(),
            done: facts.wrap_up.as_ref().is_some_and(|w| w.done),
        });
        if facts.role == Role::Assistant
            && let Some(closing) = facts.wrap_up
        {
            self.conclusion = Some(Conclusion { id: facts.id, at: facts.at, closing });
        }
    }
}

#[derive(Clone, Copy)]
struct Selection {
    start: usize,
    end: usize,
    read: ReadMessages,
}

#[derive(Clone, Copy)]
enum ReadMessages {
    Body,
    Headers(Option<Role>),
}

pub(crate) fn read(input: impl Read, start: usize, limit: usize) -> serde_json::Result<Page<'static>> {
    read_with(input, Selection { start, end: start.saturating_add(limit), read: ReadMessages::Body })
}

pub(crate) fn read_headers(input: impl Read, start: usize, limit: usize, role: Option<Role>) -> serde_json::Result<Page<'static>> {
    read_with(input, Selection { start, end: start.saturating_add(limit), read: ReadMessages::Headers(role) })
}

fn read_with(input: impl Read, selection: Selection) -> serde_json::Result<Page<'static>> {
    let mut json = serde_json::Deserializer::from_reader(input);
    let page = selection.deserialize(&mut json)?;
    json.end()?;
    Ok(page)
}

#[derive(Clone, Copy, Deserialize)]
#[serde(field_identifier, rename_all = "camelCase")]
enum Field {
    Id,
    Title,
    Area,
    CreatedAt,
    UpdatedAt,
    Temporary,
    ProjectId,
    StartedBy,
    WaitingOn,
    Messages,
    #[serde(other)]
    Other,
}

impl<'de> DeserializeSeed<'de> for Selection {
    type Value = Page<'static>;

    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Selection {
    type Value = Page<'static>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a saved conversation")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut page = Page::default();
        let mut seen = 0u16;
        while let Some(field) = map.next_key::<Field>()? {
            let bit = 1 << field as u16;
            if !matches!(field, Field::Other) && seen & bit != 0 {
                return Err(de::Error::custom("duplicate saved conversation field"));
            }
            seen |= bit;
            match field {
                Field::Id => page.id = map.next_value()?,
                Field::Title => page.title = map.next_value()?,
                Field::Area => {
                    let _: Area = map.next_value()?;
                }
                Field::CreatedAt | Field::UpdatedAt => {
                    let _: u64 = map.next_value()?;
                }
                Field::Temporary => page.temporary = map.next_value()?,
                Field::ProjectId => page.project_id = map.next_value()?,
                Field::StartedBy => page.started_by = map.next_value()?,
                Field::WaitingOn => page.waiting_on = map.next_value()?,
                Field::Messages => {
                    let history = map.next_value_seed(Messages(self))?;
                    page.messages = history.messages;
                    page.count = history.count;
                    page.last_reply = history.last_reply;
                    page.conclusion = history.conclusion;
                    page.headers = history.headers;
                }
                Field::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        for (field, name) in [
            (Field::Id, "id"),
            (Field::Title, "title"),
            (Field::Area, "area"),
            (Field::CreatedAt, "createdAt"),
            (Field::UpdatedAt, "updatedAt"),
            (Field::Messages, "messages"),
        ] {
            if seen & (1 << field as u16) == 0 {
                return Err(de::Error::missing_field(name));
            }
        }
        Ok(page)
    }
}

struct Messages(Selection);

impl<'de> DeserializeSeed<'de> for Messages {
    type Value = Page<'static>;

    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for Messages {
    type Value = Page<'static>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("saved messages")
    }

    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Self::Value, S::Error> {
        let mut page = Page::default();
        if let ReadMessages::Headers(role) = self.0.read {
            let range = self.0.start..self.0.end;
            while let Some(mut header) = seq.next_element::<Header>()? {
                if header.status == Some(Status::Streaming) {
                    header.status = Some(Status::Stopped);
                }
                page.headers.consider(&range, role, header.role, || header);
            }
            return Ok(page);
        }
        let mut selected = Vec::new();
        loop {
            let facts = if (self.0.start..self.0.end).contains(&page.count) {
                seq.next_element::<Message>()?.map(|m| {
                    let facts = Facts::from(&m);
                    selected.push(m);
                    facts
                })
            } else {
                seq.next_element::<Facts>()?
            };
            let Some(facts) = facts else { break };
            page.record(facts);
            page.count += 1;
        }
        page.messages = Cow::Owned(selected);
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, BufReader};

    #[test]
    fn reads_past_large_unselected_fields_and_keeps_the_fields_outside_the_page() {
        let prefix = br#"{"messages":[{"text":""#;
        let suffix = br#"","role":"assistant","id":"old","at":1,"wrapUp":{"recap":"Earlier answer","done":true}},
            {"id":"selected","role":"user","text":"Question","at":2},
            {"id":"last","role":"assistant","text":"Working","at":3,"status":"streaming"}],
            "projectId":"p1","title":"Old record","area":"project","id":"saved","createdAt":1,"updatedAt":3}"#;
        let input = prefix.as_slice().chain(io::repeat(b'x').take(16 * 1024 * 1024)).chain(suffix.as_slice());
        let page = read(BufReader::new(input), 1, 1).unwrap();
        assert_eq!((page.id.as_str(), page.title.as_str(), page.count), ("saved", "Old record", 3));
        assert!(page.in_project("p1"));
        assert_eq!(page.messages.len(), 1);
        assert_eq!(page.messages[0].text, "Question");
        let conclusion = page.conclusion.unwrap();
        assert_eq!((conclusion.id.as_str(), conclusion.at, conclusion.closing.recap.as_str()), ("old", 1, "Earlier answer"));
        assert!(conclusion.closing.done);
        let last = page.last_reply.unwrap();
        assert_eq!((last.id.as_str(), last.status), ("last", Some(Status::Streaming)));
    }

    #[test]
    fn header_selection_reads_the_record_id_after_ignoring_message_bodies() {
        let record = br#"{"messages":[
            {"id":"u","at":1,"role":"user","text":{"ignored":"body"}},
            {"id":"a","at":2,"role":"assistant","status":"streaming","tools":{"ignored":"journal"}}
        ],"area":"project","title":"Saved","id":"saved","createdAt":1,"updatedAt":2}"#;
        let page = read_headers(record.as_slice(), 0, 1, Some(Role::Assistant)).unwrap();
        assert_eq!(
            serde_json::to_value(page.into_headers()).unwrap(),
            serde_json::json!({"id":"saved","total":2,"matching":1,"headers":[
                {"number":2,"id":"a","role":"assistant","status":"stopped","at":2}
            ]})
        );
    }

    #[test]
    fn rejects_ambiguous_scope_and_truncated_records_even_after_the_selected_message() {
        let prefix = r#"{"messages":[{"id":"one","role":"user","text":"Private","at":1}],"area":"project",
            "id":"saved","title":"Old record","createdAt":1,"updatedAt":1,"projectId":"local","temporary":false"#;
        for suffix in [r#", "projectId":"foreign"}"#, r#", "temporary":true}"#, ",", "}[]"] {
            assert!(read(format!("{prefix}{suffix}").as_bytes(), 0, 1).is_err(), "{suffix}");
        }
        let page = read(format!("{prefix}}}").as_bytes(), usize::MAX, 10).unwrap();
        assert_eq!(page.count, 1);
        assert!(page.messages.is_empty());
        assert!(page.last_reply.is_none());
    }
}
