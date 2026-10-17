//! Complete software bar, deliverable sheet, compiler, moves, and grounded additions.
pub use super::wire::{
    BarCheck, CheckKind, CheckOutcome, CheckReceipt, CheckResult, CompilePhase, CompilePhaseKind, CompileReceipt, DeliverableSheet, Facets,
    Fact, FactSheet, Ground, GroundKind, Piece, RiskAnswers, Severity, SheetQuestion, Verbatim, WireError,
};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BarError {
    MissingMandatoryCheck(String),
    InvalidGround(String),
    TooManyJudgedChecks,
    TooManyBlockingJudgedChecks,
    TooManyJudgementOnly,
    TooManyCriticAdditions,
    Malformed(String),
}

impl BarError {
    pub fn key(&self) -> &'static str {
        match self {
            BarError::MissingMandatoryCheck(_) => "error.bar.missingMandatory",
            BarError::InvalidGround(_) => "error.bar.invalidGround",
            BarError::TooManyJudgedChecks => "error.bar.tooManyJudged",
            BarError::TooManyBlockingJudgedChecks => "error.bar.tooManyBlockingJudged",
            BarError::TooManyJudgementOnly => "error.bar.tooManyJudgementOnly",
            BarError::TooManyCriticAdditions => "error.bar.tooManyCriticAdditions",
            BarError::Malformed(_) => "error.bar.malformed",
        }
    }
}

/// Generates the Complete Software Bar (B1 to B10) per section 15.1 of generated pipelines design.
pub fn software_bar(sheet: &DeliverableSheet) -> Vec<BarCheck> {
    let mut checks = software_bar_for_first_context(sheet);
    // Measured geometry is prepared by programs; the screen and keyboard ghost
    // judgments cover the complete accessibility and four-persona reports.
    for check in &mut checks {
        check.mandatory = true;
        if matches!(
            check.id.as_str(),
            "a11y.contrast"
                | "a11y.target-size"
                | "a11y.reduced-motion"
                | "a11y.text-size"
                | "desktop.ghost-hurried"
                | "desktop.ghost-novice"
                | "desktop.ghost-distracted"
        ) {
            check.kind = CheckKind::Program;
        }
        if let Some(piece) = sheet.pieces.iter().find(|p| p.id == check.piece) {
            check.paths = piece.paths.clone();
        }
    }
    let templates = checks.clone();
    for piece in &sheet.pieces {
        if checks.iter().any(|c| c.piece == piece.id) {
            continue;
        }
        let representative = sheet.pieces.iter().find(|p| p.kind == piece.kind);
        for template in templates.iter().filter(|c| {
            representative.is_some_and(|p| c.piece == p.id)
                || piece.kind == "doc" && (c.id.starts_with("any.") || c.id == "software.other-vendor-review")
        }) {
            let mut check = template.clone();
            check.id = format!("{}.{}", piece.id, check.id);
            check.piece = piece.id.clone();
            check.owner = piece.id.clone();
            check.paths = piece.paths.clone();
            checks.push(check);
        }
    }
    for piece in sheet.pieces.iter().filter(|p| p.kind == "change") {
        for (suffix, description, proof, pass) in [
            (
                "kind.refactor-api-schema",
                "Refactor: public API and saved schema diff is empty by program",
                "proof.refactor-compat",
                "public API and schema manifests match the base",
            ),
            (
                "kind.refactor-benchmark",
                "Refactor: benchmark medians remain within measured baseline noise",
                "proof.refactor-bench",
                "measured benchmark comparison passes",
            ),
        ] {
            let first = sheet.pieces.iter().find(|p| p.kind == "change").is_some_and(|p| p.id == piece.id);
            checks.push(BarCheck {
                id: if first { suffix.into() } else { format!("{}.{}", piece.id, suffix) },
                piece: piece.id.clone(),
                attribute: suffix.into(),
                check: description.into(),
                kind: CheckKind::Program,
                severity: Severity::Blocking,
                ground: Ground { kind: GroundKind::Owner, reference: "generated-pipelines §15.1.1".into() },
                proof: proof.into(),
                owner: piece.id.clone(),
                paths: piece.paths.clone(),
                pass: pass.into(),
                evidence: "base and candidate measurement artifacts".into(),
                also: vec![],
                from: "library:software@2".into(),
                mandatory: true,
            });
        }
    }
    for check in &mut checks {
        let kind = sheet.pieces.iter().find(|p| p.id == check.piece).and_then(|p| p.change_kind);
        let required_kind = match check.attribute.as_str() {
            "kind.feature-test-first" => Some(super::wire::ChangeKind::Feature),
            "kind.bugfix-reproduce" | "any.root-cause-regression" => Some(super::wire::ChangeKind::BugFix),
            "kind.refactor-golden" | "kind.refactor-api-schema" | "kind.refactor-benchmark" => Some(super::wire::ChangeKind::Refactor),
            "kind.upgrade-changelog" => Some(super::wire::ChangeKind::DependencyUpgrade),
            _ => None,
        };
        if kind.is_some() && required_kind.is_some() && kind != required_kind {
            check.mandatory = false;
            check.from = "library:software@2:not-applicable".into();
        }
    }
    checks
}

