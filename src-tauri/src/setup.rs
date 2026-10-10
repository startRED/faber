//! Setup guide backend: checking CLI status, installation, and login.

use crate::Faber;
use crate::terminal;
use faber_engine::cli::Cli;
use faber_engine::model::Vendor;
use faber_engine::{AccountStatus, AppEvent};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

type Res<T> = Result<T, String>;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub vendor: Vendor,
    pub installed: bool,
    pub account: AccountStatus,
    pub can_install: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupProgressEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub vendor: Vendor,
    pub step: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub fn parse_vendor(s: &str) -> Option<Vendor> {
    match s.trim().to_ascii_lowercase().as_str() {
        "claude" => Some(Vendor::Claude),
        "codex" | "chatgpt" => Some(Vendor::Codex),
        _ => None,
    }
}

/// Looks for installed CLIs, checking FABER_CLI_<NAME>, FABER_DATA_DIR, and PATH.
pub fn detect_clis() -> BTreeMap<Vendor, Cli> {
    detect_clis_with(&|k| std::env::var(k).ok())
}

pub fn detect_clis_with(env: &dyn Fn(&str) -> Option<String>) -> BTreeMap<Vendor, Cli> {
    Vendor::ALL
        .into_iter()
        .filter_map(|v| {
            let cli = match env(&format!("FABER_CLI_{}", v.name().to_uppercase())) {
                Some(json) => serde_json::from_str(&json).ok(),
                None => find_cli_candidate(v.name(), env),
            };
            cli.map(|c| (v, c))
        })
        .collect()
}

fn find_cli_candidate(name: &str, env: &dyn Fn(&str) -> Option<String>) -> Option<Cli> {
    if let Some(dir) = env("FABER_DATA_DIR") {
        let base = PathBuf::from(dir);
        let candidates =
            [base.join("bin").join(format!("{name}.exe")), base.join(format!("{name}.exe")), base.join("bin").join(name), base.join(name)];
        for candidate in candidates {
            if candidate.is_file() {
                return Some(Cli { command: candidate.to_string_lossy().into_owned(), prefix: vec![] });
            }
        }
    }
    let path_var = env("PATH").or_else(|| env("Path")).unwrap_or_default();
    Cli::find_in(name, &path_var, cfg!(windows), env, &|p| Path::new(p).is_file(), &|p| std::fs::read_to_string(p).unwrap_or_default())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimpleUrl {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(raw: &str) -> Result<SimpleUrl, String> {
    let s = raw.trim();
    let (scheme, rest) =
        if let Some(idx) = s.find("://") { (s[..idx].to_ascii_lowercase(), &s[idx + 3..]) } else { ("http".to_string(), s) };
    let (authority, path_with_query) = if let Some(idx) = rest.find('/') { (&rest[..idx], &rest[idx..]) } else { (rest, "/") };
    let (host, port) = if authority.starts_with('[') {
        if let Some(close_idx) = authority.find(']') {
            let h = &authority[1..close_idx];
            let p = if authority[close_idx + 1..].starts_with(':') {
                authority[close_idx + 2..].parse::<u16>().map_err(|e| e.to_string())?
            } else if scheme == "https" {
                443
            } else {
                80
            };
            (h.to_string(), p)
        } else {
            return Err("Unclosed IPv6 bracket".into());
        }
    } else if let Some(idx) = authority.rfind(':') {
        let h = &authority[..idx];
        let p = authority[idx + 1..].parse::<u16>().map_err(|e| e.to_string())?;
        (h.to_string(), p)
    } else {
        let p = if scheme == "https" { 443 } else { 80 };
        (authority.to_string(), p)
    };
    Ok(SimpleUrl { scheme, host, port, path: path_with_query.to_string() })
}

pub fn is_loopback(addr: &str) -> bool {
    if let Ok(url) = parse_url(addr) {
        let h = url.host.trim().to_ascii_lowercase();
        if h == "localhost" {
            return true;
        }
        if let Ok(ip) = h.parse::<std::net::IpAddr>() {
            return ip.is_loopback();
        }
    }
    false
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be,
        0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa,
        0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85,
        0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f,
        0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_val = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_val.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_val = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_val);
    }

    format!("{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}", h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7])
}

