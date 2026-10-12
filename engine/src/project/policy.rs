//! Automated roles have a separate, fail-closed gate; legacy Chat/Projects settings are not evidence of isolation.
//!
//! Help/version probes on 2026-10-12 (no model turn): Claude Code 2.1.296, version 79 ms, help 272 ms;
//! Codex 0.162.1, version/help 77 ms each. These are process elapsed times, not interrupt latency.
//! Claude advertises empty native tools, deny rules, strict MCP configuration, settings, permission mode and effort.
//! Codex advertises sandbox, approval policy, config overrides and feature switches. Neither help output proves
//! command/read isolation, linked-worktree common-dir protection, hook/plugin/MCP/subagent containment, or a maximum
//! invocation exposure including nested calls. Claude's budget estimate is not a hard exposure bound.
//! Windows owns a kill-on-close job before a trusted guard may start the CLI. Other platforms still own one child.
//! Every automated role therefore stays blocked, including research/review. No real session is needed to deny it.
//!
//! Before opening admission: record version-bound containment/escape proofs, platform tree termination and measured
//! interrupt latency, and an enforceable exposure bound or verified live usage with a reserved safe stop margin.
//! Never accept provider capability names or caller-supplied booleans as those proofs. Tasks 3/5 consume this gate;
//! Task 6 executes the existing strict read/patch/proof envelopes, and Task 4 owns git/lease path enforcement.
//! The hidden fake CLI tree scenario proves Windows stop/drop/crash cleanup without PID adoption. The owner-authorized
//! opt-in experiment is in testkit/examples/project-policy-probes.rs; bounded observations never unlock admission.
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

/// This is deliberately not configurable: a manifest claim cannot unlock execution.
pub fn admit(vendor: Vendor) -> Result<(), Blocker> {
    Err(blockers(vendor)[0])
}

/// Reject every native permission request, including unknown tool names; this never parses command text.
pub async fn reject_native(
    stdin: &mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    vendor: Vendor,
    msg: &serde_json::Value,
) -> std::io::Result<bool> {
    use serde_json::json;
    let reply = match vendor {
        Vendor::Claude if msg["type"] == "control_request" && msg["request"]["subtype"] == "can_use_tool" => {
            let id = msg["request_id"].as_str().ok_or_else(|| std::io::Error::other("missing request identity"))?;
            json!({"type":"control_response", "response":{"subtype":"success", "request_id":id,
                "response":{"behavior":"deny", "message":"Native tools are disabled for project roles."}}})
        }
        Vendor::Codex if msg["method"].as_str().is_some_and(|m| m.ends_with("requestApproval")) => {
            let id = msg
                .get("id")
                .filter(|id| id.is_string() || id.is_number())
                .ok_or_else(|| std::io::Error::other("missing request identity"))?;
            json!({"id":id,"result":{"decision":"decline"}})
        }
        _ => return Ok(false),
    };
    crate::proc::write_line(stdin, &reply).await.map_err(|_| std::io::Error::other("native denial was not delivered"))?;
    Ok(true)
}

pub(crate) fn invocation(vendor: Vendor, model: Option<&str>, effort: Option<&str>, side_question: bool) -> Result<(), Blocker> {
    let prefix = match vendor {
        Vendor::Claude => "claude-",
        Vendor::Codex => "gpt-",
    };
    if side_question
        || !model.is_some_and(|m| {
            m.starts_with(prefix) && m.len() > prefix.len() && m.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        })
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
