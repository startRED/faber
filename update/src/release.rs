//! The release reader, in place of the Tauri updater plugin: the signed manifest of the channel, and the installer it points
//! to. The manifest is the plugin's own format (`latest.json`, the one every installed Faber already reads), signed with the
//! same minisign key, so one release serves both the Tauri Faber and this one.
//!
//! Nothing is installed from a release that cannot be verified: a bad signature, a platform the manifest lacks and an
//! address that is not HTTPS are errors, written to `update.log` by the caller.

use base64::Engine as _;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// The stable channel: the manifest of the latest release, which every install ever shipped already reads.
pub const STABLE_FEED: &str = "https://github.com/startRED/faber/releases/latest/download/latest.json";
/// The preview channel manifest: a release with a fixed tag whose `latest.json` every release replaces with the newest build
/// (.github/workflows/release.yml, R156).
pub const PREVIEW_FEED: &str = "https://github.com/startRED/faber/releases/download/preview-channel/latest.json";
/// An installer larger than this is refused, whatever the server says.
const MAX_DOWNLOAD: u64 = 512 * 1024 * 1024;
/// A manifest is a few hundred bytes; one larger than this is not one.
const MAX_MANIFEST: usize = 1024 * 1024;
/// A server that does not answer is given up on after this; the flow bounds the rest.
const CONNECT_WITHIN: Duration = Duration::from_secs(15);
// This must stay the key every installed Faber trusts: it is `plugins.updater.pubkey` of src-tauri/tauri.conf.json, the base64
// of a minisign public key file. Changing it strands every install that holds the old one, which would refuse the next release.
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDZFN0Q4QTY3QzlFMEI1MjIKUldRaXRlREpaNHA5YmdtQ0ZoNHJoMnJpOGJBWWNIWEtDK3R1QXF4YmY2UGpYd2VhZk5OcENHOTYK";

/// The manifest key for the system this runs on, None where Faber ships no installer.
pub fn platform_key() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("windows-x86_64")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("darwin-aarch64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("darwin-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64")
    } else {
        None
    }
}

/// One signed installer in the release manifest: the base64 of a minisign `.sig` file, and where the installer is.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Platform {
    pub signature: String,
    pub url: String,
}

/// The release manifest shared with existing installations (the Tauri updater plugin format).
#[derive(Debug, serde::Deserialize)]
pub struct Manifest {
    pub version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub platforms: HashMap<String, Platform>,
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("the manifest cannot be read: {e}"))
    }

    /// A missing platform is a failed check, not an update to install.
    pub fn platform(&self, key: &str) -> Result<&Platform, &'static str> {
        self.platforms.get(key).ok_or("the release has no installer for this system")
    }
}

/// A newer release this system can install.
#[derive(Debug, Clone)]
pub struct Found {
    pub version: String,
    pub platform: Platform,
}

/// The release in `manifest`, when it is newer than `current` (`is_upgrade`) and has an installer for `key`. A release that is
/// not newer is no error, so a missing platform in an old manifest never matters.
pub fn newer(manifest: &Manifest, key: &str, current: &semver::Version, previews: bool) -> Result<Option<Found>, String> {
    let remote = semver::Version::parse(manifest.version.trim_start_matches('v')).map_err(|e| e.to_string())?;
    if !crate::is_upgrade(current, &remote, previews) {
        return Ok(None);
    }
    let platform = manifest.platform(key)?.clone();
    Ok(Some(Found { version: remote.to_string(), platform }))
}

/// The release server and every redirect must use HTTPS.
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(CONNECT_WITHIN)
        .user_agent(concat!("faber-update/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

/// Reads the manifest of the selected channel.
pub async fn manifest(previews: bool) -> Result<Manifest, String> {
    let mut response = client()?
        .get(if previews { PREVIEW_FEED } else { STABLE_FEED })
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > MAX_MANIFEST {
            return Err("the manifest is too large".into());
        }
        body.extend_from_slice(&chunk);
    }
    Manifest::parse(&body)
}

fn https_url(url: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    if url.scheme() != "https" {
        return Err("the update address must use HTTPS".into());
    }
    Ok(url)
}

fn decode(text: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(text.trim()).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Where the bytes of an installer come from: the network, or a test.
#[allow(async_fn_in_trait)]
pub trait Source {
    /// The size the server announced, when it did.
    fn total(&self) -> Option<u64>;
    /// The next piece, None at the end.
    async fn next(&mut self) -> Result<Option<Vec<u8>>, String>;
}

/// Writes what `source` gives to `file` and checks it against `signature` with `key` as it goes, so an installer is never
/// held in memory. Only when the last byte is in and the signature holds is the file an update; the caller removes it
/// otherwise. `limit` bounds the size, `progress` hears each piece and the total.
pub async fn receive(
    source: &mut impl Source,
    file: &mut impl Write,
    signature: &str,
    key: &str,
    limit: u64,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let key = minisign_verify::PublicKey::decode(&decode(key)?).map_err(|e| e.to_string())?;
    let signature = minisign_verify::Signature::decode(&decode(signature)?).map_err(|e| e.to_string())?;
    let mut verifier = key.verify_stream(&signature).map_err(|e| e.to_string())?;
    let total = source.total();
    if total.is_some_and(|size| size > limit) {
        return Err("the update is too large".into());
    }
    let mut done = 0u64;
    while let Some(chunk) = source.next().await? {
        done += chunk.len() as u64;
        if done > limit {
            return Err("the update is too large".into());
        }
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        verifier.update(&chunk);
        progress(chunk.len() as u64, total);
    }
    file.flush().map_err(|e| e.to_string())?;
    verifier.finalize().map_err(|e| format!("the update is not signed by Faber: {e}"))
}

struct Body(reqwest::Response);

impl Source for Body {
    fn total(&self) -> Option<u64> {
        self.0.content_length()
    }

    async fn next(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.0.chunk().await.map(|chunk| chunk.map(|bytes| bytes.to_vec())).map_err(|e| e.to_string())
    }
}

/// Removes the installer being written unless it was kept: a download cut short by a timeout is dropped half way, and a half
/// file must never stay for the install to find.
struct Partial<'a> {
    path: &'a Path,
    keep: bool,
}

impl Drop for Partial<'_> {
    fn drop(&mut self) {
        if !self.keep {
            discard(self.path);
        }
    }
}