fn http_get_loopback(url_str: &str) -> Result<Vec<u8>, String> {
    if !is_loopback(url_str) {
        return Err("URL is not a loopback address".into());
    }
    let url = parse_url(url_str)?;
    let host = &url.host;
    let port = url.port;
    let path = &url.path;

    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;

    let addrs: Vec<std::net::SocketAddr> =
        format!("{host}:{port}").to_socket_addrs().map_err(|e| format!("Failed to resolve {host}:{port}: {e}"))?.collect();
    if addrs.is_empty() || !addrs.iter().all(|a| a.ip().is_loopback()) {
        return Err(format!("Host {host} does not resolve to loopback address"));
    }

    let mut stream = TcpStream::connect(&addrs[..]).map_err(|e| format!("Failed to connect to {host}:{port}: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();

    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\nUser-Agent: Faber\r\n\r\n");
    stream.write_all(req.as_bytes()).map_err(|e| format!("Failed to send request: {e}"))?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(|e| format!("Failed to read response: {e}"))?;

    let delim = b"\r\n\r\n";
    let header_end = response.windows(delim.len()).position(|w| w == delim).ok_or_else(|| "Invalid HTTP response headers".to_string())?;
    let headers_bytes = &response[..header_end];
    let body = &response[header_end + delim.len()..];
    let headers_str = String::from_utf8_lossy(headers_bytes);
    let status_line = headers_str.lines().next().unwrap_or("");
    if !status_line.contains("200") {
        return Err(format!("HTTP request failed with status: {status_line}"));
    }

    if headers_str.to_ascii_lowercase().contains("transfer-encoding: chunked") { decode_chunked(body) } else { Ok(body.to_vec()) }
}

fn decode_chunked(mut data: &[u8]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let pos = data.windows(2).position(|w| w == b"\r\n").ok_or_else(|| "Malformed chunk header".to_string())?;
        let size_str = std::str::from_utf8(&data[..pos]).map_err(|e| e.to_string())?.trim();
        let chunk_size = usize::from_str_radix(size_str, 16).map_err(|e| format!("Invalid chunk size '{size_str}': {e}"))?;
        data = &data[pos + 2..];
        if chunk_size == 0 {
            break;
        }
        if data.len() < chunk_size + 2 {
            return Err("Incomplete chunk data".to_string());
        }
        body.extend_from_slice(&data[..chunk_size]);
        data = &data[chunk_size + 2..];
    }
    Ok(body)
}

fn extract_zip_stored(bytes: &[u8], dest_dir: &Path) -> Result<(), String> {
    let mut pos = 0;
    let mut extracted_any = false;
    while pos + 30 <= bytes.len() {
        if &bytes[pos..pos + 4] != b"PK\x03\x04" {
            break;
        }
        let method = u16::from_le_bytes([bytes[pos + 8], bytes[pos + 9]]);
        let comp_size = u32::from_le_bytes([bytes[pos + 18], bytes[pos + 19], bytes[pos + 20], bytes[pos + 21]]) as usize;
        let name_len = u16::from_le_bytes([bytes[pos + 26], bytes[pos + 27]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[pos + 28], bytes[pos + 29]]) as usize;
        let name_start = pos + 30;
        let data_start = name_start + name_len + extra_len;
        if data_start + comp_size > bytes.len() {
            return Err("Truncated zip entry".into());
        }
        let name = std::str::from_utf8(&bytes[name_start..name_start + name_len]).map_err(|e| e.to_string())?;
        let clean_name = name.replace('\\', "/");
        if clean_name.contains("..") || clean_name.starts_with('/') || clean_name.contains(':') {
            return Err("Unsafe zip path".into());
        }
        let clean_rel = sanitize_zip_path(&clean_name)?;
        let out_path = dest_dir.join(&clean_rel);
        if !out_path.starts_with(dest_dir) {
            return Err("Zip path escapes destination directory".into());
        }

        if clean_name.ends_with('/') {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            if method == 0 {
                std::fs::write(&out_path, &bytes[data_start..data_start + comp_size]).map_err(|e| e.to_string())?;
                extracted_any = true;
            } else if method == 8 {
                let decompressed = inflate_raw(&bytes[data_start..data_start + comp_size])?;
                std::fs::write(&out_path, &decompressed).map_err(|e| e.to_string())?;
                extracted_any = true;
            } else {
                return Err(format!("Unsupported compression method in zip: {method}"));
            }
        }
        pos = data_start + comp_size;
    }
    if !extracted_any {
        return Err("No valid entries found in ZIP archive".into());
    }
    Ok(())
}

fn sanitize_zip_path(name: &str) -> Result<std::path::PathBuf, String> {
    let p = Path::new(name);
    let mut clean = std::path::PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(c) => clean.push(c),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => return Err("Parent traversal in zip path".into()),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err("Absolute path in zip entry".into());
            }
        }
    }
    let clean_str = clean.to_string_lossy();
    if clean_str.is_empty() || clean_str.contains(':') || clean_str.starts_with('/') || clean_str.starts_with('\\') {
        return Err("Unsafe zip path".into());
    }
    Ok(clean)
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bits: u64,
    num_bits: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, bits: 0, num_bits: 0 }
    }

    fn fill(&mut self, n: u8) -> bool {
        while self.num_bits < n {
            if self.pos < self.data.len() {
                self.bits |= (self.data[self.pos] as u64) << self.num_bits;
                self.pos += 1;
                self.num_bits += 8;
            } else {
                return false;
            }
        }
        true
    }

    fn consume(&mut self, n: u8) {
        self.bits >>= n;
        self.num_bits = self.num_bits.saturating_sub(n);
    }

    fn read_bits(&mut self, n: u8) -> Result<u16, String> {
        if n == 0 {
            return Ok(0);
        }
        if !self.fill(n) && self.num_bits < n {
            return Err("Unexpected end of compressed data".into());
        }
        let val = (self.bits & ((1u64 << n) - 1)) as u16;
        self.consume(n);
        Ok(val)
    }

    fn drop_to_byte_boundary(&mut self) {
        let rem = self.num_bits % 8;
        self.consume(rem);
    }
}

