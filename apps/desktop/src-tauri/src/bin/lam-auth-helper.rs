use localagentmanager_core::gateway::binding::{GatewayBindingCollection, GatewayBindingService};
use localagentmanager_core::gateway::sidecar::{GatewayRuntimeState, GatewayStateRepository};
use localagentmanager_core::provider_auth_command::{
    run_approved_auth_command, AuthCommandApprovalCollection, AuthCommandApprovalRepository,
};
use localagentmanager_core::provider_keychain::{
    KeychainCredentialReference, KeychainCredentialService, SystemKeychainBackend,
};
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use localagentmanager_core::{AppError, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn main() {
    if let Err(error) = run() {
        eprintln!("{}", error.code);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let token = match args.first().map(String::as_str) {
        Some("gateway-token") => gateway_token(&args)?,
        Some("keychain-token") => keychain_token(&args)?,
        Some("approved-command-token") => approved_command_token(&args)?,
        _ => return Err(arguments_invalid()),
    };
    if token.trim().is_empty() || token.contains(['\r', '\n', '\0']) {
        return Err(AppError::new(
            "AUTH_HELPER_TOKEN_INVALID",
            "auth helper resolved an invalid token",
        ));
    }
    println!("{token}");
    Ok(())
}

fn gateway_token(args: &[String]) -> Result<String> {
    if args.len() != 7
        || args[1] != "--state-root"
        || args[3] != "--profile"
        || args[5] != "--binding"
    {
        return Err(arguments_invalid());
    }
    let root = private_absolute_root(&args[2])?;
    #[cfg(debug_assertions)]
    if let Some(token) = std::env::var_os("LAM_TEST_GATEWAY_TOKEN") {
        return Ok(token.to_string_lossy().into_owned());
    }
    let service = GatewayBindingService::new(
        VersionedFileStore::<GatewayBindingCollection>::new(
            root.join("gateway-bindings.json"),
            InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(5)),
            1,
            StoreOptions {
                max_bytes: 16 * 1024 * 1024,
            },
        ),
        KeychainCredentialService::new(Arc::new(SystemKeychainBackend)),
    );
    service.token_for_helper(&args[4], &args[6])
}

fn keychain_token(args: &[String]) -> Result<String> {
    if args.len() != 7 || args[1] != "--service" || args[3] != "--account" || args[5] != "--version"
    {
        return Err(arguments_invalid());
    }
    let version = args[6].parse::<u64>().map_err(|_| arguments_invalid())?;
    let reference = KeychainCredentialReference::from_parts(&args[2], &args[4], version)?;
    KeychainCredentialService::new(Arc::new(SystemKeychainBackend))
        .with_secret(&reference, str::to_owned)
}

fn approved_command_token(args: &[String]) -> Result<String> {
    if args.len() != 5 || args[1] != "--state-root" || args[3] != "--approval-id" {
        return Err(arguments_invalid());
    }
    let root = private_absolute_root(&args[2])?;
    let repository = AuthCommandApprovalRepository::new(VersionedFileStore::<
        AuthCommandApprovalCollection,
    >::new(
        root.join("auth-command-approvals.json"),
        InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(5)),
        1,
        StoreOptions {
            max_bytes: 4 * 1024 * 1024,
        },
    ));
    let identity_key = load_install_identity(&root)?;
    let approved = repository.resolve(&args[4], &identity_key)?;
    let secret = run_approved_auth_command(&approved)?;
    Ok(secret.with_exposed(str::to_owned))
}

fn load_install_identity(root: &Path) -> Result<[u8; 32]> {
    #[cfg(debug_assertions)]
    if let Some(value) = std::env::var_os("LAM_TEST_INSTALL_IDENTITY_KEY") {
        let bytes = hex::decode(value.to_string_lossy().as_ref()).map_err(|_| {
            AppError::new(
                "GATEWAY_IDENTITY_KEY_INVALID",
                "test install identity key is invalid",
            )
        })?;
        return bytes.try_into().map_err(|_| {
            AppError::new(
                "GATEWAY_IDENTITY_KEY_INVALID",
                "test install identity key has an invalid length",
            )
        });
    }
    let state = GatewayStateRepository::new(VersionedFileStore::<GatewayRuntimeState>::new(
        root.join("gateway-state.json"),
        InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(5)),
        1,
        StoreOptions {
            max_bytes: 1024 * 1024,
        },
    ))
    .load()?;
    if !state.exists {
        return Err(AppError::new(
            "GATEWAY_STATE_MISSING",
            "Gateway install identity state is unavailable",
        ));
    }
    load_system_install_identity(&state.value.install_id)
}

#[cfg(target_os = "macos")]
fn load_system_install_identity(install_id: &str) -> Result<[u8; 32]> {
    let bytes = security_framework::passwords::get_generic_password(
        "dev.localagentmanager.desktop.provider-hub",
        install_id,
    )
    .map_err(|_| {
        AppError::new(
            "KEYCHAIN_UNAVAILABLE",
            "install identity Keychain operation failed [REDACTED]",
        )
    })?;
    bytes.try_into().map_err(|_| {
        AppError::new(
            "GATEWAY_IDENTITY_KEY_INVALID",
            "install identity key has an invalid length",
        )
    })
}

#[cfg(not(target_os = "macos"))]
fn load_system_install_identity(_install_id: &str) -> Result<[u8; 32]> {
    Err(AppError::new(
        "UNSUPPORTED_REMOTE_PROVIDER_PLATFORM",
        "install identity requires exact-tested macOS",
    ))
}

fn private_absolute_root(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err(arguments_invalid());
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| arguments_invalid())?;
    let metadata = std::fs::symlink_metadata(path).map_err(|_| arguments_invalid())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(arguments_invalid());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(arguments_invalid());
        }
    }
    Ok(canonical)
}

fn arguments_invalid() -> AppError {
    AppError::new(
        "AUTH_HELPER_ARGUMENTS_INVALID",
        "auth helper arguments are invalid",
    )
}
