//! On-demand prompts, using the sidebar's metadata to avoid opening unrelated or older files.

use crate::model::{Area, Role};
use serde::Deserialize;
use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const LIMIT: usize = 200;
const PAYLOAD_LIMIT: usize = 2 * 1024 * 1024;
const FILE_LIMIT: u64 = 16 * 1024 * 1024;
const READ_LIMIT: u64 = 64 * 1024 * 1024;

/// Startup recall must not wait for the sidebar's full-text index.
pub(crate) fn recent_files(directory: &Path, conversation_id: Option<&str>, cancelled: &AtomicBool) -> Result<Vec<String>, &'static str> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory).map_err(|_| "error.store.read")? {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(Vec::new());
        }
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        let (Some(id), Ok(modified)) = (path.file_stem().and_then(|id| id.to_str()), metadata.modified()) else { continue };
        if metadata.is_file() {
            files.push((id.to_owned(), modified));
        }
    }
    files.sort_unstable_by(|(a, left), (b, right)| {
        (Some(a.as_str()) != conversation_id)
            .cmp(&(Some(b.as_str()) != conversation_id))
            .then_with(|| right.cmp(left))
            .then_with(|| a.cmp(b))
    });
    Ok(files.into_iter().map(|(id, _)| id).collect())
}

/// Stop as soon as the file's own header decides scope; ignored bodies never allocate.
fn matches_scope(input: impl Read, id: &str, area: Area, project_id: Option<&str>) -> bool {
    struct Scope<'a> {
        id: &'a str,
        area: Area,
        project_id: Option<&'a str>,
        matches: &'a mut bool,
    }
    impl<'de> Visitor<'de> for Scope<'_> {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a saved conversation header")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
            let mut id = None;
            let mut area = None;
            let mut project: Option<Option<String>> = None;
            while let Some(key) = map.next_key::<String>()? {
                match key.as_str() {
                    "id" => id = Some(map.next_value::<String>()?),
                    "area" => area = Some(map.next_value::<Area>()?),
                    "projectId" => project = Some(map.next_value()?),
                    _ => {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                if id.as_deref().is_some_and(|id| id != self.id)
                    || area.is_some_and(|area| area != self.area)
                    || (self.area == Area::Project && project.as_ref().is_some_and(|project| project.as_deref() != self.project_id))
                {
                    return Err(serde::de::Error::custom("scope decided"));
                }
                if id.is_some() && area.is_some() && (self.area != Area::Project || project.is_some()) {
                    *self.matches = true;
                    return Err(serde::de::Error::custom("scope decided"));
                }
            }
            // A missing projectId is None; field order in older files need not match today's writer.
            *self.matches =
                id.as_deref() == Some(self.id) && area == Some(self.area) && (self.area != Area::Project || self.project_id.is_none());
            Ok(())
        }
    }
    let mut matches = false;
    let _ = serde::de::Deserializer::deserialize_map(
        &mut serde_json::Deserializer::from_reader(input),
        Scope { id, area, project_id, matches: &mut matches },
    );
    matches
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    id: String,
    area: Area,
    project_id: Option<String>,
    #[serde(deserialize_with = "prompts")]
    messages: Vec<Prompt>,
}

#[derive(Default)]
struct Prompt {
    text: String,
    at: u64,
}

impl<'de> Deserialize<'de> for Prompt {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = Prompt;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a saved message")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Prompt, M::Error> {
                let mut role = None;
                let mut wake = false;
                let mut prompt = Prompt::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "role" => role = Some(map.next_value::<Role>()?),
                        // Saved messages put role before text. Ignore replies without allocating their strings.
                        "text" if role.is_none_or(|role| role == Role::User) => prompt.text = map.next_value()?,
                        "at" => prompt.at = map.next_value()?,
                        "wake" => wake = map.next_value::<Option<IgnoredAny>>()?.is_some(),
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                if role != Some(Role::User) || wake || prompt.text.trim().is_empty() {
                    prompt.text.clear();
                }
                Ok(prompt)
            }
        }
        deserializer.deserialize_map(Fields)
    }
}

