use super::error::{AppError, Result};
use super::provider_v2::CodexProviderOptions;
use http::HeaderName;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fmt;

const FORBIDDEN_HEADER_NAMES: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "host",
    "content-length",
    "connection",
    "transfer-encoding",
];
const SENSITIVE_QUERY_TOKENS: &[&str] = &["api_key", "apikey", "token", "secret", "auth"];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CredentialSource {
    Env {
        env_key: String,
    },
    Keychain {
        service: String,
        account: String,
        version: u64,
    },
    AuthCommand {
        approval_id: String,
    },
    CodexProfile {
        profile_id: String,
    },
    None,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpstreamAuth {
    Bearer {
        source: CredentialSource,
    },
    Header {
        name: String,
        source: CredentialSource,
    },
    None,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DirectCodexAuth {
    EnvKey {
        env_key: String,
    },
    EnvHeader {
        name: String,
        env_key: String,
    },
    AuthCommand {
        approval_id: String,
        command: String,
        args: Vec<String>,
    },
    Keychain {
        service: String,
        account: String,
        version: u64,
    },
    ApprovedCommand {
        approval_id: String,
    },
    NativeApiKey,
    Gateway,
    None,
}

pub trait EnvironmentReader {
    fn get(&self, name: &str) -> Option<OsString>;
}

pub struct ProcessEnvironment;

impl EnvironmentReader for ProcessEnvironment {
    fn get(&self, name: &str) -> Option<OsString> {
        std::env::var_os(name)
    }
}

#[derive(Clone)]
pub struct SecretValue(String);

impl SecretValue {
    pub fn from_sensitive(value: String) -> Self {
        Self(value)
    }

    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }
    pub fn with_exposed<R>(&self, operation: impl FnOnce(&str) -> R) -> R {
        operation(&self.0)
    }

    pub fn display_redacted(&self) -> RedactedSecret<'_> {
        RedactedSecret(self)
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

pub struct RedactedSecret<'a>(&'a SecretValue);

impl fmt::Display for RedactedSecret<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self.0;
        formatter.write_str("[REDACTED]")
    }
}

pub fn resolve_credential(
    source: &CredentialSource,
    environment: &impl EnvironmentReader,
) -> Result<SecretValue> {
    let CredentialSource::Env { env_key } = source else {
        return Err(AppError::new(
            "PROVIDER_CREDENTIAL_SOURCE_UNSUPPORTED",
            "credential source is not available through the environment boundary",
        ));
    };
    validate_env_name(env_key)?;
    let value = environment
        .get(env_key)
        .ok_or_else(|| AppError::new("PROVIDER_CREDENTIAL_MISSING", env_key))?;
    let value = value.into_string().map_err(|_| {
        AppError::new(
            "PROVIDER_CREDENTIAL_ENCODING",
            "credential environment value is not valid Unicode",
        )
    })?;
    if value.trim().is_empty() {
        return Err(AppError::new(
            "PROVIDER_CREDENTIAL_EMPTY",
            "credential environment value is empty",
        ));
    }
    Ok(SecretValue(value))
}

pub fn validate_upstream_auth(auth: &UpstreamAuth) -> Result<()> {
    match auth {
        UpstreamAuth::None => Ok(()),
        UpstreamAuth::Bearer { source } => validate_source(source),
        UpstreamAuth::Header { name, source } => {
            validate_header_name(name, "PROVIDER_AUTH_HEADER")?;
            validate_source(source)
        }
    }
}

pub fn direct_codex_auth(auth: &UpstreamAuth) -> Result<DirectCodexAuth> {
    validate_upstream_auth(auth)?;
    match auth {
        UpstreamAuth::None => Ok(DirectCodexAuth::None),
        UpstreamAuth::Bearer {
            source: CredentialSource::Env { env_key },
        } => Ok(DirectCodexAuth::EnvKey {
            env_key: env_key.clone(),
        }),
        UpstreamAuth::Header {
            name,
            source: CredentialSource::Env { env_key },
        } => Ok(DirectCodexAuth::EnvHeader {
            name: name.to_ascii_lowercase(),
            env_key: env_key.clone(),
        }),
        UpstreamAuth::Bearer {
            source:
                CredentialSource::Keychain {
                    service,
                    account,
                    version,
                },
        } => Ok(DirectCodexAuth::Keychain {
            service: service.clone(),
            account: account.clone(),
            version: *version,
        }),
        UpstreamAuth::Bearer {
            source: CredentialSource::AuthCommand { approval_id },
        } => Ok(DirectCodexAuth::ApprovedCommand {
            approval_id: approval_id.clone(),
        }),
        UpstreamAuth::Bearer {
            source: CredentialSource::CodexProfile { .. },
        } => Ok(DirectCodexAuth::NativeApiKey),
        _ => Err(AppError::new(
            "PROVIDER_AUTH_ROUTE_UNSUPPORTED",
            "credential source requires a Codex auth helper for this route",
        )),
    }
}

pub fn validate_codex_security_options(options: &CodexProviderOptions) -> Result<()> {
    for key in options.query_params.keys() {
        let normalized = key.to_ascii_lowercase();
        if SENSITIVE_QUERY_TOKENS
            .iter()
            .any(|token| normalized.contains(token))
        {
            return Err(AppError::new(
                "PROVIDER_QUERY_PARAM_SENSITIVE",
                format!("query parameter {key} must not carry credentials"),
            ));
        }
    }
    for (name, env_key) in &options.env_http_headers {
        validate_header_name(name, "PROVIDER_ENV_HEADER")?;
        validate_env_name(env_key)?;
    }
    Ok(())
}

fn validate_source(source: &CredentialSource) -> Result<()> {
    match source {
        CredentialSource::None => Err(AppError::new(
            "PROVIDER_AUTH_CONFLICT",
            "authenticated transport requires a credential source",
        )),
        CredentialSource::Env { env_key } => validate_env_name(env_key),
        CredentialSource::Keychain {
            service,
            account,
            version,
        } if !service.trim().is_empty() && !account.trim().is_empty() && *version > 0 => Ok(()),
        CredentialSource::AuthCommand { approval_id } if !approval_id.trim().is_empty() => Ok(()),
        CredentialSource::CodexProfile { profile_id } if valid_profile_id(profile_id) => Ok(()),
        _ => Err(AppError::new(
            "PROVIDER_CREDENTIAL_INVALID",
            "credential reference is incomplete",
        )),
    }
}

fn valid_profile_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|item| item.is_ascii_alphanumeric() || matches!(item, '-' | '_' | '.'))
}

fn validate_header_name(name: &str, code_prefix: &str) -> Result<()> {
    let parsed = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
        AppError::new(
            &format!("{code_prefix}_INVALID"),
            "authentication header name is invalid",
        )
    })?;
    if FORBIDDEN_HEADER_NAMES.contains(&parsed.as_str()) {
        return Err(AppError::new(
            &format!("{code_prefix}_FORBIDDEN"),
            format!("header {} is controlled by the transport", parsed.as_str()),
        ));
    }
    Ok(())
}

fn validate_env_name(value: &str) -> Result<()> {
    let mut chars = value.chars();
    let valid = chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|item| item == '_' || item.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(AppError::new(
            "PROVIDER_ENV_INVALID",
            "invalid environment variable name",
        ))
    }
}