fn software_bar_for_first_context(sheet: &DeliverableSheet) -> Vec<BarCheck> {
    let mut checks = vec![];
    let default_piece = sheet.pieces.first().map(|p| p.id.clone()).unwrap_or_else(|| "software".into());
    let default_path = sheet.pieces.first().and_then(|p| p.paths.first().cloned()).unwrap_or_else(|| "engine/src/project.rs".into());

    let piece_for = |kind: &str| -> String {
        sheet.pieces.iter().find(|p| p.kind == kind).map(|p| p.id.clone()).unwrap_or_else(|| default_piece.clone())
    };

    let path_for = |kind: &str| -> Vec<String> {
        sheet
            .pieces
            .iter()
            .find(|p| p.kind == kind)
            .and_then(|p| p.paths.first().cloned())
            .map(|p| vec![p])
            .unwrap_or_else(|| vec![default_path.clone()])
    };

    let change_piece = piece_for("change");
    let change_paths = path_for("change");
    let screen_piece = piece_for("screen");
    let screen_paths = path_for("screen");

    // Helper to construct a check
    let mut add = |id: &str,
                   piece: &str,
                   attribute: &str,
                   desc: &str,
                   kind: CheckKind,
                   severity: Severity,
                   ground_kind: GroundKind,
                   ground_ref: &str,
                   proof: &str,
                   paths: &[String],
                   pass: &str,
                   mandatory: bool| {
        checks.push(BarCheck {
            id: id.into(),
            piece: piece.into(),
            attribute: attribute.into(),
            check: desc.into(),
            kind,
            severity,
            ground: Ground { kind: ground_kind, reference: ground_ref.into() },
            proof: proof.into(),
            owner: piece.into(),
            paths: paths.to_vec(),
            pass: pass.into(),
            evidence: "output artifact".into(),
            also: vec![],
            from: "library:software@2".into(),
            mandatory,
        });
    };

    // B1: Software Line
    add(
        "software.risk-answers",
        &change_piece,
        "software.risk",
        "Risk questions answered and saved in project",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.risk-answers",
        &change_paths,
        "all risk fields valid",
        true,
    );
    add(
        "software.callers-map",
        &change_piece,
        "software.callers",
        "Affected callers listed before code edits",
        CheckKind::ProgramJudge,
        Severity::Major,
        GroundKind::Owner,
        "docs/product/README.md",
        "proof.callers-map",
        &change_paths,
        "callers identified",
        false,
    );
    add(
        "software.test-first",
        &change_piece,
        "software.test-first",
        "A test that fails on base and passes on change written first",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.test-first",
        &change_paths,
        "failing base test then green",
        true,
    );
    add(
        "software.dependency-registry",
        &change_piece,
        "software.dependencies",
        "A new dependency exists on registry, with publisher, age, downloads and typo check",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.dependency-registry",
        &change_paths,
        "verified package metadata",
        true,
    );
    add(
        "software.quick-tests",
        &change_piece,
        "software.quick-tests",
        "Quick tests before review",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.quick-tests",
        &change_paths,
        "quick suite passes",
        true,
    );
    add(
        "software.secret-scan",
        &change_piece,
        "software.secret-scan",
        "Secret scan on outgoing change before review",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.secret-scan",
        &change_paths,
        "no secrets detected",
        true,
    );
    add(
        "software.other-vendor-review",
        &change_piece,
        "software.other-vendor-review",
        "Other-vendor review of the exact composed tree in both orders",
        CheckKind::Judge,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.other-vendor-review",
        &change_paths,
        "both orders pass",
        true,
    );
    add(
        "software.ui-screen-check",
        &screen_piece,
        "software.ui-screen",
        "Screen check when UI changed (real app, screenshots, keyboard, theme and full accessibility measurements)",
        CheckKind::ProgramJudge,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.ui-screen",
        &screen_paths,
        "real app visual passes",
        false,
    );
    add(
        "software.full-suite",
        &change_piece,
        "software.full-suite",
        "Full test suite passes before landing",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.full-suite",
        &change_paths,
        "cargo t --suite rust exits 0",
        true,
    );
    add(
        "software.receipt",
        &change_piece,
        "software.receipt",
        "Save with proof receipt",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.receipt",
        &change_paths,
        "receipt recorded",
        true,
    );
    add(
        "software.fast-path",
        &change_piece,
        "software.fast-path",
        "Small tweaks take the fast path",
        CheckKind::Program,
        Severity::Minor,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.fast-path",
        &change_paths,
        "fast path boundaries satisfied",
        false,
    );

    // B2: Client website adapted to desktop
    add(
        "desktop.spend-paths",
        &change_piece,
        "desktop.spend-paths",
        "Every spend path tested before code",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.spend-paths",
        &change_paths,
        "spend paths tested first",
        true,
    );
    add(
        "desktop.real-width-drafts",
        &screen_piece,
        "desktop.real-width-drafts",
        "New screens drafted and judged at real widths (720, 900, 1100 px)",
        CheckKind::ProgramJudge,
        Severity::Major,
        GroundKind::Owner,
        "docs/design/README.md",
        "proof.real-width-drafts",
        &screen_paths,
        "judged at real widths",
        false,
    );
    add(
        "desktop.ghost-hurried",
        &screen_piece,
        "desktop.ghost-hurried",
        "Hurried ghost user test through hidden app",
        CheckKind::ProgramJudge,
        Severity::Major,
        GroundKind::Owner,
        "docs/design/README.md",
        "proof.ghost-hurried",
        &screen_paths,
        "hurried flow complete",
        false,
    );
    add(
        "desktop.ghost-novice",
        &screen_piece,
        "desktop.ghost-novice",
        "Novice ghost user test through hidden app",
        CheckKind::ProgramJudge,
        Severity::Major,
        GroundKind::Owner,
        "docs/design/README.md",
        "proof.ghost-novice",
        &screen_paths,
        "novice flow complete",
        false,
    );
    add(
        "desktop.ghost-distracted",
        &screen_piece,
        "desktop.ghost-distracted",
        "Distracted ghost user test interrupted mid-flow at narrowest width",
        CheckKind::ProgramJudge,
        Severity::Major,
        GroundKind::Owner,
        "docs/design/README.md",
        "proof.ghost-distracted",
        &screen_paths,
        "distracted flow complete",
        false,
    );
    add(
        "desktop.ghost-keyboard",
        &screen_piece,
        "desktop.ghost-keyboard",
        "Keyboard-only ghost plus other-vendor judgment of all four hidden-app persona traces",
        CheckKind::ProgramJudge,
        Severity::Major,
        GroundKind::Owner,
        "docs/design/README.md",
        "proof.ghost-keyboard",
        &screen_paths,
        "keyboard flow and all four persona judgments pass",
        false,
    );
    add(
        "desktop.staging-only",
        &change_piece,
        "desktop.staging-only",
        "Work lands on mission branch and next slot only; human releases",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.staging-only",
        &change_paths,
        "staging target verified",
        true,
    );
    add(
        "desktop.no-signing-keys",
        &change_piece,
        "desktop.no-signing-keys",
        "No agent session holds updater signing key or publishing credentials",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.no-signing-keys",
        &change_paths,
        "signing keys absent from session",
        true,
    );

    // B3: Rust
    add(
        "rust.fmt",
        &change_piece,
        "rust.fmt",
        "cargo fmt --all --check",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.cargo-fmt",
        &change_paths,
        "cargo fmt exits 0",
        true,
    );
    add(
        "rust.clippy",
        &change_piece,
        "rust.clippy",
        "Both clippy runs pass -D warnings including await_holding_lock",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.cargo-clippy",
        &change_paths,
        "clippy exits 0 with no warnings",
        true,
    );
    add(
        "rust.audit-unwrap",
        &change_piece,
        "rust.audit-unwrap",
        "No unwrap, expect, or panic! outside tests without stated reason",
        CheckKind::ProgramJudge,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.audit-unwrap",
        &change_paths,
        "unwrap audit clean",
        true,
    );
    add(
        "rust.no-blocking-async",
        &change_piece,
        "rust.no-blocking-async",
        "No blocking calls inside async tasks",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Standard,
        "https://tokio.rs/tokio/tutorial/async",
        "proof.no-blocking-async",
        &change_paths,
        "no blocking calls found in async contexts",
        true,
    );
    add(
        "rust.unsafe-safety-note",
        &change_piece,
        "rust.unsafe-safety-note",
        "No new unsafe without SAFETY comment and review",
        CheckKind::ProgramJudge,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.unsafe-safety-note",
        &change_paths,
        "SAFETY notes documented",
        true,
    );
    add(
        "rust.locale-keys",
        &change_piece,
        "rust.locale-keys",
        "Errors return error.* and reason.* keys, never sentences",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.locale-keys",
        &change_paths,
        "only locale keys used",
        true,
    );

    // B4: Processes and data
    add(
        "process.shell-free",
        &change_piece,
        "process.shell-free",
        "Child processes never through a shell, npm shims resolved to real exe",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.shell-free",
        &change_paths,
        "direct process execution",
        true,
    );
    add(
        "process.no-visible-windows",
        &change_piece,
        "process.no-visible-windows",
        "Window audit: no visible console windows",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.no-visible-windows",
        &change_paths,
        "all spawned windows hidden",
        true,
    );
    add(
        "process.stop-kills-tree",
        &change_piece,
        "process.stop-kills-tree",
        "Stop kills the whole process tree",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.stop-kills-tree",
        &change_paths,
        "entire process tree terminated",
        true,
    );
    add(
        "data.atomic-versioned",
        &change_piece,
        "data.atomic-versioned",
        "Saved data written atomically, versioned, unknown fields kept",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.atomic-versioned",
        &change_paths,
        "atomic replacement with schema version",
        true,
    );
    add(
        "data.crash-matrix",
        &change_piece,
        "data.crash-matrix",
        "Crash and restart at every effect boundary lose nothing",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "docs/product/README.md",
        "proof.crash-matrix",
        &change_paths,
        "recovery restores durable state",
        true,
    );
    add(
        "process.idle-memory",
        &change_piece,
        "process.idle-memory",
        "Idle memory test <= 250 MB",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.idle-memory",
        &change_paths,
        "resident memory under limit",
        true,
    );
    add(
        "process.long-lived-audit",
        &change_piece,
        "process.long-lived-audit",
        "No new long-lived process without a measurement",
        CheckKind::Program,
        Severity::Major,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.long-lived-audit",
        &change_paths,
        "process lifetime measured",
        false,
    );
    add(
        "data.credential-folders-untouched",
        &change_piece,
        "data.credential-folders-untouched",
        "Credential folders never touched (~/.claude, ~/.codex, Gemini OAuth)",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.credential-folders-untouched",
        &change_paths,
        "zero reads/writes to credentials",
        true,
    );

    // B5: Tauri
    add(
        "tauri.command-inventory",
        &change_piece,
        "tauri.command-inventory",
        "Fixed command list and capability files only change with flagged review",
        CheckKind::ProgramJudge,
        Severity::Blocking,
        GroundKind::Standard,
        "https://tauri.app/security/",
        "proof.command-inventory",
        &change_paths,
        "inventory diff verified",
        false,
    );
    add(
        "tauri.csp",
        &change_piece,
        "tauri.csp",
        "Content security policy is present and not loosened (no unsafe-eval)",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Standard,
        "https://tauri.app/security/",
        "proof.csp",
        &change_paths,
        "CSP strict",
        true,
    );
    add(
        "tauri.no-remote-content",
        &change_piece,
        "tauri.no-remote-content",
        "No remote content and no IPC from remote origins",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Standard,
        "https://tauri.app/security/",
        "proof.no-remote-content",
        &change_paths,
        "local origins only",
        true,
    );
    add(
        "tauri.scoped-input-checks",
        &change_piece,
        "tauri.scoped-input-checks",
        "Every command validates and scopes its inputs in Rust",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Standard,
        "https://tauri.app/security/",
        "proof.scoped-input-checks",
        &change_paths,
        "inputs strictly scoped",
        true,
    );
    add(
        "tauri.updater-signature",
        &change_piece,
        "tauri.updater-signature",
        "Updater signature checks kept and development channel separate",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Standard,
        "https://tauri.app/security/",
        "proof.updater-signature",
        &change_paths,
        "signatures verified",
        true,
    );
    add(
        "tauri.webview-ci",
        &screen_piece,
        "tauri.webview-ci",
        "Heavy screens checked on webview CI or explicit weaker/not-run result",
        CheckKind::ProgramJudge,
        Severity::Major,
        GroundKind::Owner,
        "docs/product/README.md",
        "proof.webview-ci",
        &screen_paths,
        "webview evidence present",
        false,
    );

    // B6: SolidJS, TypeScript and screen
    add(
        "screen.lint",
        &screen_piece,
        "screen.lint",
        "npm run lint exits 0",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.screen-lint",
        &screen_paths,
        "lint exits 0",
        true,
    );
    add(
        "screen.typecheck",
        &screen_piece,
        "screen.typecheck",
        "Strict npm run typecheck exits 0",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.screen-typecheck",
        &screen_paths,
        "typecheck exits 0",
        true,
    );
    add(
        "screen.build",
        &screen_piece,
        "screen.build",
        "npm run build exits 0",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.screen-build",
        &screen_paths,
        "build exits 0",
        true,
    );
    add(
        "screen.no-props-destructure",
        &screen_piece,
        "screen.no-props-destructure",
        "Solid props never destructured and effects clean up timers",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Documentation,
        "https://docs.solidjs.com/",
        "proof.no-props-destructure",
        &screen_paths,
        "reactivity preserved",
        true,
    );
    add(
        "screen.windowed-lists",
        &screen_piece,
        "screen.windowed-lists",
        "Long lists stay windowed (only last 40 messages drawn)",
        CheckKind::Program,
        Severity::Major,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.windowed-lists",
        &screen_paths,
        "DOM bounded to 40 items",
        false,
    );
    add(
        "screen.locales-hand-authored",
        &screen_piece,
        "screen.locales-hand-authored",
        "Every visible string in ui/locales/ with all 5 languages present",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.locales",
        &screen_paths,
        "all 5 locales present and complete",
        true,
    );
    add(
        "screen.design-tokens",
        &screen_piece,
        "screen.design-tokens",
        "Only design-system tokens and components; amber only for needs-you",
        CheckKind::Program,
        Severity::Major,
        GroundKind::Owner,
        "docs/design/README.md",
        "proof.design-tokens",
        &screen_paths,
        "palette locked",
        false,
    );
    add(
        "screen.no-duplicate-controls",
        &screen_piece,
        "screen.no-duplicate-controls",
        "No duplicate screen, button or setting",
        CheckKind::ProgramJudge,
        Severity::Major,
        GroundKind::Owner,
        "docs/design/README.md",
        "proof.no-duplicate-controls",
        &screen_paths,
        "product map clean of duplicates",
        false,
    );
    add(
        "screen.coherence-widths",
        &screen_piece,
        "screen.coherence-widths",
        "Coherence pass of every screen at 720, 900 and 1100 px and each theme",
        CheckKind::ProgramJudge,
        Severity::Major,
        GroundKind::Owner,
        "docs/design/README.md",
        "proof.coherence-widths",
        &screen_paths,
        "rendered coherence across widths",
        false,
    );

    // B7: Accessibility
    add(
        "a11y.contrast",
        &screen_piece,
        "a11y.contrast",
        "WCAG 2.2 AA contrast 4.5:1 and 3:1",
        CheckKind::ProgramJudge,
        Severity::Blocking,
        GroundKind::Standard,
        "https://www.w3.org/TR/WCAG22/",
        "proof.a11y-contrast",
        &screen_paths,
        "contrast meets AA",
        true,
    );
    add(
        "a11y.target-size",
        &screen_piece,
        "a11y.target-size",
        "Targets at least 24 px",
        CheckKind::ProgramJudge,
        Severity::Blocking,
        GroundKind::Standard,
        "https://www.w3.org/TR/WCAG22/",
        "proof.a11y-target-size",
        &screen_paths,
        "touch/click target size >= 24 px",
        true,
    );
    add(
        "a11y.keyboard-reach",
        &screen_piece,
        "a11y.keyboard-reach",
        "Everything reachable by keyboard, visible focus and focus restoration",
        CheckKind::ProgramJudge,
        Severity::Blocking,
        GroundKind::Standard,
        "https://www.w3.org/TR/WCAG22/",
        "proof.a11y-keyboard-reach",
        &screen_paths,
        "full keyboard reach and restoration",
        true,
    );
    add(
        "a11y.reduced-motion",
        &screen_piece,
        "a11y.reduced-motion",
        "Reduced motion respected",
        CheckKind::ProgramJudge,
        Severity::Blocking,
        GroundKind::Standard,
        "https://www.w3.org/TR/WCAG22/",
        "proof.a11y-reduced-motion",
        &screen_paths,
        "prefers-reduced-motion honored",
        true,
    );
    add(
        "a11y.text-size",
        &screen_piece,
        "a11y.text-size",
        "Text from 13 to 18 px",
        CheckKind::ProgramJudge,
        Severity::Major,
        GroundKind::Standard,
        "https://www.w3.org/TR/WCAG22/",
        "proof.a11y-text-size",
        &screen_paths,
        "text scale within bounds",
        false,
    );

    // B8: Supply chain and safety
    add(
        "safety.osv-vulnerabilities",
        &change_piece,
        "safety.osv-vulnerabilities",
        "Known vulnerabilities in Cargo.lock and package-lock.json (OSV)",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Standard,
        "https://osv.dev/",
        "proof.osv-scan",
        &change_paths,
        "no reported vulnerabilities",
        true,
    );
    add(
        "safety.spdx-licenses",
        &change_piece,
        "safety.spdx-licenses",
        "Licenses of every dependency allowed (SPDX)",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Standard,
        "https://spdx.org/licenses/",
        "proof.spdx-licenses",
        &change_paths,
        "all licenses conform to policy",
        true,
    );
    add(
        "safety.secret-scan",
        &change_piece,
        "safety.secret-scan",
        "Secret scan on every outgoing change",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.secret-scan",
        &change_paths,
        "no secret keys or tokens in patch",
        true,
    );
    add(
        "safety.no-silent-telemetry",
        &change_piece,
        "safety.no-silent-telemetry",
        "No telemetry and no new network destination without a logged reason",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.no-silent-telemetry",
        &change_paths,
        "network destinations verified",
        true,
    );

    // B9: Change kinds
    add(
        "kind.feature-test-first",
        &change_piece,
        "kind.feature-test-first",
        "Feature: test first",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.test-first",
        &change_paths,
        "failing test before feature implementation",
        true,
    );
    add(
        "kind.bugfix-reproduce",
        &change_piece,
        "kind.bugfix-reproduce",
        "Bug fix: reproduce from report, then a failing test, callers checked",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.bugfix-reproduce",
        &change_paths,
        "reproduction test fails on base",
        true,
    );
    add(
        "kind.refactor-golden",
        &change_piece,
        "kind.refactor-golden",
        "Refactor: golden outputs captured on base, public API and schema diff empty",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.refactor-golden",
        &change_paths,
        "golden outputs match exactly",
        true,
    );
    add(
        "kind.upgrade-changelog",
        &change_piece,
        "kind.upgrade-changelog",
        "Dependency upgrade: changelog read, breaking changes listed and tested",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.upgrade-changelog",
        &change_paths,
        "changelog and breaking changes verified",
        true,
    );

    // B10: Any change
    add(
        "any.root-cause-regression",
        &change_piece,
        "any.root-cause-regression",
        "Every bug fix has a test that fails without the fix and names its root cause",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.root-cause",
        &change_paths,
        "regression test references root cause",
        true,
    );
    add(
        "any.edge-cases",
        &change_piece,
        "any.edge-cases",
        "Edge cases tried on purpose (empty, huge, Unicode paths, offline, timeouts)",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "docs/product/README.md",
        "proof.edge-cases",
        &change_paths,
        "edge cases pass without panics",
        true,
    );
    add(
        "any.schema-versioned",
        &change_piece,
        "any.schema-versioned",
        "Public contracts and saved schemas unchanged or versioned",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.schema-versioned",
        &change_paths,
        "compatibility preserved across versions",
        true,
    );
    add(
        "any.token-latency",
        &change_piece,
        "any.token-latency",
        "Time to first token stays <= 2 s",
        CheckKind::Program,
        Severity::Major,
        GroundKind::Owner,
        "docs/product/README.md",
        "proof.token-latency",
        &change_paths,
        "token latency measured <= 2s",
        false,
    );
    add(
        "any.docs-and-locales",
        &change_piece,
        "any.docs-and-locales",
        "Docs and locales updated when behaviour changes",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.docs-and-locales",
        &change_paths,
        "documentation and locale files in sync",
        true,
    );
    add(
        "any.coherent-commit",
        &change_piece,
        "any.coherent-commit",
        "English commit message, one coherent change",
        CheckKind::Program,
        Severity::Blocking,
        GroundKind::Owner,
        "AGENTS.md",
        "proof.coherent-commit",
        &change_paths,
        "commit adheres to guidelines",
        true,
    );

    checks
}