/// Removes an installer and the folder it was kept in; whatever is not there is fine.
pub fn discard(installer: &Path) {
    let _ = std::fs::remove_file(installer);
    if let Some(folder) = installer.parent() {
        let _ = std::fs::remove_dir(folder);
    }
}

/// Downloads the installer to `installer` (its folder is made) and verifies it against the Faber key. What stays on the disk
/// is always a verified installer: the file goes when anything fails, and when the download is dropped half way.
pub async fn download(platform: &Platform, installer: &Path, progress: impl FnMut(u64, Option<u64>)) -> Result<(), String> {
    let url = https_url(&platform.url)?;
    let response = client()?.get(url).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(installer.parent().ok_or("no folder for the installer")?).map_err(|e| e.to_string())?;
    let mut partial = Partial { path: installer, keep: false };
    let mut file = std::fs::File::create(installer).map_err(|e| e.to_string())?;
    let result = receive(&mut Body(response), &mut file, &platform.signature, PUBLIC_KEY, MAX_DOWNLOAD, progress).await;
    drop(file);
    partial.keep = result.is_ok();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test key pair made once with the Tauri signer; only the public key and the signed file are kept (never a private key).
    const TEST_KEY: &str = include_str!("../tests/fixtures/test.key.pub");
    const PAYLOAD: &[u8] = include_bytes!("../tests/fixtures/payload.bin");
    const SIGNATURE: &str = include_str!("../tests/fixtures/payload.bin.sig");

    /// Pieces handed out one by one, as a download would.
    struct Pieces(std::vec::IntoIter<Vec<u8>>, Option<u64>);

    impl Pieces {
        fn of(bytes: &[u8], size: usize, announced: Option<u64>) -> Self {
            Self(bytes.chunks(size).map(<[u8]>::to_vec).collect::<Vec<_>>().into_iter(), announced)
        }
    }

    impl Source for Pieces {
        fn total(&self) -> Option<u64> {
            self.1
        }

        async fn next(&mut self) -> Result<Option<Vec<u8>>, String> {
            Ok(self.0.next())
        }
    }

    async fn run(bytes: &[u8], signature: &str, key: &str, limit: u64) -> (Result<(), String>, Vec<u8>, u64) {
        let (mut file, mut seen) = (Vec::new(), 0u64);
        let result = receive(&mut Pieces::of(bytes, 5, Some(bytes.len() as u64)), &mut file, signature, key, limit, |n, _| seen += n).await;
        (result, file, seen)
    }

    #[tokio::test]
    async fn the_signed_payload_is_written_and_every_piece_is_reported() {
        let (result, file, seen) = run(PAYLOAD, SIGNATURE, TEST_KEY, 1024).await;
        assert_eq!(result, Ok(()));
        assert_eq!(file, PAYLOAD);
        assert_eq!(seen, PAYLOAD.len() as u64);
    }

    #[tokio::test]
    async fn a_tampered_file_a_wrong_key_and_a_broken_signature_are_refused() {
        let mut tampered = PAYLOAD.to_vec();
        tampered[0] ^= 1;
        assert!(run(&tampered, SIGNATURE, TEST_KEY, 1024).await.0.is_err(), "one changed byte");
        assert!(run(&PAYLOAD[..PAYLOAD.len() - 1], SIGNATURE, TEST_KEY, 1024).await.0.is_err(), "a cut file");
        assert!(run(PAYLOAD, SIGNATURE, PUBLIC_KEY, 1024).await.0.is_err(), "signed by another key than Faber release key");
        assert!(run(PAYLOAD, "not a signature", TEST_KEY, 1024).await.0.is_err());
        assert!(run(PAYLOAD, SIGNATURE, "not a key", 1024).await.0.is_err());
    }

    #[tokio::test]
    async fn a_file_over_the_limit_is_refused_before_and_while_it_arrives() {
        let limit = PAYLOAD.len() as u64 - 1;
        let mut file = Vec::new();
        let mut pieces = Pieces::of(PAYLOAD, 5, Some(PAYLOAD.len() as u64));
        let announced = receive(&mut pieces, &mut file, SIGNATURE, TEST_KEY, limit, |_, _| {});
        assert_eq!(announced.await, Err("the update is too large".into()));
        assert!(file.is_empty(), "nothing is written for a size the server announced");
        let (mut pieces, mut sink) = (Pieces::of(PAYLOAD, 5, None), Vec::new());
        let unannounced = receive(&mut pieces, &mut sink, SIGNATURE, TEST_KEY, limit, |_, _| {});
        assert_eq!(unannounced.await, Err("the update is too large".into()), "a server that says nothing is counted too");
    }

    #[test]
    fn the_key_is_the_one_every_installed_faber_trusts() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../../src-tauri/tauri.conf.json")).unwrap();
        assert_eq!(conf["plugins"]["updater"]["pubkey"], PUBLIC_KEY, "a different key strands every install");
        assert_eq!(
            conf["plugins"]["updater"]["endpoints"],
            serde_json::json!([STABLE_FEED]),
            "installs shipped so far read this address; changing it strands them"
        );
        assert_ne!(PREVIEW_FEED, STABLE_FEED);
        assert!(PREVIEW_FEED.ends_with("/preview-channel/latest.json"), "a fixed tag, never the newest release");
    }

    #[test]
    fn installer_addresses_must_use_https() {
        assert!(https_url("https://example.org/update").is_ok());
        assert!(https_url("http://example.org/update").is_err());
        assert!(https_url("file:///tmp/update").is_err());
        assert!(https_url("not an address").is_err());
    }

    fn sample() -> Manifest {
        Manifest::parse(
            br#"{
            "version":"1.2.3", "notes":"notes", "pub_date":"2026-01-01T00:00:00Z",
            "platforms":{
                "windows-x86_64":{"signature":"s1","url":"https://example.org/1"},
                "darwin-aarch64":{"signature":"s2","url":"https://example.org/2"},
                "darwin-x86_64":{"signature":"s3","url":"https://example.org/3"},
                "linux-x86_64":{"signature":"s4","url":"https://example.org/4"}
            }}"#,
        )
        .unwrap()
    }

    #[test]
    fn the_manifest_reads_every_platform_and_refuses_a_missing_one() {
        let manifest = sample();
        assert_eq!((manifest.version.as_str(), manifest.notes.as_deref()), ("1.2.3", Some("notes")));
        for (key, url) in [("windows-x86_64", "1"), ("darwin-aarch64", "2"), ("darwin-x86_64", "3"), ("linux-x86_64", "4")] {
            assert_eq!(manifest.platform(key).unwrap().url, format!("https://example.org/{url}"));
        }
        assert!(manifest.platform("linux-aarch64").is_err());
        assert!(Manifest::parse(b"{}").is_err() && Manifest::parse(b"not json").is_err());
        let bare = Manifest::parse(br#"{"version":"1.0.0","platforms":{}}"#).unwrap();
        assert!(bare.notes.is_none() && bare.pub_date.is_none(), "notes and date are optional");
    }

    #[test]
    fn a_newer_release_is_found_only_for_a_system_it_has_an_installer_for() {
        let current: semver::Version = "1.2.2".parse().unwrap();
        let found = newer(&sample(), "windows-x86_64", &current, false).unwrap().unwrap();
        assert_eq!((found.version.as_str(), found.platform.signature.as_str()), ("1.2.3", "s1"));
        assert!(newer(&sample(), "linux-aarch64", &current, false).is_err(), "newer, but nothing to install here");
        let same: semver::Version = "1.2.3".parse().unwrap();
        assert!(newer(&sample(), "linux-aarch64", &same, false).unwrap().is_none(), "not newer: a missing platform never matters");
        let preview: semver::Version = "1.2.3-preview.4".parse().unwrap();
        assert!(newer(&sample(), "windows-x86_64", &preview, false).unwrap().is_none(), "R156: previews off wait for the next line");
        assert!(newer(&sample(), "windows-x86_64", &preview, true).unwrap().is_some());
        let tagged =
            Manifest::parse(br#"{"version":"v1.3.0","platforms":{"windows-x86_64":{"signature":"s","url":"https://x"}}}"#).unwrap();
        assert_eq!(newer(&tagged, "windows-x86_64", &current, false).unwrap().unwrap().version, "1.3.0", "a leading v is read");
        let broken = Manifest::parse(br#"{"version":"next","platforms":{}}"#).unwrap();
        assert!(newer(&broken, "windows-x86_64", &current, false).is_err());
    }

    #[test]
    fn this_system_has_a_manifest_key_when_faber_ships_for_it() {
        if cfg!(all(windows, target_arch = "x86_64")) {
            assert_eq!(platform_key(), Some("windows-x86_64"));
        }
    }
}
