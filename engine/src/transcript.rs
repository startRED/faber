//! Bounded, page-at-a-time persistence for the visible part of a child session.
use crate::store::{self, Disk, Fault, RealDisk};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MAX_PAGE_BYTES: usize = 64 * 1024;
pub const MAX_TEXT_CHUNK_BYTES: usize = 8 * 1024;
const MAX_ENTRIES_PER_PAGE: usize = 128;
const MAX_CONVERSATION_ID_BYTES: usize = 128;
const MAX_CALL_ID_BYTES: usize = 1024;
const PAGE_VERSION: u8 = 1;
const CALL_FILE: &str = "call.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptKind {
    Text,
    Tool,
    Result,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptEntry {
    pub at: usize,
    pub kind: TranscriptKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    pub text: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    version: u8,
    entries: Vec<TranscriptEntry>,
}

/// A helper call written beside its pages: what is needed to place it again if the saved record never got that far.
#[derive(Serialize, Deserialize)]
struct CallRecord {
    message: String,
    tool: crate::model::ToolUse,
}

/// Uses immutable, atomic page files so a crash can lose the current buffer, never corrupt an earlier page.
pub struct TranscriptStore {
    root: PathBuf,
    disk: Arc<dyn Disk>,
}

impl TranscriptStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_disk(root, Arc::new(RealDisk))
    }

    pub fn with_disk(root: impl Into<PathBuf>, disk: Arc<dyn Disk>) -> Self {
        Self { root: root.into(), disk }
    }

    /// Saves one immutable numbered page. The caller advances its page count only after success.
    pub fn append_page(&self, conversation: &str, call: &str, page: u64, entries: &[TranscriptEntry]) -> Result<(), Fault> {
        let bytes = encode_page(entries)?;
        validate_conversation_id(conversation)?;
        validate_call_id(call)?;
        ensure_directory(&self.root)?;
        let conversation_dir = self.root.join(conversation);
        ensure_directory(&conversation_dir)?;
        let directory = conversation_dir.join(call_component(call));
        ensure_directory(&directory)?;
        let path = directory.join(page_component(page));
        let temp = path.with_file_name(format!("{}.tmp", page_component(page)));
        reject_symlink(&temp)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Err(Fault::Corrupt),
            Ok(metadata) if metadata.is_file() => {
                return if self.read_page(conversation, call, page)?.as_deref() == Some(entries) { Ok(()) } else { Err(Fault::Write) };
            }
            Ok(_) => return Err(Fault::Corrupt),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(_) => return Err(Fault::Read),
        }
        store::replace(self.disk.as_ref(), &path, &bytes)
    }

    /// Reads only the requested page; missing is distinct from a corrupt or unreadable page.
    pub fn read_page(&self, conversation: &str, call: &str, page: u64) -> Result<Option<Vec<TranscriptEntry>>, Fault> {
        let Some(directory) = self.existing_call_dir(conversation, call)? else { return Ok(None) };
        let path = directory.join(page_component(page));
        let Some(bytes) = read_bounded(&path)? else { return Ok(None) };
        decode_page(&bytes).map(Some)
    }

    /// Keeps the child tool a `Tool` page describes beside that page. The conversation record is written at most every two
    /// seconds, so after a crash the record may not hold the step yet, and the page alone only names it.
    pub fn append_tool(&self, conversation: &str, call: &str, page: u64, tool: &crate::model::ToolUse) -> Result<(), Fault> {
        self.append_json(conversation, call, &format!("{page:020}.tool.json"), tool)
    }

    /// Keeps a helper call that the saved record may not hold yet, with the message it belongs to, so its pages can be placed
    /// again after a crash. Written once, with the call's first page.
    pub fn append_call(&self, conversation: &str, message: &str, tool: &crate::model::ToolUse) -> Result<(), Fault> {
        let record = CallRecord { message: message.to_string(), tool: tool.clone() };
        self.append_json(conversation, &tool.id, CALL_FILE, &record)
    }

    fn append_json(&self, conversation: &str, call: &str, name: &str, value: &impl Serialize) -> Result<(), Fault> {
        validate_conversation_id(conversation)?;
        validate_call_id(call)?;
        ensure_directory(&self.root)?;
        let conversation_dir = self.root.join(conversation);
        ensure_directory(&conversation_dir)?;
        let directory = conversation_dir.join(call_component(call));
        ensure_directory(&directory)?;
        let path = directory.join(name);
        reject_symlink(&path)?;
        reject_symlink(&path.with_file_name(format!("{name}.tmp")))?;
        let bytes = serde_json::to_vec(value).map_err(|_| Fault::Corrupt)?;
        store::replace(self.disk.as_ref(), &path, &bytes)
    }

    fn read_tool(&self, conversation: &str, call: &str, page: u64) -> Result<Option<crate::model::ToolUse>, Fault> {
        let Some(directory) = self.existing_call_dir(conversation, call)? else { return Ok(None) };
        let path = directory.join(format!("{page:020}.tool.json"));
        reject_symlink(&path)?;
        store::read_checked(&path)
    }

    /// Brings a conversation read from disk up to what reached disk after its record was saved. Pages and the files beside them
    /// are written before the record counts them, so after a crash they hold more than the record says: helper calls the record
    /// never got, the contiguous pages after each call's saved count, and the child steps those pages describe. A page that
    /// cannot be read back marks its call failed instead of hiding it. The normal case costs one directory listing for the
    /// conversation and one missing-page check per helper call.
    pub fn recover(&self, conversation: &str, messages: &mut [crate::model::Message]) {
        // A conversation no helper ever kept a page for has nothing to find.
        if matches!(existing_directory(&self.root), Ok(false)) || matches!(existing_directory(&self.root.join(conversation)), Ok(false)) {
            return;
        }
        let mut recorded = std::collections::HashSet::new();
        let mut helpers = false;
        for tool in messages.iter().flat_map(|message| message.tools.iter().flatten()) {
            helpers |= tool.transcript.is_some();
            recorded.insert(call_component(&tool.id));
        }
        for (message, tool) in self.unrecorded_calls(conversation, &recorded) {
            if let Some(message) = messages.iter_mut().find(|m| m.id == message) {
                message.tools.get_or_insert_default().push(tool);
                helpers = true;
            }
        }
        if !helpers {
            return;
        }
        for message in messages.iter_mut() {
            let Some(tools) = &mut message.tools else { continue };
            let mut index = 0;
            while index < tools.len() {
                let call = tools[index].id.clone();
                if let Some(status) = tools[index].transcript.clone() {
                    let (status, found) = self.pages_after(conversation, &call, status);
                    for child in found {
                        if !tools.iter().any(|tool| tool.id == child.id) {
                            tools.push(child);
                        }
                    }
                    tools[index].transcript = Some(status);
                }
                index += 1;
            }
        }
    }

    /// The helper calls with files on disk that the record does not hold, oldest first. One whose file cannot be read is left
    /// out: nothing can place it.
    fn unrecorded_calls(&self, conversation: &str, recorded: &std::collections::HashSet<String>) -> Vec<(String, crate::model::ToolUse)> {
        if !valid_conversation_id(conversation) || !matches!(existing_directory(&self.root), Ok(true)) {
            return vec![];
        }
        let directory = self.root.join(conversation);
        if !matches!(existing_directory(&directory), Ok(true)) {
            return vec![];
        }
        let Ok(entries) = fs::read_dir(&directory) else { return vec![] };
        let mut found: Vec<_> = entries
            .flatten()
            .filter(|entry| !recorded.contains(entry.file_name().to_string_lossy().as_ref()))
            .filter_map(|entry| {
                let path = entry.path().join(CALL_FILE);
                reject_symlink(&path).ok()?;
                let record: CallRecord = store::read_checked(&path).ok()??;
                (entry.file_name().to_string_lossy() == call_component(&record.tool.id)).then_some((record.message, record.tool))
            })
            .collect();
        found.sort_by(|a, b| (a.1.started, &a.1.id).cmp(&(b.1.started, &b.1.id)));
        found
    }

    /// Counts the contiguous pages written after `status.pages` and collects the child steps they describe.
    fn pages_after(
        &self,
        conversation: &str,
        call: &str,
        mut status: crate::model::Transcript,
    ) -> (crate::model::Transcript, Vec<crate::model::ToolUse>) {
        let mut children = vec![];
        loop {
            match self.read_page(conversation, call, status.pages) {
                Ok(Some(entries)) => {
                    for entry in entries.iter().filter(|entry| entry.kind == TranscriptKind::Tool) {
                        match self.read_tool(conversation, call, status.pages) {
                            Ok(Some(child))
                                if entry.tool.as_deref() == Some(child.id.as_str()) && child.parent.as_deref() == Some(call) =>
                            {
                                children.push(child)
                            }
                            Ok(None) => (), // Pages written before the separate tool file existed.
                            _ => status.failed = true,
                        }
                    }
                    status.pages += 1;
                }
                Ok(None) => break,
                Err(_) => {
                    status.failed = true;
                    break;
                }
            }
        }
        (status, children)
    }

    /// Copies only the explicitly retained calls, one page in memory at a time.
    pub fn copy_calls(&self, source: &str, destination: &str, calls: &[(String, u64)]) -> Result<(), Fault> {
        validate_conversation_id(source)?;
        validate_conversation_id(destination)?;
        for (call, pages) in calls {
            validate_call_id(call)?;
            for page in 0..*pages {
                let entries = self.read_page(source, call, page)?.ok_or(Fault::Corrupt)?;
                self.append_page(destination, call, page, &entries)?;
            }
        }
        Ok(())
    }

    /// Removes all child transcripts owned by this conversation.
    pub fn remove_conversation(&self, conversation: &str) -> Result<(), Fault> {
        validate_conversation_id(conversation)?;
        if !existing_directory(&self.root)? {
            return Ok(());
        }
        let path = self.root.join(conversation);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(Fault::Read),
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(Fault::Corrupt),
            Ok(_) => fs::remove_dir_all(path).map_err(|error| Fault::of_write(&error)),
        }
    }

    fn existing_call_dir(&self, conversation: &str, call: &str) -> Result<Option<PathBuf>, Fault> {
        validate_conversation_id(conversation)?;
        validate_call_id(call)?;
        if !existing_directory(&self.root)? {
            return Ok(None);
        }
        let conversation_dir = self.root.join(conversation);
        if !existing_directory(&conversation_dir)? {
            return Ok(None);
        }
        let call_dir = conversation_dir.join(call_component(call));
        if !existing_directory(&call_dir)? {
            return Ok(None);
        }
        Ok(Some(call_dir))
    }
}

