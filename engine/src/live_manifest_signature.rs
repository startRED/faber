//! Offline authentication; a verified document does not grant installation authority.

use crate::live_manifest::{Accepted, Document, Invalid, validate_document};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Envelope,
    Key,
    Base64,
    Signature,
    Utf8,
    Document(Invalid),
}

#[derive(Debug)]
pub struct VerifiedDocument {
    payload: Vec<u8>,
    document: Document,
}

impl VerifiedDocument {
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    key_id: String,
    payload: String,
    signature: String,
}

fn decode(value: &str) -> Result<Vec<u8>, Error> {
    let bytes = STANDARD.decode(value).map_err(|_| Error::Base64)?;
    if bytes.is_empty() || STANDARD.encode(&bytes) != value {
        return Err(Error::Base64);
    }
    Ok(bytes)
}

pub fn validate_signed_document(
    envelope: &[u8],
    pins: &BTreeMap<String, [u8; 32]>,
    now: &str,
    app_version: &str,
    previous: Option<&Accepted>,
) -> Result<VerifiedDocument, Error> {
    if envelope.len() > 1024 * 1024 || envelope.iter().find(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n')) != Some(&b'{') {
        return Err(Error::Envelope);
    }
    // Serde structs also accept positional arrays; the object guard above forbids that form.
    let envelope: Envelope = serde_json::from_slice(envelope).map_err(|_| Error::Envelope)?;
    let id = envelope.key_id.as_bytes();
    if id.is_empty()
        || id.len() > 64
        || !id[0].is_ascii_lowercase() && !id[0].is_ascii_digit()
        || !id.iter().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    {
        return Err(Error::Envelope);
    }
    let key = pins.get(&envelope.key_id).ok_or(Error::Key)?;
    let payload = decode(&envelope.payload)?;
    let signature = decode(&envelope.signature)?;
    if signature.len() != 64 {
        return Err(Error::Signature);
    }
    let provider = rustls::crypto::ring::default_provider();
    let algorithm = provider
        .signature_verification_algorithms
        .mapping
        .iter()
        .find(|(scheme, _)| *scheme == rustls::SignatureScheme::ED25519)
        .and_then(|(_, algorithms)| algorithms.first())
        .ok_or(Error::Signature)?;
    algorithm.verify_signature(key, &payload, &signature).map_err(|_| Error::Signature)?;
    std::str::from_utf8(&payload).map_err(|_| Error::Utf8)?;
    let document = validate_document(&payload, now, app_version, previous).map_err(Error::Document)?;
    Ok(VerifiedDocument { payload, document })
}
