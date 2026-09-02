use super::error::{AppError, Result};
use super::provider_credentials::DirectCodexAuth;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderHubPaths {
    canonical_root: PathBuf,
    legacy_root: PathBuf,
}

impl ProviderHubPaths {
    pub fn for_home(home: &Path) -> Self {
        Self {
            canonical_root: super::lam_paths::LamPaths::for_home(home).provider_hub_root(),
            legacy_root: home
                .join("Library")
                .join("Application Support")
                .join("dev.localagentmanager.desktop")
                .join("provider-hub"),
        }
    }

    pub fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }

    pub fn legacy_root(&self) -> &Path {
        &self.legacy_root
    }

    pub fn ensure_canonical_root(&self) -> Result<PathBuf> {
        let canonical_exists = self.canonical_root.exists();
        let legacy_exists = self.legacy_root.exists();
        if canonical_exists && legacy_exists && directory_has_entries(&self.legacy_root)? {
            return Err(AppError::new(
                "PROVIDER_HUB_ROOT_CONFLICT",
                "both canonical and legacy Provider Hub roots contain state",
            ));
        }
        if canonical_exists {
            validate_private_directory(&self.canonical_root)?;
            if legacy_exists {
                fs::remove_dir(&self.legacy_root).map_err(|_| {
                    AppError::new(
                        "PROVIDER_HUB_ROOT_CONFLICT",
                        "empty legacy Provider Hub root could not be removed",
                    )
                })?;
            }
            return canonical_path(&self.canonical_root);
        }
        if legacy_exists {
            validate_private_directory(&self.legacy_root)?;
            let parent = self.canonical_root.parent().ok_or_else(root_invalid)?;
            create_private_directory(parent)?;
            fs::rename(&self.legacy_root, &self.canonical_root).map_err(|_| {
                AppError::new(
                    "PROVIDER_HUB_MIGRATION_FAILED",
                    "legacy Provider Hub root could not be migrated atomically",
                )
            })?;
        } else {
            create_private_directory(&self.canonical_root)?;
        }
        validate_private_directory(&self.canonical_root)?;
        canonical_path(&self.canonical_root)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthHelperRuntime {
    pub executable: PathBuf,
    pub state_root: PathBuf,
    pub profile_id: String,
    pub gateway_binding_id: Option<String>,
}

pub const GATEWAY_FIRST_RESPONSE_TIMEOUT_ENV: &str = "LAM_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECS";
pub const GATEWAY_REQUEST_TIMEOUT_ENV: &str = "LAM_GATEWAY_REQUEST_TIMEOUT_SECS";
pub const CODEX_MODEL_CATALOG_ENV: &str = "LAM_CODEX_MODEL_CATALOG_PATH";

pub fn gateway_first_response_timeout_from_env(value: Option<&OsStr>) -> Result<Duration> {
    let Some(value) = value else {
        return Ok(Duration::from_secs(
            super::types::DEFAULT_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS,
        ));
    };
    let seconds = value
        .to_str()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| {
            (super::types::MIN_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS
                ..=super::types::MAX_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS)
                .contains(seconds)
        })
        .ok_or_else(|| {
            AppError::new(
                "GATEWAY_TIMEOUT_CONFIG_INVALID",
                "Gateway first response timeout configuration is invalid",
            )
        })?;
    Ok(Duration::from_secs(seconds))
}

pub fn gateway_request_timeout_from_env(value: Option<&OsStr>) -> Result<Duration> {
    let Some(value) = value else {
        return Ok(Duration::from_secs(
            super::types::DEFAULT_GATEWAY_REQUEST_TIMEOUT_SECONDS,
        ));
    };
    let seconds = value
        .to_str()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| {
            (super::types::MIN_GATEWAY_REQUEST_TIMEOUT_SECONDS
                ..=super::types::MAX_GATEWAY_REQUEST_TIMEOUT_SECONDS)
                .contains(seconds)
        })
        .ok_or_else(|| {
            AppError::new(
                "GATEWAY_TIMEOUT_CONFIG_INVALID",
                "Gateway request timeout configuration is invalid",
            )
        })?;
    Ok(Duration::from_secs(seconds))
}

