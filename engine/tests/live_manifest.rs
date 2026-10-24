//! Offline metadata validation grants no installation authority and touches no account, disk or process.

use faber_engine::live_manifest::{Accepted, validate_document};
use serde_json::{Value, json};

const NOW: &str = "2026-10-24T12:00:00Z";

fn document() -> Value {
    json!({
        "schema": 1,
        "revision": 7,
        "issued_at": "2026-10-24T11:00:00Z",
        "expires_at": "2026-10-25T11:00:00Z",
        "minimum_app_version": "0.1.0",
        "catalog": [],
        "compatibility": [],
        "policies": [
            {"provider": "claude", "install": "allow", "launch": "allow", "reason": "Qualified policy fixture", "evidence": "1".repeat(64)},
            {"provider": "codex", "install": "suspend", "launch": "suspend", "reason": "Qualification unavailable", "evidence": "2".repeat(64)}
        ],
        "drivers": []
    })
}

fn check(value: &Value) -> bool {
    validate_document(&serde_json::to_vec(value).unwrap(), NOW, "0.2.0", None).is_ok()
}

#[test]
fn valid_offline_metadata_preserves_a_restrictive_policy() {
    let parsed = validate_document(&serde_json::to_vec(&document()).unwrap(), NOW, "0.2.0", None).unwrap();
    assert_eq!(parsed.revision, 7);
    assert_eq!(parsed.policies.len(), 2);
    assert_eq!(serde_json::to_value(&parsed.policies[1]).unwrap()["install"], "suspend");
}

#[test]
fn unknown_fields_and_duplicate_json_names_are_refused_at_every_depth() {
    let mut extra = document();
    extra["permissions"] = json!("full");
    assert!(!check(&extra));
    extra = document();
    extra["policies"][0]["command"] = json!("run");
    assert!(!check(&extra));
    let bytes = serde_json::to_string(&document()).unwrap();
    for duplicate in [
        bytes.replacen("\"revision\":7", "\"revision\":8,\"revision\":7", 1),
        bytes.replacen("\"install\":\"allow\"", "\"install\":\"suspend\",\"install\":\"allow\"", 1),
    ] {
        assert_ne!(duplicate, bytes);
        assert!(validate_document(duplicate.as_bytes(), NOW, "0.2.0", None).is_err());
    }
}

fn positional(value: &Value, fields: &[&str]) -> Value {
    Value::Array(fields.iter().map(|field| value[*field].clone()).collect())
}

#[test]
fn positional_arrays_cannot_replace_a_named_document_object() {
    let value = document();
    let array = positional(
        &value,
        &["schema", "revision", "issued_at", "expires_at", "minimum_app_version", "catalog", "compatibility", "policies", "drivers"],
    );
    assert!(!check(&array));
}

#[test]
fn positional_arrays_cannot_replace_nested_named_objects() {
    let item = &faber_engine::tools::catalog()[0];
    let mut value = qualified();
    value["catalog"] = json!([{"id": item.id, "commit": item.commit, "digest": item.digest}]);
    assert!(check(&value));
    for (section, fields) in [
        ("catalog", &["id", "commit", "digest"][..]),
        ("policies", &["provider", "install", "launch", "reason", "evidence"][..]),
        ("drivers", &["id", "provider", "variant"][..]),
        (
            "compatibility",
            &["provider", "version", "operating_system", "architecture", "artifact_digest", "driver", "outcome", "evidence", "reason"][..],
        ),
    ] {
        let mut changed = value.clone();
        changed[section][0] = positional(&value[section][0], fields);
        assert!(!check(&changed), "accepted positional {section}");
    }
    let mut changed = value.clone();
    changed["compatibility"][0]["evidence"] =
        positional(&value["compatibility"][0]["evidence"], &["startup", "turn", "cancellation", "cleanup"]);
    assert!(!check(&changed));
}