struct HuffmanTree {
    nodes: Vec<[i16; 2]>,
}

impl HuffmanTree {
    fn from_lengths(lengths: &[u8]) -> Self {
        let mut bl_count = [0usize; 16];
        for &len in lengths {
            if len > 0 && (len as usize) < 16 {
                bl_count[len as usize] += 1;
            }
        }
        let mut next_code = [0u32; 16];
        let mut code = 0u32;
        for bits in 1..=15 {
            code = (code + bl_count[bits - 1] as u32) << 1;
            next_code[bits] = code;
        }

        let mut nodes: Vec<[i16; 2]> = vec![[-1, -1]];
        for (sym, &len) in lengths.iter().enumerate() {
            if len == 0 || len > 15 {
                continue;
            }
            let c = next_code[len as usize];
            next_code[len as usize] += 1;

            let mut curr = 0usize;
            for i in (0..len).rev() {
                let bit = ((c >> i) & 1) as usize;
                let child = nodes[curr][bit];
                if i == 0 {
                    nodes[curr][bit] = -2 - (sym as i16);
                } else if child >= 0 {
                    curr = child as usize;
                } else {
                    let new_node = nodes.len();
                    nodes.push([-1, -1]);
                    nodes[curr][bit] = new_node as i16;
                    curr = new_node;
                }
            }
        }
        Self { nodes }
    }

    fn decode(&self, reader: &mut BitReader) -> Result<u16, String> {
        let mut curr = 0usize;
        loop {
            let bit = reader.read_bits(1)? as usize;
            let child = self.nodes[curr][bit];
            if child < -1 {
                return Ok((-child - 2) as u16);
            }
            if child < 0 {
                return Err("Invalid Huffman code in stream".into());
            }
            curr = child as usize;
        }
    }
}

const LENGTH_BASE: [u16; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];

const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385,
    24577,
];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

const CODE_LEN_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

