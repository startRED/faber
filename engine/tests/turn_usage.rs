use faber_engine::claude::parse_claude_message;
use faber_engine::codex::{CodexTurnState, parse_codex_notification};
use faber_engine::events::EngineEvent;
use serde_json::{Value, json};

fn done(events: Vec<EngineEvent>) -> EngineEvent {
    events.into_iter().next().expect("one Done")
}

#[test]
fn a_claude_result_splits_the_cache_numbers_and_keeps_the_cumulative_cost() {
    let msg = json!({
        "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0123,
        "usage": {"input_tokens": 4, "cache_read_input_tokens": 4537, "cache_creation_input_tokens": 5391, "output_tokens": 483,
                  "cache_creation": {"ephemeral_1h_input_tokens": 5391, "ephemeral_5m_input_tokens": 0}},
        "modelUsage": {"main": {"inputTokens": 4, "cacheReadInputTokens": 4537, "cacheCreationInputTokens": 5391, "outputTokens": 483}}
    });
    let EngineEvent::Done {
        input_tokens, cache_read_tokens, cache_write_tokens, cache_write_1h_tokens, cost_total_usd, incomplete, ..
    } = done(parse_claude_message(&msg))
    else {
        panic!("not a Done")
    };
    assert_eq!((input_tokens, cache_read_tokens, cache_write_tokens, cache_write_1h_tokens), (9932, 4537, 5391, 5391));
    assert_eq!(cost_total_usd, Some(0.0123));
    assert!(!incomplete);
}

#[test]
fn sub_model_work_marks_the_numbers_incomplete_but_keeps_the_cost() {
    // claude/parallel.jsonl: usage shows 9,932 prompt tokens, modelUsage 35,552.
    let msg = json!({
        "type": "result", "subtype": "success", "total_cost_usd": 0.4,
        "usage": {"input_tokens": 4, "cache_read_input_tokens": 4537, "cache_creation_input_tokens": 5391, "output_tokens": 483},
        "modelUsage": {
            "main": {"inputTokens": 4, "cacheReadInputTokens": 4537, "cacheCreationInputTokens": 5391, "outputTokens": 483},
            "fetch": {"inputTokens": 25620, "cacheReadInputTokens": 0, "cacheCreationInputTokens": 0, "outputTokens": 1557}
        }
    });
    let EngineEvent::Done { incomplete, cost_total_usd, input_tokens, .. } = done(parse_claude_message(&msg)) else { panic!("not a Done") };
    assert!(incomplete);
    assert_eq!(cost_total_usd, Some(0.4));
    assert_eq!(input_tokens, 9932);
}

#[test]
fn an_aborted_result_has_no_counters() {
    let msg = json!({"type": "result", "subtype": "error_during_execution", "is_error": true, "total_cost_usd": 0, "usage": {}});
    let EngineEvent::Done { ok, cost_total_usd, input_tokens, incomplete, .. } = done(parse_claude_message(&msg)) else {
        panic!("not a Done")
    };
    assert!(!ok);
    assert_eq!((cost_total_usd, input_tokens, incomplete), (None, 0, false));
    let missing = json!({"type": "result", "subtype": "success"});
    let EngineEvent::Done { cost_total_usd, .. } = done(parse_claude_message(&missing)) else { panic!("not a Done") };
    assert_eq!(cost_total_usd, None, "a result with no cost is not a cost of 0");
}

fn update(input: u64, cached: u64, output: u64) -> Value {
    json!({"method": "thread/tokenUsage/updated", "params": {"tokenUsage": {"total": {"inputTokens": input, "cachedInputTokens": cached, "cacheWriteInputTokens": 0, "outputTokens": output}}}})
}

fn started() -> Value {
    json!({"method": "turn/started", "params": {"turn": {"id": "t"}}})
}

fn completed(status: &str) -> Value {
    json!({"method": "turn/completed", "params": {"turn": {"status": status}}})
}