#[test]
fn invalid_revision_schema_and_expiration_never_pass_as_current() {
    for (field, bad) in [
        ("schema", json!(2)),
        ("revision", json!(0)),
        ("revision", json!(-1)),
        ("revision", json!(9007199254740992_u64)),
        ("revision", json!(1.5)),
        ("issued_at", json!("2026-10-24T12:00:01Z")),
        ("issued_at", json!("2026-02-30T11:00:00Z")),
        ("issued_at", json!("2026-10-24T11:00:60Z")),
        ("issued_at", json!("2026-10-24T11:00:00+00:00")),
        ("expires_at", json!(NOW)),
        ("expires_at", json!("2026-10-24T10:00:00Z")),
    ] {
        let mut value = document();
        value[field] = bad;
        assert!(!check(&value), "accepted invalid {field}: {value}");
    }
}

#[test]
fn provider_policy_cannot_be_missing_duplicated_or_extended_with_new_powers() {
    let mut missing = document();
    missing["policies"].as_array_mut().unwrap().pop();
    assert!(!check(&missing));
    let mut duplicate = document();
    duplicate["policies"][1] = duplicate["policies"][0].clone();
    assert!(!check(&duplicate));
    for (field, bad) in [("provider", "unknown"), ("install", "execute"), ("evidence", "latest"), ("reason", "")] {
        let mut value = document();
        value["policies"][0][field] = json!(bad);
        assert!(!check(&value));
    }
}

#[test]
fn revision_replay_and_equal_revision_equivocation_are_refused() {
    let bytes = serde_json::to_vec(&document()).unwrap();
    let first = validate_document(&bytes, NOW, "0.2.0", None).unwrap();
    let state = Accepted { revision: first.revision, digest: first.digest.clone() };
    assert!(validate_document(&bytes, NOW, "0.2.0", Some(&state)).is_ok());
    let reformatted = serde_json::to_vec_pretty(&document()).unwrap();
    assert!(validate_document(&reformatted, NOW, "0.2.0", Some(&state)).is_err());
    let mut value = document();
    value["revision"] = json!(6);
    assert!(validate_document(&serde_json::to_vec(&value).unwrap(), NOW, "0.2.0", Some(&state)).is_err());
    value["revision"] = json!(8);
    assert!(validate_document(&serde_json::to_vec(&value).unwrap(), NOW, "0.2.0", Some(&state)).is_ok());
}

#[test]
fn a_damaged_previous_revision_is_not_treated_as_no_revision_floor() {
    let bytes = serde_json::to_vec(&document()).unwrap();
    for state in [
        Accepted { revision: 0, digest: "a".repeat(64) },
        Accepted { revision: 6, digest: String::new() },
        Accepted { revision: 6, digest: "A".repeat(64) },
    ] {
        assert!(validate_document(&bytes, NOW, "0.2.0", Some(&state)).is_err());
    }
}

#[test]
fn minimum_app_version_is_compared_numerically_and_requires_canonical_components() {
    let mut value = document();
    value["minimum_app_version"] = json!("0.10.0");
    assert!(!check(&value));
    assert!(validate_document(&serde_json::to_vec(&value).unwrap(), NOW, "0.11.0", None).is_ok());
    for bad in ["0.01.0", "0.1", "0.1.0.0", "4294967296.0.0", "latest"] {
        value["minimum_app_version"] = json!(bad);
        assert!(!check(&value));
    }
}

#[test]
fn preview_app_versions_do_not_claim_their_future_stable_version() {
    let mut value = document();
    value["minimum_app_version"] = json!("0.2.0");
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(validate_document(&bytes, NOW, "0.2.0-preview.1", None).is_err());
    assert!(validate_document(&bytes, NOW, "0.2.0+build.1", None).is_ok());
    value["minimum_app_version"] = json!("0.1.9");
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(validate_document(&bytes, NOW, "0.2.0-preview.1", None).is_ok());
    for bad in ["0.2.0-", "0.2.0-preview..1", "0.2.0-01", "0.2.0+"] {
        assert!(validate_document(&bytes, NOW, bad, None).is_err());
    }
}

