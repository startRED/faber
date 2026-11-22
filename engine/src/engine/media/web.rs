//! Direct media for the screen's scoped protocol. The renderer never sees the network response.
use super::{CHUNK, IMAGE_LIMIT, MediaChunk, MediaError, bounds, kind};
use crate::model::WebSearch;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{CONTENT_ENCODING, CONTENT_RANGE, LOCATION, RANGE};
use reqwest::{Client, Response, StatusCode, Url};
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Semaphore;

#[cfg(test)]
mod tests;

const PRIVATE: &str = "error.media.webPrivate";
const HTTPS: &str = "error.media.webHttps";
const SIZE: &str = "error.media.webSize";
const KIND: &str = "error.media.webKind";
const UNREACHABLE: &str = "error.media.webUnreachable";
const PROBE: u64 = 4096;
const ITEM_LIMIT: u64 = 128 * CHUNK;
const REDIRECTS: usize = 3;
static FETCHES: Semaphore = Semaphore::const_new(2);
static PENDING: Semaphore = Semaphore::const_new(64);
static RESOLUTIONS: Semaphore = Semaphore::const_new(2);

pub(super) fn mode(web: WebSearch) -> Result<(), &'static str> {
    match web {
        WebSearch::Off => Err("error.media.webOff"),
        WebSearch::Cached => Err("error.media.webCached"),
        WebSearch::Live => Ok(()),
    }
}

pub(super) fn policy(web: WebSearch, blocked: &[String], url: &Url) -> Result<(), &'static str> {
    mode(web)?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(HTTPS);
    }
    if url.as_str().len() > super::PATH_LIMIT {
        return Err(UNREACHABLE);
    }
    crate::browser::check_blocked_url(url.as_str(), blocked).map_err(|_| "error.media.webBlocked")
}

fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !matches!(
                (a, b, c),
                (0 | 10 | 127 | 224..=255, _, _)
                    | (100, 64..=127, _)
                    | (169, 254, _)
                    | (172, 16..=31, _)
                    | (192, 0, 0 | 2)
                    | (192, 88, 99)
                    | (192, 168, _)
                    | (198, 18 | 19, _)
                    | (198, 51, 100)
                    | (203, 0, 113)
            )
        }
        IpAddr::V6(ip) => {
            let [a, b, ..] = ip.segments();
            matches!(a, 0x2000..=0x3fff) && !matches!((a, b), (0x2001, 0..=0x01ff | 0x0db8) | (0x2002, _) | (0x3fff, _))
        }
    }
}

#[derive(Clone)]
struct Target {
    url: Url,
    addresses: Arc<[SocketAddr]>,
}

impl Resolve for Target {
    fn resolve(&self, name: Name) -> Resolving {
        let allowed = self.url.host_str() == Some(name.as_str());
        let addresses = self.addresses.to_vec();
        Box::pin(async move {
            if !allowed {
                return Err(io::Error::other("media DNS pin does not match the request").into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

async fn lookup(host: String, port: u16) -> Result<Vec<SocketAddr>, &'static str> {
    // Keep the permit in the blocking lookup after a caller times out. A stuck OS resolver cannot pile up work.
    let permit = RESOLUTIONS.try_acquire().map_err(|_| UNREACHABLE)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (host.as_str(), port).to_socket_addrs().map(|addresses| addresses.collect()).map_err(|_| UNREACHABLE)
    })
    .await
    .map_err(|_| UNREACHABLE)?
}

async fn target<R: Future<Output = Result<Vec<SocketAddr>, &'static str>>>(
    mut url: Url,
    check: &impl Fn(&Url) -> Result<(), &'static str>,
    resolve: &impl Fn(String, u16) -> R,
) -> Result<Target, &'static str> {
    check(&url)?;
    url.set_fragment(None);
    let host = url.host_str().ok_or(HTTPS)?;
    let port = url.port_or_known_default().ok_or(HTTPS)?;
    let addresses = match host.trim_matches(['[', ']']).parse::<IpAddr>() {
        Ok(ip) => vec![SocketAddr::new(ip, port)],
        Err(_) => resolve(host.to_owned(), port).await?,
    };
    if addresses.is_empty() {
        return Err(UNREACHABLE);
    }
    if addresses.iter().any(|address| !public(address.ip())) {
        return Err(PRIVATE);
    }
    Ok(Target { url, addresses: addresses.into() })
}

fn client(pin: Target) -> Result<Client, &'static str> {
    static ROOTS: OnceLock<Vec<reqwest::Certificate>> = OnceLock::new();
    // Offline chain verification prevents certificate AIA or revocation URLs from opening unpinned connections.
    let roots = ROOTS.get_or_init(|| {
        rustls_native_certs::load_native_certs()
            .certs
            .into_iter()
            .filter_map(|certificate| reqwest::Certificate::from_der(certificate.as_ref()).ok())
            .collect()
    });
    crate::tls::install();
    Client::builder()
        .tls_certs_only(roots.iter().cloned())
        .dns_resolver(Arc::new(pin))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .user_agent("Faber-Media")
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| UNREACHABLE)
}

