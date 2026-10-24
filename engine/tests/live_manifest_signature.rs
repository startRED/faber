//! Public synthetic signatures, independently generated and checked by Node; no production signing material.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use faber_engine::live_manifest::{Accepted, Invalid};
use faber_engine::live_manifest_signature::{Error, VerifiedDocument, validate_signed_document};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const NOW: &str = "2026-10-24T12:00:00Z";

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/live-manifest-signatures.json")).unwrap()
}

fn pins(fixture: &Value) -> BTreeMap<String, [u8; 32]> {
    let hex = fixture["public_key_hex"].as_str().unwrap();
    let key: Vec<u8> = (0..hex.len()).step_by(2).map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap()).collect();
    BTreeMap::from([("fixture".into(), key.try_into().unwrap())])
}

fn check(envelope: &Value, pins: &BTreeMap<String, [u8; 32]>) -> Result<VerifiedDocument, Error> {
    validate_signed_document(&serde_json::to_vec(envelope).unwrap(), pins, NOW, "0.2.0", None)
}

#[test]
fn native_signature_verification_preserves_exact_unicode_bytes_and_restricted_policy() {
    let fixture = fixture();
    let verified = check(&fixture["document"], &pins(&fixture)).unwrap();
    assert_eq!(verified.payload(), fixture["document_payload"].as_str().unwrap().as_bytes());
    assert_eq!(verified.document().revision, 7);
    assert_eq!(verified.document().policies[1].install, "suspend");
}

#[test]
fn altered_payload_is_refused_before_attempting_to_parse_its_metadata() {
    let fixture = fixture();
    let mut envelope = fixture["document"].clone();
    envelope["payload"] = json!(STANDARD.encode(b"not JSON"));
    assert!(matches!(check(&envelope, &pins(&fixture)), Err(Error::Signature)));
    assert!(matches!(check(&fixture["opaque"], &pins(&fixture)), Err(Error::Document(Invalid::Schema))));
}

#[test]
fn wrong_signatures_and_public_keys_cannot_authenticate_a_document() {
    let fixture = fixture();
    let original = &fixture["document"];
    let mut signature = STANDARD.decode(original["signature"].as_str().unwrap()).unwrap();
    signature[0] ^= 1;
    let mut envelope = original.clone();
    envelope["signature"] = json!(STANDARD.encode(&signature));
    assert!(matches!(check(&envelope, &pins(&fixture)), Err(Error::Signature)));
    envelope["signature"] = json!(STANDARD.encode(&signature[..63]));
    assert!(matches!(check(&envelope, &pins(&fixture)), Err(Error::Signature)));
    assert!(matches!(check(original, &BTreeMap::new()), Err(Error::Key)));
    assert!(matches!(check(original, &BTreeMap::from([("fixture".into(), [0; 32])])), Err(Error::Signature)));
    envelope = original.clone();
    envelope["key_id"] = json!("unknown");
    assert!(matches!(check(&envelope, &pins(&fixture)), Err(Error::Key)));
}

#[test]
fn invalid_authenticated_utf8_is_refused_after_signature_verification() {
    let fixture = fixture();
    let mut envelope = fixture["invalid_utf8"].clone();
    assert!(matches!(check(&envelope, &pins(&fixture)), Err(Error::Utf8)));
    let mut signature = STANDARD.decode(envelope["signature"].as_str().unwrap()).unwrap();
    signature[0] ^= 1;
    envelope["signature"] = json!(STANDARD.encode(signature));
    assert!(matches!(check(&envelope, &pins(&fixture)), Err(Error::Signature)));
}