/// Splits at UTF-8 boundaries so the serialized record can be checked before allocating its JSON bytes.
pub fn split_text(text: &str) -> impl Iterator<Item = &str> {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start == text.len() {
            return None;
        }
        let mut end = (start + MAX_TEXT_CHUNK_BYTES).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let part = &text[start..end];
        start = end;
        Some(part)
    })
}

fn encode_page(entries: &[TranscriptEntry]) -> Result<Vec<u8>, Fault> {
    if entries.is_empty() || entries.len() > MAX_ENTRIES_PER_PAGE || !entries.iter().all(valid_entry) {
        return Err(Fault::Corrupt);
    }
    let mut bound = 64usize;
    for entry in entries {
        bound = bound.saturating_add(128).saturating_add(json_string_bound(&entry.text));
        if let Some(tool) = &entry.tool {
            bound = bound.saturating_add(json_string_bound(tool));
        }
    }
    if bound > MAX_PAGE_BYTES {
        return Err(Fault::Corrupt);
    }
    let bytes = serde_json::to_vec(&Page { version: PAGE_VERSION, entries: entries.to_vec() }).map_err(|_| Fault::Corrupt)?;
    (bytes.len() <= MAX_PAGE_BYTES).then_some(bytes).ok_or(Fault::Corrupt)
}

