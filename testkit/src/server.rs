//! A minimal loopback HTTP server that plays the part of the release host for the setup scenarios: the release
//! description and one zipped program. Anything else that is asked for is written down as unexpected.

use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// The asset the engine looks for on Windows x86-64.
pub const ASSET: &str = "codex-x86_64-pc-windows-msvc.exe.zip";

/// A zip holding one file, like the official release asset.
pub fn zip_of(name: &str, payload: &[u8]) -> io::Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    zip.start_file(name, zip::write::SimpleFileOptions::default()).map_err(io::Error::other)?;
    zip.write_all(payload)?;
    Ok(zip.finish().map_err(io::Error::other)?.into_inner())
}

#[derive(Default)]
struct Log {
    requests: Vec<String>,
    unexpected: Vec<String>,
}

pub struct Server {
    pub origin: String,
    log: Arc<Mutex<Log>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Serves `archive` (a zip) as the latest release.
    pub fn start(archive: Vec<u8>) -> io::Result<Server> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let origin = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
        let digest = format!("{:x}", Sha256::digest(&archive));
        let release = json!({
            "tag_name": "setup-replay",
            "assets": [{ "name": ASSET, "browser_download_url": format!("{origin}/codex.zip"), "digest": format!("sha256:{digest}") }],
        })
        .to_string();
        let (log, stop) = (Arc::new(Mutex::new(Log::default())), Arc::new(AtomicBool::new(false)));
        let (thread_log, thread_stop) = (log.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let (archive, release) = (Arc::new(archive), Arc::new(release));
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        let (log, archive, release) = (thread_log.clone(), archive.clone(), release.clone());
                        std::thread::spawn(move || {
                            let _ = answer(stream, peer, &log, &archive, &release);
                        });
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        });
        Ok(Server { origin, log, stop, thread: Some(thread) })
    }

    /// Every path asked for, in order.
    pub fn requests(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).requests.clone()
    }

    /// Paths nothing should ask for, and requests that did not come from this computer.
    pub fn unexpected(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).unexpected.clone()
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

fn answer(mut stream: TcpStream, peer: SocketAddr, log: &Mutex<Log>, archive: &[u8], release: &str) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    // The rest of the request is not needed; read up to the blank line so the client is not cut off mid-send.
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
    {
        let mut log = log.lock().unwrap_or_else(|e| e.into_inner());
        log.requests.push(path.clone());
        if !peer.ip().is_loopback() {
            log.unexpected.push(format!("{path} from {peer}"));
        }
    }
    let (status, kind, body): (&str, &str, &[u8]) = match path.as_str() {
        "/releases/latest" | "/" => ("200 OK", "application/json", release.as_bytes()),
        "/codex.zip" => ("200 OK", "application/zip", archive),
        _ => {
            log.lock().unwrap_or_else(|e| e.into_inner()).unexpected.push(path);
            ("404 Not Found", "text/plain", b"not found")
        }
    };
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())?;
    stream.write_all(body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn get(origin: &str, path: &str) -> (String, Vec<u8>) {
        let mut stream = TcpStream::connect(origin.trim_start_matches("http://")).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (String::from_utf8_lossy(&raw[..split]).into_owned(), raw[split + 4..].to_vec())
    }

    #[test]
    fn it_serves_the_release_and_the_archive_and_writes_down_anything_else() {
        let archive = zip_of("codex-x86_64-pc-windows-msvc.exe", b"MZ fake program").unwrap();
        let mut server = Server::start(archive.clone()).unwrap();
        let (head, body) = get(&server.origin, "/releases/latest");
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        let release: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(release["assets"][0]["name"], ASSET);
        assert_eq!(release["assets"][0]["browser_download_url"], format!("{}/codex.zip", server.origin));
        assert_eq!(release["assets"][0]["digest"], format!("sha256:{:x}", Sha256::digest(&archive)));
        let (head, body) = get(&server.origin, "/codex.zip");
        assert!(head.contains("application/zip"), "{head}");
        assert_eq!(body, archive);
        assert!(server.unexpected().is_empty());
        let (head, _) = get(&server.origin, "/somewhere/else");
        assert!(head.starts_with("HTTP/1.1 404"), "{head}");
        assert_eq!(server.requests(), ["/releases/latest", "/codex.zip", "/somewhere/else"]);
        assert_eq!(server.unexpected(), ["/somewhere/else"]);
        server.stop();
    }

    #[test]
    fn the_archive_holds_the_program_under_the_name_the_release_uses() {
        let archive = zip_of("codex-x86_64-pc-windows-msvc.exe", b"payload").unwrap();
        let mut zip = zip::ZipArchive::new(io::Cursor::new(archive)).unwrap();
        let mut file = zip.by_index(0).unwrap();
        assert_eq!(file.name(), "codex-x86_64-pc-windows-msvc.exe");
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        assert_eq!(text, "payload");
    }
}