fn qualified() -> Value {
    let mut value = document();
    value["drivers"] = json!([{"id": "fixture", "provider": "claude", "variant": "stream-json"}]);
    value["compatibility"] = json!([{
        "provider": "claude", "version": "1.2.3", "operating_system": "windows", "architecture": "x86_64", "artifact_digest": "a".repeat(64),
        "driver": "fixture", "outcome": "qualified", "reason": "Synthetic schema fixture only",
        "evidence": {"startup": "1".repeat(64), "turn": "2".repeat(64), "cancellation": "3".repeat(64), "cleanup": "4".repeat(64)}
    }]);
    value
}

#[test]
fn qualification_needs_all_four_evidence_links_and_a_matching_compiled_driver() {
    assert!(check(&qualified()));
    let mut no_artifact = qualified();
    no_artifact["compatibility"][0]["artifact_digest"] = Value::Null;
    assert!(!check(&no_artifact));
    for field in ["startup", "turn", "cancellation", "cleanup"] {
        let mut value = qualified();
        value["compatibility"][0]["evidence"][field] = Value::Null;
        assert!(!check(&value), "qualified without {field}");
    }
    for (field, bad) in [
        ("driver", "missing"),
        ("provider", "codex"),
        ("version", "latest"),
        ("version", "^1.2.3"),
        ("version", "v1.2.3"),
        ("operating_system", "unknown"),
    ] {
        let mut value = qualified();
        value["compatibility"][0][field] = json!(bad);
        assert!(!check(&value));
    }
    let mut value = qualified();
    value["drivers"][0]["variant"] = json!("downloaded-script");
    assert!(!check(&value));
    value = qualified();
    value["drivers"][0]["command"] = json!("run downloaded-script");
    assert!(!check(&value));
    value = qualified();
    let duplicate = value["compatibility"][0].clone();
    value["compatibility"].as_array_mut().unwrap().push(duplicate);
    assert!(!check(&value));
}

#[test]
fn authenticated_bytes_still_require_utf8_and_bounded_document_parsing() {
    assert!(validate_document(&[0xc3, 0x28], NOW, "0.2.0", None).is_err());
    let mut oversized = vec![b' '; 1024 * 1024];
    oversized.extend(serde_json::to_vec(&document()).unwrap());
    assert!(validate_document(&oversized, NOW, "0.2.0", None).is_err());
    let mut value = qualified();
    let base = value["compatibility"][0].clone();
    value["compatibility"] = Value::Array(
        (0..513)
            .map(|i| {
                let mut row = base.clone();
                row["version"] = json!(format!("1.0.{i}"));
                row
            })
            .collect(),
    );
    assert!(!check(&value));
    let bytes = serde_json::to_string(&qualified()).unwrap();
    let duplicate = bytes.replacen("\"startup\":", "\"startup\":null,\"startup\":", 1);
    assert_ne!(duplicate, bytes);
    assert!(validate_document(duplicate.as_bytes(), NOW, "0.2.0", None).is_err());
}

#[test]
fn absent_security_fields_and_unbounded_reasons_are_not_defaulted() {
    for field in ["schema", "revision", "issued_at", "expires_at", "minimum_app_version", "catalog", "compatibility", "policies", "drivers"]
    {
        let mut value = document();
        value.as_object_mut().unwrap().remove(field);
        assert!(!check(&value), "defaulted absent {field}");
    }
    for reason in ["x".repeat(513), "line\nbreak".into(), "\0".into()] {
        let mut value = document();
        value["policies"][0]["reason"] = json!(reason);
        assert!(!check(&value));
    }
    let mut value = document();
    value["policies"][0]["reason"] = json!("á".repeat(512));
    assert!(check(&value));
}

