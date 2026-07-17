use localagentmanager_core::provider_runtime::gateway_first_response_timeout_from_env;
use std::ffi::OsStr;
use std::time::Duration;

#[test]
fn gateway_first_response_timeout_env_defaults_validates_and_parses() {
    assert_eq!(
        gateway_first_response_timeout_from_env(None).unwrap(),
        Duration::from_secs(60)
    );
    assert_eq!(
        gateway_first_response_timeout_from_env(Some(OsStr::new("120"))).unwrap(),
        Duration::from_secs(120)
    );
    for invalid in ["", "bad", "9", "601"] {
        assert_eq!(
            gateway_first_response_timeout_from_env(Some(OsStr::new(invalid)))
                .unwrap_err()
                .code,
            "GATEWAY_TIMEOUT_CONFIG_INVALID"
        );
    }
}
