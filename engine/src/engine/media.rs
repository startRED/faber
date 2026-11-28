//! Conversation-scoped local media. A grant holds a name, never file contents.
use super::Engine;
use faber_winproc::rewind::Root;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

#[cfg(test)]
mod tests;
mod web;

const CHUNK: u64 = 1024 * 1024;
const IMAGE_LIMIT: u64 = 8 * CHUNK;
const PATH_LIMIT: usize = 4096;
const GRANTS: usize = 512;
const GONE: &str = "error.media.gone";
const PATH: &str = "error.media.path";
const CLOSED: &str = "error.media.closed";

#[derive(Debug)]
pub struct MediaChunk {
    pub kind: &'static str,
    pub bytes: Vec<u8>,
    pub content_range: Option<String>,
}

#[derive(Debug)]
pub struct MediaError {
    pub key: &'static str,
    pub length: Option<u64>,
    pub retry_after: Option<u64>,
}

impl From<&'static str> for MediaError {
    fn from(key: &'static str) -> Self {
        Self { key, length: None, retry_after: None }
    }
}

#[derive(Default)]
pub(super) struct Media(Option<View>);

struct View {
    id: String,
    lease: Uuid,
    root: Option<Folder>,
    grants: HashMap<Uuid, Grant>,
}

#[derive(Clone)]
enum Grant {
    Local(Arc<str>),
    Web(Arc<reqwest::Url>),
}

#[derive(Clone, PartialEq, Eq)]
struct Folder {
    path: PathBuf,
    identity: Vec<u8>,
}

impl Folder {
    fn snapshot(root: &Root) -> Option<Self> {
        Some(Self { path: root.path().to_path_buf(), identity: root.identity().ok()? })
    }

    fn open(&self) -> Option<Root> {
        let root = crate::visual::pin(&self.path)?;
        (root.identity().ok()? == self.identity).then_some(root)
    }
}

impl Media {
    fn activate(&mut self, id: &str) -> Uuid {
        let lease = Uuid::new_v4();
        self.0 = Some(View { id: id.to_owned(), lease, root: None, grants: HashMap::new() });
        lease
    }

    #[cfg(test)]
    fn enter(&mut self, id: &str, root: Root) -> Uuid {
        let lease = self.activate(id);
        self.0.as_mut().unwrap().root = Folder::snapshot(&root);
        lease
    }

    fn leave(&mut self, lease: Uuid) {
        if self.0.as_ref().is_some_and(|view| view.lease == lease) {
            self.0 = None;
        }
    }

    fn grant(&mut self, id: &str, path: &str) -> Result<Uuid, &'static str> {
        self.insert(id, Grant::Local(Arc::from(path)))
    }

    fn insert(&mut self, id: &str, grant: Grant) -> Result<Uuid, &'static str> {
        let view = self.0.as_mut().filter(|view| view.id == id).ok_or(CLOSED)?;
        view.room()?;
        let token = Uuid::new_v4();
        view.grants.insert(token, grant);
        Ok(token)
    }

    fn release(&mut self, lease: Uuid, token: Uuid) {
        if let Some(view) = self.0.as_mut().filter(|view| view.lease == lease) {
            view.grants.remove(&token);
        }
    }

    fn source(&self, token: Uuid) -> Option<(Folder, Arc<str>)> {
        let view = self.0.as_ref()?;
        let Grant::Local(path) = view.grants.get(&token)? else { return None };
        Some((view.root.as_ref()?.clone(), path.clone()))
    }
}

impl Engine {
    /// The screen opens a lease when it starts showing a conversation, even before its first media request.
    pub fn media_enter(&self, id: &str) -> String {
        self.with(|core| core.media.activate(id)).simple().to_string()
    }

    pub fn media_leave(&self, lease: &str) {
        if let Ok(lease) = Uuid::parse_str(lease) {
            self.with(|core| core.media.leave(lease));
        }
    }

