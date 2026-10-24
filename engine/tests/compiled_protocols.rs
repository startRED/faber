//! The manifest and the real launch argument builders must use the same compiled protocols.

use faber_engine::claude::{ClaudeLaunch, claude_args};
use faber_engine::codex::codex_args;
use faber_engine::live_manifest::validate_document;
use faber_engine::model::{Area, Permissions, Vendor};
use serde_json::{Value, json};

fn launch(area: Area, permissions: Permissions) -> ClaudeLaunch {
    ClaudeLaunch {
        area,
        permissions,
        model: None,
        effort: None,
        resume: None,
        user_hooks: false,
        instructions: None,
        user_name: None,
        research: None,
        group_instructions: None,
        flags: Default::default(),
        tuning: Default::default(),
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        tools: Default::default(),
    }
}

#[test]
fn compiled_protocols_preserve_the_existing_provider_wire_contracts() {
    assert_eq!(Vendor::Claude.protocol(), "stream-json");
    assert_eq!(Vendor::Codex.protocol(), "app-server");
}

#[test]
fn compiled_protocols_are_used_by_both_real_argument_builders_in_every_permission_mode() {
    for area in [Area::Chat, Area::Project] {
        for permissions in [
            Permissions::Ask,
            Permissions::AcceptEdits,
            Permissions::Plan,
            Permissions::Auto,
            Permissions::FullAccess,
            Permissions::Isolated,
        ] {
            let args = claude_args(&launch(area, permissions));
            for flag in ["--input-format", "--output-format"] {
                let at = args.iter().position(|arg| arg == flag).unwrap();
                assert_eq!(args[at + 1], Vendor::Claude.protocol(), "{area:?}/{permissions:?}/{flag}");
            }
            for windows in [false, true] {
                for user_hooks in [false, true] {
                    assert_eq!(codex_args(area, windows, user_hooks, permissions)[0], Vendor::Codex.protocol());
                }
            }
        }
    }
}

fn descriptor(provider: &str, variant: &str) -> Value {
    let fixture: Value = serde_json::from_str(include_str!("../../tests/fixtures/live-manifest-signatures.json")).unwrap();
    let mut document: Value = serde_json::from_str(fixture["document_payload"].as_str().unwrap()).unwrap();
    document["drivers"] = json!([{"id":"fixture", "provider":provider, "variant":variant}]);
    document["compatibility"] = json!([{
        "provider":provider, "version":"1.2.3", "operating_system":"windows", "architecture":"x86_64",
        "artifact_digest":null, "driver":"fixture", "outcome":"unqualified", "reason":"Synthetic unqualified contract fixture",
        "evidence":{"startup":null,"turn":null,"cancellation":null,"cleanup":null}
    }]);
    document
}

#[test]
fn compiled_protocols_bound_manifest_descriptors_to_the_actual_provider_protocol_pair() {
    let accepts =
        |document: Value| validate_document(&serde_json::to_vec(&document).unwrap(), "2026-10-24T12:00:00Z", "0.2.0", None).is_ok();
    for vendor in Vendor::ALL {
        assert!(accepts(descriptor(vendor.name(), vendor.protocol())));
    }
    for (provider, variant) in [
        ("claude", "app-server"),
        ("codex", "stream-json"),
        ("unknown", "app-server"),
        ("claude", "stream-json --unsafe"),
        ("codex", "APP-SERVER"),
    ] {
        assert!(!accepts(descriptor(provider, variant)), "accepted {provider}/{variant}");
    }
}