/// Owner decision E10: the quality bar wins over the date; never let the lead remove a mandatory check.
pub fn verify_mandatory_checks(sheet: &DeliverableSheet, proposed_bar: &[BarCheck]) -> Result<(), BarError> {
    let full_bar = software_bar(sheet);
    let mut proposed_ids = BTreeSet::new();
    for check in proposed_bar {
        check.validate().map_err(|_| BarError::Malformed(check.id.clone()))?;
        if !proposed_ids.insert(check.id.as_str()) {
            return Err(BarError::Malformed(check.id.clone()));
        }
    }

    for check in &full_bar {
        {
            let proposed =
                proposed_bar.iter().find(|c| c.id == check.id).ok_or_else(|| BarError::MissingMandatoryCheck(check.id.clone()))?;
            let mut expected = check.clone();
            // Ownership is assigned by compilation, never a change to the requirement.
            expected.owner = proposed.owner.clone();
            if &expected != proposed {
                return Err(BarError::Malformed(check.id.clone()));
            }
        }
    }
    Ok(())
}

/// Validates grounded additions: repo heading exists in markdown, or official URL is valid.
pub fn validate_ground(repo_root: &Path, ground: &Ground) -> Result<(), BarError> {
    match ground.kind {
        GroundKind::Repository => {
            let (file_part, heading_part) = ground
                .reference
                .split_once('#')
                .filter(|(_, h)| !h.trim().is_empty())
                .ok_or_else(|| BarError::InvalidGround(ground.reference.clone()))?;
            let scoped = super::lease::normalized(file_part).map_err(|_| BarError::InvalidGround(ground.reference.clone()))?;
            super::kit::delivery::scoped_path(file_part).map_err(|_| BarError::InvalidGround(ground.reference.clone()))?;
            if scoped.is_empty() {
                return Err(BarError::InvalidGround(ground.reference.clone()));
            }

            let file_path =
                super::lease::real_path(&repo_root.join(scoped)).map_err(|_| BarError::InvalidGround(ground.reference.clone()))?;
            if !file_path.is_file() {
                return Err(BarError::InvalidGround(format!("repository file '{}' does not exist", file_part)));
            }

            if !heading_part.is_empty() {
                let file =
                    super::files::ScopedFile::new(&file_path, false).map_err(|_| BarError::InvalidGround(ground.reference.clone()))?;
                let bytes = file
                    .read_limited(super::wire::MAX_FILE as u64)
                    .map_err(|_| BarError::InvalidGround(ground.reference.clone()))?
                    .ok_or_else(|| BarError::InvalidGround(ground.reference.clone()))?
                    .0;
                let content = String::from_utf8(bytes).map_err(|_| BarError::InvalidGround(ground.reference.clone()))?;

                let target_slug = slugify(heading_part);
                let found = markdown_heading_exists(&content, &target_slug);

                if !found {
                    return Err(BarError::InvalidGround(format!("heading '#{}' not found in '{}'", heading_part, file_part)));
                }
            }
            Ok(())
        }
        GroundKind::Standard | GroundKind::Documentation => super::proof::native::validate_source_url(&ground.reference)
            .map(|_| ())
            .map_err(|_| BarError::InvalidGround(ground.reference.clone())),
        GroundKind::Owner | GroundKind::Evidence | GroundKind::Judgement => {
            if ground.reference.trim().is_empty() {
                Err(BarError::InvalidGround("empty ground reference".into()))
            } else {
                Ok(())
            }
        }
    }
}

