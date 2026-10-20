//! A bounded, volatile index of the person's words and final answers. No tool, thinking or attachment text is retained.

use crate::model::{Area, Conversation, Message, Role, Status};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

// Safety net for pathological vocabularies, independent of message age or length.
const WORD_LIMIT: usize = 20_000;
const HIT_LIMIT: usize = 30;
const PAYLOAD_BUDGET: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub struct Search {
    directory: PathBuf,
    state: Arc<Mutex<Index>>,
    wake: Arc<Condvar>,
}

#[derive(Default)]
struct Index {
    documents: BTreeMap<String, Arc<Document>>,
    touched: HashSet<String>,
    ready: bool,
    failed: bool,
    pending: BTreeSet<String>,
    stopped: bool,
    revision: u64,
    payload: usize,
}

struct Document {
    area: Area,
    updated: u64,
    words: Box<str>,
    complete: bool,
    revision: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub conversation_id: String,
    pub area: Area,
    pub title: String,
    pub message_index: usize,
    pub snippet: String,
    pub highlights: Vec<Highlight>,
}

/// UTF-16 offsets in the snippet, matching the screen's string offsets.
#[derive(Debug, Serialize)]
pub struct Highlight {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Serialize)]
pub struct Results {
    pub ready: bool,
    pub hits: Vec<Hit>,
}

/// Cheap Latin accent folding for the five screen languages; decomposed combining accents are ignored too.
fn fold(c: char) -> Option<char> {
    Some(match c {
        '\u{0300}'..='\u{036f}' => return None,
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        other => other,
    })
}

fn normalize(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).filter_map(fold).collect()
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    word_spans(text).map(|(_, word)| word)
}

fn word_spans(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.char_indices()
        .filter(|(_, c)| !c.is_alphanumeric())
        .map(|(byte, c)| (byte, c.len_utf8()))
        .chain(std::iter::once((text.len(), 0)))
        .scan(0, move |start, (end, separator)| {
            let word = (*start, &text[*start..end]);
            *start = end + separator;
            Some(word)
        })
        .filter(|(_, word)| (2..=32).contains(&word.chars().count()))
}

/// Binary search directly in the newline-delimited words; no per-word pointers or offsets are retained.
fn has_prefix(sorted: &str, prefix: &str) -> bool {
    let (mut low, mut high) = (0, sorted.len());
    while low < high {
        let middle = low + (high - low) / 2;
        let start = sorted.as_bytes()[low..middle].iter().rposition(|b| *b == b'\n').map_or(low, |at| low + at + 1);
        let end = sorted.as_bytes()[middle..high].iter().position(|b| *b == b'\n').map_or(high, |at| middle + at);
        let word = &sorted[start..end];
        if word.starts_with(prefix) {
            return true;
        }
        if word < prefix {
            low = end + 1;
        } else {
            high = start.saturating_sub(1);
        }
    }
    false
}

fn visible(message: &Message) -> &str {
    if message.wake.is_some()
        || (message.role == Role::Assistant && message.status == Some(Status::Streaming) && message.finished_at.is_none())
    {
        return "";
    }
    let start = if message.role == Role::Assistant { message.activity_end.unwrap_or(0) } else { 0 };
    let mut units = 0;
    let byte = message.text.char_indices().find_map(|(byte, c)| {
        let before = units;
        units += c.len_utf16();
        (before >= start).then_some(byte)
    });
    &message.text[byte.unwrap_or(message.text.len())..]
}

fn document(c: &Conversation) -> Document {
    let mut distinct = BTreeSet::new();
    let mut complete = true;
    for message in &c.messages {
        let normalized = normalize(visible(message));
        for word in words(&normalized) {
            if !distinct.contains(word) {
                distinct.insert(Box::<str>::from(word));
                if distinct.len() > WORD_LIMIT {
                    complete = false;
                    distinct.pop_last();
                }
            }
        }
    }
    Document {
        area: c.area,
        updated: c.updated_at,
        words: distinct.into_iter().collect::<Vec<_>>().join("\n").into_boxed_str(),
        complete,
        revision: 0,
    }
}

