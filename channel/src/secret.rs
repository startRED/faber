//! The channel's secret, new on every run of Faber, and the tokens that name the conversation an AI works in.
//!
//! The running Faber writes 32 random bytes as hex to `<data>/channel/secret`, a file only the signed-in user can read, and
//! every client must say them in its `hello`. The pipe or socket already admits only that user; the secret adds that a
//! client can read Faber's own data folder in this run, so a process that only finds the channel's name (another account, a
//! sandboxed or lower-integrity process, a page in a browser) gets nothing, and a secret copied yesterday opens nothing today.
//!
//! An AI's token is `fa1.<session and conversation in base64url>.<HMAC-SHA256 of it under the secret, base64url>`: Faber gives it to
//! the CLI that runs a conversation, and the channel reads from it which conversation, and so which family, the AI acts for.
//! Any program of the same user may copy a live token from another process; the rules of who may do what keep a well-meant
//! AI on its own work, and are no wall against a program of the person's that sets out to break them.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The file of the secret inside the data folder.
pub fn file(data: &Path) -> PathBuf {
    data.join("channel").join("secret")
}

#[derive(Clone)]
pub struct Secret([u8; 32], Arc<Mutex<HashMap<String, String>>>, Arc<AtomicU64>, tokio::sync::watch::Sender<u64>);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

impl Secret {
    /// 32 bytes from the system's random source.
    pub fn fresh() -> io::Result<Secret> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
        Ok(Secret(bytes, Arc::default(), Arc::default(), tokio::sync::watch::channel(0).0))
    }

    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The secret a client read, from its hex. Anything else is no secret.
    pub fn from_hex(text: &str) -> Option<Secret> {
        let text = text.trim();
        if text.len() != 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(Secret(bytes, Arc::default(), Arc::default(), tokio::sync::watch::channel(0).0))
    }

    /// Whether a client said this secret, compared in a time that does not depend on where the two first differ.
    pub fn said_by(&self, said: &str) -> bool {
        Secret::from_hex(said).is_some_and(|other| same(&self.0, &other.0))
    }

    /// Writes it where clients read it, replacing the last run's: whole and renamed into place, so a client never reads half
    /// of one. On Windows the file's list names only the signed-in user; elsewhere it is `0600` in a `0700` folder.
    pub fn write(&self, data: &Path) -> io::Result<()> {
        let path = file(data);
        let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no channel folder"))?;
        private_folder(dir)?;
        let part = dir.join(format!("secret.{}.part", std::process::id()));
        let _ = std::fs::remove_file(&part);
        private_file(&part, self.hex().as_bytes())?;
        std::fs::rename(&part, &path).inspect_err(|_| {
            let _ = std::fs::remove_file(&part);
        })
    }

    /// The secret of the Faber that keeps its data in `data`: `NotFound` when none ever ran there.
    pub fn read(data: &Path) -> io::Result<Secret> {
        let text = std::fs::read_to_string(file(data))?;
        Secret::from_hex(&text).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "the channel's secret is not one"))
    }

    /// The token of an AI that works in `conversation`.
    pub fn token(&self, conversation: &str) -> String {
        let payload = format!("{}:{}", self.2.fetch_add(1, Ordering::Relaxed), conversation);
        let token = format!("fa1.{}.{}", URL_SAFE_NO_PAD.encode(&payload), URL_SAFE_NO_PAD.encode(self.tag(&payload)));
        self.1.lock().unwrap_or_else(|e| e.into_inner()).insert(token.clone(), conversation.into());
        token
    }

    /// The conversation a token names, when this secret made it.
    pub fn conversation_of(&self, token: &str) -> Option<String> {
        if !self.signed(token) {
            return None;
        }
        self.1.lock().unwrap_or_else(|e| e.into_inner()).get(token).cloned()
    }

    /// Admit a request under the same lock that revokes its session.
    pub(crate) fn admit<T>(&self, token: Option<&str>, run: impl FnOnce() -> T) -> Option<T> {
        if let Some(token) = token {
            let active = self.1.lock().unwrap_or_else(|e| e.into_inner());
            active.contains_key(token).then(run)
        } else {
            Some(run())
        }
    }

    pub(crate) fn signed(&self, token: &str) -> bool {
        let mut parts = token.trim().split('.');
        let (Some("fa1"), Some(id), Some(tag), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else { return false };
        let Some(payload) = URL_SAFE_NO_PAD.decode(id).ok().and_then(|bytes| String::from_utf8(bytes).ok()) else { return false };
        let Some(tag) = URL_SAFE_NO_PAD.decode(tag).ok() else { return false };
        same(&tag, &self.tag(&payload))
    }

    /// Revokes exactly this session's token, including after its conversation resumes.
    pub fn revoke(&self, token: &str) {
        self.1.lock().unwrap_or_else(|e| e.into_inner()).remove(token);
        self.3.send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    pub(crate) fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.3.subscribe()
    }

    /// HMAC-SHA256 (RFC 2104) of the conversation id under the secret, with a label so the tag means nothing else.
    fn tag(&self, conversation: &str) -> [u8; 32] {
        const BLOCK: usize = 64;
        let mut key = [0u8; BLOCK];
        key[..32].copy_from_slice(&self.0);
        let pad = |byte: u8| key.map(|k| k ^ byte);
        let inner = Sha256::new().chain_update(pad(0x36)).chain_update(b"faber agent\0").chain_update(conversation.as_bytes()).finalize();
        Sha256::new().chain_update(pad(0x5c)).chain_update(inner).finalize().into()
    }
}