async fn request(pin: Target, range: String) -> Result<Response, &'static str> {
    client(pin.clone())?.get(pin.url).header(RANGE, range).send().await.map_err(|_| UNREACHABLE)
}

pub(super) async fn fetch(
    url: Url,
    range: Option<&str>,
    probe_only: bool,
    check: impl Fn(&Url) -> Result<(), &'static str>,
) -> Result<MediaChunk, MediaError> {
    check(&url)?;
    let _pending = PENDING.try_acquire().map_err(|_| "error.media.limit")?;
    tokio::time::timeout(Duration::from_secs(15), async {
        let _permit = FETCHES.acquire().await.map_err(|_| UNREACHABLE)?;
        fetch_with(url, range, probe_only, &check, &lookup, &request).await
    })
    .await
    .map_err(|_| UNREACHABLE)?
}

async fn fetch_with<R, S>(
    mut url: Url,
    range: Option<&str>,
    probe_only: bool,
    check: &impl Fn(&Url) -> Result<(), &'static str>,
    resolve: &impl Fn(String, u16) -> R,
    send: &impl Fn(Target, String) -> S,
) -> Result<MediaChunk, MediaError>
where
    R: Future<Output = Result<Vec<SocketAddr>, &'static str>>,
    S: Future<Output = Result<Response, &'static str>>,
{
    for hop in 0..=REDIRECTS {
        let pin = target(url, check, resolve).await?;
        check(&pin.url)?;
        let response = send(pin.clone(), format!("bytes=0-{}", PROBE - 1)).await?;
        check(&pin.url)?;
        if response.status().is_redirection() {
            if hop == REDIRECTS {
                return Err(UNREACHABLE.into());
            }
            let location = response.headers().get(LOCATION).and_then(|value| value.to_str().ok()).ok_or(UNREACHABLE)?;
            url = pin.url.join(location).map_err(|_| HTTPS)?;
            continue;
        }
        let result = media(response, range, probe_only, &pin, check, send).await;
        check(&pin.url)?;
        return result;
    }
    Err(UNREACHABLE.into())
}

fn layout(response: &Response) -> Result<(u64, u64, u64), &'static str> {
    if response.headers().contains_key(CONTENT_ENCODING) {
        return Err(UNREACHABLE);
    }
    match response.status() {
        StatusCode::OK => {
            let total = response.content_length().filter(|length| *length > 0).ok_or(UNREACHABLE)?;
            Ok((0, total - 1, total))
        }
        StatusCode::PARTIAL_CONTENT => {
            let value = response.headers().get(CONTENT_RANGE).and_then(|value| value.to_str().ok()).ok_or(UNREACHABLE)?;
            let (span, total) = value.strip_prefix("bytes ").and_then(|value| value.split_once('/')).ok_or(UNREACHABLE)?;
            let (start, end) = span.split_once('-').ok_or(UNREACHABLE)?;
            let parse = |part: &str| part.parse::<u64>().map_err(|_| UNREACHABLE);
            let (start, end, total) = (parse(start)?, parse(end)?, parse(total)?);
            if start > end || end >= total || response.content_length().is_some_and(|length| length != end - start + 1) {
                return Err(UNREACHABLE);
            }
            Ok((start, end, total))
        }
        _ => Err(UNREACHABLE),
    }
}

