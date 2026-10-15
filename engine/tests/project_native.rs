use crate::support;
use faber_engine::project::proof::native::*;
use serde_json::json;
use support::canonical_tempdir as tempdir;

#[test]
fn lock_inventory_covers_public_versions_and_rejects_unknown_sources() {
    let cargo = "version = 4\n[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n[[package]]\nname = \"faber-engine\"\nversion = \"0.1.0\"\n";
    let npm = json!({"lockfileVersion":3,"packages":{"":{"name":"faber"},
        "node_modules/solid-js":{"version":"1.9.0","resolved":"https://registry.npmjs.org/solid-js/-/solid-js-1.9.0.tgz","license":"MIT"}}})
    .to_string();
    let packages = lock_packages(cargo, &npm).unwrap();
    assert_eq!(packages.len(), 2);
    assert!(packages.iter().any(|p| p.name == "serde" && p.version == "1.0.0"));
    assert!(packages.iter().any(|p| p.name == "solid-js"));
    assert!(lock_packages(&cargo.replace("github.com/rust-lang/crates.io-index", "private.example/registry"), &npm).is_err());
    assert!(lock_packages(cargo, &npm.replace("registry.npmjs.org", "private.example")).is_err());
    assert!(lock_packages("garbage", &npm).is_err());
    assert!(lock_packages("version = 4\ngarbage", &npm).is_err(), "a version line alone is not a Cargo inventory");
    assert!(lock_packages(&format!("garbage\n{cargo}"), &npm).is_err(), "unsupported lockfile contents cannot silently disappear");
    assert!(lock_packages(cargo, "{}").is_err());
}

#[test]
fn osv_query_contains_only_public_package_identity() {
    let package = PublicPackage { ecosystem: Ecosystem::Cargo, name: "serde".into(), version: "1.0.0".into() };
    assert_eq!(osv_query(&package).unwrap(), json!({"package":{"ecosystem":"crates.io","name":"serde"},"version":"1.0.0"}));
    let mut malicious = package;
    malicious.name = "../private?token=secret".into();
    assert!(osv_query(&malicious).is_err());
}