    /// An element that unloaded its source must get a fresh URL for a later load.
    pub fn media_release(&self, lease: &str, url: &str) {
        let token = url.rsplit('/').next().and_then(|part| Uuid::parse_str(part).ok());
        if let (Ok(lease), Some(token)) = (Uuid::parse_str(lease), token) {
            self.with(|core| core.media.release(lease, token));
        }
    }

    pub fn media_close(&self, id: &str) {
        self.with(|core| {
            if core.media.0.as_ref().is_some_and(|view| view.id == id) {
                core.media.0 = None;
            }
        });
    }

    pub fn media_clear(&self) {
        self.with(|core| core.media.0 = None);
    }

    /// Resolve the root from the actual conversation, then validate through the same handle used for range reads.
    pub fn media_grant(&self, id: &str, path: &str) -> Result<String, &'static str> {
        let (lease, root, folder) = self.with(|core| {
            let view = core.media.0.as_ref().filter(|view| view.id == id).ok_or(CLOSED)?;
            let folder = if view.root.is_none() {
                Some(
                    match core.convs.get(id) {
                        Some(conversation) => core.folder_of(conversation),
                        None => core.folder_of(&core.store.get(id).ok_or(GONE)?),
                    }
                    .map_err(|_| GONE)?,
                )
            } else {
                None
            };
            Ok::<_, &str>((view.lease, view.root.clone(), folder))
        })?;
        let root = if let Some(root) = root {
            root.open()
        } else {
            let folder = folder.as_deref().ok_or(GONE)?;
            crate::permissions::checked_folder(folder).ok_or(GONE)?;
            crate::visual::pin(folder)
        }
        .ok_or(GONE)?;
        let path = relative(&root, path).ok_or(PATH)?;
        let (_, len, kind) = open(&root, &path).map_err(|error| error.key)?;
        if kind.starts_with("image/") && len > IMAGE_LIMIT {
            return Err("error.media.limit");
        }
        let folder = Folder::snapshot(&root).ok_or(GONE)?;
        self.with(|core| {
            let view = core.media.0.as_mut().filter(|view| view.lease == lease).ok_or(CLOSED)?;
            if view.root.as_ref().is_some_and(|old| old != &folder) {
                return Err(GONE);
            }
            view.root = Some(folder);
            core.media.grant(id, &path).map(|token| token.simple().to_string())
        })
    }

    pub async fn media_grant_web(&self, id: &str, address: &str) -> Result<String, MediaError> {
        let lease = self.with(|core| {
            web::mode(core.store.settings().tuning().web)?;
            let view = core.media.0.as_ref().filter(|view| view.id == id).ok_or(CLOSED)?;
            view.room()?;
            Ok::<_, &'static str>(view.lease)
        })?;
        if address.len() > PATH_LIMIT {
            return Err("error.media.webUnreachable".into());
        }
        let url = reqwest::Url::parse(address).map_err(|_| "error.media.webHttps")?;
        web::fetch(url.clone(), None, true, |target| self.web_policy(lease, None, target)).await?;
        self.with(|core| {
            core.media.0.as_ref().filter(|view| view.lease == lease).ok_or(CLOSED)?;
            core.media.insert(id, Grant::Web(Arc::new(url))).map(|token| token.simple().to_string()).map_err(MediaError::from)
        })
    }

    fn web_policy(&self, lease: Uuid, token: Option<Uuid>, url: &reqwest::Url) -> Result<(), &'static str> {
        self.with(|core| {
            let view = core.media.0.as_ref().filter(|view| view.lease == lease).ok_or(CLOSED)?;
            if token.is_some_and(|token| !view.grants.contains_key(&token)) {
                return Err(CLOSED);
            }
            let settings = core.store.settings();
            web::policy(settings.tuning().web, settings.blocked_hosts.as_deref().unwrap_or_default(), url)
        })
    }

    pub async fn media_read_route(&self, token: &str, range: Option<&str>) -> Result<MediaChunk, MediaError> {
        let parsed = Uuid::parse_str(token).map_err(|_| CLOSED)?;
        let (lease, grant) = self
            .with(|core| {
                let view = core.media.0.as_ref()?;
                Some((view.lease, view.grants.get(&parsed)?.clone()))
            })
            .ok_or(CLOSED)?;
        match grant {
            Grant::Local(_) => {
                let engine = self.clone();
                let token = token.to_owned();
                let range = range.map(str::to_owned);
                tokio::task::spawn_blocking(move || engine.media_read(&token, range.as_deref())).await.map_err(|_| GONE)?
            }
            Grant::Web(url) => web::fetch((*url).clone(), range, false, |target| self.web_policy(lease, Some(parsed), target)).await,
        }
    }

    fn media_read(&self, token: &str, range: Option<&str>) -> Result<MediaChunk, MediaError> {
        let token = Uuid::parse_str(token).map_err(|_| CLOSED)?;
        let (root, path) = self.with(|core| core.media.source(token)).ok_or(CLOSED)?;
        let result = root.open().ok_or(GONE.into()).and_then(|root| read(&root, &path, range));
        self.with(|core| core.media.source(token)).ok_or(CLOSED)?;
        result
    }
}