#[test]
fn a_resumed_codex_turn_is_the_difference_of_the_thread_totals() {
    // codex/resume.jsonl: 14,204 input before the turn, 28,435 after; 14,080 of the difference was cached; 5 output.
    let mut state = CodexTurnState::new(true);
    parse_codex_notification(&update(14_204, 7_000, 20), &mut state);
    parse_codex_notification(&started(), &mut state);
    parse_codex_notification(&update(28_435, 21_080, 25), &mut state);
    let EngineEvent::Done { ok, input_tokens, cache_read_tokens, output_tokens, incomplete, .. } =
        done(parse_codex_notification(&completed("completed"), &mut state))
    else {
        panic!("not a Done")
    };
    assert!(ok);
    assert_eq!((input_tokens, cache_read_tokens, output_tokens, incomplete), (14_231, 14_080, 5, false));
}

#[test]
fn a_new_codex_thread_starts_from_nothing() {
    let mut state = CodexTurnState::new(false);
    parse_codex_notification(&started(), &mut state);
    parse_codex_notification(&update(14_254, 6_912, 30), &mut state);
    let EngineEvent::Done { input_tokens, cache_read_tokens, .. } = done(parse_codex_notification(&completed("completed"), &mut state))
    else {
        panic!("not a Done")
    };
    assert_eq!((input_tokens, cache_read_tokens), (14_254, 6_912));
}

#[test]
fn a_resumed_thread_with_no_update_before_the_turn_reports_no_usage() {
    let mut state = CodexTurnState::new(true);
    parse_codex_notification(&started(), &mut state);
    parse_codex_notification(&update(28_435, 21_080, 25), &mut state);
    let EngineEvent::Done { input_tokens, incomplete, .. } = done(parse_codex_notification(&completed("completed"), &mut state)) else {
        panic!("not a Done")
    };
    assert_eq!(input_tokens, 0, "not the whole thread");
    assert!(incomplete);
}

#[test]
fn an_interrupted_turn_reports_what_arrived_and_the_next_one_does_not_pay_for_it_again() {
    let mut state = CodexTurnState::new(false);
    parse_codex_notification(&started(), &mut state);
    parse_codex_notification(&update(1_000, 0, 10), &mut state);
    let EngineEvent::Done { ok, input_tokens, .. } = done(parse_codex_notification(&completed("interrupted"), &mut state)) else {
        panic!("not a Done")
    };
    assert!(!ok);
    assert_eq!(input_tokens, 1_000);
    parse_codex_notification(&started(), &mut state);
    parse_codex_notification(&update(1_800, 900, 14), &mut state);
    let EngineEvent::Done { input_tokens, cache_read_tokens, output_tokens, .. } =
        done(parse_codex_notification(&completed("completed"), &mut state))
    else {
        panic!("not a Done")
    };
    assert_eq!((input_tokens, cache_read_tokens, output_tokens), (800, 900, 4));
}

#[test]
fn codex_cache_writes_are_per_turn_and_missing_fields_are_incomplete() {
    let mut state = CodexTurnState::new(false);
    parse_codex_notification(&started(), &mut state);
    let mut first = update(1000, 400, 10);
    first["params"]["tokenUsage"]["total"]["cacheWriteInputTokens"] = 200.into();
    parse_codex_notification(&first, &mut state);
    let EngineEvent::Done { cache_write_tokens, incomplete, .. } = done(parse_codex_notification(&completed("completed"), &mut state))
    else {
        panic!("not Done")
    };
    assert_eq!((cache_write_tokens, incomplete), (200, false));
    parse_codex_notification(&started(), &mut state);
    let mut next = update(1500, 600, 20);
    next["params"]["tokenUsage"]["total"]["cacheWriteInputTokens"] = 300.into();
    parse_codex_notification(&next, &mut state);
    let EngineEvent::Done { cache_write_tokens, .. } = done(parse_codex_notification(&completed("completed"), &mut state)) else {
        panic!("not Done")
    };
    assert_eq!(cache_write_tokens, 100);
    parse_codex_notification(&started(), &mut state);
    next["params"]["tokenUsage"]["total"].as_object_mut().unwrap().remove("cacheWriteInputTokens");
    parse_codex_notification(&next, &mut state);
    let EngineEvent::Done { incomplete, .. } = done(parse_codex_notification(&completed("completed"), &mut state)) else {
        panic!("not Done")
    };
    assert!(incomplete, "an absent write counter is not a known zero");
}
