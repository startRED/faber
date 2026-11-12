//! Where a conversation can go back to: a saved point's summary and the vendor session a reply can fork from. `rewind`
//! saves and restores the points; the conversation model keeps these.

use crate::model::Vendor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub saved: usize,
    pub skipped: usize,
    pub unavailable: bool,
    pub incomplete: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    pub entry: Option<String>,
    pub turn: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PointInfo {
    pub id: String,
    pub message_id: String,
    pub turn_id: String,
    pub at: u64,
    pub coverage: Coverage,
    pub sealed: bool,
    pub cursor: Cursor,
}

/// A qualified native cut; an incomplete point never gets one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeRewind {
    pub thread: String,
    pub before_turn: String,
}

/// Where a vendor's own session can fork from: the reply's native turn or entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePoint {
    pub session: String,
    pub cursor: Cursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeFork {
    pub vendor: Vendor,
    pub point: NativePoint,
}

impl NativePoint {
    pub fn valid(&self, vendor: Vendor) -> bool {
        let id = |id: &str| !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
        if !id(&self.session) {
            return false;
        }
        match vendor {
            Vendor::Claude => {
                uuid::Uuid::parse_str(&self.session).is_ok()
                    && self.cursor.entry.as_deref().is_some_and(|entry| uuid::Uuid::parse_str(entry).is_ok())
            }
            Vendor::Codex => self.cursor.turn.as_deref().is_some_and(id),
            Vendor::Gemini => false,
        }
    }
}