#[test]
fn nested_executable_fields_missing_fields_and_malformed_hashes_are_refused() {
    for section in ["drivers", "compatibility", "policies"] {
        let mut value = qualified();
        value[section][0]["env"] = json!({"ACCESS": "full"});
        assert!(!check(&value));
        value = qualified();
        let first_key = value[section][0].as_object().unwrap().keys().next().unwrap().clone();
        value[section][0].as_object_mut().unwrap().remove(&first_key);
        assert!(!check(&value));
    }
    for bad in ["a".repeat(63), "A".repeat(64), "g".repeat(64)] {
        let mut value = qualified();
        value["compatibility"][0]["artifact_digest"] = json!(bad);
        assert!(!check(&value));
        value = qualified();
        value["compatibility"][0]["evidence"]["startup"] = json!(bad);
        assert!(!check(&value));
    }
    let mut value = qualified();
    value["compatibility"][0]["evidence"]["command"] = json!("run");
    assert!(!check(&value));
    value = qualified();
    value["compatibility"][0]["evidence"].as_object_mut().unwrap().remove("cleanup");
    assert!(!check(&value));
    value = qualified();
    value["compatibility"][0]["architecture"] = json!("unknown");
    assert!(!check(&value));
    value = qualified();
    value["compatibility"][0]["architecture"] = json!("aarch64");
    assert!(!check(&value)); // Windows arm64 is not part of the supported installer.
    value = qualified();
    let duplicate = value["drivers"][0].clone();
    value["drivers"].as_array_mut().unwrap().push(duplicate);
    assert!(!check(&value));
    value = qualified();
    value["compatibility"] = json!([]);
    assert!(!check(&value));
}

#[test]
fn catalog_cannot_duplicate_or_rebind_a_compiled_identity() {
    let item = &faber_engine::tools::catalog()[0];
    let original = json!({"id": item.id, "commit": item.commit, "digest": item.digest});
    let mut value = document();
    value["catalog"] = json!([original, original]);
    assert!(!check(&value));
    value["catalog"] = json!([original]);
    value["catalog"][0]["id"] = json!("unknown-item");
    assert!(!check(&value));
    value["catalog"] = json!([original]);
    value["catalog"][0]["commit"] = json!("0".repeat(40));
    assert!(!check(&value));
}

#[test]
fn compiled_driver_and_qualification_collections_have_enforced_bounds() {
    for count in [64, 65] {
        let mut value = qualified();
        let row = value["compatibility"][0].clone();
        value["drivers"] = Value::Array(
            (0..count)
                .map(|i| {
                    json!({
                        "id": format!("driver-{i}"), "provider": "claude", "variant": "stream-json"
                    })
                })
                .collect(),
        );
        value["compatibility"] = Value::Array(
            (0..count)
                .map(|i| {
                    let mut record = row.clone();
                    record["version"] = json!(format!("1.0.{i}"));
                    record["driver"] = json!(format!("driver-{i}"));
                    record
                })
                .collect(),
        );
        assert_eq!(check(&value), count == 64);
    }
    let mut value = qualified();
    let row = value["compatibility"][0].clone();
    value["compatibility"] = Value::Array(
        (0..512)
            .map(|i| {
                let mut record = row.clone();
                record["version"] = json!(format!("1.0.{i}"));
                record
            })
            .collect(),
    );
    assert!(check(&value));
    let mut value = document();
    value["revision"] = json!(9007199254740991_u64);
    value["minimum_app_version"] = json!("4294967295.0.0");
    assert!(validate_document(&serde_json::to_vec(&value).unwrap(), NOW, "4294967295.0.0", None).is_ok());
}

#[test]
fn remote_catalog_pins_must_match_the_existing_compiled_item() {
    let item = &faber_engine::tools::catalog()[0];
    let mut value = document();
    value["catalog"] = json!([{"id": item.id, "commit": item.commit, "digest": item.digest}]);
    assert!(check(&value));
    value["catalog"][0]["digest"] = json!("0".repeat(64));
    assert!(!check(&value));
    value["catalog"][0]["digest"] = json!(item.digest);
    value["catalog"][0]["args"] = json!(["--grant-access"]);
    assert!(!check(&value));
}
