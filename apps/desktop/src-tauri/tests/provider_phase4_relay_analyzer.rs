use localagentmanager_core::provider_relay_compatibility::{
    analyze_relay_compatibility, RelayCompatibilityDisposition, RelayHistoryItem,
    RelayHistoryItemKind, RelayTargetCapabilities,
};

fn target(function_tools: bool, representation_metadata: bool) -> RelayTargetCapabilities {
    RelayTargetCapabilities {
        function_tools,
        representation_metadata,
    }
}

#[test]
fn text_and_completed_verified_function_round_trip_are_compatible() {
    let items = vec![
        RelayHistoryItem::new("m1", RelayHistoryItemKind::Text),
        RelayHistoryItem::new(
            "c1",
            RelayHistoryItemKind::FunctionCall {
                call_id: "call-1".into(),
                verified: true,
            },
        ),
        RelayHistoryItem::new(
            "r1",
            RelayHistoryItemKind::FunctionResult {
                call_id: "call-1".into(),
            },
        ),
    ];
    let report = analyze_relay_compatibility(&items, &target(true, true));
    assert_eq!(
        report.disposition,
        RelayCompatibilityDisposition::Compatible
    );
    assert!(report.issues.is_empty());
    assert!(!report.confirmation_required);
}

#[test]
fn unfinished_or_unverified_function_state_blocks() {
    for items in [
        vec![RelayHistoryItem::new(
            "c1",
            RelayHistoryItemKind::FunctionCall {
                call_id: "call-1".into(),
                verified: true,
            },
        )],
        vec![
            RelayHistoryItem::new(
                "c1",
                RelayHistoryItemKind::FunctionCall {
                    call_id: "call-1".into(),
                    verified: false,
                },
            ),
            RelayHistoryItem::new(
                "r1",
                RelayHistoryItemKind::FunctionResult {
                    call_id: "call-1".into(),
                },
            ),
        ],
    ] {
        let report = analyze_relay_compatibility(&items, &target(true, true));
        assert_eq!(report.disposition, RelayCompatibilityDisposition::Blocked);
        assert!(!report.issues.is_empty());
    }
}

#[test]
fn every_stateful_or_unsupported_item_type_fails_closed() {
    let blocked = vec![
        RelayHistoryItemKind::PreviousResponseState,
        RelayHistoryItemKind::EncryptedReasoning,
        RelayHistoryItemKind::HostedTool,
        RelayHistoryItemKind::McpTool,
        RelayHistoryItemKind::ComputerUse,
        RelayHistoryItemKind::Image,
        RelayHistoryItemKind::Audio,
        RelayHistoryItemKind::File,
        RelayHistoryItemKind::Unknown,
        RelayHistoryItemKind::Corrupt,
    ];
    for (index, kind) in blocked.into_iter().enumerate() {
        let report = analyze_relay_compatibility(
            &[RelayHistoryItem::new(format!("i{index}"), kind)],
            &target(true, true),
        );
        assert_eq!(report.disposition, RelayCompatibilityDisposition::Blocked);
        assert_eq!(report.issues.len(), 1);
        assert!(!report.issues[0].recovery_action.is_empty());
    }
}

#[test]
fn harmless_representation_loss_requires_explicit_confirmation() {
    let report = analyze_relay_compatibility(
        &[RelayHistoryItem::new(
            "meta-1",
            RelayHistoryItemKind::RepresentationMetadata,
        )],
        &target(true, false),
    );
    assert_eq!(
        report.disposition,
        RelayCompatibilityDisposition::CompatibleWithLoss
    );
    assert!(report.confirmation_required);
    assert_eq!(report.transformations.len(), 1);
}

#[test]
fn report_is_deterministic_and_contains_no_payload_or_secret() {
    let items = vec![RelayHistoryItem::new(
        "safe-id",
        RelayHistoryItemKind::PreviousResponseState,
    )];
    let first = analyze_relay_compatibility(&items, &target(false, false));
    let second = analyze_relay_compatibility(&items, &target(false, false));
    assert_eq!(first, second);
    let json = serde_json::to_string(&first).unwrap();
    assert!(!json.contains("api_key"));
    assert!(!json.contains("authorization"));
    assert!(!json.contains("session content"));
}
