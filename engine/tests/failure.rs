use crate::support::fixture;

use faber_engine::claude::parse_claude_message;
use faber_engine::codex::{CodexTurnState, parse_codex_notification};
use faber_engine::events::EngineEvent;
use serde_json::{Value, json};

fn reason(events: Vec<EngineEvent>) -> Option<&'static str> {
    events.into_iter().find_map(|e| if let EngineEvent::Reason(key) = e { Some(key) } else { None })
}

/// The recorded end of a Codex turn that failed (a usage limit, `steer-limit`), with another `codexErrorInfo` and message.
fn codex_end(info: Value, message: &str) -> Option<&'static str> {
    let mut end = fixture("codex", "steer-limit").into_iter().map(|e| e.msg).find(|m| m["method"] == "turn/completed").unwrap();
    end["params"]["turn"]["error"] = json!({"message": message, "codexErrorInfo": info, "additionalDetails": null});
    reason(parse_codex_notification(&end, &mut CodexTurnState::default()))
}

fn claude_end(result: Value) -> Option<&'static str> {
    let mut msg = json!({"type":"result","subtype":"success","is_error":true,"total_cost_usd":0.0,"usage":{}});
    msg.as_object_mut().unwrap().extend(result.as_object().unwrap().clone());
    reason(parse_claude_message(&msg))
}

#[test]
fn a_codex_turn_that_failed_says_the_kind_its_error_names() {
    let recorded = fixture("codex", "steer-limit").into_iter().map(|e| e.msg).find(|m| m["method"] == "turn/completed").unwrap();
    assert_eq!(reason(parse_codex_notification(&recorded, &mut CodexTurnState::default())), Some("reason.fail.limit"));
    for (info, key) in [
        (json!("serverOverloaded"), "reason.fail.overloaded"),
        (json!("internalServerError"), "reason.fail.overloaded"),
        (json!("rateLimitExceeded"), "reason.fail.rateLimit"),
        (json!("contextWindowExceeded"), "reason.fail.contextFull"),
        (json!("unauthorized"), "reason.fail.signIn"),
        (json!("badRequest"), "reason.fail.rejected"),
        (json!({"httpConnectionFailed": {"httpStatusCode": null}}), "reason.fail.connection"),
        (json!({"responseStreamDisconnected": {"httpStatusCode": null}}), "reason.fail.connection"),
        // A connection failure that carries the server's status says what the status means.
        (json!({"responseStreamConnectionFailed": {"httpStatusCode": 401}}), "reason.fail.signIn"),
        (json!({"httpConnectionFailed": {"httpStatusCode": 429}}), "reason.fail.rateLimit"),
        // No permission for a model is no sign-in matter: signing in again would not change it.
        (json!({"httpConnectionFailed": {"httpStatusCode": 403}}), "reason.fail.rejected"),
        (json!({"responseTooManyFailedAttempts": {"httpStatusCode": 503}}), "reason.fail.overloaded"),
        // What Codex cannot tell apart, and a word this version does not know, stay generic.
        (json!("other"), "reason.failed"),
        (json!("sandboxError"), "reason.failed"),
        (json!("aWordFromANewerCodex"), "reason.failed"),
    ] {
        assert_eq!(codex_end(info.clone(), "something went wrong"), Some(key), "{info}");
    }
    // With no kind named, the message's words still say it.
    assert_eq!(codex_end(Value::Null, "stream disconnected before completion"), Some("reason.fail.connection"));
    assert_eq!(codex_end(Value::Null, "something went wrong"), Some("reason.failed"));
}

#[test]
fn a_claude_turn_that_failed_says_the_kind_its_error_words_name() {
    for (words, key) in [
        (r#"API Error: 529 {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#, "reason.fail.overloaded"),
        (r#"API Error: 500 {"type":"error","error":{"type":"api_error","message":"Internal server error"}}"#, "reason.fail.overloaded"),
        (
            r#"API Error: 429 {"type":"error","error":{"type":"rate_limit_error","message":"Number of requests has exceeded your rate limit"}}"#,
            "reason.fail.rateLimit",
        ),
        ("API Error: Connection error.", "reason.fail.connection"),
        ("API Error: Request timed out.", "reason.fail.connection"),
        ("Invalid API key · Please run /login", "reason.fail.signIn"),
        ("OAuth token has expired. Please obtain a new token or refresh your existing token.", "reason.fail.signIn"),
        ("Prompt is too long", "reason.fail.contextFull"),
        ("Credit balance is too low", "reason.fail.limit"),
        (
            r#"API Error: 400 {"type":"error","error":{"type":"invalid_request_error","message":"messages: text content blocks must be non-empty"}}"#,
            "reason.fail.rejected",
        ),
        (
            r#"API Error: 403 {"type":"error","error":{"type":"permission_error","message":"not allowed to use this model"}}"#,
            "reason.fail.rejected",
        ),
        // The API's own status and type decide over a loose word in its message.
        (
            r#"API Error: 400 {"type":"error","error":{"type":"invalid_request_error","message":"network is not a supported parameter"}}"#,
            "reason.fail.rejected",
        ),
        (
            r#"API Error: 400 {"type":"error","error":{"type":"invalid_request_error","message":"max_tokens is larger than the context window"}}"#,
            "reason.fail.rejected",
        ),
        (
            r#"API Error: 400 {"type":"error","error":{"type":"invalid_request_error","message":"Your credit balance is too low"}}"#,
            "reason.fail.limit",
        ),
        (
            r#"API Error: 400 {"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 210000 tokens > 200000 maximum"}}"#,
            "reason.fail.contextFull",
        ),
        ("The tool timed out while reading the network share", "reason.fail.connection"),
        ("Something unexpected happened", "reason.failed"),
    ] {
        assert_eq!(claude_end(json!({ "result": words })), Some(key), "result: {words}");
        assert_eq!(claude_end(json!({ "subtype": "error_during_execution", "errors": [words] })), Some(key), "errors: {words}");
    }
    // The CLI's own ending word says why first: a step limit is a step limit whatever its error words say.
    assert_eq!(claude_end(json!({"subtype": "error_max_turns", "result": "API Error: Connection error."})), Some("reason.turnLimit"));
    assert_eq!(
        claude_end(json!({"subtype": "error_during_execution", "terminal_reason": "aborted_streaming", "errors": ["Request timed out"]})),
        Some("reason.interrupted")
    );
}
