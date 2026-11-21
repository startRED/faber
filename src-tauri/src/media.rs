use faber_engine::Engine;
use tauri::http::{Method, Request, Response};

pub const SCHEME: &str = "faber-media";
pub const CUSTOM_ORIGIN: &str = "faber-media://localhost";
pub const ORIGIN: &str = if cfg!(windows) { "http://faber-media.localhost" } else { CUSTOM_ORIGIN };

fn token(uri: &str) -> Option<&str> {
    // The WebView adapter can report the registered scheme even when the screen uses its Windows alias.
    let path = uri.strip_prefix(ORIGIN).or_else(|| uri.strip_prefix(CUSTOM_ORIGIN))?;
    let token = path.strip_prefix('/')?;
    (token.len() == 32 && token.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))).then_some(token)
}

pub fn denied() -> Response<Vec<u8>> {
    failure("error.media.closed", None)
}

pub fn respond(engine: &Engine, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let destination = request.headers().get("sec-fetch-dest");
    if request.method() != Method::GET || destination.is_some_and(|value| !matches!(value.to_str(), Ok("image" | "video" | "audio"))) {
        return denied();
    }
    let uri = request.uri().to_string();
    let Some(token) = token(&uri) else { return denied() };
    let ranges = request.headers().get_all("range");
    let range = match ranges.iter().count() {
        0 => None,
        1 => Some(ranges.iter().next().and_then(|value| value.to_str().ok()).unwrap_or("invalid")),
        _ => Some("invalid"),
    };
    match engine.media_read(token, range) {
        Ok(chunk) => {
            let mut response = headers()
                .status(if chunk.content_range.is_some() { 206 } else { 200 })
                .header("Content-Type", chunk.kind)
                .header("Content-Length", chunk.bytes.len())
                .header("Accept-Ranges", "bytes");
            if let Some(range) = chunk.content_range {
                response = response.header("Content-Range", range);
            }
            response.body(chunk.bytes).expect("validated media response")
        }
        Err(error) => failure(error.key, error.length),
    }
}

fn headers() -> tauri::http::response::Builder {
    Response::builder()
        .header("Cache-Control", "no-store")
        .header("Referrer-Policy", "no-referrer")
        .header("Content-Security-Policy", "default-src 'none'; sandbox")
        .header("X-Content-Type-Options", "nosniff")
}

fn failure(key: &str, length: Option<u64>) -> Response<Vec<u8>> {
    let mut response = headers().header("X-Faber-Media-Error", key);
    if let Some(length) = length {
        response = response.status(416).header("Content-Range", format!("bytes */{length}"));
    } else {
        response = response.status(match key {
            "error.media.closed" => 403,
            "error.media.limit" => 413,
            _ => 404,
        });
    }
    response.body(Vec::new()).expect("fixed media error headers")
}