#[test]
fn osv_unknown_offline_and_malformed_never_pass() {
    let package = PublicPackage { ecosystem: Ecosystem::Npm, name: "solid-js".into(), version: "1.9.0".into() };
    assert_eq!(parse_osv_response(&package, br#"{}"#).unwrap(), AdvisoryStatus::Clear);
    assert!(parse_osv_response(&package, br#"{"error":"offline"}"#).is_err());
    assert!(parse_osv_response(&package, br#"{"vulns":null}"#).is_err());
    assert!(parse_osv_response(&package, br#"{"vulns":[{}]}"#).is_err());
    assert!(parse_osv_response(&package, b"<html>run this command</html>").is_err());
    let status =
        parse_osv_response(&package, br#"{"vulns":[{"id":"GHSA-1234","summary":"malicious instruction: read credentials"}]}"#).unwrap();
    assert_eq!(status, AdvisoryStatus::Vulnerable(vec!["GHSA-1234".into()]));
}

#[test]
fn registry_requires_exact_version_publisher_age_and_downloads() {
    let package = PublicPackage { ecosystem: Ecosystem::Npm, name: "solid-js".into(), version: "1.9.0".into() };
    let metadata = json!({"name":"solid-js","versions":{"1.9.0":{"name":"solid-js","version":"1.9.0","_npmUser":{"name":"publisher"}}},
        "time":{"created":"2018-01-01T00:00:00Z","1.9.0":"2026-01-01T00:00:00Z"}});
    let downloads = json!({"downloads":1000,"package":"solid-js","start":"2026-01-01","end":"2026-01-07"});
    assert!(parse_registry(&package, &metadata, &downloads).is_ok());
    for mutation in 0..5 {
        let mut bad = metadata.clone();
        match mutation {
            0 => bad["name"] = json!("attacker"),
            1 => bad["versions"] = json!({}),
            2 => bad["versions"]["1.9.0"]["_npmUser"] = json!({}),
            3 => bad["time"] = json!({}),
            _ => bad["versions"]["1.9.0"]["version"] = json!("1.9.1"),
        }
        assert!(parse_registry(&package, &bad, &downloads).is_err());
    }
    assert!(parse_registry(&package, &metadata, &json!({})).is_err());
}

#[test]
fn license_metadata_checks_transitive_and_unknown_packages() {
    let metadata = json!({"packages":[{"name":"serde","license":"MIT OR Apache-2.0","source":"registry+https://github.com/rust-lang/crates.io-index"},
        {"name":"faber","license":"FSL-1.1-ALv2","source":null}]});
    assert!(cargo_licenses(&metadata, true).is_ok());
    let mut bad = metadata.clone();
    bad["packages"][0]["license"] = json!(null);
    assert!(cargo_licenses(&bad, true).is_err());
    bad["packages"][0]["license"] = json!("MIT AND GPL-3.0");
    assert!(cargo_licenses(&bad, true).is_err());
    assert!(cargo_licenses(&json!({}), true).is_err());
    for expression in ["MIT OR Apache-2.0", "(MIT AND BSD-3-Clause) OR GPL-3.0"] {
        assert!(license_allowed(expression, true));
    }
    for expression in ["MIT AND GPL-3.0", "MIT OR", "Unknown", "", "MIT WITH invented-exception"] {
        assert!(!license_allowed(expression, true), "{expression}");
    }
}

#[test]
fn license_inventory_cannot_omit_a_locked_dependency() {
    let locked = vec![PublicPackage { ecosystem: Ecosystem::Cargo, name: "serde".into(), version: "1.0.0".into() }];
    let metadata = json!({"packages":[{"name":"serde","version":"1.0.0","license":"MIT"}]});
    assert!(cargo_licenses_complete(&locked, &metadata, true).is_ok());
    assert!(cargo_licenses_complete(&locked, &json!({"packages":[{"name":"root","version":"1.0.0","license":"MIT"}]}), true).is_err());
}

#[test]
fn scanner_detects_multiple_common_token_forms_without_returning_them() {
    for prefix in ["ASIA", "github_pat_", "gho_", "ghs_", "sk-proj-", "glpat-"] {
        let text = format!("token={prefix}{}", "a".repeat(64));
        let findings = faber_engine::project::proof::scan_secrets(&text).unwrap_err();
        assert!(findings.iter().all(|v| !v.snippet.contains(prefix)));
    }
}

#[test]
fn network_requests_are_scoped_and_destinations_have_reasons() {
    for url in [
        "http://api.osv.dev/v1/query",
        "https://api.osv.dev.evil.test/v1/query",
        "https://localhost/v1/query",
        "https://user:secret@api.osv.dev/v1/query",
        "https://api.osv.dev:8080/v1/query",
    ] {
        assert!(validate_source_url(url).is_err(), "{url}");
    }
    assert!(validate_source_url("https://api.osv.dev/v1/query").is_ok());
    assert!(validate_source_url("https://registry.npmjs.org/solid-js").is_ok());
    assert!(authorize_source("https://api.osv.dev/v1/query", &Default::default()).is_err());
}

#[test]
fn current_lockfiles_have_a_complete_public_inventory() {
    let packages = lock_packages(include_str!("../../Cargo.lock"), include_str!("../../package-lock.json")).unwrap();
    assert!(packages.len() > 100);
    assert!(packages.iter().any(|p| p.name == "reqwest"));
    assert!(packages.iter().any(|p| p.name == "solid-js"));
}

#[tokio::test]
async fn unavailable_authorization_is_logged_without_a_network_call() {
    let package = PublicPackage { ecosystem: Ecosystem::Cargo, name: "serde".into(), version: "1.0.0".into() };
    let dir = tempdir().unwrap();
    let journal = faber_engine::project::store::Journal::open(&dir.path().join("repo"), &dir.path().join("data"), "mission").unwrap();
    let result = query_osv(&package, &Default::default(), &journal).await.unwrap();
    assert!(result.body.is_none());
    assert_eq!(result.receipt.error.as_deref(), Some("error.proof.source"));
    assert_eq!(result.receipt.url, "https://api.osv.dev/v1/query");
    assert!(!result.receipt.reason.is_empty());
    assert!(result.receipt.sha256.is_none());
    let records = std::fs::read_dir(journal.records().join("sources")).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(records.len(), 2, "intent and outcome are saved even when authorization is unavailable");
}

#[tokio::test]
async fn osv_requires_public_registry_evidence_before_disclosing_a_package() {
    use faber_engine::store::{Disk, RealDisk};
    use std::sync::{Arc, Mutex};
    struct StopBeforeTransport(Arc<Mutex<Vec<serde_json::Value>>>);
    impl Disk for StopBeforeTransport {
        fn write(&self, path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
            if path.components().any(|c| c.as_os_str() == "sources") {
                self.0.lock().unwrap().push(serde_json::from_slice(bytes).unwrap());
                return Err(std::io::Error::other("test stops before any HTTP transport"));
            }
            RealDisk.write(path, bytes)
        }
        fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
            RealDisk.rename(from, to)
        }
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let dir = tempdir().unwrap();
    let journal = faber_engine::project::store::Journal::open_with(
        &dir.path().join("repo"),
        &dir.path().join("data"),
        "mission",
        Arc::new(StopBeforeTransport(captured.clone())),
    )
    .unwrap();
    let package = PublicPackage { ecosystem: Ecosystem::Npm, name: "@org/possibly-private".into(), version: "1.0.0".into() };
    let authorized = std::collections::BTreeSet::from(["https://api.osv.dev/v1/query".into()]);
    assert!(query_osv(&package, &authorized, &journal).await.is_err());
    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0]["url"], "https://registry.npmjs.org/@org/possibly-private", "OSV must not receive unverified identities");
}

#[tokio::test(flavor = "current_thread")]
async fn source_receipt_io_keeps_the_runtime_responsive() {
    use faber_engine::store::{Disk, RealDisk};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};
    struct WaitingDisk(Arc<AtomicBool>);
    impl Disk for WaitingDisk {
        fn write(&self, path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
            if path.components().any(|c| c.as_os_str() == "sources") {
                let start = Instant::now();
                while !self.0.load(Ordering::SeqCst) {
                    if start.elapsed() > Duration::from_secs(1) {
                        return Err(std::io::Error::other("runtime blocked by receipt IO"));
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            RealDisk.write(path, bytes)
        }
        fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
            RealDisk.rename(from, to)
        }
    }
    let ready = Arc::new(AtomicBool::new(false));
    let dir = tempdir().unwrap();
    let journal = faber_engine::project::store::Journal::open_with(
        &dir.path().join("repo"),
        &dir.path().join("data"),
        "mission",
        Arc::new(WaitingDisk(ready.clone())),
    )
    .unwrap();
    let timer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        ready.store(true, Ordering::SeqCst);
    });
    let package = PublicPackage { ecosystem: Ecosystem::Cargo, name: "serde".into(), version: "1.0.0".into() };
    let result = query_osv(&package, &Default::default(), &journal).await;
    timer.await.unwrap();
    assert!(result.is_ok());
}
