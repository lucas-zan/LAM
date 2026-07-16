use super::binding::{GatewayBindingCollection, GatewayBindingService};
use super::identity::load_or_create_system_install_identity;
use super::launcher::{InstallManifest, InstallManifestVerifier, MacCodeSignIdentityVerifier};
use super::sidecar::{
    GatewayRuntimeState, GatewayStateRepository, RestartDecision, SupervisorPolicy,
};
use crate::services::error::{AppError, Result};
use crate::services::provider_keychain::{KeychainCredentialService, SystemKeychainBackend};
use crate::services::provider_runtime::ProviderHubPaths;
use crate::services::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

pub async fn monitor_packaged_gateway(home_root: PathBuf) -> Result<()> {
    let root = ProviderHubPaths::for_home(&home_root).ensure_canonical_root()?;
    let lock = InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(5));
    let state = GatewayStateRepository::new(VersionedFileStore::<GatewayRuntimeState>::new(
        root.join("gateway-state.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    ));
    let bindings = GatewayBindingService::new(
        VersionedFileStore::<GatewayBindingCollection>::new(
            root.join("gateway-bindings.json"),
            lock,
            1,
            StoreOptions::default(),
        ),
        KeychainCredentialService::new(Arc::new(SystemKeychainBackend)),
    );
    let policy = SupervisorPolicy::new(
        3,
        Duration::from_millis(100),
        Duration::from_secs(2),
        Duration::from_secs(30),
    )?;
    let mut failures = 0_u32;
    loop {
        let active = bindings
            .load()?
            .value
            .bindings
            .into_iter()
            .filter(|binding| binding.revoked_at.is_none())
            .collect::<Vec<_>>();
        if active.is_empty() {
            failures = 0;
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }
        let snapshot = state.load()?;
        if snapshot.value.process_id.is_some_and(process_exists) {
            failures = 0;
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }
        failures += 1;
        match start_packaged_sidecar(&root, &snapshot.value, &active) {
            Ok(()) => tokio::time::sleep(Duration::from_secs(1)).await,
            Err(error) => match policy.restart_decision(failures, rand::random()) {
                RestartDecision::RestartAfter(delay) => tokio::time::sleep(delay).await,
                RestartDecision::Failed => {
                    return Err(AppError::new("GATEWAY_HEALTH_FAILED", error.code))
                }
            },
        }
    }
}

fn start_packaged_sidecar(
    root: &Path,
    state: &GatewayRuntimeState,
    bindings: &[super::binding::GatewayBinding],
) -> Result<()> {
    let (install_root, manifest_path) = install_paths()?;
    let manifest: InstallManifest =
        serde_json::from_slice(&fs::read(manifest_path).map_err(|_| {
            AppError::new(
                "GATEWAY_MANIFEST_MISSING",
                "installation manifest is unavailable",
            )
        })?)
        .map_err(|_| {
            AppError::new(
                "GATEWAY_MANIFEST_INVALID",
                "installation manifest is invalid",
            )
        })?;
    let installation =
        InstallManifestVerifier::new(install_root, Arc::new(MacCodeSignIdentityVerifier))
            .verify(manifest)?;
    let identity = load_or_create_system_install_identity(&state.install_id)?;
    let temp_root = std::env::var_os("DARWIN_USER_TEMP_DIR")
        .or_else(|| std::env::var_os("TMPDIR"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            AppError::new(
                "GATEWAY_CONTROL_PATH_UNSAFE",
                "temporary directory is unavailable",
            )
        })?;
    let control_path = super::sidecar::gateway_control_socket_path(&temp_root, &state.install_id);
    let control_parent = control_path.parent().ok_or_else(|| {
        AppError::new(
            "GATEWAY_CONTROL_PATH_UNSAFE",
            "Gateway control path has no parent",
        )
    })?;
    fs::create_dir_all(&control_parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&control_parent, fs::Permissions::from_mode(0o700))?;
    }
    let mut command = Command::new(&installation.component("gateway")?.path);
    command
        .env_clear()
        .env("LAM_PROVIDER_HUB_ROOT", root)
        .env("LAM_GATEWAY_CONTROL_SOCKET", control_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    for binding in bindings {
        let source = match &binding.provider.upstream_auth {
            crate::services::provider_credentials::UpstreamAuth::Bearer { source }
            | crate::services::provider_credentials::UpstreamAuth::Header { source, .. } => {
                Some(source)
            }
            crate::services::provider_credentials::UpstreamAuth::None => None,
        };
        if let Some(crate::services::provider_credentials::CredentialSource::Env { env_key }) =
            source
        {
            let value = std::env::var_os(env_key).ok_or_else(|| {
                AppError::new(
                    "PROVIDER_CREDENTIAL_MISSING",
                    "Provider environment credential is unavailable",
                )
            })?;
            command.env(env_key, value);
        }
    }
    let mut child = command.spawn().map_err(|_| {
        AppError::new(
            "GATEWAY_START_FAILED",
            "Gateway sidecar could not be launched",
        )
    })?;
    child
        .stdin
        .take()
        .ok_or_else(|| AppError::new("GATEWAY_BOOTSTRAP_FAILED", "bootstrap pipe is unavailable"))?
        .write_all(&identity)
        .map_err(|_| AppError::new("GATEWAY_BOOTSTRAP_FAILED", "bootstrap delivery failed"))?;
    Ok(())
}

fn install_paths() -> Result<(PathBuf, PathBuf)> {
    if let (Some(root), Some(manifest)) = (
        std::env::var_os("LAM_INSTALL_ROOT"),
        std::env::var_os("LAM_INSTALL_MANIFEST"),
    ) {
        return Ok((PathBuf::from(root), PathBuf::from(manifest)));
    }
    let executable = std::env::current_exe().map_err(|_| {
        AppError::new(
            "GATEWAY_INSTALL_ROOT_INVALID",
            "application executable is unavailable",
        )
    })?;
    let contents = executable.parent().and_then(Path::parent).ok_or_else(|| {
        AppError::new(
            "GATEWAY_INSTALL_ROOT_INVALID",
            "app bundle layout is invalid",
        )
    })?;
    Ok((
        contents.into(),
        contents.join("Resources/provider-gateway-install-manifest.json"),
    ))
}

fn process_exists(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