fn prompts<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Prompt>, D::Error> {
    struct Messages;
    impl<'de> Visitor<'de> for Messages {
        type Value = Vec<Prompt>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("saved messages")
        }
        fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Vec<Prompt>, S::Error> {
            let mut kept: Vec<Prompt> = Vec::new();
            let mut bytes = 0;
            while let Some(prompt) = seq.next_element::<Prompt>()? {
                if prompt.text.is_empty() || prompt.text.len() > PAYLOAD_LIMIT {
                    continue;
                }
                if let Some(duplicate) = kept.iter().position(|old| old.text == prompt.text) {
                    if kept[duplicate].at > prompt.at {
                        continue;
                    }
                    bytes -= kept.remove(duplicate).text.len();
                }
                bytes += prompt.text.len();
                // Later messages win equal timestamps, matching their order in a saved conversation.
                let at = kept.partition_point(|old| old.at > prompt.at);
                kept.insert(at, prompt);
                while kept.len() > LIMIT || bytes > PAYLOAD_LIMIT {
                    bytes -= kept.pop().unwrap().text.len();
                }
            }
            Ok(kept)
        }
    }
    deserializer.deserialize_seq(Messages)
}

/// Checks cancellation before every buffered refill, including inside a large ignored attachment or reply.
struct Cancellable<'a, R> {
    input: R,
    cancelled: &'a AtomicBool,
    remaining: &'a mut u64,
}

impl<R: Read> Read for Cancellable<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::other("cancelled"));
        }
        let limit = bytes.len().min((*self.remaining).min(8192) as usize);
        if limit == 0 {
            return Ok(0);
        }
        let read = self.input.read(&mut bytes[..limit])?;
        *self.remaining -= read as u64;
        Ok(read)
    }
}