fn decode_page(bytes: &[u8]) -> Result<Vec<TranscriptEntry>, Fault> {
    if bytes.len() > MAX_PAGE_BYTES {
        return Err(Fault::Corrupt);
    }
    let page: Page = serde_json::from_slice(bytes).map_err(|_| Fault::Corrupt)?;
    if page.version != PAGE_VERSION
        || page.entries.is_empty()
        || page.entries.len() > MAX_ENTRIES_PER_PAGE
        || !page.entries.iter().all(valid_entry)
    {
        return Err(Fault::Corrupt);
    }
    Ok(page.entries)
}

fn valid_entry(entry: &TranscriptEntry) -> bool {
    entry.text.len() <= MAX_TEXT_CHUNK_BYTES
        && match entry.kind {
            TranscriptKind::Text => entry.tool.is_none(),
            TranscriptKind::Tool | TranscriptKind::Result => entry.tool.as_deref().is_some_and(valid_call_id),
        }
}

fn json_string_bound(value: &str) -> usize {
    2usize.saturating_add(
        value
            .chars()
            .map(|ch| {
                if ch <= '\u{1f}' {
                    6
                } else if matches!(ch, '"' | '\\') {
                    2
                } else {
                    ch.len_utf8()
                }
            })
            .sum::<usize>(),
    )
}

fn validate_conversation_id(id: &str) -> Result<(), Fault> {
    valid_conversation_id(id).then_some(()).ok_or(Fault::Corrupt)
}

fn valid_conversation_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_CONVERSATION_ID_BYTES && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn validate_call_id(id: &str) -> Result<(), Fault> {
    valid_call_id(id).then_some(()).ok_or(Fault::Corrupt)
}

fn valid_call_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_CALL_ID_BYTES
}

