use faber_engine::budget::{Billing, Reach};
use faber_engine::signals::{Signals, claude_plan, claude_signals, codex_account, codex_signals};
use serde_json::json;

#[test]
fn a_claude_event_with_room_clears_and_says_extra_usage_is_off() {
    // The shape of tests/fixtures/claude/chat.jsonl.
    let msg = json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed", "resetsAt": 1791627000, "rateLimitType": "five_hour",
        "overageStatus": "rejected", "overageDisabledReason": "org_level_disabled", "isUsingOverage": false}});
    assert_eq!(
        claude_signals(&msg),
        Signals { reach: Some(Reach::Clear), overage_active: Some(false), paid_fallback: Some(false), ..Signals::default() }
    );
}

#[test]
fn a_claude_event_that_blocks_a_vendor_wide_window_is_out_until_its_reset() {
    let msg = json!({"rate_limit_info": {"status": "rejected", "resetsAt": 1791878400, "rateLimitType": "seven_day", "overageStatus": "allowed", "isUsingOverage": true}});
    let s = claude_signals(&msg);
    assert_eq!(s.reach, Some(Reach::Out(Some(1791878400.0))));
    assert_eq!((s.overage_active, s.paid_fallback), (Some(true), Some(true)));
}

#[test]
fn a_family_window_status_does_not_block_the_vendor() {
    let msg = json!({"rate_limit_info": {"status": "rejected", "resetsAt": 1, "rateLimitType": "seven_day_opus"}});
    assert_eq!(claude_signals(&msg).reach, None);
}

#[test]
fn a_codex_rate_limit_update_gives_plan_credits_and_reach() {
    // The shape of tests/fixtures/codex/chat.jsonl.
    let msg = json!({"method": "account/rateLimits/updated", "params": {"rateLimits": {"primary": {"usedPercent": 47, "windowDurationMins": 10080, "resetsAt": 1791967370},
        "secondary": null, "credits": {"hasCredits": false, "unlimited": false, "balance": "0"}, "spendControlReached": null, "planType": "pro", "rateLimitReachedType": null}}});
    assert_eq!(
        codex_signals(&msg),
        Signals { reach: Some(Reach::Clear), paid_fallback: Some(false), plan: Some("pro".into()), ..Signals::default() }
    );
    let out = json!({"params": {"rateLimits": {"primary": {"resetsAt": 100}, "secondary": {"resetsAt": 900}, "rateLimitReachedType": "primary", "credits": {"hasCredits": true}}}});
    let s = codex_signals(&out);
    assert_eq!(s.reach, Some(Reach::Out(Some(900.0))));
    assert_eq!(s.paid_fallback, Some(true));
}

#[test]
fn the_codex_sign_in_mode_decides_the_billing() {
    let chatgpt = json!({"method": "account/updated", "params": {"authMode": "chatgpt", "planType": "pro"}});
    assert_eq!(codex_account(&chatgpt), Signals { plan: Some("pro".into()), billing: Some(Billing::Subscription), ..Signals::default() });
    let key = json!({"params": {"authMode": "apikey"}});
    assert_eq!(codex_account(&key).billing, Some(Billing::UsageBilled));
    let cloud = json!({"params": {"authMode": "amazonBedrock"}});
    assert_eq!(codex_account(&cloud).billing, Some(Billing::UsageBilled), "any way in but ChatGPT is billed by use");
    let metered = json!({"params": {"authMode": "chatgpt", "planType": "self_serve_business_usage_based"}});
    assert_eq!(codex_account(&metered).billing, Some(Billing::UsageBilled), "a plan billed by use is not a subscription");
    assert_eq!(codex_account(&json!({"params": {}})).billing, Some(Billing::Unknown), "a missing mode clears the verdict");
    let limits = json!({"params": {"rateLimits": {"planType": "enterprise_cbp_usage_based"}}});
    assert_eq!(codex_signals(&limits).billing, Some(Billing::UsageBilled));
    assert_eq!(
        codex_signals(&json!({"params": {"rateLimits": {"planType": "pro"}}})).billing,
        None,
        "a limit update never proves a subscription"
    );
}

#[test]
fn claude_auth_status_gives_the_plan_and_drops_the_rest() {
    let s = claude_plan(r#"{"loggedIn": true, "email": "someone@example.com", "subscriptionType": "max"}"#);
    assert_eq!(s, Signals { plan: Some("max".into()), billing: Some(Billing::Subscription), ..Signals::default() });
    let key = claude_plan(r#"{"loggedIn": true, "authMethod": "api_key"}"#);
    assert_eq!((key.plan, key.billing), (None, Some(Billing::UsageBilled)));
    assert_eq!(claude_plan("not json"), Signals::default());
    assert_eq!(claude_plan(r#"{"loggedIn": true}"#).billing, Some(Billing::Unknown), "a missing field does not prove a subscription");
}

#[test]
fn an_unrecognised_or_signed_out_account_does_not_prove_subscription_billing() {
    assert_eq!(codex_account(&json!({"params": {"authMode": "chatgpt-unrecognised"}})).billing, Some(Billing::Unknown));
    assert_eq!(codex_account(&json!({"params": {"authMode": null}})).billing, Some(Billing::Unknown));
    assert_eq!(claude_plan(r#"{"loggedIn":false,"subscriptionType":"max"}"#).billing, Some(Billing::Unknown));
    assert_eq!(claude_plan(r#"{"loggedIn":true,"authMethod":"api_key","subscriptionType":"max"}"#).billing, Some(Billing::UsageBilled));
}

#[test]
fn absent_credit_and_limit_fields_do_not_clear_previous_guards() {
    let signals = codex_signals(&json!({"params":{"rateLimits":{"primary":{"usedPercent":50}}}}));
    assert_eq!(signals.paid_fallback, None);
    assert_eq!(signals.reach, None);
    let partial = codex_signals(&json!({"params":{"rateLimits":{"credits":{"hasCredits":false}}}}));
    assert_eq!(partial.paid_fallback, None);
}

#[test]
fn a_message_that_says_nothing_about_the_account_is_empty() {
    assert!(Signals::default().is_empty());
    assert!(claude_signals(&json!({ "type": "rate_limit_event", "rate_limit_info": {} })).is_empty());
    assert!(!claude_signals(&json!({ "rate_limit_info": { "isUsingOverage": false } })).is_empty());
}
