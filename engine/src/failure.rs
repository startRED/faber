//! What kind of failure ended a turn, from the CLI's own report: Codex names it (`codexErrorInfo`), Claude Code only writes it
//! (`errors`, `result`), so its words are matched against a fixed list. Each kind has one plain sentence with a next step; what
//! neither tells apart keeps the generic `reason.failed` or `reason.other` (rule 6 of docs/product/models.md).

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The plan's usage is spent (a plan limit the quota recovery did not take).
    Limit,
    /// Too many requests in a short time.
    RateLimit,
    /// The vendor's service is busy or failed on its side.
    Overloaded,
    /// The connection to the vendor's service failed or dropped.
    Connection,
    /// The sign-in is no longer valid.
    SignIn,
    /// The vendor's service refused the request as it was.
    Rejected,
    /// The conversation no longer fits the model's context.
    ContextFull,
}

impl Failure {
    pub fn key(self) -> &'static str {
        match self {
            Failure::Limit => "reason.fail.limit",
            Failure::RateLimit => "reason.fail.rateLimit",
            Failure::Overloaded => "reason.fail.overloaded",
            Failure::Connection => "reason.fail.connection",
            Failure::SignIn => "reason.fail.signIn",
            Failure::Rejected => "reason.fail.rejected",
            Failure::ContextFull => "reason.fail.contextFull",
        }
    }
}

/// Delay before a Codex overload continuation, indexed by the number of retries already attempted.
pub fn overload_retry_delay(kind: Failure, retries: usize) -> Option<std::time::Duration> {
    (kind == Failure::Overloaded).then(|| [2, 4, 8, 16, 30].get(retries).copied()).flatten().map(std::time::Duration::from_secs)
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[test]
    fn overload_retry_delays_are_bounded_and_other_failures_do_not_retry() {
        let delays: Vec<_> = (0..6).map(|n| overload_retry_delay(Failure::Overloaded, n).map(|d| d.as_secs())).collect();
        assert_eq!(delays, [Some(2), Some(4), Some(8), Some(16), Some(30), None]);
        assert_eq!(overload_retry_delay(Failure::RateLimit, 0), None);
    }
}

/// The kind Codex names in `error.codexErrorInfo` (codex-cli 0.166.0, `v2/TurnError`): a camelCase word, or an object whose
/// one key is the word and may carry the upstream `httpStatusCode`. None for what it calls `other`, a sandbox error or a word
/// this version does not know.
pub fn from_codex(info: &Value) -> Option<Failure> {
    let (word, status) = match info {
        Value::String(word) => (word.as_str(), None),
        Value::Object(map) => {
            let (word, inner) = map.iter().next()?;
            (word.as_str(), inner["httpStatusCode"].as_u64())
        }
        _ => return None,
    };
    Some(match word {
        "usageLimitExceeded" => Failure::Limit,
        "rateLimitExceeded" => Failure::RateLimit,
        "serverOverloaded" | "flexUnavailable" | "internalServerError" => Failure::Overloaded,
        "unauthorized" => Failure::SignIn,
        "badRequest" | "modelNotFound" | "cyberPolicy" | "misalignmentPolicyViolation" => Failure::Rejected,
        "contextWindowExceeded" => Failure::ContextFull,
        "httpConnectionFailed" | "responseStreamConnectionFailed" | "responseStreamDisconnected" | "responseTooManyFailedAttempts" => {
            match status {
                Some(401) => Failure::SignIn,
                Some(429) => Failure::RateLimit,
                Some(400 | 403 | 404) => Failure::Rejected,
                Some(500..=599) => Failure::Overloaded,
                _ => Failure::Connection,
            }
        }
        _ => return None,
    })
}

/// The kind a CLI's error text names, from fixed lists of the words Claude Code and the vendors' APIs write (`API Error: 529
/// {"type":"error","error":{"type":"overloaded_error"…`, `Please run /login`, `Prompt is too long`). The API's own status and
/// error type come first, so a loose word in a refused request (`network is not a supported parameter`) never decides; within a
/// list the first kind with a phrase in the text wins. None when no phrase is there.
pub fn from_text(text: &str) -> Option<Failure> {
    // What only the API writes: a status, an error type, a message of its own. A too-long prompt and a spent balance come as
    // refused requests (400), so they go before that.
    const NAMED: &[(Failure, &[&str])] = &[
        (Failure::ContextFull, &["prompt is too long", "context_length_exceeded", "input is too long", "context limit reached"]),
        (Failure::Limit, &["credit balance", "usage limit", "out of credits", "quota exceeded", "quota exhausted"]),
        (Failure::SignIn, &["authentication_error", "api error: 401", "invalid api key", "/login", "oauth token"]),
        (Failure::RateLimit, &["rate_limit_error", "api error: 429"]),
        (
            Failure::Overloaded,
            &["overloaded_error", "api error: 529", "api error: 503", "api error: 502", "api error: 500", "\"api_error\""],
        ),
        // A refusal for want of permission (403) is no sign-in matter: signing in again would not change it.
        (
            Failure::Rejected,
            &["invalid_request_error", "permission_error", "not_found_error", "api error: 400", "api error: 403", "api error: 404"],
        ),
    ];
    // What a person or a library may write too, only when the API named nothing.
    const LOOSE: &[(Failure, &[&str])] = &[
        (Failure::ContextFull, &["context window"]),
        (Failure::SignIn, &["not logged in", "unauthorized"]),
        (Failure::RateLimit, &["rate limit", "too many requests"]),
        (Failure::Overloaded, &["overloaded", "internal server error", "service unavailable"]),
        (
            Failure::Connection,
            &[
                "connection error",
                "connection closed",
                "connection reset",
                "disconnected",
                "timed out",
                "econnreset",
                "enotfound",
                "fetch failed",
                "socket hang up",
            ],
        ),
        (Failure::Rejected, &["bad request"]),
    ];
    let text = text.to_ascii_lowercase();
    let find =
        |list: &[(Failure, &[&str])]| list.iter().find(|(_, phrases)| phrases.iter().any(|p| text.contains(p))).map(|(kind, _)| *kind);
    find(NAMED).or_else(|| find(LOOSE))
}

/// The key of a turn that did not end well: the CLI's own ending word when it says why (a step or spending limit, a stop), else
/// the kind of failure its error names, else the generic key.
pub fn reason(generic: &'static str, kind: Option<Failure>) -> &'static str {
    match (generic, kind) {
        ("reason.failed" | "reason.other", Some(kind)) => kind.key(),
        _ => generic,
    }
}
