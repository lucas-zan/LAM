use localagentmanager_core::provider_credentials::{
    direct_codex_auth, resolve_credential, validate_codex_security_options, validate_upstream_auth,
    CredentialSource, DirectCodexAuth, EnvironmentReader, UpstreamAuth,
};
use localagentmanager_core::provider_v2::CodexProviderOptions;
use std::collections::BTreeMap;
use std::ffi::OsString;

#[derive(Default)]
struct FakeEnvironment(BTreeMap<String, OsString>);

impl EnvironmentReader for FakeEnvironment {
    fn get(&self, name: &str) -> Option<OsString> {
        self.0.get(name).cloned()
    }
}

fn env_source() -> CredentialSource {
    CredentialSource::Env {
        env_key: "EXAMPLE_API_KEY".into(),
    }
}

#[test]
fn bearer_and_named_header_env_resolve_without_a_plaintext_return_surface() {
    let marker = "LAM_TEST_SECRET_API_KEY_sk-rpg405-7f3a";
    let environment = FakeEnvironment(BTreeMap::from([(
        "EXAMPLE_API_KEY".into(),
        OsString::from(marker),
    )]));
    let secret = resolve_credential(&env_source(), &environment).unwrap();
    assert_eq!(secret.with_exposed(|value| value.len()), marker.len());
    assert_eq!(format!("{secret:?}"), "SecretValue([REDACTED])");
    assert!(!format!("{}", secret.display_redacted()).contains(marker));

    let bearer = UpstreamAuth::Bearer {
        source: env_source(),
    };
    let header = UpstreamAuth::Header {
        name: "anthropic-api-key".into(),
        source: env_source(),
    };
    validate_upstream_auth(&bearer).unwrap();
    validate_upstream_auth(&header).unwrap();
    assert_eq!(
        direct_codex_auth(&bearer).unwrap(),
        DirectCodexAuth::EnvKey {
            env_key: "EXAMPLE_API_KEY".into()
        }
    );
    assert_eq!(
        direct_codex_auth(&header).unwrap(),
        DirectCodexAuth::EnvHeader {
            name: "anthropic-api-key".into(),
            env_key: "EXAMPLE_API_KEY".into()
        }
    );
}

#[test]
fn env_resolution_reports_missing_empty_and_non_unicode_without_leaking_values() {
    let missing = resolve_credential(&env_source(), &FakeEnvironment::default()).unwrap_err();
    assert_eq!(missing.code, "PROVIDER_CREDENTIAL_MISSING");

    let empty = FakeEnvironment(BTreeMap::from([(
        "EXAMPLE_API_KEY".into(),
        OsString::from("  "),
    )]));
    assert_eq!(
        resolve_credential(&env_source(), &empty).unwrap_err().code,
        "PROVIDER_CREDENTIAL_EMPTY"
    );

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let invalid = FakeEnvironment(BTreeMap::from([(
            "EXAMPLE_API_KEY".into(),
            OsString::from_vec(vec![0xff]),
        )]));
        assert_eq!(
            resolve_credential(&env_source(), &invalid)
                .unwrap_err()
                .code,
            "PROVIDER_CREDENTIAL_ENCODING"
        );
    }
}

#[test]
fn invalid_sources_headers_and_route_mappings_fail_closed() {
    for (auth, code) in [
        (
            UpstreamAuth::Bearer {
                source: CredentialSource::None,
            },
            "PROVIDER_AUTH_CONFLICT",
        ),
        (
            UpstreamAuth::Header {
                name: "Authorization".into(),
                source: env_source(),
            },
            "PROVIDER_AUTH_HEADER_FORBIDDEN",
        ),
        (
            UpstreamAuth::Header {
                name: "bad header".into(),
                source: env_source(),
            },
            "PROVIDER_AUTH_HEADER_INVALID",
        ),
        (
            UpstreamAuth::Bearer {
                source: CredentialSource::Env {
                    env_key: "bad-name".into(),
                },
            },
            "PROVIDER_ENV_INVALID",
        ),
    ] {
        assert_eq!(validate_upstream_auth(&auth).unwrap_err().code, code);
    }

    let keychain = UpstreamAuth::Bearer {
        source: CredentialSource::Keychain {
            service: "lam.remote-provider".into(),
            account: "credential-1".into(),
            version: 1,
        },
    };
    assert_eq!(
        direct_codex_auth(&keychain).unwrap(),
        DirectCodexAuth::Keychain {
            service: "lam.remote-provider".into(),
            account: "credential-1".into(),
            version: 1,
        }
    );
    assert_eq!(
        direct_codex_auth(&UpstreamAuth::Bearer {
            source: CredentialSource::AuthCommand {
                approval_id: "approved-command".into(),
            },
        })
        .unwrap(),
        DirectCodexAuth::ApprovedCommand {
            approval_id: "approved-command".into(),
        }
    );
    assert_eq!(
        direct_codex_auth(&UpstreamAuth::None).unwrap(),
        DirectCodexAuth::None
    );
    assert_eq!(
        direct_codex_auth(&UpstreamAuth::Bearer {
            source: CredentialSource::CodexProfile {
                profile_id: "work-api".into(),
            },
        })
        .unwrap(),
        DirectCodexAuth::NativeApiKey
    );
}

#[test]
fn sensitive_static_options_are_rejected_and_feature_env_headers_are_allowed() {
    let mut options = CodexProviderOptions::default();
    options
        .query_params
        .insert("api_key".into(), "not-even-a-real-secret".into());
    assert_eq!(
        validate_codex_security_options(&options).unwrap_err().code,
        "PROVIDER_QUERY_PARAM_SENSITIVE"
    );

    let mut options = CodexProviderOptions::default();
    options
        .env_http_headers
        .insert("Authorization".into(), "EXAMPLE_API_KEY".into());
    assert_eq!(
        validate_codex_security_options(&options).unwrap_err().code,
        "PROVIDER_ENV_HEADER_FORBIDDEN"
    );

    let mut options = CodexProviderOptions::default();
    options
        .env_http_headers
        .insert("X-Provider-Feature".into(), "EXAMPLE_FEATURE".into());
    validate_codex_security_options(&options).unwrap();
}
