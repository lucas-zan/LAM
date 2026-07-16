use chrono::{TimeZone, Utc};
use localagentmanager_core::adapters::protocol::ChatUsage;
use localagentmanager_core::adapters::terminal::*;
use std::time::Duration;

#[test]
fn usage_preserves_known_details_and_never_fabricates_missing_values() {
    let full: ChatUsage = serde_json::from_value(serde_json::json!({
      "prompt_tokens":8,"completion_tokens":3,"total_tokens":11,
      "prompt_tokens_details":{"cached_tokens":2},
      "completion_tokens_details":{"reasoning_tokens":1}
    }))
    .unwrap();
    let mapped = normalize_usage(Some(&full)).unwrap().unwrap();
    assert_eq!(mapped.input_tokens_details.unwrap().cached_tokens, Some(2));
    assert_eq!(
        mapped.output_tokens_details.unwrap().reasoning_tokens,
        Some(1)
    );
    assert_eq!(normalize_usage(None).unwrap(), None);

    let inconsistent: ChatUsage = serde_json::from_value(serde_json::json!({
      "prompt_tokens":8,"completion_tokens":3,"total_tokens":99
    }))
    .unwrap();
    assert_eq!(
        normalize_usage(Some(&inconsistent)).unwrap_err().code,
        StableErrorCode::UsageInconsistent
    );
}

#[test]
fn http_statuses_are_classified_without_exposing_upstream_body() {
    let secret = b"upstream body sk-secret private prompt";
    for (status, category, retryable) in [
        (400, ErrorCategory::Validation, false),
        (401, ErrorCategory::Auth, false),
        (403, ErrorCategory::Auth, false),
        (408, ErrorCategory::Timeout, true),
        (409, ErrorCategory::Upstream, true),
        (429, ErrorCategory::Upstream, true),
        (500, ErrorCategory::Upstream, true),
        (503, ErrorCategory::Upstream, true),
    ] {
        let error = normalize_upstream_http(status, Some("3"), secret, false);
        assert_eq!(error.category, category);
        assert_eq!(error.retryable, retryable);
        assert_eq!(error.upstream_status, Some(status));
        assert_eq!(error.retry_after, Some(Duration::from_secs(3)));
        let encoded = format!("{error:?} {}", serde_json::to_string(&error).unwrap());
        assert!(!encoded.contains("sk-secret"));
        assert!(!encoded.contains("private prompt"));
    }
}

#[test]
fn retry_after_supports_seconds_and_http_dates_with_a_bound() {
    let now = Utc.with_ymd_and_hms(2015, 10, 21, 7, 27, 0).unwrap();
    assert_eq!(parse_retry_after("12", now), Some(Duration::from_secs(12)));
    assert_eq!(
        parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT", now),
        Some(Duration::from_secs(60))
    );
    assert_eq!(parse_retry_after("not-a-date", now), None);
    assert_eq!(parse_retry_after("999999999", now), Some(MAX_RETRY_AFTER));
}

#[test]
fn transport_terminal_types_distinguish_first_byte_and_replay_safety() {
    for (kind, category) in [
        (TransportFailure::Connect, ErrorCategory::Upstream),
        (TransportFailure::Timeout, ErrorCategory::Timeout),
        (TransportFailure::Cancelled, ErrorCategory::Cancelled),
        (TransportFailure::Disconnect, ErrorCategory::Upstream),
        (TransportFailure::MalformedResponse, ErrorCategory::Upstream),
        (TransportFailure::BufferOverflow, ErrorCategory::Internal),
    ] {
        let before = normalize_transport_failure(kind, false);
        let after = normalize_transport_failure(kind, true);
        assert_eq!(before.category, category);
        assert_eq!(after.category, category);
        assert!(!after.replay_safe);
    }
    assert!(normalize_transport_failure(TransportFailure::Connect, false).replay_safe);
    assert!(!normalize_transport_failure(TransportFailure::Disconnect, false).replay_safe);
}

#[test]
fn every_stable_error_code_has_a_recovery_action_and_no_replay_decision() {
    for code in StableErrorCode::ALL {
        assert_ne!(recovery_action(*code), RecoveryAction::Unknown);
    }
    let error = normalize_upstream_http(429, None, b"{}", false);
    assert!(error.retryable);
    assert!(!error.replay_safe);
    assert_eq!(error.application_retry_count, 0);
}