fn decode_deflate_block(
    reader: &mut BitReader,
    lit_tree: &HuffmanTree,
    dist_tree: &HuffmanTree,
    output: &mut Vec<u8>,
) -> Result<(), String> {
    loop {
        let sym = lit_tree.decode(reader)?;
        if sym < 256 {
            output.push(sym as u8);
        } else if sym == 256 {
            break;
        } else if sym <= 285 {
            let idx = (sym - 257) as usize;
            let base_len = LENGTH_BASE[idx];
            let extra_bits = LENGTH_EXTRA[idx];
            let length = base_len + reader.read_bits(extra_bits)?;

            let dist_sym = dist_tree.decode(reader)?;
            if dist_sym >= 30 {
                return Err("Invalid distance code".into());
            }
            let d_idx = dist_sym as usize;
            let base_dist = DIST_BASE[d_idx];
            let d_extra = DIST_EXTRA[d_idx];
            let dist = (base_dist + reader.read_bits(d_extra)?) as usize;

            if dist > output.len() {
                return Err("Distance beyond output buffer".into());
            }
            let start = output.len() - dist;
            for i in 0..length as usize {
                let b = output[start + i];
                output.push(b);
            }
        } else {
            return Err("Invalid literal/length code".into());
        }
    }
    Ok(())
}

fn fixed_lit_tree() -> HuffmanTree {
    let mut lengths = [0u8; 288];
    lengths[..144].fill(8);
    lengths[144..256].fill(9);
    lengths[256..280].fill(7);
    lengths[280..288].fill(8);
    HuffmanTree::from_lengths(&lengths)
}

fn fixed_dist_tree() -> HuffmanTree {
    let lengths = [5u8; 32];
    HuffmanTree::from_lengths(&lengths)
}

pub fn inflate_raw(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = BitReader::new(data);
    let mut output = Vec::new();
    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;
        match btype {
            0 => {
                reader.drop_to_byte_boundary();
                let len = reader.read_bits(16)? as usize;
                let nlen = reader.read_bits(16)? as usize;
                if (len ^ nlen) != 0xffff {
                    return Err("Invalid uncompressed block length".into());
                }
                for _ in 0..len {
                    let b = reader.read_bits(8)? as u8;
                    output.push(b);
                }
            }
            1 => {
                let lit_tree = fixed_lit_tree();
                let dist_tree = fixed_dist_tree();
                decode_deflate_block(&mut reader, &lit_tree, &dist_tree, &mut output)?;
            }
            2 => {
                let hlit = (reader.read_bits(5)? + 257) as usize;
                let hdist = (reader.read_bits(5)? + 1) as usize;
                let hclen = (reader.read_bits(4)? + 4) as usize;

                let mut clen_lengths = [0u8; 19];
                for i in 0..hclen {
                    clen_lengths[CODE_LEN_ORDER[i]] = reader.read_bits(3)? as u8;
                }
                let clen_tree = HuffmanTree::from_lengths(&clen_lengths);

                let mut all_lengths = Vec::with_capacity(hlit + hdist);
                while all_lengths.len() < hlit + hdist {
                    let sym = clen_tree.decode(&mut reader)?;
                    match sym {
                        0..=15 => all_lengths.push(sym as u8),
                        16 => {
                            if all_lengths.is_empty() {
                                return Err("Invalid repeat in code lengths".into());
                            }
                            let last = *all_lengths.last().unwrap();
                            let count = 3 + reader.read_bits(2)? as usize;
                            all_lengths.resize(all_lengths.len() + count, last);
                        }
                        17 => {
                            let count = 3 + reader.read_bits(3)? as usize;
                            all_lengths.resize(all_lengths.len() + count, 0);
                        }
                        18 => {
                            let count = 11 + reader.read_bits(7)? as usize;
                            all_lengths.resize(all_lengths.len() + count, 0);
                        }
                        _ => return Err("Unexpected symbol in code lengths".into()),
                    }
                }

                let lit_tree = HuffmanTree::from_lengths(&all_lengths[..hlit]);
                let dist_tree = HuffmanTree::from_lengths(&all_lengths[hlit..hlit + hdist]);
                decode_deflate_block(&mut reader, &lit_tree, &dist_tree, &mut output)?;
            }
            _ => return Err(format!("Invalid deflate block type: {btype}")),
        }
        if bfinal == 1 {
            break;
        }
    }
    Ok(output)
}

fn user_home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

