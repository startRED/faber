use super::*;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn another_gallery_item_waits_for_a_fetch_slot_instead_of_failing() {
    let occupied = FETCHES.acquire_many(2).await.unwrap();
    let check = |url: &Url| policy(WebSearch::Live, &[], url);
    let mut next = std::pin::pin!(fetch(Url::parse("https://127.0.0.1/next").unwrap(), None, true, check));
    std::future::poll_fn(|cx| {
        assert!(next.as_mut().poll(cx).is_pending(), "a busy fetch slot should queue the item");
        std::task::Poll::Ready(())
    })
    .await;
    drop(occupied);
    assert_eq!(next.await.unwrap_err().key, PRIVATE);
}

#[test]
fn non_public_addresses_include_all_local_and_reserved_routes() {
    for address in [
        "0.0.0.0",
        "10.5.4.3",
        "100.127.0.1",
        "127.0.0.1",
        "169.254.169.254",
        "172.31.9.2",
        "192.0.2.1",
        "192.168.0.1",
        "198.19.0.1",
        "198.51.100.80",
        "203.0.113.1",
        "224.0.0.1",
        "255.255.255.255",
        "::",
        "::1",
        "::ffff:127.0.0.1",
        "fd01::1",
        "fe80::1",
        "ff02::1",
        "2001:db8::1",
        "2002:7f00:1::1",
        "3fff::1",
    ] {
        assert!(!public(address.parse().unwrap()), "{address}");
    }
    for address in ["8.8.8.8", "1.1.1.1", "2606:4700::1111"] {
        assert!(public(address.parse().unwrap()), "{address}");
    }
}

#[tokio::test]
async fn denied_modes_and_targets_never_reach_the_resolver() {
    let calls = AtomicUsize::new(0);
    for (mode, address, expected) in [
        (WebSearch::Off, "https://pictures.invalid/x", "error.media.webOff"),
        (WebSearch::Cached, "https://pictures.invalid/x", "error.media.webCached"),
        (WebSearch::Live, "http://pictures.invalid/x", "error.media.webHttps"),
        (WebSearch::Live, "https://user:password@pictures.invalid/x", "error.media.webHttps"),
        (WebSearch::Live, "https://cdn.blocked.invalid/x", "error.media.webBlocked"),
        (WebSearch::Live, "https://127.0.0.1/x", "error.media.webPrivate"),
    ] {
        let check = |url: &Url| policy(mode, &["blocked.invalid".into()], url);
        let resolve = |_, _| {
            calls.fetch_add(1, Relaxed);
            async { unreachable!() }
        };
        let result = target(Url::parse(address).unwrap(), &check, &resolve).await;
        assert_eq!(result.err(), Some(expected), "{address}");
    }
    assert_eq!(calls.load(Relaxed), 0);
}

#[tokio::test]
async fn mixed_dns_answer_is_refused_before_connection() {
    let check = |url: &Url| policy(WebSearch::Live, &[], url);
    let resolve = |_, _| async { Ok(vec!["8.8.8.8:443".parse().unwrap(), "10.0.0.1:443".parse().unwrap()]) };
    let result = target(Url::parse("https://pictures.invalid/x").unwrap(), &check, &resolve).await;
    assert_eq!(result.err(), Some("error.media.webPrivate"));
}