pub fn resolve_launcher_executable() -> Result<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("LAM_LAUNCHER_EXECUTABLE") {
        return validate_runtime_executable(Path::new(&path), "CODEX_LAUNCHER_INVALID");
    }
    let current = std::env::current_exe().map_err(|_| launcher_invalid())?;
    #[cfg(not(debug_assertions))]
    {
        return verified_installation(&current)?
            .component("launcher")
            .map(|component| component.path.clone());
    }
    #[cfg(debug_assertions)]
    resolve_debug_sibling(&current, "lam", "CODEX_LAUNCHER_INVALID")
}

#[cfg(not(debug_assertions))]
fn verified_installation(current: &Path) -> Result<super::gateway::launcher::VerifiedInstallation> {
    use super::gateway::launcher::{
        InstallManifest, InstallManifestVerifier, MacCodeSignIdentityVerifier,
    };
    use std::sync::Arc;

    let contents = current
        .parent()
        .and_then(Path::parent)
        .ok_or_else(launcher_invalid)?;
    let manifest = fs::read(contents.join("Resources/provider-gateway-install-manifest.json"))
        .map_err(|_| launcher_invalid())?;
    let manifest: InstallManifest =
        serde_json::from_slice(&manifest).map_err(|_| launcher_invalid())?;
    InstallManifestVerifier::new(
        contents.to_path_buf(),
        Arc::new(MacCodeSignIdentityVerifier),
    )
    .verify(manifest)
}

pub fn resolve_auth_helper_executable() -> Result<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("LAM_AUTH_HELPER") {
        return validate_helper_executable(Path::new(&path));
    }
    let current = std::env::current_exe().map_err(|_| helper_invalid())?;
    #[cfg(not(debug_assertions))]
    {
        use crate::services::gateway::launcher::{
            InstallManifest, InstallManifestVerifier, MacCodeSignIdentityVerifier,
        };
        use std::sync::Arc;
        let contents = current
            .parent()
            .and_then(Path::parent)
            .ok_or_else(helper_invalid)?;
        let manifest: InstallManifest = serde_json::from_slice(
            &fs::read(contents.join("Resources/provider-gateway-install-manifest.json"))
                .map_err(|_| helper_invalid())?,
        )
        .map_err(|_| helper_invalid())?;
        let installation = InstallManifestVerifier::new(
            contents.to_path_buf(),
            Arc::new(MacCodeSignIdentityVerifier),
        )
        .verify(manifest)?;
        return Ok(installation.component("auth-helper")?.path.clone());
    }
    #[cfg(debug_assertions)]
    {
        let parent = current.parent().ok_or_else(helper_invalid)?;
        for candidate in [
            parent.join("lam-auth-helper"),
            parent
                .parent()
                .map(|value| value.join("lam-auth-helper"))
                .unwrap_or_default(),
        ] {
            if candidate.exists() {
                return validate_helper_executable(&candidate);
            }
        }
        Err(helper_invalid())
    }
}

pub fn materialize_codex_auth(
    auth: &DirectCodexAuth,
    runtime: &AuthHelperRuntime,
) -> Result<DirectCodexAuth> {
    if matches!(
        auth,
        DirectCodexAuth::EnvKey { .. }
            | DirectCodexAuth::EnvHeader { .. }
            | DirectCodexAuth::AuthCommand { .. }
            | DirectCodexAuth::NativeApiKey
            | DirectCodexAuth::None
    ) {
        return Ok(auth.clone());
    }
    let helper = validate_helper_executable(&runtime.executable)?;
    let state_root = validate_state_root(&runtime.state_root)?;
    validate_identifier(&runtime.profile_id, "CODEX_PROFILE_ID_INVALID")?;
    let command = helper.to_string_lossy().into_owned();
    match auth {
        DirectCodexAuth::Gateway => {
            let binding = runtime.gateway_binding_id.as_deref().ok_or_else(|| {
                AppError::new(
                    "GATEWAY_BINDING_NOT_FOUND",
                    "Gateway auth materialization requires a binding id",
                )
            })?;
            validate_identifier(binding, "GATEWAY_BINDING_ID_INVALID")?;
            Ok(DirectCodexAuth::AuthCommand {
                approval_id: format!("gateway-binding:{binding}"),
                command,
                args: vec![
                    "gateway-token".into(),
                    "--state-root".into(),
                    state_root.to_string_lossy().into_owned(),
                    "--profile".into(),
                    runtime.profile_id.clone(),
                    "--binding".into(),
                    binding.into(),
                ],
            })
        }
        DirectCodexAuth::Keychain {
            service,
            account,
            version,
        } => Ok(DirectCodexAuth::AuthCommand {
            approval_id: "bundled-keychain-helper-v1".into(),
            command,
            args: vec![
                "keychain-token".into(),
                "--service".into(),
                service.clone(),
                "--account".into(),
                account.clone(),
                "--version".into(),
                version.to_string(),
            ],
        }),
        DirectCodexAuth::ApprovedCommand { approval_id } => {
            validate_identifier(approval_id, "AUTH_COMMAND_APPROVAL_INVALID")?;
            Ok(DirectCodexAuth::AuthCommand {
                approval_id: approval_id.clone(),
                command,
                args: vec![
                    "approved-command-token".into(),
                    "--state-root".into(),
                    state_root.to_string_lossy().into_owned(),
                    "--approval-id".into(),
                    approval_id.clone(),
                ],
            })
        }
        _ => unreachable!("resolved auth variants returned above"),
    }
}

