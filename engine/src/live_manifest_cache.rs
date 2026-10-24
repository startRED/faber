//! Offline persistence only; cached metadata never grants installation authority.

use crate::live_manifest::Accepted;
use crate::live_manifest_signature::{self, VerifiedDocument, validate_signed_document};
use crate::store::{Disk, Fault, replace_with};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug)]
pub enum Error {
    Busy,
    State,
    Io(io::Error),
    Write(Fault),
    Verification(live_manifest_signature::Error),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Floor {
    revision: u64,
    digest: String,
}

fn lock(root: &Path) -> Result<File, Error> {
    fs::create_dir_all(root).map_err(Error::Io)?;
    // Never replace or remove this file: every reader and publisher must lock the same identity.
    let file =
        OpenOptions::new().read(true).write(true).create(true).truncate(false).open(root.join("manifest.lock")).map_err(Error::Io)?;
    // shortcut: lock failures share Busy; distinguish unsupported locks before adding automatic retries.
    file.try_lock().map_err(|_| Error::Busy)?;
    Ok(file)
}

fn read(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, Error> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::Io(error)),
    };
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes).map_err(Error::Io)?;
    if bytes.len() > limit {
        return Err(Error::State);
    }
    Ok(Some(bytes))
}

fn floor(root: &Path) -> Result<Option<Accepted>, Error> {
    let marker = read(&root.join("initialized"), 16)?;
    let record = read(&root.join("accepted.json"), 4096)?;
    let bytes = match (marker, record) {
        (None, None) => return if read(&root.join("manifest.json"), 1024 * 1024)?.is_none() { Ok(None) } else { Err(Error::State) },
        (Some(marker), Some(bytes)) if marker == b"1\n" => bytes,
        _ => return Err(Error::State),
    };
    // Serde structs also accept positional arrays; trust state permits only named fields.
    if bytes.iter().find(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n')) != Some(&b'{') {
        return Err(Error::State);
    }
    let floor: Floor = serde_json::from_slice(&bytes).map_err(|_| Error::State)?;
    if floor.revision == 0
        || floor.revision > 9_007_199_254_740_991
        || floor.digest.len() != 64
        || !floor.digest.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Error::State);
    }
    Ok(Some(Accepted { revision: floor.revision, digest: floor.digest }))
}

pub fn load(root: &Path, pins: &BTreeMap<String, [u8; 32]>, now: &str, app_version: &str) -> Result<Option<VerifiedDocument>, Error> {
    let _owner = lock(root)?;
    let Some(floor) = floor(root)? else {
        return Ok(None);
    };
    let envelope = read(&root.join("manifest.json"), 1024 * 1024)?.ok_or(Error::State)?;
    let verified = validate_signed_document(&envelope, pins, now, app_version, Some(&floor)).map_err(Error::Verification)?;
    if verified.document().revision != floor.revision || verified.document().digest != floor.digest {
        return Err(Error::State);
    }
    Ok(Some(verified))
}

pub fn publish(
    root: &Path,
    disk: &dyn Disk,
    envelope: &[u8],
    pins: &BTreeMap<String, [u8; 32]>,
    now: &str,
    app_version: &str,
) -> Result<VerifiedDocument, Error> {
    let _owner = lock(root)?;
    let previous = floor(root)?;
    let verified = validate_signed_document(envelope, pins, now, app_version, previous.as_ref()).map_err(Error::Verification)?;
    let floor = Floor { revision: verified.document().revision, digest: verified.document().digest.clone() };
    let bytes = serde_json::to_vec(&floor).map_err(|_| Error::State)?;
    if previous.is_none() {
        replace_with(disk, &root.join("initialized"), b"1\n", false).map_err(Error::Write)?;
    }
    // Advance the floor first. A failed cache replacement must never restore older authority.
    replace_with(disk, &root.join("accepted.json"), &bytes, false).map_err(Error::Write)?;
    replace_with(disk, &root.join("manifest.json"), envelope, false).map_err(Error::Write)?;
    Ok(verified)
}
