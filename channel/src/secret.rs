//! The channel's secret, new on every run of Faber, and the tokens that name the conversation an AI works in.
//!
//! The running Faber writes 32 random bytes as hex to `<data>/channel/secret`, a file only the signed-in user can read, and
//! every client must say them in its `hello`. The pipe or socket already admits only that user; the secret adds that a
//! client can read Faber's own data folder in this run, so a process that only finds the channel's name (another account, a
//! sandboxed or lower-integrity process, a page in a browser) gets nothing, and a secret copied yesterday opens nothing today.
//!
//! An AI's token contains 256 random bits, independent of the conversation and channel secret, and a tag.
//! The in-memory registry associates it with the conversation, and so the family the AI acts for.
//! Any program of the same user may copy a live token from another process; the rules of who may do what keep a well-meant
//! AI on its own work, and are no wall against a program of the person's that sets out to break them.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use subtle::ConstantTimeEq;

/// The file of the secret inside the data folder.
pub fn file(data: &Path) -> PathBuf {
    data.join("channel").join("secret")
}

struct Session {
    conversation: String,
    revoked: Arc<AtomicBool>,
    closed: tokio::sync::watch::Sender<bool>,
    tasks: Mutex<HashMap<u64, tokio::task::AbortHandle>>,
    next: AtomicU64,
    changed: tokio::sync::Notify,
    runtime: Mutex<Option<tokio::runtime::Handle>>,
}

struct RequestGuard(Arc<Session>, u64);
impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.0.tasks.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.1);
        self.0.changed.notify_one();
    }
}