/// Equal bytes, compared without stopping at the first difference.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(windows)]
fn private_folder(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(windows)]
fn private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    faber_winproc::write_user_only(path, bytes)
}

#[cfg(unix)]
fn private_folder(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let meta = std::fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.uid() != rustix::process::geteuid().as_raw() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the channel folder is not this user's own folder"));
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(unix)]
fn private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc_nofollow()).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// `O_NOFOLLOW`, so a link planted at the file's name is never written through.
#[cfg(unix)]
fn libc_nofollow() -> i32 {
    rustix::fs::OFlags::NOFOLLOW.bits() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_is_new_each_time_and_reads_back_from_its_file() {
        let data = tempfile::tempdir().unwrap();
        let (one, two) = (Secret::fresh().unwrap(), Secret::fresh().unwrap());
        assert_ne!(one.hex(), two.hex());
        assert_eq!(one.hex().len(), 64);
        one.write(data.path()).unwrap();
        two.write(data.path()).unwrap();
        let read = Secret::read(data.path()).unwrap();
        assert!(read.said_by(&two.hex()) && !read.said_by(&one.hex()), "the last run's secret replaces the one before");
        let left: Vec<_> = std::fs::read_dir(file(data.path()).parent().unwrap()).unwrap().collect();
        assert_eq!(left.len(), 1, "no part file is left behind");
    }

    #[test]
    fn nothing_but_the_exact_secret_is_said() {
        let secret = Secret::fresh().unwrap();
        assert!(secret.said_by(&secret.hex().to_uppercase()), "hex in either case");
        for wrong in ["", "00", &"0".repeat(64), &format!("{}x", &secret.hex()[..63]), &format!("{}0", secret.hex())] {
            assert!(!secret.said_by(wrong), "{wrong}");
        }
        assert_eq!(Secret::read(Path::new("/no/such/faber")).unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_token_names_its_conversation_only_under_the_secret_that_made_it() {
        let (secret, other) = (Secret::fresh().unwrap(), Secret::fresh().unwrap());
        let token = secret.token("c-1.é/x");
        assert_eq!(secret.conversation_of(&token).as_deref(), Some("c-1.é/x"));
        assert_eq!(other.conversation_of(&token), None, "a token from another run");
        let forged = format!("fa1.{}.{}", URL_SAFE_NO_PAD.encode("c-2"), token.rsplit('.').next().unwrap());
        assert_eq!(secret.conversation_of(&forged), None, "another conversation under the same tag");
        for bad in ["", "fa1", "fa1..", "fa2.YQ.YQ", &format!("{token}.x"), &token.replace("fa1", "fa0")] {
            assert_eq!(secret.conversation_of(bad), None, "{bad}");
        }
    }

    #[test]
    fn revoking_one_session_does_not_restore_it_when_the_conversation_reopens() {
        let secret = Secret::fresh().unwrap();
        let first = secret.token("one");
        let next = secret.token("one");
        assert_ne!(first, next);
        secret.revoke(&first);
        assert!(secret.signed(&first));
        assert_eq!(secret.conversation_of(&first), None);
        assert_eq!(secret.conversation_of(&next).as_deref(), Some("one"));
    }

    #[test]
    fn the_tag_is_hmac_sha256() {
        // RFC 4231 test case 2 uses a short key; this checks the construction with our own key and label instead.
        let secret = Secret::from_hex(&"0b".repeat(32)).unwrap();
        let mut key = [0u8; 64];
        key[..32].copy_from_slice(&[0x0b; 32]);
        let inner = Sha256::new().chain_update(key.map(|k| k ^ 0x36)).chain_update(b"faber agent\0c1").finalize();
        let outer: [u8; 32] = Sha256::new().chain_update(key.map(|k| k ^ 0x5c)).chain_update(inner).finalize().into();
        assert_eq!(secret.tag("c1"), outer);
    }
}