async fn body(response: &mut Response, bytes: &mut Vec<u8>, wanted: usize, cap: usize) -> Result<(), &'static str> {
    let exact = wanted == cap;
    while exact || bytes.len() < wanted {
        let Some(chunk) = response.chunk().await.map_err(|_| UNREACHABLE)? else {
            return if bytes.len() >= wanted { Ok(()) } else { Err(UNREACHABLE) };
        };
        let room = cap - bytes.len();
        if exact && chunk.len() > room {
            return Err(UNREACHABLE);
        }
        let count = room.min(chunk.len());
        bytes.reserve_exact(count);
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok(())
}

async fn media<S: Future<Output = Result<Response, &'static str>>>(
    mut response: Response,
    range: Option<&str>,
    probe_only: bool,
    pin: &Target,
    check: &impl Fn(&Url) -> Result<(), &'static str>,
    send: &impl Fn(Target, String) -> S,
) -> Result<MediaChunk, MediaError> {
    let partial = response.status() == StatusCode::PARTIAL_CONTENT;
    let (start, end, total) = layout(&response)?;
    let prefix = total.min(PROBE);
    if total > ITEM_LIMIT {
        return Err(SIZE.into());
    }
    if start != 0 || (partial && end != prefix - 1) {
        return Err(UNREACHABLE.into());
    }
    let mut bytes = Vec::with_capacity(prefix as usize);
    let cap = if partial { prefix } else { total.min(IMAGE_LIMIT) };
    body(&mut response, &mut bytes, prefix as usize, cap as usize).await?;
    let media_kind = sniff(&bytes).ok_or(KIND)?;
    let image = media_kind.starts_with("image/");
    if image && total > IMAGE_LIMIT {
        return Err(SIZE.into());
    }
    if !partial && !image && total > CHUNK {
        return Err(UNREACHABLE.into());
    }
    if probe_only {
        return Ok(MediaChunk { kind: media_kind, bytes: Vec::new(), content_range: None });
    }
    // Serve SVG (and every other picture) whole so the bytes sent are checked as one image.
    let (first, last) = if image {
        (0, total - 1)
    } else if let Some(range) = range {
        bounds(range, total).ok_or(MediaError { key: "error.media.range", length: Some(total) })?
    } else {
        (0, total.min(CHUNK) - 1)
    };
    if !partial || last < prefix {
        if !partial {
            body(&mut response, &mut bytes, total as usize, total as usize).await?;
        }
        bytes.truncate((last + 1) as usize);
        bytes.drain(..first as usize);
    } else {
        drop(response);
        check(&pin.url)?;
        let mut response = send(pin.clone(), format!("bytes={first}-{last}")).await?;
        check(&pin.url)?;
        let (start, end, length) = layout(&response)?;
        if (start, end, length) != (first, last, total) {
            return Err(if length > ITEM_LIMIT || (image && length > IMAGE_LIMIT) { SIZE } else { UNREACHABLE }.into());
        }
        bytes = Vec::with_capacity((last - first + 1) as usize);
        let size = (last - first + 1) as usize;
        body(&mut response, &mut bytes, size, size).await?;
        if first == 0 && sniff(&bytes) != Some(media_kind) {
            return Err(KIND.into());
        }
    }
    let content_range = (!image && (range.is_some() || last + 1 < total)).then(|| format!("bytes {first}-{last}/{total}"));
    Ok(MediaChunk { kind: media_kind, bytes, content_range })
}

fn sniff(bytes: &[u8]) -> Option<&'static str> {
    // Use the local route's signatures without trusting the address or a declared HTTP type.
    if bytes.get(8..12) == Some(b"M4A ") {
        return kind("media.m4a", bytes);
    }
    [
        "media.png",
        "media.jpg",
        "media.gif",
        "media.webp",
        "media.avif",
        "media.bmp",
        "media.svg",
        "media.mp4",
        "media.m4a",
        "media.webm",
        "media.mp3",
        "media.wav",
        "media.ogg",
        "media.flac",
    ]
    .into_iter()
    .find_map(|extension| kind(extension, bytes))
}
