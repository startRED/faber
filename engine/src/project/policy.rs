//! Automated roles have a separate, fail-closed gate; legacy Chat/Projects settings are not evidence of isolation.
//!
//! Help/version probes on 2026-10-12 (no model turn): Claude Code 2.1.296, version 79 ms, help 272 ms;
//! Codex 0.162.1, version/help 77 ms each. These are process elapsed times, not interrupt latency.
//! Claude advertises empty native tools, deny rules, strict MCP configuration, settings, permission mode and effort.
//! Codex advertises sandbox, approval policy, config overrides and feature switches. Neither help output proves
//! command/read isolation, linked-worktree common-dir protection, hook/plugin/MCP/subagent containment, or a maximum
//! invocation exposure including nested calls. Claude's budget estimate is not a hard exposure bound.
//! Windows creates the CLI suspended inside a kill-on-close job before it runs (proc::spawn_command). Other platforms
//! still own one child.
//! Every automated role stays blocked, including research/review, until `qualification` finds a private, fresh receipt
//! for the exact installed CLI version and executable hash (Windows only). No caller input can substitute for it.
//!
//! Before opening admission: record version-bound containment/escape proofs, platform tree termination and measured
//! interrupt latency, and an enforceable exposure bound or verified live usage with a reserved safe stop margin.
//! Never accept provider capability names or caller-supplied booleans as those proofs. Tasks 3/5 consume this gate;
//! Task 6 executes the existing strict read/patch/proof envelopes, and Task 4 owns git/lease path enforcement.
//! The hidden fake CLI tree scenario proves Windows stop/drop/crash cleanup without PID adoption. The owner-authorized
//! opt-in experiment is in testkit/examples/project-policy-probes.rs; bounded observations never unlock admission.
//! Owned-tree termination covers inherited job members, not WMI/scheduled-task/COM broker launches. Restrictions
//! must exclude those command surfaces before admission; native reads, repo config and mention expansion are unproven.
//! Evidence and remaining limits: docs/research/2026-10-12-project-policy-probes.md (six session slots consumed).
//! Protocol reference (config overrides, thread sandbox and approval policy):
//! https://developers.openai.com/codex/app-server . This describes requests, not installed-version enforcement.

use super::Role;
use crate::model::Vendor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    Invocation,
    Restrictions,
    ProcessTree,
    Exposure,
}

impl Blocker {
    pub fn key(self) -> &'static str {
        match self {
            Self::Invocation => "error.project.invocation",
            Self::Restrictions => "reason.project.restrictionsUnproven",
            Self::ProcessTree => "reason.project.processTreeUnproven",
            Self::Exposure => "reason.project.exposureUnproven",
        }
    }
}

pub fn blockers(_vendor: Vendor) -> &'static [Blocker] {
    #[cfg(windows)]
    {
        &[Blocker::Restrictions, Blocker::Exposure]
    }
    #[cfg(not(windows))]
    &[Blocker::Restrictions, Blocker::ProcessTree, Blocker::Exposure]
}

/// Not configurable: no parameter, manifest or flag unlocks execution. Only a qualification receipt does, and only
/// while it matches the installed program. This pre-check looks at the CLI found on PATH; the launch itself is checked
/// again against the exact program it starts (`admit_launch`).
pub fn admit(vendor: Vendor) -> Result<(), Blocker> {
    super::qualification::admitted(vendor, &|| crate::cli::find_cli(vendor.name()).filter(|c| c.prefix.is_empty()).map(|c| c.command))
        .map(|_| ())
        .map_err(|_| blockers(vendor)[0])
}

/// The authoritative check, made by the driver constructors with the program they are about to start. Returns the
/// SHA-256 the receipt names, for the check of the started process image before it runs (`proc::spawn_expecting`).
pub fn admit_launch(vendor: Vendor, bin: &str) -> Result<String, Blocker> {
    super::qualification::admitted(vendor, &|| Some(bin.to_string())).map_err(|_| blockers(vendor)[0])
}

/// Fixed role controls; these values are not an admission receipt.
pub fn output_maximum(vendor: Vendor, role: Role) -> u64 {
    match vendor {
        Vendor::Claude if role == Role::Writer => 32_000,
        Vendor::Claude => 8_000,
        // The installed config/protocol has no model-output control.
        Vendor::Codex => 128_000,
    }
}