impl View {
    fn room(&self) -> Result<(), &'static str> {
        if self.grants.len() >= GRANTS { Err("error.media.limit") } else { Ok(()) }
    }
}

fn relative(root: &Root, path: &str) -> Option<String> {
    let path = if cfg!(windows) { path.replace('\\', "/") } else { path.to_owned() };
    if path.len() > PATH_LIMIT || path.starts_with('/') || path.contains(':') {
        return None;
    }
    let file = crate::fileview::literal(root.path(), &path)?;
    let absolute = root.path().join(&file.path);
    let normal = |path: &Path| crate::project::git::native_path(path).replace('\\', "/").to_ascii_lowercase();
    let absolute = normal(&absolute);
    let moved = crate::permissions::relocated_credentials(&|name| std::env::var(name).ok());
    (!moved.iter().any(|(path, _)| {
        let path = normal(Path::new(path));
        let path = path.trim_end_matches('/');
        absolute == path || absolute.strip_prefix(path).is_some_and(|rest| rest.starts_with('/'))
    }))
    .then_some(file.path)
}

fn open(root: &Root, path: &str) -> Result<(std::fs::File, u64, &'static str), MediaError> {
    let mut file = root.open_file(path).map_err(|_| GONE)?;
    let len = file.metadata().map_err(|_| GONE)?.len();
    let mut probe = [0; 4096];
    let n = len.min(probe.len() as u64) as usize;
    file.read_exact(&mut probe[..n]).map_err(|_| GONE)?;
    let kind = kind(path, &probe[..n]).ok_or(GONE)?;
    Ok((file, len, kind))
}

fn read(root: &Root, path: &str, range: Option<&str>) -> Result<MediaChunk, MediaError> {
    let path = relative(root, path).ok_or(PATH)?;
    let (mut file, len, kind) = open(root, &path)?;
    let (start, end) = match range {
        Some(header) => bounds(header, len).ok_or(MediaError { key: "error.media.range", length: Some(len), retry_after: None })?,
        None if kind.starts_with("image/") && len > IMAGE_LIMIT => return Err("error.media.limit".into()),
        None if kind.starts_with("image/") => (0, len - 1),
        None => (0, len.min(CHUNK) - 1),
    };
    let mut bytes = vec![0; (end - start + 1) as usize];
    file.seek(SeekFrom::Start(start)).and_then(|_| file.read_exact(&mut bytes)).map_err(|_| GONE)?;
    let content_range = (range.is_some() || end + 1 < len).then(|| format!("bytes {start}-{end}/{len}"));
    Ok(MediaChunk { kind, bytes, content_range })
}