fn call_component(id: &str) -> String {
    crate::hex(&Sha256::digest(id.as_bytes()))
}

fn page_component(page: u64) -> String {
    format!("{page:020}.json")
}

fn ensure_directory(path: &Path) -> Result<(), Fault> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => return Err(Fault::Corrupt),
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(_) => return Err(Fault::Read),
    }
    fs::create_dir_all(path).map_err(|error| Fault::of_write(&error))?;
    let metadata = fs::symlink_metadata(path).map_err(|_| Fault::Read)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() { Err(Fault::Corrupt) } else { Ok(()) }
}

fn existing_directory(path: &Path) -> Result<bool, Fault> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(Fault::Read),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(Fault::Corrupt),
        Ok(_) => Ok(true),
    }
}

fn reject_symlink(path: &Path) -> Result<(), Fault> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(Fault::Corrupt),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(Fault::Read),
    }
}

fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>, Fault> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Fault::Read),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_PAGE_BYTES as u64 {
        return Err(Fault::Corrupt);
    }
    let file = fs::File::open(path).map_err(|_| Fault::Read)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_PAGE_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|_| Fault::Read)?;
    if bytes.len() > MAX_PAGE_BYTES {
        return Err(Fault::Corrupt);
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> TranscriptEntry {
        TranscriptEntry { at: 0, kind: TranscriptKind::Text, tool: None, text: text.into() }
    }

    #[test]
    fn split_text_preserves_large_non_ascii_text_without_mid_codepoint_cuts() {
        let original = "é🙂語".repeat(12_000);
        let pieces: Vec<_> = split_text(&original).collect();
        assert!(pieces.len() > 1);
        assert!(pieces.iter().all(|piece| piece.len() <= MAX_TEXT_CHUNK_BYTES));
        assert_eq!(pieces.concat(), original);
        for piece in pieces {
            let record = entry(piece);
            assert_eq!(decode_page(&encode_page(std::slice::from_ref(&record)).unwrap()).unwrap(), vec![record]);
        }
    }

    #[test]
    fn page_encoding_round_trips_ordered_text_and_tool_records() {
        let entries = vec![
            TranscriptEntry { at: 0, kind: TranscriptKind::Text, tool: None, text: "before".into() },
            TranscriptEntry { at: 1, kind: TranscriptKind::Tool, tool: Some("tool/unsafe:id".into()), text: String::new() },
            TranscriptEntry { at: 1, kind: TranscriptKind::Result, tool: Some("tool/unsafe:id".into()), text: "full output".into() },
        ];
        assert_eq!(decode_page(&encode_page(&entries).unwrap()).unwrap(), entries);
    }

    #[test]
    fn page_rejects_excess_bytes_and_corrupt_data() {
        let too_large = vec![entry(&"x".repeat(MAX_PAGE_BYTES))];
        assert_eq!(encode_page(&too_large), Err(Fault::Corrupt));
        assert_eq!(decode_page(b"not json"), Err(Fault::Corrupt));
        assert_eq!(decode_page(&vec![b' '; MAX_PAGE_BYTES + 1]), Err(Fault::Corrupt));
    }

    #[test]
    fn path_components_reject_invalid_conversation_ids_and_hash_call_ids() {
        assert!(valid_conversation_id("c1"));
        assert!(valid_conversation_id(&"a".repeat(128)));
        assert!(!valid_conversation_id(""));
        assert!(!valid_conversation_id("../escape"));
        assert!(!valid_conversation_id(&"a".repeat(129)));
        assert!(!valid_call_id(""));
        assert!(!valid_call_id(&"x".repeat(MAX_CALL_ID_BYTES + 1)));

        let safe = call_component("../escape");
        assert_eq!(safe.len(), 64);
        assert!(safe.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(!safe.contains('/') && !safe.contains('\\'));
        assert!(page_component(u64::MAX).ends_with(".json"));

        let store = TranscriptStore::new("not-created");
        assert_eq!(store.read_page("../outside", "call", 0), Err(Fault::Corrupt));
        assert_eq!(store.read_page("c1", "", 0), Err(Fault::Corrupt));
        assert_eq!(store.append_page("../outside", "call", 0, &[entry("text")]), Err(Fault::Corrupt));
    }

    #[test]
    fn record_validation_keeps_kinds_and_tool_ids_consistent() {
        assert!(valid_entry(&entry("words")));
        assert!(!valid_entry(&TranscriptEntry { at: 0, kind: TranscriptKind::Tool, tool: None, text: String::new() }));
        assert!(!valid_entry(&TranscriptEntry { at: 0, kind: TranscriptKind::Text, tool: Some("tool".into()), text: "words".into() }));
    }
}