#[derive(Clone)]
pub struct Secret([u8; 32], Arc<Mutex<HashMap<String, Arc<Session>>>>, tokio::sync::watch::Sender<u64>);

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
        Ok(Secret(bytes, Arc::default(), tokio::sync::watch::channel(0).0))
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
        Some(Secret(bytes, Arc::default(), tokio::sync::watch::channel(0).0))
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
    pub fn token(&self, conversation: &str) -> io::Result<String> {
        let mut random = [0u8; 32];
        getrandom::fill(&mut random).map_err(|e| io::Error::other(e.to_string()))?;
        let payload = URL_SAFE_NO_PAD.encode(random);
        let token = format!("fa1.{}.{}", payload, URL_SAFE_NO_PAD.encode(self.tag(&payload)));
        self.1.lock().unwrap_or_else(|e| e.into_inner()).insert(
            token.clone(),
            Arc::new(Session {
                conversation: conversation.into(),
                revoked: Arc::new(AtomicBool::new(false)),
                closed: tokio::sync::watch::channel(false).0,
                tasks: Mutex::default(),
                next: AtomicU64::new(0),
                changed: tokio::sync::Notify::new(),
                runtime: Mutex::new(None),
            }),
        );
        Ok(token)
    }

    /// The conversation a token names, when this secret made it.
    pub fn conversation_of(&self, token: &str) -> Option<String> {
        if !self.signed(token) {
            return None;
        }
        self.session(token).map(|session| session.conversation.clone())
    }

    fn session(&self, token: &str) -> Option<Arc<Session>> {
        let active = self.1.lock().unwrap_or_else(|e| e.into_inner());
        let mut found = None;
        for (key, session) in active.iter() {
            if same(key.as_bytes(), token.as_bytes()) {
                found = Some(session.clone());
            }
        }
        found
    }

    pub(crate) fn closed(&self, token: &str) -> Option<tokio::sync::watch::Receiver<bool>> {
        self.session(token).map(|session| session.closed.subscribe())
    }

    /// Registers the task before it can run, under the same lock that closes admission.
    pub(crate) fn spawn(
        &self,
        token: Option<&str>,
        run: impl FnOnce(Option<Arc<AtomicBool>>) -> super::server::Task + Send + 'static,
    ) -> bool {
        let Some(token) = token else {
            tokio::spawn(run(None));
            return true;
        };
        let active = self.1.lock().unwrap_or_else(|e| e.into_inner());
        let mut session = None;
        for (key, value) in active.iter() {
            if same(key.as_bytes(), token.as_bytes()) {
                session = Some(value.clone());
            }
        }
        let Some(session) = session else { return false };
        *session.runtime.lock().unwrap_or_else(|e| e.into_inner()) = Some(tokio::runtime::Handle::current());
        let id = session.next.fetch_add(1, Ordering::Relaxed);
        let (start, ready) = tokio::sync::oneshot::channel();
        let guard = RequestGuard(session.clone(), id);
        let flag = session.revoked.clone();
        let task = tokio::spawn(async move {
            let _guard = guard;
            if ready.await.is_ok() {
                run(Some(flag)).await;
            }
        });
        session.tasks.lock().unwrap_or_else(|e| e.into_inner()).insert(id, task.abort_handle());
        let _ = start.send(());
        true
    }

    pub(crate) fn signed(&self, token: &str) -> bool {
        let mut parts = token.trim().split('.');
        let (Some("fa1"), Some(id), Some(tag), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else { return false };
        if !URL_SAFE_NO_PAD.decode(id).is_ok_and(|bytes| bytes.len() == 32) {
            return false;
        }
        let Some(tag) = URL_SAFE_NO_PAD.decode(tag).ok() else { return false };
        same(&tag, &self.tag(id))
    }

    /// Revokes exactly this session's token, including after its conversation resumes.
    pub fn revoke(&self, token: &str) {
        let session = {
            let mut active = self.1.lock().unwrap_or_else(|e| e.into_inner());
            let mut key = None;
            for candidate in active.keys() {
                if same(candidate.as_bytes(), token.as_bytes()) {
                    key = Some(candidate.clone());
                }
            }
            let session = key.and_then(|key| active.remove(&key));
            if let Some(session) = &session {
                session.revoked.store(true, Ordering::Release);
                session.closed.send_replace(true);
            }
            session
        };
        if let Some(session) = session {
            let runtime = session.runtime.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(runtime) = runtime {
                runtime.spawn(async move {
                    let drain = async {
                        loop {
                            let notified = session.changed.notified();
                            if session.tasks.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
                                break;
                            }
                            notified.await;
                        }
                    };
                    if tokio::time::timeout(std::time::Duration::from_secs(5), drain).await.is_err() {
                        let tasks: Vec<_> = session.tasks.lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect();
                        for task in tasks {
                            task.abort();
                        }
                    }
                });
            }
        }
        self.2.send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    pub(crate) fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.2.subscribe()
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
    bool::from(a.ct_eq(b))
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
        let token = secret.token("c-1.é/x").unwrap();
        assert_eq!(secret.conversation_of(&token).as_deref(), Some("c-1.é/x"));
        assert_eq!(other.conversation_of(&token), None, "a token from another run");
        let forged = format!("fa1.{}.{}", URL_SAFE_NO_PAD.encode("c-2"), token.rsplit('.').next().unwrap());
        assert_eq!(secret.conversation_of(&forged), None, "another conversation under the same tag");
        for bad in ["", "fa1", "fa1..", "fa2.YQ.YQ", &format!("{token}.x"), &token.replace("fa1", "fa0")] {
            assert_eq!(secret.conversation_of(bad), None, "{bad}");
        }
    }

    #[test]
    fn session_tokens_are_random_and_cannot_be_reconstructed_from_the_channel_secret() {
        let secret = Secret::fresh().unwrap();
        let first = secret.token("one").unwrap();
        let second = secret.token("one").unwrap();
        let id = first.split('.').nth(1).unwrap();
        let random = URL_SAFE_NO_PAD.decode(id).unwrap();
        assert_eq!(random.len(), 32);
        assert_ne!(first, second);
        let old_payload = "0:one";
        let forged = format!("fa1.{}.{}", URL_SAFE_NO_PAD.encode(old_payload), URL_SAFE_NO_PAD.encode(secret.tag(old_payload)));
        assert_eq!(secret.conversation_of(&forged), None);
        assert_eq!(secret.conversation_of(&first).as_deref(), Some("one"));
    }

    #[test]
    fn revoking_one_session_does_not_restore_it_when_the_conversation_reopens() {
        let secret = Secret::fresh().unwrap();
        let first = secret.token("one").unwrap();
        let next = secret.token("one").unwrap();
        assert_ne!(first, next);
        secret.revoke(&first);
        assert!(secret.signed(&first));
        assert_eq!(secret.conversation_of(&first), None);
        assert_eq!(secret.conversation_of(&next).as_deref(), Some("one"));
    }

    #[tokio::test]
    async fn revoke_returns_immediately_and_aborts_a_stuck_request() {
        let secret = Secret::fresh().unwrap();
        let token = secret.token("one").unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (dropped, done) = tokio::sync::oneshot::channel::<()>();
        assert!(secret.spawn(Some(&token), move |_| Box::pin(async move {
            let _dropped = dropped;
            let _ = started.send(());
            std::future::pending::<()>().await;
        })));
        ready.await.unwrap();
        let before = std::time::Instant::now();
        secret.revoke(&token);
        assert!(before.elapsed() < std::time::Duration::from_secs(1));
        assert!(!secret.spawn(Some(&token), |_| Box::pin(async {})));
        assert!(tokio::time::timeout(std::time::Duration::from_secs(7), done).await.unwrap().is_err());
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