fn markdown_heading_exists(content: &str, target: &str) -> bool {
    let mut fence = None;
    for line in content.lines() {
        let indent = line.bytes().take_while(|b| *b == b' ').count();
        if indent > 3 || line.starts_with('\t') {
            continue;
        }
        let line = line.trim();
        let first = line.as_bytes().first().copied();
        let run = line.bytes().take_while(|b| Some(*b) == first).count();
        if let Some((marker, length)) = fence {
            if first == Some(marker) && run >= length && line[run..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        if matches!(first, Some(b'`' | b'~')) && run >= 3 {
            fence = Some((first.unwrap_or_default(), run));
            continue;
        }
        if first == Some(b'#')
            && (1..=6).contains(&run)
            && line[run..].starts_with(char::is_whitespace)
            && slugify(line[run..].trim().trim_end_matches('#').trim()) == target
        {
            return true;
        }
    }
    false
}

fn slugify(text: &str) -> String {
    text.to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Enforces judged check caps and filters per piece:
/// - At most 12 judged checks per piece.
/// - At most 6 blocking judged checks per piece.
/// - Judgement-only ground at most 30% of the bar.
pub fn filter_and_validate_bar(piece_id: &str, checks: &[BarCheck]) -> Result<(), BarError> {
    let piece_checks: Vec<&BarCheck> = checks.iter().filter(|c| c.piece == piece_id).collect();
    if piece_checks.is_empty() {
        return Ok(());
    }

    let judged_count = piece_checks.iter().filter(|c| matches!(c.kind, CheckKind::Judge | CheckKind::ProgramJudge)).count();

    if judged_count > 12 {
        return Err(BarError::TooManyJudgedChecks);
    }

    let blocking_judged = piece_checks
        .iter()
        .filter(|c| matches!(c.kind, CheckKind::Judge | CheckKind::ProgramJudge) && c.severity == Severity::Blocking)
        .count();

    if blocking_judged > 6 {
        return Err(BarError::TooManyBlockingJudgedChecks);
    }

    let judgement_only_count = piece_checks.iter().filter(|c| c.ground.kind == GroundKind::Judgement).count();

    if (judgement_only_count as f64 / piece_checks.len() as f64) > 0.30 {
        return Err(BarError::TooManyJudgementOnly);
    }

    Ok(())
}

/// Normalizes attributes, merges duplicate checks from two drafters, and marks agreed ones.
pub fn merge_drafters(a: &[BarCheck], b: &[BarCheck]) -> Vec<BarCheck> {
    let mut out: Vec<BarCheck> = vec![];

    for check_a in a {
        let mut merged = check_a.clone();
        merged.attribute = merged.attribute.to_ascii_lowercase();

        if let Some(matching_b) = b.iter().find(|cb| cb.piece == merged.piece && cb.attribute.to_ascii_lowercase() == merged.attribute) {
            merged.from = "agreed".into();
            if merged.also.is_empty() && matching_b.ground != merged.ground {
                merged.also.push(matching_b.ground.clone());
            }
        }
        out.push(merged);
    }

    for check_b in b {
        let norm_b = check_b.attribute.to_ascii_lowercase();
        if !out.iter().any(|c| c.piece == check_b.piece && c.attribute == norm_b) {
            let mut item = check_b.clone();
            item.attribute = norm_b;
            out.push(item);
        }
    }

    out
}

/// Applies critic additions (at most 3).
pub fn apply_critic(bar: &[BarCheck], additions: &[BarCheck]) -> Result<Vec<BarCheck>, BarError> {
    if additions.len() > 3 {
        return Err(BarError::TooManyCriticAdditions);
    }

    let mut out = bar.to_vec();
    for addition in additions {
        let norm = addition.attribute.to_ascii_lowercase();
        if let Some(existing) = out.iter_mut().find(|c| c.piece == addition.piece && c.attribute == norm) {
            if existing.mandatory && existing != addition {
                return Err(BarError::Malformed(existing.id.clone()));
            }
            *existing = addition.clone();
            existing.attribute = norm;
        } else {
            let mut item = addition.clone();
            item.attribute = norm;
            out.push(item);
        }
    }

    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MoveKind {
    Ask,
    Reuse,
    Diverge,
    Measure,
    InPlace,
    Guard,
    Judge,
    Gate,
    Outliers,
    Source,
    TasteCapture,
    Craft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GhostPersona {
    Hurried,
    Novice,
    Distracted,
    KeyboardOnly,
}

pub fn screen_draft_widths() -> &'static [u32] {
    &[720, 900, 1100]
}

pub fn all_ghost_personas() -> &'static [GhostPersona] {
    &[GhostPersona::Hurried, GhostPersona::Novice, GhostPersona::Distracted, GhostPersona::KeyboardOnly]
}

/// Triggers moves per section 7 / 15.1 of generated pipelines design.
pub fn plan_moves_for_piece(piece: &Piece, sheet: &DeliverableSheet) -> Vec<MoveKind> {
    let mut moves = vec![];
    if !sheet.questions.is_empty() {
        moves.push(MoveKind::Ask);
    }
    moves.push(MoveKind::Guard);

    if piece.kind == "screen" {
        moves.push(MoveKind::Reuse);
        moves.push(MoveKind::Diverge);
        moves.push(MoveKind::InPlace);
        moves.push(MoveKind::Measure);
        moves.push(MoveKind::Judge);
    } else if piece.kind == "change" {
        if sheet.risk.money == Some(true) {
            moves.push(MoveKind::Measure);
        }
        moves.push(MoveKind::Craft);
        if sheet.risk.irreversible.iter().any(|s| s.contains("publish") || s.contains("release") || s.contains("buy")) {
            moves.push(MoveKind::Gate);
        }
        moves.push(MoveKind::Judge);
    } else if piece.kind == "doc" {
        if sheet.facts.iter().any(|f| f.text.contains(&piece.id)) {
            moves.push(MoveKind::Source);
        }
        moves.push(MoveKind::Craft);
        moves.push(MoveKind::Judge);
    } else {
        moves.push(MoveKind::Craft);
        moves.push(MoveKind::Judge);
    }

    moves.retain(|m| *m != MoveKind::Measure && *m != MoveKind::Judge && *m != MoveKind::Gate);
    moves.extend([MoveKind::Measure, MoveKind::Judge, MoveKind::Gate]);
    moves
}

/// Stalled flow returns to drafts (diverge).
pub fn stalled_flow_target(stalled_step: MoveKind) -> MoveKind {
    match stalled_step {
        MoveKind::InPlace | MoveKind::Judge | MoveKind::Measure | MoveKind::Craft => MoveKind::Diverge,
        _ => MoveKind::Diverge,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastPathCandidate {
    pub changed_piece: String,
    pub changed_attribute_or_fact: String,
    pub is_reversible: bool,
    pub risk_switched_on: bool,
}

impl FastPathCandidate {
    pub fn qualifies(&self) -> bool {
        self.is_reversible && !self.risk_switched_on && !self.changed_piece.is_empty() && !self.changed_attribute_or_fact.is_empty()
    }
    pub fn qualifies_for(&self, before: &DeliverableSheet, after: &DeliverableSheet, bar: &[BarCheck]) -> bool {
        self.qualifies()
            && before.risk == after.risk
            && before.risk.public.is_some()
            && before.risk.others_data.is_some()
            && before.risk.money.is_some()
            && before.risk.minors.is_some()
            && before.risk.regulated.is_some()
            && before.risk.commercial.is_some()
            && before.pieces.iter().any(|p| p.id == self.changed_piece)
            && after.pieces.iter().any(|p| p.id == self.changed_piece)
            && (bar.iter().any(|c| c.piece == self.changed_piece && c.attribute == self.changed_attribute_or_fact)
                || before.facts.iter().any(|f| f.id == self.changed_attribute_or_fact))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilerError {
    GenericNodeLacksCheck(String),
    GenericNodeLacksPath(String),
    OrphanCheck(String),
    OrphanEvidence(String),
    OrphanTask(String),
    Cycle(Vec<String>),
    Timeout,
}

impl CompilerError {
    pub fn key(&self) -> &'static str {
        match self {
            CompilerError::GenericNodeLacksCheck(_) => "error.compiler.genericNodeLacksCheck",
            CompilerError::GenericNodeLacksPath(_) => "error.compiler.genericNodeLacksPath",
            CompilerError::OrphanCheck(_) => "error.compiler.orphanCheck",
            CompilerError::OrphanEvidence(_) => "error.compiler.orphanEvidence",
            CompilerError::OrphanTask(_) => "error.compiler.orphanTask",
            CompilerError::Cycle(_) => "error.compiler.cycle",
            CompilerError::Timeout => "error.compiler.timeout",
        }
    }
}

/// Extends the MVP graph validator (AS-7, B:737):
/// - Rejects generic nodes lacking check ID or scoped path
/// - Rejects orphan checks and tasks
pub fn validate_compiler_graph(tasks: &[super::Task], bar: &[BarCheck], deliverables: &[String]) -> Result<(), CompilerError> {
    super::Plan::new("compiler", 0, tasks.to_vec()).validate().map_err(|_| CompilerError::OrphanTask("invalid-graph".into()))?;
    for task in tasks {
        if task.checks.is_empty() {
            return Err(CompilerError::GenericNodeLacksCheck(task.id.clone()));
        }
        if task.paths.is_empty() {
            return Err(CompilerError::GenericNodeLacksPath(task.id.clone()));
        }
    }

    for check in bar {
        let has_owner = tasks.iter().any(|t| t.id == check.owner && t.checks.contains(&check.id));
        if !has_owner {
            return Err(CompilerError::OrphanCheck(check.id.clone()));
        }
        if check.validate().is_err() || check.paths.is_empty() || check.evidence.trim().is_empty() || check.pass.trim().is_empty() {
            return Err(CompilerError::OrphanEvidence(check.id.clone()));
        }
        let owner = tasks.iter().find(|t| t.id == check.owner).ok_or_else(|| CompilerError::OrphanCheck(check.id.clone()))?;
        if !owner.proofs.contains(&check.proof) || check.paths.iter().any(|p| !owner.paths.iter().any(|area| super::overlaps(area, p))) {
            return Err(CompilerError::OrphanEvidence(check.id.clone()));
        }
    }

    let mut needed: BTreeSet<String> = BTreeSet::new();
    for task in tasks {
        let serves_check = bar.iter().any(|c| c.owner == task.id || task.checks.contains(&c.id));
        let serves_deliverable = task.deliverables.iter().any(|d| deliverables.contains(d));
        if serves_check || serves_deliverable {
            needed.insert(task.id.clone());
        }
    }

    // Propagate backwards along requires
    let task_map: std::collections::BTreeMap<&str, &super::Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut changed = true;
    while changed {
        changed = false;
        let current: Vec<String> = needed.iter().cloned().collect();
        for id in current {
            if let Some(task) = task_map.get(id.as_str()) {
                for req in &task.requires {
                    if needed.insert(req.clone()) {
                        changed = true;
                    }
                }
            }
        }
    }

    for task in tasks {
        if !needed.contains(&task.id) {
            return Err(CompilerError::OrphanTask(task.id.clone()));
        }
        if task.checks.iter().any(|id| !bar.iter().any(|c| &c.id == id)) {
            return Err(CompilerError::OrphanCheck(task.id.clone()));
        }
        if task.paths.iter().any(|p| super::lease::normalized(p).is_err() || super::clean_path(p).is_none_or(|p| p.is_empty())) {
            return Err(CompilerError::GenericNodeLacksPath(task.id.clone()));
        }
    }

    Ok(())
}

/// Walks backward from checks and deliverables, pruning unneeded tasks.
pub fn back_chain_and_prune(tasks: Vec<super::Task>, bar: &[BarCheck], deliverables: &[String]) -> (Vec<super::Task>, Vec<Verbatim>) {
    let mut needed: BTreeSet<String> = BTreeSet::new();
    for task in &tasks {
        let serves_check = bar.iter().any(|c| c.owner == task.id || task.checks.contains(&c.id));
        let serves_deliverable = task.deliverables.iter().any(|d| deliverables.contains(d));
        if serves_check || serves_deliverable {
            needed.insert(task.id.clone());
        }
    }

    let task_map: std::collections::BTreeMap<&str, &super::Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut changed = true;
    while changed {
        changed = false;
        let current: Vec<String> = needed.iter().cloned().collect();
        for id in current {
            if let Some(task) = task_map.get(id.as_str()) {
                for req in &task.requires {
                    if needed.insert(req.clone()) {
                        changed = true;
                    }
                }
            }
        }
    }

    let mut kept = vec![];
    let mut dropped = vec![];

    for task in tasks {
        if needed.contains(&task.id) {
            kept.push(task);
        } else {
            dropped.push(Verbatim { id: task.id.clone(), text: "unneeded task pruned by compiler back-chain".into() });
        }
    }

    (kept, dropped)
}

/// Kill-early order: cheap proofs first (Program before Judge) respecting topological dependencies.
pub fn order_kill_early(tasks: &[super::Task], bar: &[BarCheck]) -> Result<Vec<super::Task>, CompilerError> {
    let check_map: std::collections::BTreeMap<&str, &BarCheck> = bar.iter().map(|c| (c.id.as_str(), c)).collect();

    let task_cost: std::collections::BTreeMap<&str, u32> = tasks
        .iter()
        .map(|t| {
            let is_judge = t
                .checks
                .iter()
                .any(|cid| check_map.get(cid.as_str()).is_some_and(|c| matches!(c.kind, CheckKind::Judge | CheckKind::ProgramJudge)));
            (t.id.as_str(), if is_judge { 1 } else { 0 })
        })
        .collect();

    let mut in_degrees: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut dependents: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();

    for t in tasks {
        in_degrees.insert(&t.id, t.requires.len());
        for req in &t.requires {
            dependents.entry(req.as_str()).or_default().push(&t.id);
        }
    }

    let mut ready: Vec<&super::Task> = tasks.iter().filter(|t| in_degrees[&t.id.as_str()] == 0).collect();

    ready.sort_by_key(|t| (task_cost[&t.id.as_str()], &t.id));

    let mut ordered = vec![];
    let task_lookup: std::collections::BTreeMap<&str, &super::Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    while !ready.is_empty() {
        let next = ready.remove(0);
        ordered.push(next.clone());

        if let Some(deps) = dependents.get(next.id.as_str()) {
            for dep in deps {
                let deg = in_degrees.get_mut(*dep).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    ready.push(task_lookup[*dep]);
                }
            }
        }
        ready.sort_by_key(|t| (task_cost[&t.id.as_str()], &t.id));
    }

    if ordered.len() < tasks.len() {
        let stuck: Vec<String> = tasks.iter().filter(|t| in_degrees[&t.id.as_str()] > 0).map(|t| t.id.clone()).collect();
        return Err(CompilerError::Cycle(stuck));
    }

    Ok(ordered)
}

/// Identifies which task/lease owns repair for a failed path.
pub fn failing_path_lease_holder(failed_path: &str, tasks: &[super::Task]) -> Option<String> {
    let failed_path = super::lease::normalized(failed_path).ok()?;
    let owners = tasks
        .iter()
        .filter(|t| t.kind.writes() && t.writes.iter().any(|w| super::lease::normalized(w).is_ok_and(|w| contains_path(&w, &failed_path))))
        .collect::<Vec<_>>();
    if owners.len() == 1 { Some(owners[0].id.clone()) } else { None }
}

fn contains_path(area: &str, path: &str) -> bool {
    let (area, path) = (area.to_lowercase(), path.to_lowercase());
    area.is_empty() || path == area || path.starts_with(&format!("{area}/"))
}

/// Runtime repair authority comes from held leases, not task descriptions or read scopes.
pub fn repair_lease<'a>(failed_path: &str, mission: &str, leases: &'a [super::Lease]) -> Option<&'a super::Lease> {
    let path = super::lease::normalized(failed_path).ok()?;
    let owners = leases
        .iter()
        .filter(|l| {
            l.mission == mission
                && l.state == super::LeaseState::Held
                && l.paths.iter().any(|p| super::lease::normalized(p).is_ok_and(|p| contains_path(&p, &path)))
        })
        .collect::<Vec<_>>();
    if owners.len() == 1 { Some(owners[0]) } else { None }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairTarget {
    Drafts(String),
    Owner { task: String, generation: u64 },
}

pub fn failed_check_target(check: &BarCheck, path: &str, mission: &str, leases: &[super::Lease]) -> Result<RepairTarget, CompilerError> {
    if check.attribute.starts_with("screen.")
        || check.attribute.starts_with("a11y.")
        || check.attribute.starts_with("desktop.ghost-")
        || check.attribute == "desktop.real-width-drafts"
        || check.attribute == "software.ui-screen"
    {
        return Ok(RepairTarget::Drafts(check.piece.clone()));
    }
    let lease = repair_lease(path, mission, leases).ok_or_else(|| CompilerError::OrphanCheck(check.id.clone()))?;
    Ok(RepairTarget::Owner { task: lease.task.clone(), generation: lease.generation })
}
