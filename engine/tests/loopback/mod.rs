//! A loopback HTTP server for the download tests. It stands where the release host stands, so no test reaches the real
//! network. The handler writes the whole response itself, which lets a test send a body in steps, cut it short or
//! generate it without ever holding it.
#![allow(dead_code)]

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

pub struct Server {
    /// `http://127.0.0.1:<port>`
    pub origin: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Answers every request by calling `handler(origin, path, stream)`; the stream is closed when the handler returns.
    pub fn start(handler: impl Fn(&str, &str, &mut TcpStream) -> io::Result<()> + Send + Sync + 'static) -> Server {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let (stop, handler) = (Arc::new(AtomicBool::new(false)), Arc::new(handler));
        let (thread_stop, thread_origin) = (stop.clone(), origin.clone());
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let (handler, origin) = (handler.clone(), thread_origin.clone());
                        std::thread::spawn(move || {
                            let _ = stream.set_nonblocking(false);
                            if let Some(path) = request_path(&stream) {
                                let _ = handler(&origin, &path, &mut stream);
                            }
                        });
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
                    Err(_) => break,
                }
            }
        });
        Server { origin, stop, thread: Some(thread) }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The path of a `GET`, read from the request line; the rest of the request head is read and dropped.
fn request_path(stream: &TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let path = line.split_whitespace().nth(1)?.to_string();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 || line == "\r\n" {
            return Some(path);
        }
    }
}

/// The status line and headers of a 200 response whose body is `length` bytes, or, with `None`, runs until the
/// connection closes.
pub fn head(stream: &mut TcpStream, length: Option<u64>) -> io::Result<()> {
    match length {
        Some(n) => write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n"),
        None => write!(stream, "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n"),
    }
}

/// A complete 200 response.
pub fn send(stream: &mut TcpStream, body: &[u8]) -> io::Result<()> {
    head(stream, Some(body.len() as u64))?;
    stream.write_all(body)
}

pub fn not_found(stream: &mut TcpStream) -> io::Result<()> {
    write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
}

/// A response that sends a redirect and nothing else.
pub fn redirect(stream: &mut TcpStream, to: &str) -> io::Result<()> {
    write!(stream, "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
}