impl Index {
    fn insert(&mut self, id: String, doc: Document) {
        self.payload += doc.words.len();
        if let Some(old) = self.documents.insert(id, Arc::new(doc)) {
            self.payload -= old.words.len();
        }
        while self.payload > PAYLOAD_BUDGET {
            let id = self
                .documents
                .iter()
                .filter(|(_, doc)| !doc.words.is_empty())
                .min_by_key(|(_, doc)| doc.updated)
                .map(|(id, _)| id.clone())
                .unwrap();
            let old = &self.documents[&id];
            self.payload -= old.words.len();
            self.documents.insert(
                id,
                Arc::new(Document { words: "".into(), complete: false, area: old.area, updated: old.updated, revision: old.revision }),
            );
        }
    }
}

impl Search {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory, state: Arc::default(), wake: Arc::default() }
    }

    /// One engine-lifetime worker outside the engine lock, at the platform's background priority.
    pub fn start(&self) -> std::io::Result<std::sync::mpsc::Receiver<()>> {
        let search = self.clone();
        let (ready, built) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("conversation-index".into()).spawn(move || {
            #[cfg(any(windows, target_os = "macos"))]
            let priority = faber_winproc::background_thread();
            // Linux nice values belong to the calling thread, unlike the process-wide POSIX interpretation elsewhere.
            #[cfg(target_os = "linux")]
            let priority = rustix::process::nice(19);
            #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
            if priority.is_err() {
                search.state.lock().unwrap_or_else(|e| e.into_inner()).failed = true;
                return;
            }
            search.build();
            let _ = ready.send(());
            loop {
                let mut index = search.state.lock().unwrap_or_else(|e| e.into_inner());
                while index.pending.is_empty() && !index.stopped {
                    index = search.wake.wait(index).unwrap_or_else(|e| e.into_inner());
                }
                if index.stopped {
                    return;
                }
                let id = index.pending.pop_first().unwrap();
                let revision = index.documents[&id].revision;
                drop(index);
                let doc = crate::store::read_checked::<Conversation>(&search.directory.join(format!("{id}.json")))
                    .ok()
                    .flatten()
                    .filter(|c| c.id == id)
                    .map(|c| document(&c));
                let mut index = search.state.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(mut doc) = doc
                    && index.documents.get(&id).is_some_and(|d| d.revision == revision)
                {
                    doc.revision = revision;
                    index.insert(id, doc);
                }
                search.wake.notify_all();
            }
        });
        if spawned.is_err() {
            self.state.lock().unwrap_or_else(|e| e.into_inner()).failed = true;
        }
        spawned.map(|_| built)
    }

    fn build(&self) {
        let Ok(files) = std::fs::read_dir(&self.directory) else {
            self.state.lock().unwrap_or_else(|e| e.into_inner()).failed = true;
            return;
        };
        for file in files.flatten().filter(|f| f.path().extension().is_some_and(|e| e == "json")) {
            if self.state.lock().unwrap_or_else(|e| e.into_inner()).stopped {
                return;
            }
            let Some(c) = crate::store::read_checked::<Conversation>(&file.path()).ok().flatten() else { continue };
            let doc = document(&c);
            let mut index = self.state.lock().unwrap_or_else(|e| e.into_inner());
            // A save or removal during the scan always wins over the older record read from disk.
            if !index.touched.contains(&c.id) {
                index.insert(c.id, doc);
            }
        }
        let mut index = self.state.lock().unwrap_or_else(|e| e.into_inner());
        index.touched = HashSet::new();
        index.ready = true;
        self.wake.notify_all();
    }

    pub(crate) fn saved(&self, c: &Conversation) {
        if c.id.is_empty() || !c.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return;
        }
        let mut index = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !index.ready {
            index.touched.insert(c.id.clone());
        }
        index.revision += 1;
        let revision = index.revision;
        // Only metadata crosses the save path. Pending records remain searchable through the file fallback.
        if let Some(old) = index
            .documents
            .insert(c.id.clone(), Arc::new(Document { area: c.area, updated: c.updated_at, words: "".into(), complete: false, revision }))
        {
            index.payload -= old.words.len();
        }
        index.pending.insert(c.id.clone());
        self.wake.notify_one();
    }

    pub(crate) fn removed(&self, id: &str) {
        let mut index = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !index.ready {
            index.touched.insert(id.to_string());
        }
        index.pending.remove(id);
        if let Some(old) = index.documents.remove(id) {
            index.payload -= old.words.len();
        }
    }

    pub(crate) fn stop(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).stopped = true;
        self.wake.notify_all();
    }

    /// Called on a blocking worker, never on an async worker or under the engine lock. Only matching files are read,
    /// one at a time, for original spelling in snippets; that text is not held in the index.
    pub fn query(&self, query: &str, area: Area) -> Result<Results, &'static str> {
        self.query_cancellable(query, area, &AtomicBool::new(false))
    }

    pub fn query_cancellable(&self, query: &str, area: Area, cancelled: &AtomicBool) -> Result<Results, &'static str> {
        self.query_until(query, area, || cancelled.load(Ordering::Relaxed))
    }

    fn query_until(&self, query: &str, area: Area, mut cancelled: impl FnMut() -> bool) -> Result<Results, &'static str> {
        let normalized = normalize(&query.chars().take(256).collect::<String>());
        let words: Vec<String> = words(&normalized).map(str::to_string).collect();
        let index = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if index.failed {
            return Err("error.store.read");
        }
        let ready = index.ready;
        if words.is_empty() {
            return Ok(Results { ready, hits: vec![] });
        }
        let mut docs: Vec<_> =
            index.documents.iter().filter(|(_, doc)| doc.area == area).map(|(id, doc)| (id.clone(), doc.clone())).collect();
        drop(index);
        // Share immutable documents while scanning, so saving a reply never waits for a search.
        docs.sort_by(|(a, left), (b, right)| right.updated.cmp(&left.updated).then_with(|| a.cmp(b)));
        let mut hits = Vec::new();
        for (id, doc) in docs {
            if cancelled() {
                break;
            }
            if doc.complete && !words.iter().all(|word| has_prefix(&doc.words, word)) {
                continue;
            }
            // Use the same id validation as the store before forming a file path.
            if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                continue;
            }
            let Some(c) = crate::store::read_checked::<Conversation>(&self.directory.join(format!("{id}.json"))).ok().flatten() else {
                continue;
            };
            if c.id != id || c.area != area {
                continue;
            }
            for (message_index, message) in c.messages.iter().enumerate() {
                let Some((snippet, highlights)) = snippet(visible(message), &words) else { continue };
                hits.push(Hit { conversation_id: id, area, title: c.title, message_index, snippet, highlights });
                break;
            }
            if hits.len() == HIT_LIMIT {
                break;
            }
        }
        Ok(Results { ready, hits })
    }
}

