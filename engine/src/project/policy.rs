//! Automated roles have a separate, fail-closed gate; legacy Chat/Projects settings are not evidence of isolation.
//!
//! Help/version probes on 2026-10-12 (no model turn): Claude Code 2.1.296, version 79 ms, help 272 ms;
//! Codex 0.162.1, version/help 77 ms each. These are process elapsed times, not interrupt latency.
//! Claude advertises empty native tools, deny rules, strict MCP configuration, settings, permission mode and effort.
//! Codex advertises sandbox, approval policy, config overrides and feature switches. Neither help output proves
//! command/read isolation, linked-worktree common-dir protection, hook/plugin/MCP/subagent containment, or a maximum
//! invocation exposure including nested calls. Claude's budget estimate is not a hard exposure bound.
//! The current engine also owns only a child handle, not an installed pre-execution process-tree wrapper.
//! Every automated role therefore stays blocked, including research/review. No real session is needed to deny it.
//!
//! Before opening admission: record version-bound containment/escape proofs, owned-tree termination and measured
//! interrupt latency, and an enforceable exposure bound or verified live usage with a reserved safe stop margin.
//! Never accept provider capability names or caller-supplied booleans as those proofs. Tasks 3/5 consume this gate;
//! Task 6 executes the existing strict read/patch/proof envelopes, and Task 4 owns git/lease path enforcement.
//! No hidden native escape scenario or other-vendor model review was run: all role admission is closed and this
//! task authorizes help/version and fake CLI calls only. These remain rollout gates, not waived proofs.
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
    &[Blocker::Restrictions, Blocker::ProcessTree, Blocker::Exposure]
}

/// This is deliberately not configurable: a manifest claim cannot unlock execution.
pub fn admit(vendor: Vendor) -> Result<(), Blocker> {
    Err(blockers(vendor)[0])
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