/// Reject every native permission request and fail closed if an ungranted tool reaches execution.
pub async fn reject_native(
    stdin: &mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    vendor: Vendor,
    msg: &serde_json::Value,
) -> std::io::Result<bool> {
    use serde_json::json;
    let escaped = match vendor {
        Vendor::Codex => {
            msg["method"] == "item/completed"
                && !matches!(
                    msg["params"]["item"]["type"].as_str(),
                    Some("userMessage" | "agentMessage" | "reasoning" | "plan" | "contextCompaction")
                )
                && msg["params"]["item"]["status"] != "failed"
                && msg["params"]["item"]["status"] != "declined"
        }
        Vendor::Claude => {
            (msg["type"] == "assistant"
                && msg["message"]["content"].as_array().is_some_and(|items| items.iter().any(|item| item["type"] == "tool_use")))
                || (msg["type"] == "stream_event"
                    && msg["event"]["type"] == "content_block_start"
                    && msg["event"]["content_block"]["type"] == "tool_use")
        }
    };
    if escaped {
        return Err(std::io::Error::other("ungranted native tool executed"));
    }
    let reply = match vendor {
        Vendor::Claude if msg["type"] == "control_request" && msg["request"]["subtype"] == "can_use_tool" => {
            let id = msg["request_id"].as_str().ok_or_else(|| std::io::Error::other("missing request identity"))?;
            json!({"type":"control_response", "response":{"subtype":"success", "request_id":id,
                "response":{"behavior":"deny", "message":"Native tools are disabled for project roles."}}})
        }
        Vendor::Codex
            if matches!(msg["method"].as_str(), Some("item/commandExecution/requestApproval" | "item/fileChange/requestApproval")) =>
        {
            let id = msg
                .get("id")
                .filter(|id| id.is_string() || id.is_number())
                .ok_or_else(|| std::io::Error::other("missing request identity"))?;
            json!({"id":id,"result":{"decision":"decline"}})
        }
        Vendor::Codex if matches!(msg["method"].as_str(), Some("item/permissions/requestApproval" | "mcpServer/elicitation/request")) => {
            let id = msg
                .get("id")
                .filter(|id| id.is_string() || id.is_number())
                .ok_or_else(|| std::io::Error::other("missing request identity"))?;
            let result = if msg["method"] == "item/permissions/requestApproval" {
                json!({"permissions":{},"scope":"turn"})
            } else {
                json!({"action":"decline","content":null})
            };
            json!({"id":id,"result":result})
        }
        Vendor::Claude if msg["type"] == "control_request" => return Err(std::io::Error::other("ungranted server request")),
        Vendor::Codex if msg.get("method").is_some() && msg.get("id").is_some() => {
            return Err(std::io::Error::other("ungranted server request"));
        }
        _ => return Ok(false),
    };
    crate::proc::write_line(stdin, &reply).await.map_err(|_| std::io::Error::other("native denial was not delivered"))?;
    Ok(true)
}

pub(crate) fn invocation(vendor: Vendor, model: Option<&str>, effort: Option<&str>, side_question: bool) -> Result<(), Blocker> {
    if side_question
        || !model.is_some_and(|m| super::worker::full_model_id(vendor, m))
        || !matches!(effort, Some("low" | "medium" | "high" | "xhigh" | "max"))
    {
        return Err(Blocker::Invocation);
    }
    Ok(())
}

pub(crate) fn instructions(role: Role) -> &'static str {
    match role {
        Role::Lead => {
            "Return one versioned project JSON envelope per turn. Request scoped reads, plan/amend, named proofs or decisions. Never execute tools or change source. The engine grants authority."
        }
        Role::Writer => {
            "Return one versioned project JSON envelope per turn. Request scoped reads, propose hash-checked patches in the lease or request named proofs. Never execute tools, git, or change the grant. The engine applies patches."
        }
        Role::TestProposal => {
            "Return one versioned project JSON envelope per turn. Request scoped reads and return findings or a proposed test patch artifact. Never mutate source or execute tools. The engine stores proposals outside source."
        }
        Role::Research | Role::Reviewer => {
            "Return one versioned project JSON envelope per turn. Request scoped reads and return findings. Never mutate source or execute tools. The engine grants authority."
        }
    }
}
