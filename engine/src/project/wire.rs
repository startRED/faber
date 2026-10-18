//! The limits and checks the check receipts and the kit's labels share: bounded sizes, ids, hashes and paths that stay
//! inside a worktree. Anything outside them is refused before anything acts on it.
//!
//! Versioned and documented with examples in `docs/design/projects.md`.

use super::{clean_path, valid_id};

mod rework;
pub use rework::*;

/// The largest text, in bytes, read at all.
pub const MAX_ENVELOPE: usize = 512 * 1024;
/// The most paths (or other list items) one record may carry.
pub const MAX_PATHS: usize = 64;
/// The largest file content, in bytes.
pub const MAX_FILE: usize = 256 * 1024;
/// The largest free text (a summary, a question), in bytes.
pub const MAX_TEXT: usize = 8 * 1024;

/// Why a record was refused, as an `error.wire.*` key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    TooBig,
    Malformed,
    Schema,
    Stale,
    Path,
}

impl WireError {
    pub fn key(&self) -> &'static str {
        match self {
            WireError::TooBig => "error.wire.tooBig",
            WireError::Malformed => "error.wire.malformed",
            WireError::Schema => "error.wire.schema",
            WireError::Stale => "error.wire.stale",
            WireError::Path => "error.wire.path",
        }
    }
}

fn text(s: &str) -> Result<(), WireError> {
    if s.len() > MAX_TEXT { Err(WireError::TooBig) } else { Ok(()) }
}

fn texts(list: &[String]) -> Result<(), WireError> {
    if list.len() > MAX_PATHS {
        return Err(WireError::TooBig);
    }
    list.iter().try_for_each(|s| text(s))
}

/// A path a worker may name: inside the worktree and not the worktree itself.
fn path(p: &str) -> Result<(), WireError> {
    super::kit::delivery::scoped_path(p)?;
    match clean_path(p) {
        Some(clean) if !clean.is_empty() => Ok(()),
        _ => Err(WireError::Path),
    }
}

fn paths(list: &[String]) -> Result<(), WireError> {
    if list.len() > MAX_PATHS {
        return Err(WireError::TooBig);
    }
    list.iter().try_for_each(|p| path(p))
}

fn hash(h: &str) -> Result<(), WireError> {
    let ok = h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if ok { Ok(()) } else { Err(WireError::Malformed) }
}

fn ids(list: &[String]) -> Result<(), WireError> {
    if list.len() > MAX_PATHS {
        return Err(WireError::TooBig);
    }
    if list.iter().all(|id| valid_id(id)) { Ok(()) } else { Err(WireError::Malformed) }
}
