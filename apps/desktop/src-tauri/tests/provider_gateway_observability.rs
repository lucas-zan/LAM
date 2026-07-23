use localagentmanager_core::gateway::observability::{
    capacity_identity_hash, queued_body_envelope, GatewayRejectionReason, GatewayTimeoutStage,
    Stage5LoadRecommendations,
};
use localagentmanager_core::provider_credentials::{CredentialSource, UpstreamAuth};
use std::time::Duration;

fn keychain_auth(account: &str, version: u64) -> UpstreamAuth {
    UpstreamAuth::Bearer {
        source: CredentialSource::Keychain {
            service: "lam.remote-provider".into(),
            account: account.into(),
            version,
        },
    }
}

#[test]
fn capacity_identity_is_stable_redacted_and_credential_scoped() {
    let endpoint = "https://api.example.com/v1";
    let first = capacity_identity_hash(endpoint, &keychain_auth("credential/a/v1", 1)).unwrap();
    let same = capacity_identity_hash(
        "https://API.EXAMPLE.COM/v1/",
        &keychain_auth("credential/a/v1", 1),
    )
    .unwrap();
    let different = capacity_identity_hash(endpoint, &keychain_auth("credential/b/v1", 1)).unwrap();

    assert_eq!(first, same);
    assert_ne!(first, different);
    assert_eq!(first.len(), 16);
    assert!(first.chars().all(|value| value.is_ascii_hexdigit()));
    assert!(!first.contains("example"));
    assert!(!first.contains("credential"));
}

#[test]
fn capacity_identity_rejects_ambiguous_endpoints() {
    let auth = keychain_auth("credential/a/v1", 1);
    for endpoint in [
        "not a url",
        "http://api.example.com/v1",
        "https://api.example.com/v1?api_key=secret",
        "https://api.example.com/v1#fragment",
    ] {
        assert!(
            capacity_identity_hash(endpoint, &auth).is_err(),
            "{endpoint}"
        );
    }
}

#[test]
fn capacity_identity_does_not_depend_on_secret_values() {
    let auth = UpstreamAuth::Bearer {
        source: CredentialSource::Env {
            env_key: "STAGE4_TEST_API_KEY".into(),
        },
    };
    std::env::set_var("STAGE4_TEST_API_KEY", "LAM_TEST_SECRET_VALUE");
    let identity = capacity_identity_hash("https://api.example.com/v1", &auth).unwrap();
    std::env::remove_var("STAGE4_TEST_API_KEY");

    assert!(!identity.contains("LAM_TEST_SECRET_VALUE"));
}

#[test]
fn timeout_and_rejection_labels_use_a_bounded_allowlist() {
    let timeout_labels = [
        GatewayTimeoutStage::Queue,
        GatewayTimeoutStage::Handler,
        GatewayTimeoutStage::UpstreamFirstByte,
        GatewayTimeoutStage::UpstreamIdle,
        GatewayTimeoutStage::UpstreamTotal,
        GatewayTimeoutStage::Other,
    ]
    .map(|value| serde_json::to_string(&value).unwrap());
    assert_eq!(
        timeout_labels,
        [
            "\"queue\"",
            "\"handler\"",
            "\"upstream_first_byte\"",
            "\"upstream_idle\"",
            "\"upstream_total\"",
            "\"other\"",
        ]
    );
    assert_eq!(
        serde_json::to_string(&GatewayRejectionReason::GlobalQueueFull).unwrap(),
        "\"global_queue_full\""
    );
    assert_eq!(
        serde_json::to_string(&GatewayRejectionReason::BindingQueueFull).unwrap(),
        "\"binding_queue_full\""
    );
    assert_eq!(
        GatewayTimeoutStage::from_code("UPSTREAM_FIRST_BYTE_TIMEOUT"),
        GatewayTimeoutStage::UpstreamFirstByte
    );
    assert_eq!(
        GatewayTimeoutStage::from_code("UNBOUNDED_RAW_ERROR_MESSAGE"),
        GatewayTimeoutStage::Other
    );
}

#[test]
fn load_envelope_and_stage5_recommendations_are_bounded() {
    const MIB: usize = 1024 * 1024;
    assert_eq!(queued_body_envelope(4 * MIB, 64).unwrap(), 256 * MIB);
    assert_eq!(queued_body_envelope(4 * MIB, 8).unwrap(), 32 * MIB);
    assert!(queued_body_envelope(usize::MAX, 2).is_err());

    let recommendations = Stage5LoadRecommendations::default();
    assert_eq!(recommendations.global_queued_body_bytes, 32 * MIB);
    assert_eq!(recommendations.binding_queued_body_bytes, 8 * MIB);
    assert_eq!(recommendations.queue_timeout, Duration::from_secs(30));
    assert!(recommendations.global_queued_body_bytes < 256 * MIB);
    assert!(recommendations.binding_queued_body_bytes < 32 * MIB);
    assert!(recommendations.queue_timeout < Duration::from_secs(15 * 60));
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit 256 MiB Stage 4 RSS probe"]
fn rss_probe_for_sixty_four_four_mib_bodies() {
    const BODY_BYTES: usize = 4 * 1024 * 1024;
    let before = peak_rss_bytes();
    let bodies = (0..64)
        .map(|index| vec![index as u8; BODY_BYTES])
        .collect::<Vec<_>>();
    std::hint::black_box(&bodies);
    let after = peak_rss_bytes();
    let delta = after.saturating_sub(before);
    println!("stage4_rss_before={before} after={after} delta={delta}");
    assert!(
        delta >= 240 * 1024 * 1024,
        "RSS probe did not touch all bodies"
    );
}

#[cfg(target_os = "macos")]
fn peak_rss_bytes() -> usize {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    assert_eq!(result, 0);
    let usage = unsafe { usage.assume_init() };
    usage.ru_maxrss as usize
}