fn snippet(text: &str, words: &[String]) -> Option<(String, Vec<Highlight>)> {
    let normalized = normalize(text);
    let mut first = usize::MAX;
    for word in words {
        first = first.min(word_spans(&normalized).find(|(_, token)| token.starts_with(word))?.0);
    }
    let mut normalized_byte = 0;
    let first_byte = text.char_indices().find_map(|(byte, c)| {
        normalized_byte += c.to_lowercase().filter_map(fold).map(char::len_utf8).sum::<usize>();
        (normalized_byte > first).then_some(byte)
    })?;
    let byte_start = text[..first_byte].char_indices().rev().nth(35).map_or(0, |(byte, _)| byte);
    let byte_end = text[byte_start..].char_indices().nth(80).map_or(text.len(), |(byte, _)| byte_start + byte);
    // Only the snippet's small window needs an offset map, even for an enormous answer.
    let mut offsets = Vec::new();
    let mut normalized_byte = text[..byte_start].chars().flat_map(char::to_lowercase).filter_map(fold).map(char::len_utf8).sum::<usize>();
    for (byte, c) in text[byte_start..byte_end].char_indices() {
        for folded in c.to_lowercase().filter_map(fold) {
            offsets.push((normalized_byte, byte_start + byte));
            normalized_byte += folded.len_utf8();
        }
    }
    offsets.push((normalized_byte, byte_end));
    let prefix = if byte_start > 0 { "…" } else { "" };
    let prefix_units = usize::from(byte_start > 0);
    let suffix = if byte_end < text.len() { "…" } else { "" };
    let snippet = format!("{prefix}{}{suffix}", &text[byte_start..byte_end]);
    let mut ranges = Vec::new();
    for word in words {
        for (a, _) in word_spans(&normalized).filter(|(_, token)| token.starts_with(word)) {
            let b = a + word.len();
            let Some(from) = offsets.binary_search_by_key(&a, |(n, _)| *n).ok().map(|i| offsets[i].1) else { continue };
            let to = offsets.binary_search_by_key(&b, |(n, _)| *n).ok().map_or(byte_end, |i| offsets[i].1);
            if from >= byte_start && from < byte_end {
                ranges.push((
                    prefix_units + text[byte_start..from].encode_utf16().count(),
                    prefix_units + text[byte_start..to.min(byte_end)].encode_utf16().count(),
                ));
            }
        }
    }
    ranges.sort_unstable();
    let mut highlights: Vec<Highlight> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = highlights.last_mut().filter(|last| last.end >= start) {
            last.end = last.end.max(end);
        } else {
            highlights.push(Highlight { start, end });
        }
    }
    Some((snippet, highlights))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Wake;
    use crate::store::Store;
    use serde_json::json;

    fn conversation(id: &str, texts: &[&str]) -> Conversation {
        serde_json::from_value(json!({
            "id": id, "title": "A title", "area": "chat", "createdAt": 1, "updatedAt": 1,
            "messages": texts.iter().enumerate().map(|(i, text)| Message::new(i.to_string(), Role::User, (*text).into(), 1)).collect::<Vec<_>>()
        })).unwrap()
    }

    #[test]
    fn search_only_indexes_the_person_and_final_answer_and_folds_accents() {
        let mut message = Message::new("answer".into(), Role::Assistant, "🦀 narration. CAFÉ com açúcar".into(), 1);
        message.status = Some(Status::Done);
        message.activity_end = Some("🦀 narration. ".encode_utf16().count());
        assert_eq!(normalize(visible(&message)), "cafe com acucar");
        let (snippet, highlights) = snippet(visible(&message), &["cafe".into(), "acucar".into()]).unwrap();
        assert_eq!(snippet, "CAFÉ com açúcar");
        assert_eq!(highlights.iter().map(|h| (h.start, h.end)).collect::<Vec<_>>(), [(0, 4), (9, 15)]);
        message.status = Some(Status::Streaming);
        assert_eq!(visible(&message), "");
        message.finished_at = Some(2);
        assert!(!visible(&message).is_empty());
        message.wake = Some(Wake::of("wake.keepGoing"));
        assert_eq!(visible(&message), "");
        assert_eq!(normalize("AÇÃO ÜBER déjà"), "acao uber deja");
    }

    #[test]
    fn search_snippets_are_bounded_unicode_safe_and_mark_the_first_word_anywhere() {
        let text = format!("{}🦀 Café cafe\u{301} CAFÉ {}", "before ".repeat(20), "after ".repeat(20));
        let (snippet, highlights) = snippet(&text, &["cafe".into()]).unwrap();
        assert!(snippet.chars().count() <= 82);
        let units: Vec<_> = snippet.encode_utf16().collect();
        assert_eq!(highlights.len(), 3);
        for h in highlights {
            assert_eq!(normalize(&String::from_utf16(&units[h.start..h.end]).unwrap()).trim(), "cafe");
        }
        assert!(snippet.starts_with('…') && snippet.ends_with('…'));
        assert!(super::snippet("one word", &["one".into(), "absent".into()]).is_none());
    }

    #[test]
    fn search_requires_all_prefixes_in_one_message_and_returns_thirty_conversations_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        store.save(&conversation("split", &["café", "words"]));
        assert!(store.search().query("cafe words", Area::Chat).unwrap().hits.is_empty());
        store.save(&conversation("prefix", &["decafewords", "CAFÉ, wordsmith", "café words"]));
        let result = store.search().query("CAFE,word", Area::Chat).unwrap();
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].message_index, 1);
        assert_eq!(result.hits[0].snippet, "CAFÉ, wordsmith");
        assert!(store.search().query("afe", Area::Chat).unwrap().hits.is_empty());
        assert!(store.search().query("cafe", Area::Project).unwrap().hits.is_empty());
        for i in 0..60 {
            let mut c = conversation(&format!("many-{i}"), &["needle"; 2]);
            c.updated_at = i;
            store.save(&c);
            if i >= 50 {
                // Stale candidates must not use up the hit budget.
                std::fs::write(
                    dir.path().join(format!("conversations/{}.json", c.id)),
                    serde_json::to_vec(&conversation(&c.id, &["edited"])).unwrap(),
                )
                .unwrap();
            }
        }
        let result = store.search().query("needle", Area::Chat).unwrap();
        assert_eq!(result.hits.len(), HIT_LIMIT);
        assert_eq!(result.hits[0].conversation_id, "many-49");
        assert_eq!(result.hits[29].conversation_id, "many-20");
        assert!(result.hits.iter().all(|h| h.message_index == 0));
        assert!(store.search().query("  ", Area::Chat).unwrap().hits.is_empty());
    }

    #[test]
    fn search_words_are_sorted_unique_bounded_and_binary_prefixes_match_a_linear_scan() {
        let text = format!("Café café,99 a {} {} end", "b".repeat(32), "c".repeat(33));
        let doc = document(&conversation("words", &[&text]));
        assert_eq!(&*doc.words, format!("99\n{}\ncafe\nend", "b".repeat(32)));
        let vocabulary = ["12", "abc", "abcd", "az", "cafe", "café", "end", "über", "世界", "世界中", "中文"];
        let sorted = vocabulary.join("\n");
        for token in vocabulary.into_iter().chain(["", "00", "bb", "z", "é", "文", "𐀀"]) {
            for end in token.char_indices().map(|(at, _)| at).chain(std::iter::once(token.len())) {
                let prefix = &token[..end];
                assert_eq!(has_prefix(&sorted, prefix), vocabulary.iter().any(|word| word.starts_with(prefix)), "{prefix}");
            }
        }
        assert!(!has_prefix("", "abc"));
        let all: Vec<_> = (0..WORD_LIMIT + 10).map(|i| format!("word{i:05}")).collect();
        let forward = document(&conversation("limit", &[&all.join(" ")]));
        let backward = document(&conversation("limit", &[&all.into_iter().rev().collect::<Vec<_>>().join(" ")]));
        assert_eq!(forward.words, backward.words, "the safety net never depends on recency");
        assert_eq!(words(&forward.words).count(), WORD_LIMIT);
    }

    #[test]
    fn search_saves_and_removals_during_startup_win_over_the_scan() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        store.save(&conversation("updated", &["old"]));
        store.save(&conversation("removed", &["old"]));
        let search = Search::new(dir.path().join("conversations"));
        store.save(&conversation("updated", &["new"]));
        search.saved(&conversation("updated", &["new"]));
        search.removed("removed");
        search.build();
        let index = search.state.lock().unwrap();
        assert!(index.ready && index.touched.is_empty());
        assert!(!index.documents.contains_key("removed"));
        assert!(!index.documents["updated"].complete);
        drop(index);
        assert_eq!(search.query("new", Area::Chat).unwrap().hits.len(), 1);
        assert!(search.query("old", Area::Chat).unwrap().hits.is_empty());
    }

    #[test]
    fn search_save_only_queues_metadata_and_coalesces_without_scanning_under_the_engine_lock() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let search = store.search();
        let c = conversation("large", &[&"many words ".repeat(100_000)]);
        for _ in 0..10 {
            search.saved(&c);
        }
        let index = search.state.lock().unwrap();
        assert_eq!(index.pending.len(), 1);
        assert_eq!(index.payload, 0);
        assert!(index.documents["large"].words.is_empty(), "saving must never fold message text");
        drop(index);
        store.save(&conversation("large", &["latest"]));
        search.start().unwrap().recv().unwrap();
        let mut index = search.state.lock().unwrap();
        while !index.documents["large"].complete {
            let (next, timeout) = search.wake.wait_timeout(index, std::time::Duration::from_secs(10)).unwrap();
            assert!(!timeout.timed_out(), "background worker did not update the saved conversation");
            index = next;
        }
        assert_eq!(&*index.documents["large"].words, "latest");
        drop(index);
        store.save(&conversation("large", &["afterstartup"]));
        let mut index = search.state.lock().unwrap();
        while !index.documents["large"].complete {
            let (next, timeout) = search.wake.wait_timeout(index, std::time::Duration::from_secs(10)).unwrap();
            assert!(!timeout.timed_out(), "the worker must continue accepting saves after startup");
            index = next;
        }
        assert_eq!(&*index.documents["large"].words, "afterstartup");
        drop(index);
        drop(store);
        assert!(search.state.lock().unwrap().stopped, "the worker must stop with its store");
    }

    #[test]
    fn search_total_budget_evicts_oldest_words_without_losing_coverage() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        for i in 0..28 {
            let text = (0..WORD_LIMIT).map(|word| format!("word{i:08}{word:020}")).collect::<Vec<_>>().join(" ");
            let mut c = conversation(&format!("diverse-{i}"), &[&text]);
            c.updated_at = i;
            std::fs::write(dir.path().join(format!("conversations/{}.json", c.id)), serde_json::to_vec(&c).unwrap()).unwrap();
        }
        let search = store.search();
        search.build();
        let index = search.state.lock().unwrap();
        assert!(index.payload <= PAYLOAD_BUDGET);
        assert!(!index.documents["diverse-0"].complete);
        assert!(index.documents["diverse-27"].complete);
        println!(
            "SEARCH BUDGET: {} B retained from {} B diverse word payload; oldest fallback searchable",
            index.payload,
            28 * (WORD_LIMIT * 33 - 1)
        );
        drop(index);
        assert_eq!(search.query("word00000000", Area::Chat).unwrap().hits[0].conversation_id, "diverse-0");
        let overflow = (0..WORD_LIMIT + 10).map(|i| format!("token{i:05}")).collect::<Vec<_>>().join(" ");
        store.save(&conversation("overflow", &[&overflow]));
        search.build();
        assert_eq!(search.query("token20009", Area::Chat).unwrap().hits[0].conversation_id, "overflow");
    }

    #[test]
    fn search_cancelled_scan_stops_before_opening_the_next_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        store.save(&conversation("cancelled", &["needle"]));
        let cancelled = AtomicBool::new(true);
        assert!(store.search().query_cancellable("needle", Area::Chat, &cancelled).unwrap().hits.is_empty());
        cancelled.store(false, Ordering::Relaxed);
        assert_eq!(store.search().query_cancellable("needle", Area::Chat, &cancelled).unwrap().hits.len(), 1);
        store.save(&conversation("next", &["needle"]));
        let mut files = 0;
        let result = store
            .search()
            .query_until("needle", Area::Chat, || {
                files += 1;
                files > 1
            })
            .unwrap();
        assert_eq!(files, 2, "cancellation is checked again before the next file");
        assert_eq!(result.hits.len(), 1, "a superseded scan must stop after its current file");
    }

    #[test]
    #[ignore = "isolated startup memory and processor measurement; run with --ignored --nocapture --test-threads=1"]
    fn search_index_startup_cost_for_one_thousand_messages() {
        measure(100, 10, false);
    }

    #[test]
    #[ignore = "isolated long-conversation measurement"]
    fn search_index_startup_cost_for_long_conversations() {
        measure(20, 200, false);
    }

    #[test]
    #[ignore = "isolated 1000-conversation search measurement"]
    fn search_index_startup_cost_for_one_thousand_conversations() {
        measure(1000, 1, false);
    }

    #[test]
    #[ignore = "isolated diverse-vocabulary budget measurement"]
    fn search_index_startup_cost_for_diverse_vocabularies() {
        measure(1000, 1, true);
    }

    fn measure(conversations: usize, messages: usize, diverse: bool) {
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let text = "Searchable final answer with café and words. ".repeat(96);
        for i in 0..conversations {
            let diverse_text =
                if diverse { (0..1000).map(|word| format!("word{i:08}{word:020}")).collect::<Vec<_>>().join(" ") } else { text.clone() };
            let mut c = conversation(&format!("conversation-{i}"), &vec![diverse_text.as_str(); messages]);
            c.updated_at = i as u64;
            // Write fixtures directly, so the measurement starts with an empty index.
            std::fs::write(dir.path().join(format!("conversations/{}.json", c.id)), serde_json::to_vec(&c).unwrap()).unwrap();
        }
        let pid = Pid::from_u32(std::process::id());
        let mut system = System::new();
        let refresh = ProcessRefreshKind::nothing().with_memory().with_cpu();
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh);
        let before = system.process(pid).unwrap();
        let (memory_before, cpu_before) = (before.memory(), before.accumulated_cpu_time());
        let started = std::time::Instant::now();
        store.search().start().unwrap().recv().unwrap();
        let elapsed = started.elapsed();
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh);
        let after = system.process(pid).unwrap();
        let search = store.search();
        let index = search.state.lock().unwrap();
        let text_bytes: usize = index.documents.values().map(|d| d.words.len()).sum();
        let containers = index.documents.len() * std::mem::size_of::<Box<str>>();
        let vector_bytes: usize = index
            .documents
            .values()
            .map(|d| {
                std::mem::size_of::<Vec<Box<str>>>()
                    + words(&d.words).map(|word| word.len() + std::mem::size_of::<Box<str>>()).sum::<usize>()
            })
            .sum();
        let document_bytes = index.documents.len() * std::mem::size_of::<Document>();
        assert!(index.ready && !index.failed);
        assert_eq!(index.documents.len(), conversations);
        assert!(text_bytes + containers < vector_bytes);
        assert!(text_bytes <= PAYLOAD_BUDGET);
        if !diverse {
            assert!(text_bytes + document_bytes <= 1_500_000);
        }
        println!(
            "SEARCH COST: {} messages / {conversations} conversations; joined word payload {text_bytes} B; Box<str> storage {} B vs Vec<Box<str>> {vector_bytes} B; document structs {document_bytes} B; resident before {memory_before} B, after {} B, delta {} B; startup wall {:.2} ms, process CPU {} ms",
            conversations * messages,
            text_bytes + containers,
            after.memory(),
            after.memory().saturating_sub(memory_before),
            elapsed.as_secs_f64() * 1000.0,
            after.accumulated_cpu_time().saturating_sub(cpu_before)
        );
        drop(index);
        let query_started = std::time::Instant::now();
        let results = search.query(if diverse { "word" } else { "cafe words" }, Area::Chat).unwrap();
        println!("SEARCH QUERY: {} hits; {:.2} ms", results.hits.len(), query_started.elapsed().as_secs_f64() * 1000.0);
        assert!(!results.hits.is_empty());
        if diverse {
            assert!(!search.state.lock().unwrap().documents["conversation-0"].complete);
            let started = std::time::Instant::now();
            let fallback = search.query("word00000000", Area::Chat).unwrap();
            assert_eq!(fallback.hits[0].conversation_id, "conversation-0");
            println!("SEARCH FALLBACK: oldest evicted conversation found; {:.2} ms", started.elapsed().as_secs_f64() * 1000.0);
        }
        assert!(after.memory() < 400 * 1024 * 1024, "the index measurement alone exceeds R125");
    }
}