#[test]
fn a_valid_signature_does_not_override_schema_freshness_or_revision_rules() {
    let fixture = fixture();
    let pins = pins(&fixture);
    assert!(matches!(check(&fixture["bad_schema"], &pins), Err(Error::Document(Invalid::Schema))));
    assert!(matches!(check(&fixture["expired"], &pins), Err(Error::Document(Invalid::Freshness))));
    let bytes = serde_json::to_vec(&fixture["document"]).unwrap();
    assert!(matches!(validate_signed_document(&bytes, &pins, NOW, "0.0.1", None), Err(Error::Document(Invalid::Support))));
    let verified = check(&fixture["document"], &pins).unwrap();
    let accepted = Accepted { revision: 7, digest: verified.document().digest.clone() };
    assert!(validate_signed_document(&bytes, &pins, NOW, "0.2.0", Some(&accepted)).is_ok());
    let newer = Accepted { revision: 8, digest: accepted.digest };
    assert!(matches!(validate_signed_document(&bytes, &pins, NOW, "0.2.0", Some(&newer)), Err(Error::Document(Invalid::Revision))));
}

#[test]
fn envelope_requires_exact_named_fields_instead_of_feed_supplied_trust() {
    let fixture = fixture();
    let original = &fixture["document"];
    for bad in [
        json!([original["key_id"], original["payload"], original["signature"]]),
        json!({"key_id": "fixture", "payload": original["payload"], "signature": original["signature"], "public_key": fixture["public_key_hex"]}),
        json!({"key_id": "fixture", "payload": original["payload"]}),
        Value::Null,
    ] {
        assert!(matches!(check(&bad, &pins(&fixture)), Err(Error::Envelope)));
    }
    let bytes = serde_json::to_string(original).unwrap();
    for duplicated in [
        bytes.replacen("\"key_id\":\"fixture\"", "\"key_id\":\"unknown\",\"key_id\":\"fixture\"", 1),
        bytes.replacen("\"payload\":", "\"payload\":\"AAAA\",\"payload\":", 1),
    ] {
        assert_ne!(duplicated, bytes);
        assert!(matches!(validate_signed_document(duplicated.as_bytes(), &pins(&fixture), NOW, "0.2.0", None), Err(Error::Envelope)));
    }
}

#[test]
fn key_id_and_base64_cannot_be_repaired_or_normalized_into_trust() {
    let fixture = fixture();
    let original = &fixture["document"];
    for key in ["", "Fixture", "-fixture", "__proto__", &"a".repeat(65)] {
        let mut envelope = original.clone();
        envelope["key_id"] = json!(key);
        assert!(matches!(check(&envelope, &pins(&fixture)), Err(Error::Envelope)));
    }
    for field in ["payload", "signature"] {
        for bad in ["", "Zg", "Zg===", "Zh==", " Zg==", "Zg==\n", "AA__"] {
            let mut envelope = original.clone();
            envelope[field] = json!(bad);
            assert!(matches!(check(&envelope, &pins(&fixture)), Err(Error::Base64)));
        }
    }
    let mut envelope = original.clone();
    let mut signature = envelope["signature"].as_str().unwrap().to_owned();
    let at = signature.len() - 3;
    let replacement = match signature.as_bytes()[at] {
        b'A' => "B",
        b'Q' => "R",
        b'g' => "h",
        b'w' => "x",
        _ => panic!("noncanonical fixture"),
    };
    signature.replace_range(at..at + 1, replacement);
    envelope["signature"] = json!(signature);
    assert!(matches!(check(&envelope, &pins(&fixture)), Err(Error::Base64)));
}

#[test]
fn raw_envelope_size_is_bounded_before_json_decoding() {
    let fixture = fixture();
    let pins = pins(&fixture);
    let mut bytes = serde_json::to_vec(&fixture["document"]).unwrap();
    bytes.resize(1024 * 1024, b' ');
    assert!(validate_signed_document(&bytes, &pins, NOW, "0.2.0", None).is_ok());
    bytes.push(b' ');
    assert!(matches!(validate_signed_document(&bytes, &pins, NOW, "0.2.0", None), Err(Error::Envelope)));
    for bad in [&[][..], &[0xc3, 0x28][..], b"{".as_slice()] {
        assert!(matches!(validate_signed_document(bad, &pins, NOW, "0.2.0", None), Err(Error::Envelope)));
    }
}
