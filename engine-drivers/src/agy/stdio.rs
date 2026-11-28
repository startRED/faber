use std::io::{self, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::sync::mpsc;

pub(super) const FLAG: &str = "--faber-conversation-mcp";

fn valid_secret(secret: &str) -> bool {
    secret.len() == 64 && secret.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Dispatch before initializing the app or creating any window.
pub fn stdio_mode() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != FLAG {
        return None;
    }
    let port = args.next().and_then(|port| port.to_str()?.parse::<u16>().ok());
    Some(match (port, args.next()) {
        (Some(port), None) => run(port),
        _ => 1,
    })
}

fn run(port: u16) -> i32 {
    let Ok(secret) = std::env::var(super::bridge::ENV) else { return 1 };
    if !valid_secret(&secret) {
        return 1;
    }
    let Ok(mut socket) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)) else { return 1 };
    if socket.write_all(secret.as_bytes()).is_err() {
        return 1;
    }
    let Ok(mut input_socket) = socket.try_clone() else { return 1 };
    let (tx, rx) = mpsc::channel();
    let input_tx = tx.clone();
    std::thread::spawn(move || {
        let result = io::copy(&mut io::stdin().lock(), &mut input_socket);
        let _ = input_tx.send(result.is_ok());
    });
    std::thread::spawn(move || {
        let result = io::copy(&mut socket, &mut io::stdout().lock());
        let _ = tx.send(result.is_ok());
    });
    if rx.recv().unwrap_or(false) { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    #[test]
    fn accepts_only_lowercase_hex_secret() {
        assert!(super::valid_secret(&"a0".repeat(32)));
        assert!(!super::valid_secret(&"A0".repeat(32)));
        assert!(!super::valid_secret("abc"));
    }
}