#[test]
fn response_kind_ignores_the_url_and_http_content_type() {
    assert_eq!(sniff(b"\x89PNG\r\n\x1a\n"), Some("image/png"));
    assert_eq!(sniff(b"RIFF\x00\x00\x00\x00WAVE"), Some("audio/wav"));
    assert_eq!(sniff(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"), Some("image/svg+xml"));
    assert_eq!(sniff(b"<!DOCTYPE html><html><svg>"), None);
}

async fn server(replies: Vec<(&'static str, String, Vec<u8>)>) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (range, headers, body) in replies {
            let (mut conn, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                assert!(request.len() < 4096);
                request.push(conn.read_u8().await.unwrap());
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(request.lines().any(|header| header == format!("range: {range}")), "{request}");
            assert!(request.lines().any(|header| header == "user-agent: faber-media"));
            for header in ["cookie:", "authorization:", "referer:"] {
                assert!(!request.lines().any(|line| line.starts_with(header)), "{request}");
            }
            conn.write_all(headers.as_bytes()).await.unwrap();
            conn.write_all(&body).await.unwrap();
        }
    });
    (address, server)
}

fn partial(first: usize, last: usize, size: usize) -> String {
    format!(
        "HTTP/1.1 206 Partial Content\r\nContent-Type: text/html\r\nContent-Range: bytes {first}-{last}/{size}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        last - first + 1
    )
}

async fn fixture(listener: SocketAddr, blocked: &[String], range: Option<&str>, lookups: &AtomicUsize) -> Result<MediaChunk, MediaError> {
    let check = |url: &Url| policy(WebSearch::Live, blocked, url);
    let resolve = |_, port| {
        let first = lookups.fetch_add(1, Relaxed) == 0;
        async move {
            let ip = if first { "8.8.8.8" } else { "127.0.0.1" };
            Ok(vec![SocketAddr::new(ip.parse().unwrap(), port)])
        }
    };
    let send = |mut pin: Target, range: String| async move {
        assert!(pin.addresses.iter().all(|address| public(address.ip())));
        // The fixture is plain HTTP on loopback. Keep the production client and DNS pinning on its test transport.
        pin.url.set_scheme("http").unwrap();
        pin.url.set_port(Some(listener.port())).unwrap();
        pin.addresses = Arc::from([listener]);
        request(pin, range).await
    };
    tokio::time::timeout(
        Duration::from_secs(5),
        fetch_with(Url::parse("https://fixture.invalid/opaque").unwrap(), range, false, &check, &resolve, &send),
    )
    .await
    .expect("local fixture timed out")
}

#[tokio::test]
async fn seek_uses_exact_remote_bytes_and_does_not_resolve_again_after_the_address_is_checked() {
    let mut file = vec![0; 6000];
    file[..12].copy_from_slice(b"RIFF\x00\x00\x00\x00WAVE");
    file[4500..4504].copy_from_slice(b"seek");
    let (addr, server) = server(vec![
        ("bytes=0-4095", partial(0, 4095, file.len()), file[..4096].to_vec()),
        ("bytes=4500-4503", partial(4500, 4503, file.len()), file[4500..4504].to_vec()),
    ])
    .await;
    let lookups = AtomicUsize::new(0);
    let chunk = fixture(addr, &[], Some("bytes=4500-4503"), &lookups).await.unwrap();
    assert_eq!(lookups.load(Relaxed), 1);
    assert_eq!(chunk.bytes, b"seek");
    assert_eq!(chunk.kind, "audio/wav");
    assert_eq!(chunk.content_range.as_deref(), Some("bytes 4500-4503/6000"));
    server.await.unwrap();
}

#[tokio::test]
async fn redirects_cannot_reach_a_blocked_site_or_a_private_address() {
    for (location, expected, expected_lookups) in [
        ("http://fixture.invalid/downgrade", HTTPS, 1),
        ("https://127.0.0.1/secret", PRIVATE, 1),
        ("https://sub.blocked.invalid/secret", "error.media.webBlocked", 1),
        ("https://fixture.invalid/rebind", PRIVATE, 2),
    ] {
        let headers = format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let (addr, server) = server(vec![("bytes=0-4095", headers, vec![])]).await;
        let lookups = AtomicUsize::new(0);
        let error = fixture(addr, &["blocked.invalid".into()], None, &lookups).await.unwrap_err();
        assert_eq!(error.key, expected);
        assert_eq!(lookups.load(Relaxed), expected_lookups);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn an_https_media_request_never_accepts_or_falls_back_to_plain_http() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut conn, _) = listener.accept().await.unwrap();
        assert_eq!(conn.read_u8().await.unwrap(), 0x16, "the client must open with a TLS handshake");
        conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\n\x89PNG\r\n\x1a\n").await.unwrap();
    });
    let url = Url::parse(&format!("https://fixture.invalid:{}/", address.port())).unwrap();
    let target = Target { url, addresses: Arc::from([address]) };
    let result = tokio::time::timeout(Duration::from_secs(5), request(target, "bytes=0-4095".into())).await.unwrap();
    assert_eq!(result.unwrap_err(), UNREACHABLE);
    server.await.unwrap();
}

#[tokio::test]
async fn revocation_during_an_open_request_discards_the_reply() {
    use std::sync::atomic::AtomicBool;
    let body = b"\x89PNG\r\n\x1a\n";
    let (address, server) = server(vec![("bytes=0-4095", partial(0, 7, 8), body.to_vec())]).await;
    let revoked = AtomicBool::new(false);
    let check = |url: &Url| {
        if revoked.load(Relaxed) {
            return Err(super::super::CLOSED);
        }
        policy(WebSearch::Live, &[], url)
    };
    let resolve = |_, port| async move { Ok(vec![SocketAddr::new("8.8.8.8".parse().unwrap(), port)]) };
    let send = |mut pin: Target, range: String| {
        let revoked = &revoked;
        async move {
            pin.url.set_scheme("http").unwrap();
            pin.url.set_port(Some(address.port())).unwrap();
            pin.addresses = Arc::from([address]);
            let response = request(pin, range).await?;
            revoked.store(true, Relaxed);
            Ok(response)
        }
    };
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        fetch_with(Url::parse("https://fixture.invalid/picture").unwrap(), None, false, &check, &resolve, &send),
    )
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().key, super::super::CLOSED);
    server.await.unwrap();
}

#[tokio::test]
async fn size_and_bytes_are_refused_with_distinct_keys() {
    for (headers, content, expected) in
        [(partial(0, 4095, ITEM_LIMIT as usize + 1), vec![0; 4096], SIZE), (partial(0, 12, 13), b"<html></html>".to_vec(), KIND)]
    {
        let (addr, server) = server(vec![("bytes=0-4095", headers, content)]).await;
        let refusal = fixture(addr, &[], None, &AtomicUsize::new(0)).await.unwrap_err();
        assert_eq!(refusal.key, expected);
        server.await.unwrap();
    }
}