pub(crate) fn read(
    directory: &Path,
    candidates: impl IntoIterator<Item = String>,
    area: Area,
    project_id: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<Vec<String>, &'static str> {
    let mut history: Vec<String> = Vec::new();
    let mut bytes = 0;
    let mut remaining = READ_LIMIT;
    'files: for id in candidates {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(Vec::new());
        }
        if history.len() == LIMIT || remaining == 0 || bytes == PAYLOAD_LIMIT {
            break;
        }
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            continue;
        }
        let Ok(mut file) = std::fs::File::open(directory.join(format!("{id}.json"))) else { continue };
        // Oversized files cannot make a focus read unbounded, even if all their contents would be skipped.
        let Ok(metadata) = file.metadata() else { continue };
        if metadata.len() > FILE_LIMIT || metadata.len() > remaining {
            continue;
        }
        let input = Cancellable { input: (&mut file).take(FILE_LIMIT), cancelled, remaining: &mut remaining };
        if !matches_scope(BufReader::new(input), &id, area, project_id) || file.seek(SeekFrom::Start(0)).is_err() {
            continue;
        }
        let input = Cancellable { input: file.take(FILE_LIMIT), cancelled, remaining: &mut remaining };
        // Damaged or concurrently removed files do not hide the others; no partial file escapes.
        let Ok(saved) = serde_json::from_reader::<_, Saved>(BufReader::new(input)) else { continue };
        if saved.id != id || saved.area != area || (area == Area::Project && saved.project_id.as_deref() != project_id) {
            continue;
        }
        for prompt in saved.messages {
            if history.contains(&prompt.text) || bytes + prompt.text.len() > PAYLOAD_LIMIT {
                continue;
            }
            bytes += prompt.text.len();
            history.push(prompt.text);
            if history.len() == LIMIT {
                break 'files;
            }
        }
    }
    if cancelled.load(Ordering::Relaxed) {
        history.clear();
    }
    Ok(history)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_history_checks_scope_before_reading_message_bodies() {
        for (header, area, project) in [
            (r#"{"id":"saved","area":"project","projectId":"other","messages":"#, Area::Chat, None),
            (r#"{"id":"saved","area":"project","projectId":"other","messages":"#, Area::Project, Some("wanted")),
        ] {
            let cancelled = AtomicBool::new(false);
            let mut remaining = READ_LIMIT;
            let input = header.as_bytes().chain(io::repeat(b' '));
            let input = Cancellable { input, cancelled: &cancelled, remaining: &mut remaining };
            assert!(!matches_scope(BufReader::new(input), "saved", area, project));
            assert!(READ_LIMIT - remaining <= 8192, "out-of-scope messages must not be scanned");
        }
        // Older files may put the scope fields after messages, or omit a null projectId.
        assert!(matches_scope(
            br#"{"messages":[],"area":"project","id":"saved","projectId":"wanted"}"#.as_slice(),
            "saved",
            Area::Project,
            Some("wanted")
        ));
        assert!(matches_scope(br#"{"messages":[],"area":"project","id":"saved"}"#.as_slice(), "saved", Area::Project, None));
    }

    #[test]
    fn prompt_history_cold_files_use_modification_time_and_keep_the_open_conversation_first() {
        let dir = tempfile::tempdir().unwrap();
        for (id, modified, area, project) in [
            ("open", 1, "chat", None),
            ("newer", 3, "chat", None),
            ("older", 2, "chat", None),
            ("wrong-area", 5, "project", None),
            ("wrong-project", 6, "project", Some("other")),
            ("project", 4, "project", Some("wanted")),
        ] {
            let path = dir.path().join(format!("{id}.json"));
            std::fs::write(
                &path,
                serde_json::to_vec(&serde_json::json!({
                    "id": id, "area": area, "projectId": project, "updatedAt": 100 - modified,
                    "messages": [{"role": "user", "text": id, "at": 1}]
                }))
                .unwrap(),
            )
            .unwrap();
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(modified)))
                .unwrap();
        }
        let cancelled = AtomicBool::new(false);
        let candidates = recent_files(dir.path(), Some("open"), &cancelled).unwrap();
        assert_eq!(candidates, ["open", "wrong-project", "wrong-area", "project", "newer", "older"]);
        assert_eq!(read(dir.path(), candidates, Area::Chat, None, &cancelled).unwrap(), ["open", "newer", "older"]);
        let candidates = recent_files(dir.path(), None, &cancelled).unwrap();
        assert_eq!(read(dir.path(), candidates, Area::Project, Some("wanted"), &cancelled).unwrap(), ["project"]);
    }

    #[test]
    fn prompt_history_cancels_during_a_buffered_file_read() {
        struct CancelOnRead<'a>(&'a AtomicBool);
        impl Read for CancelOnRead<'_> {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                bytes.fill(b' ');
                self.0.store(true, Ordering::Relaxed);
                Ok(bytes.len())
            }
        }
        let cancelled = AtomicBool::new(false);
        let mut remaining = READ_LIMIT;
        let input = Cancellable { input: CancelOnRead(&cancelled), cancelled: &cancelled, remaining: &mut remaining };
        let mut input = BufReader::new(input);
        let error = io::copy(&mut input, &mut io::sink()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(READ_LIMIT - remaining, 8192, "abandoned work stops on the very next refill");
    }

    #[test]
    fn prompt_history_stops_before_opening_older_conversations_at_the_cap() {
        let directory = tempfile::tempdir().unwrap();
        let messages: Vec<_> = (0..200).map(|at| serde_json::json!({"role": "user", "text": format!("prompt {at}"), "at": at})).collect();
        std::fs::write(
            directory.path().join("newest.json"),
            serde_json::to_vec(&serde_json::json!({
                "id": "newest", "area": "chat", "messages": messages,
            }))
            .unwrap(),
        )
        .unwrap();
        let candidates = std::iter::once("newest".to_owned()).chain(std::iter::from_fn(|| panic!("older files must not be requested")));
        assert_eq!(read(directory.path(), candidates, Area::Chat, None, &AtomicBool::new(false)).unwrap().len(), LIMIT);
    }

    #[test]
    fn prompt_history_reader_enforces_the_total_input_budget() {
        let cancelled = AtomicBool::new(false);
        let mut remaining = 12345;
        let mut input = Cancellable { input: io::repeat(b'x'), cancelled: &cancelled, remaining: &mut remaining };
        let bytes = io::copy(&mut input, &mut io::sink()).unwrap();
        assert_eq!(bytes, 12345);
        assert_eq!(remaining, 0);
    }
}