fn validate_helper_executable(path: &Path) -> Result<PathBuf> {
    validate_runtime_executable(path, "AUTH_HELPER_EXECUTABLE_INVALID")
}

fn validate_runtime_executable(path: &Path, code: &'static str) -> Result<PathBuf> {
    let invalid = || AppError::new(code, "runtime executable is unavailable or unsafe");
    if !path.is_absolute() {
        return Err(invalid());
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid());
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0
        || metadata.permissions().mode() & 0o022 != 0
        || (metadata.uid() != unsafe { libc::geteuid() } && metadata.uid() != 0)
    {
        return Err(invalid());
    }
    canonical_path(path).map_err(|_| invalid())
}

#[cfg(debug_assertions)]
fn resolve_debug_sibling(current: &Path, name: &str, code: &'static str) -> Result<PathBuf> {
    let parent = current
        .parent()
        .ok_or_else(|| AppError::new(code, "runtime executable parent is unavailable"))?;
    for candidate in [
        parent.join(name),
        parent
            .parent()
            .map(|value| value.join(name))
            .unwrap_or_default(),
    ] {
        if candidate.exists() {
            return validate_runtime_executable(&candidate, code);
        }
    }
    Err(AppError::new(code, "runtime executable is unavailable"))
}

fn launcher_invalid() -> AppError {
    AppError::new(
        "CODEX_LAUNCHER_INVALID",
        "LAM Codex launcher is unavailable",
    )
}

fn validate_state_root(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(root_invalid());
    }
    validate_private_directory(path)?;
    canonical_path(path)
}

fn create_private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn validate_private_directory(path: &Path) -> Result<()> {
    let mut metadata = fs::symlink_metadata(path).map_err(|_| root_invalid())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(root_invalid());
    }
    #[cfg(unix)]
    {
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(root_invalid());
        }
        if metadata.permissions().mode() & 0o077 != 0 {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                .map_err(|_| root_invalid())?;
            metadata = fs::symlink_metadata(path).map_err(|_| root_invalid())?;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(root_invalid());
            }
        }
    }
    Ok(())
}

fn directory_has_entries(path: &Path) -> Result<bool> {
    fs::read_dir(path)
        .map_err(|_| root_invalid())?
        .next()
        .transpose()
        .map(|entry| entry.is_some())
        .map_err(Into::into)
}

fn canonical_path(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(|_| root_invalid())
}

fn validate_identifier(value: &str, code: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'/'))
    {
        return Err(AppError::new(code, "identifier is invalid"));
    }
    Ok(())
}

fn helper_invalid() -> AppError {
    AppError::new(
        "AUTH_HELPER_EXECUTABLE_INVALID",
        "auth helper must be an absolute owner-controlled executable",
    )
}

fn root_invalid() -> AppError {
    AppError::new(
        "PROVIDER_HUB_ROOT_INVALID",
        "Provider Hub root is not a private owner-controlled directory",
    )
}