fn bounds(header: &str, len: u64) -> Option<(u64, u64)> {
    let (start, end) = header.strip_prefix("bytes=")?.split_once('-')?;
    let number = |part: &str| part.bytes().all(|b| b.is_ascii_digit()).then(|| part.parse::<u64>().ok()).flatten();
    let last = len.checked_sub(1)?;
    if start.is_empty() {
        let count = number(end)?;
        return (count != 0).then(|| (len.saturating_sub(count.min(CHUNK)), last));
    }
    let start = number(start)?;
    let end = if end.is_empty() { last } else { number(end)?.min(last) };
    (start <= end).then(|| (start, end.min(start.saturating_add(CHUNK - 1))))
}

fn kind(path: &str, b: &[u8]) -> Option<&'static str> {
    let at = |n, magic: &[u8]| b.get(n..).is_some_and(|rest| rest.starts_with(magic));
    let iso = at(4, b"ftyp") && b.len() >= 16 && u32::from_be_bytes(b[..4].try_into().ok()?) >= 16;
    let brand = |names: &[&[u8]]| {
        iso && (names.contains(&&b[8..12])
            || b[16..]
                .as_chunks::<4>()
                .0
                .iter()
                .take((u32::from_be_bytes(b[..4].try_into().unwrap()) as usize - 16) / 4)
                .any(|brand| names.contains(&brand.as_slice())))
    };
    let ogg = b.starts_with(b"OggS\0") && b.get(26).is_some_and(|n| b.len() > 27 + *n as usize);
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" if b.starts_with(b"\x89PNG\r\n\x1a\n") => Some("image/png"),
        "jpg" | "jpeg" if b.starts_with(b"\xff\xd8\xff") => Some("image/jpeg"),
        "gif" if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") => Some("image/gif"),
        "webp" if b.starts_with(b"RIFF") && at(8, b"WEBP") => Some("image/webp"),
        "avif" if brand(&[b"avif", b"avis"]) => Some("image/avif"),
        "bmp" if b.starts_with(b"BM") && b.len() >= 26 => Some("image/bmp"),
        "svg" if svg(b) => Some("image/svg+xml"),
        "mp4" if brand(&[b"isom", b"iso2", b"mp41", b"mp42", b"avc1", b"dash", b"iso5", b"iso6"]) => Some("video/mp4"),
        "webm" if b.starts_with(b"\x1a\x45\xdf\xa3") && b.windows(7).take(128).any(|w| w == b"\x42\x82\x84webm") => Some("video/webm"),
        "mp3" if b.starts_with(b"ID3") || b.get(0..3).is_some_and(|h| h[0] == 0xff && h[1] & 0xe6 == 0xe2 && h[2] & 0xf0 != 0xf0) => {
            Some("audio/mpeg")
        }
        "wav" if b.starts_with(b"RIFF") && at(8, b"WAVE") => Some("audio/wav"),
        "ogg" if ogg => Some("audio/ogg"),
        "opus" if ogg && at(27 + b[26] as usize, b"OpusHead") => Some("audio/ogg"),
        "m4a" if brand(&[b"M4A ", b"isom", b"mp42"]) => Some("audio/mp4"),
        "flac" if b.starts_with(b"fLaC") => Some("audio/flac"),
        _ => None,
    }
}

fn svg(mut b: &[u8]) -> bool {
    b = b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(b);
    loop {
        b = b.trim_ascii_start();
        let marker: &[u8] = if b.starts_with(b"<?xml ") {
            b"?>"
        } else if b.starts_with(b"<!--") {
            b"-->"
        } else {
            break;
        };
        let Some(end) = b.windows(marker.len()).position(|w| w == marker) else { return false };
        b = &b[end + marker.len()..];
    }
    b.strip_prefix(b"<svg").and_then(|rest| rest.first()).is_some_and(|next| next.is_ascii_whitespace() || matches!(next, b'>' | b'/'))
}