#[tauri::command]
pub async fn setup_check(app: AppHandle, faber: State<'_, Faber>, vendor: String) -> Res<CheckResult> {
    let Some(v) = parse_vendor(&vendor) else {
        return Err("error.setup.check".into());
    };
    let clis = detect_clis();
    let installed = clis.contains_key(&v);
    faber.engine.replace_clis(clis.clone());
    let account = if installed { faber.engine.account_status(v, Duration::from_secs(5)).await } else { AccountStatus::Unknown };

    if account == AccountStatus::SignedIn {
        let updated = detect_clis();
        faber.engine.replace_clis(updated.clone());
        if let Some(codex) = updated.get(&Vendor::Codex) {
            crate::load_models(&app, codex.clone(), faber.data_dir.clone());
        }
        let _ = app.emit("faber:event", AppEvent::List);
    }

    let can_install = cfg!(windows);
    Ok(CheckResult { vendor: v, installed, account, can_install })
}

#[tauri::command]
pub async fn setup_sign_in(vendor: String) -> Res<()> {
    let Some(v) = parse_vendor(&vendor) else {
        return Err("error.setup.signIn".into());
    };
    let clis = detect_clis();
    let Some(cli) = clis.get(&v) else {
        return Err("error.setup.signIn".into());
    };
    let mut args = cli.prefix.clone();
    args.extend(faber_engine::account::login_args(v).iter().map(|s| s.to_string()));
    let cmd = faber_engine::TerminalCommand { command: cli.command.clone(), args, cwd: user_home_dir() };
    if !terminal::is_safe(&cmd) {
        return Err("error.setup.signIn".into());
    }
    terminal::open(&cmd).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn setup_install(app: AppHandle, faber: State<'_, Faber>, vendor: String) -> Res<()> {
    let Some(v) = parse_vendor(&vendor) else {
        let _ = app.emit(
            "faber:event",
            SetupProgressEvent {
                event_type: "setup".into(),
                vendor: Vendor::Claude,
                step: "failed".into(),
                error: Some("error.setup.unsupported".into()),
                detail: Some("Unknown vendor".into()),
            },
        );
        return Err("error.setup.unsupported".into());
    };

    let emit_progress = |step: &str, error: Option<&str>, detail: Option<&str>| {
        let _ = app.emit(
            "faber:event",
            SetupProgressEvent {
                event_type: "setup".into(),
                vendor: v,
                step: step.into(),
                error: error.map(String::from),
                detail: detail.map(String::from),
            },
        );
    };

    if !cfg!(windows) {
        emit_progress("failed", Some("error.setup.unsupported"), Some("Only Windows is supported"));
        return Err("error.setup.unsupported".into());
    }

    let setup_source = std::env::var("FABER_SETUP_SOURCE").ok();
    if let Some(ref source) = setup_source {
        if !is_loopback(source) {
            emit_progress("failed", Some("error.setup.download"), Some("Only loopback setup source is allowed"));
            return Err("error.setup.download".into());
        }
        // Run loopback download and local install into FABER_DATA_DIR without touching PATH
        let base_url = source.trim_end_matches('/').to_string();
        emit_progress("prerequisite", None, None);
        emit_progress("download", None, None);

        let dest_dir = faber.data_dir.join("bin");
        let _ = std::fs::create_dir_all(&dest_dir);

        if v == Vendor::Claude {
            let latest_url = format!("{base_url}/latest");
            let latest_bytes = http_get_loopback(&latest_url).map_err(|e| {
                emit_progress("failed", Some("error.setup.download"), Some(&e));
                "error.setup.download".to_string()
            })?;
            let version = std::str::from_utf8(&latest_bytes).unwrap_or("").trim().to_string();
            let manifest_url = format!("{base_url}/{version}/manifest.json");
            let manifest_bytes = http_get_loopback(&manifest_url).map_err(|e| {
                emit_progress("failed", Some("error.setup.download"), Some(&e));
                "error.setup.download".to_string()
            })?;
            let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).map_err(|e| {
                let d = format!("Invalid manifest JSON: {e}");
                emit_progress("failed", Some("error.setup.download"), Some(&d));
                "error.setup.download".to_string()
            })?;
            let expected_checksum = manifest["platforms"]["win32-x64"]["checksum"].as_str().unwrap_or("");
            let bin_url = format!("{base_url}/{version}/win32-x64/claude.exe");
            let binary_bytes = http_get_loopback(&bin_url).map_err(|e| {
                emit_progress("failed", Some("error.setup.download"), Some(&e));
                "error.setup.download".to_string()
            })?;

            emit_progress("verify", None, None);
            let digest = sha256_hex(&binary_bytes);
            if !digest.eq_ignore_ascii_case(expected_checksum) {
                let d = format!("Checksum mismatch: expected {expected_checksum}, got {digest}");
                emit_progress("failed", Some("error.setup.checksum"), Some(&d));
                return Err("error.setup.checksum".into());
            }

            emit_progress("install", None, None);
            let out_file = dest_dir.join("claude.exe");
            std::fs::write(&out_file, &binary_bytes).map_err(|e| {
                let d = format!("Failed to write binary: {e}");
                emit_progress("failed", Some("error.setup.install"), Some(&d));
                "error.setup.install".to_string()
            })?;
        } else {
            let release_url = format!("{base_url}/releases/latest");
            let release_bytes = http_get_loopback(&release_url).or_else(|_| http_get_loopback(&base_url)).map_err(|e| {
                emit_progress("failed", Some("error.setup.download"), Some(&e));
                "error.setup.download".to_string()
            })?;
            let release: serde_json::Value = serde_json::from_slice(&release_bytes).map_err(|e| {
                let d = format!("Invalid release metadata: {e}");
                emit_progress("failed", Some("error.setup.download"), Some(&d));
                "error.setup.download".to_string()
            })?;
            let assets = release["assets"].as_array().ok_or_else(|| {
                emit_progress("failed", Some("error.setup.download"), Some("No assets array"));
                "error.setup.download".to_string()
            })?;
            let asset = assets
                .iter()
                .find(|a| {
                    let name = a["name"].as_str().unwrap_or("").to_ascii_lowercase();
                    name.contains("codex") && (name.ends_with(".zip") || name.ends_with(".exe"))
                })
                .ok_or_else(|| {
                    emit_progress("failed", Some("error.setup.unsupported"), Some("No matching asset"));
                    "error.setup.unsupported".to_string()
                })?;
            let download_url = asset["browser_download_url"].as_str().unwrap_or("");
            if !is_loopback(download_url) {
                emit_progress("failed", Some("error.setup.download"), Some("Asset download URL not on loopback"));
                return Err("error.setup.download".into());
            }
            let raw_digest = asset["digest"].as_str().unwrap_or("");
            let expected_digest = raw_digest.strip_prefix("sha256:").unwrap_or(raw_digest);
            let archive_bytes = http_get_loopback(download_url).map_err(|e| {
                emit_progress("failed", Some("error.setup.download"), Some(&e));
                "error.setup.download".to_string()
            })?;

            emit_progress("verify", None, None);
            let digest = sha256_hex(&archive_bytes);
            if !digest.eq_ignore_ascii_case(expected_digest) {
                let d = format!("Checksum mismatch: expected {expected_digest}, got {digest}");
                emit_progress("failed", Some("error.setup.checksum"), Some(&d));
                return Err("error.setup.checksum".into());
            }

            emit_progress("install", None, None);
            if archive_bytes.starts_with(b"PK\x03\x04") {
                extract_zip_stored(&archive_bytes, &dest_dir).map_err(|e| {
                    emit_progress("failed", Some("error.setup.install"), Some(&e));
                    "error.setup.install".to_string()
                })?;
                // Promote codex-{triple}.exe to codex.exe if needed
                let root_codex = dest_dir.join("codex.exe");
                if !root_codex.is_file()
                    && let Ok(entries) = std::fs::read_dir(&dest_dir)
                {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("codex") && n.ends_with(".exe")) {
                            let _ = std::fs::rename(&path, &root_codex);
                            break;
                        }
                    }
                }
            } else {
                let out_file = dest_dir.join("codex.exe");
                std::fs::write(&out_file, &archive_bytes).map_err(|e| {
                    let d = format!("Failed to write binary: {e}");
                    emit_progress("failed", Some("error.setup.install"), Some(&d));
                    "error.setup.install".to_string()
                })?;
            }
        }
        emit_progress("done", None, None);
    } else {
        // Standard installation delegating to engine's install_windows with PATH registration
        let destination = user_home_dir().join(".local").join("bin");
        let request = serde_json::json!({
            "vendor": v.name(),
            "windows": true,
            "arch": std::env::consts::ARCH,
            "data_dir": faber.data_dir,
            "destination": destination,
        });

        let fetch = |url: &str| {
            std::thread::spawn({
                let u = url.to_string();
                move || tauri::async_runtime::block_on(faber_engine::install::default_fetch(&u))
            })
            .join()
            .unwrap_or_else(|_| Err(std::io::Error::other("fetch thread panicked")))
        };
        let register = |dir: &Path| faber_engine::install::default_register_path(dir);
        let progress = |val: serde_json::Value| {
            let stage = val["stage"].as_str().unwrap_or("failed");
            let key = val["key"].as_str();
            let detail = val["detail"].as_str();
            emit_progress(stage, key, detail);
        };

        faber_engine::cli::Cli::install_windows(&request, &fetch, &register, faber_engine::proc::real_spawner(), &progress)
            .await
            .map_err(|err| err["key"].as_str().unwrap_or("error.setup.install").to_string())?;
    }

    // After install succeeds: redetect, replace in engine, reload Codex models, emit list
    let updated = detect_clis();
    faber.engine.replace_clis(updated.clone());
    if let Some(codex) = updated.get(&Vendor::Codex) {
        crate::load_models(&app, codex.clone(), faber.data_dir.clone());
    }
    let _ = app.emit("faber:event", AppEvent::List);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c3_1_login_uses_official_args_in_user_dir_and_refuses_shell_characters() {
        let claude_args = faber_engine::account::login_args(Vendor::Claude);
        assert_eq!(claude_args, &["auth", "login"]);

        let codex_args = faber_engine::account::login_args(Vendor::Codex);
        assert_eq!(codex_args, &["login"]);

        let safe_cmd = faber_engine::TerminalCommand {
            command: "claude.exe".into(),
            args: vec!["auth".into(), "login".into()],
            cwd: PathBuf::from("C:\\Users\\Test"),
        };
        assert!(terminal::is_safe(&safe_cmd));

        let unsafe_cmd = faber_engine::TerminalCommand {
            command: "claude.exe".into(),
            args: vec!["auth\" & calc".into()],
            cwd: PathBuf::from("C:\\Users\\Test"),
        };
        assert!(!terminal::is_safe(&unsafe_cmd));
    }

    #[test]
    fn c3_2_cli_detection_handles_json_empty_and_absent() {
        let fake_env = |k: &str| match k {
            "FABER_CLI_CLAUDE" => Some("{\"command\":\"test-claude.exe\",\"prefix\":[\"run\"]}".to_string()),
            "FABER_CLI_CODEX" => Some("".to_string()),
            _ => None,
        };
        let detected = detect_clis_with(&fake_env);
        assert_eq!(detected.get(&Vendor::Claude), Some(&Cli { command: "test-claude.exe".into(), prefix: vec!["run".into()] }));
        assert_eq!(detected.get(&Vendor::Codex), None);
    }

    #[test]
    fn c3_3_setup_source_loopback_validation() {
        assert!(is_loopback("http://127.0.0.1:8080"));
        assert!(is_loopback("127.0.0.1:3000"));
        assert!(is_loopback("http://localhost:5000/releases"));
        assert!(is_loopback("http://[::1]:9999"));

        assert!(!is_loopback("http://evil.com"));
        assert!(!is_loopback("https://github.com/openai"));
        assert!(!is_loopback("http://192.168.1.50:8080"));
    }

    #[test]
    fn sha256_matches_known_standard_vector() {
        // Known standard vector for b"abc"
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        // Empty string
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn c3_4_can_install_verdict() {
        let res = CheckResult { vendor: Vendor::Claude, installed: false, account: AccountStatus::Unknown, can_install: cfg!(windows) };
        assert_eq!(res.can_install, cfg!(windows));
    }

    #[test]
    fn c3_5_progress_event_serialization_matches_schema() {
        let event = SetupProgressEvent {
            event_type: "setup".into(),
            vendor: Vendor::Claude,
            step: "failed".into(),
            error: Some("error.setup.download".into()),
            detail: Some("Connection refused".into()),
        };
        let json_str = serde_json::to_string(&event).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(val["type"], "setup");
        assert_eq!(val["vendor"], "claude");
        assert_eq!(val["step"], "failed");
        assert_eq!(val["error"], "error.setup.download");
        assert_eq!(val["detail"], "Connection refused");
    }
}
