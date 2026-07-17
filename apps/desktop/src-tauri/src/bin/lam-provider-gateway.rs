use localagentmanager_core::gateway::binding::{
    binding_requires_gateway, GatewayBindingCollection, GatewayBindingService,
};
use localagentmanager_core::gateway::catalog::CodexModelDefaultsCatalog;
use localagentmanager_core::gateway::routes::GatewayRouteComposer;
use localagentmanager_core::gateway::server::{
    FileMetadataObserver, GatewayLoopbackServer, GatewayServerConfig, HealthProofKey,
};
use localagentmanager_core::gateway::sidecar::{
    AuthenticatedControl, ControlSocketServer, GatewayRuntimeState, GatewayStateRepository,
    SupervisorPolicy,
};
use localagentmanager_core::gateway::upstream::{
    KeychainAndEnvironmentCredentialResolver, NetworkTargetPolicy, ProductionNetworkTargetPolicy,
    SecureUpstreamClient, UpstreamClientConfig,
};
use localagentmanager_core::provider_keychain::{KeychainCredentialService, SystemKeychainBackend};
use localagentmanager_core::provider_runtime::{
    gateway_first_response_timeout_from_env, CODEX_MODEL_CATALOG_ENV,
    GATEWAY_FIRST_RESPONSE_TIMEOUT_ENV,
};
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
#[cfg(debug_assertions)]
use std::future::Future;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
#[cfg(debug_assertions)]
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{}", error.code);
        std::process::exit(1);
    }
}

async fn run() -> localagentmanager_core::Result<()> {
    let root = required_path("LAM_PROVIDER_HUB_ROOT")?;
    let control_path = required_path("LAM_GATEWAY_CONTROL_SOCKET")?;
    let mut identity_key = [0_u8; 32];
    std::io::stdin()
        .read_exact(&mut identity_key)
        .map_err(|_| {
            localagentmanager_core::AppError::new(
                "GATEWAY_BOOTSTRAP_SECRET_MISSING",
                "sidecar bootstrap identity is unavailable",
            )
        })?;
    let lock = InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(5));
    let state_repo = GatewayStateRepository::new(VersionedFileStore::<GatewayRuntimeState>::new(
        root.join("gateway-state.json"),
        lock.clone(),
        1,
        StoreOptions {
            max_bytes: 1024 * 1024,
        },
    ));
    let state = state_repo.load()?;
    if !state.exists {
        return Err(localagentmanager_core::AppError::new(
            "GATEWAY_STATE_NOT_INITIALIZED",
            "Gateway state is not initialized",
        ));
    }
    let pid = std::process::id();
    let stale = state
        .value
        .process_id
        .is_some_and(|existing| !process_exists(existing));
    let claimed =
        state_repo.claim_process(state.revision, pid, stale, &chrono::Utc::now().to_rfc3339())?;
    let binding_service = Arc::new(GatewayBindingService::new(
        VersionedFileStore::<GatewayBindingCollection>::new(
            root.join("gateway-bindings.json"),
            lock,
            1,
            StoreOptions {
                max_bytes: 16 * 1024 * 1024,
            },
        ),
        KeychainCredentialService::new(Arc::new(SystemKeychainBackend)),
    ));
    let upstream = Arc::new(SecureUpstreamClient::new(
        UpstreamClientConfig {
            connect_timeout: Duration::from_secs(10),
            first_byte_timeout: gateway_first_response_timeout_from_env(
                std::env::var_os(GATEWAY_FIRST_RESPONSE_TIMEOUT_ENV).as_deref(),
            )?,
            stream_idle_timeout: Duration::from_secs(60),
            total_timeout: Duration::from_secs(15 * 60),
            max_response_bytes: 32 * 1024 * 1024,
            max_inflight: 16,
        },
        network_target_policy()?,
        Arc::new(KeychainAndEnvironmentCredentialResolver::new(
            KeychainCredentialService::new(Arc::new(SystemKeychainBackend)),
        )),
    )?);
    let builtin_model_defaults = CodexModelDefaultsCatalog::builtin()?;
    let model_defaults = std::env::var_os(CODEX_MODEL_CATALOG_ENV)
        .map(PathBuf::from)
        .and_then(|path| CodexModelDefaultsCatalog::from_path(&path, 4 * 1024 * 1024).ok())
        .map(|local| builtin_model_defaults.clone().overlay(local))
        .unwrap_or(builtin_model_defaults);
    let handler = Arc::new(GatewayRouteComposer::new_with_model_defaults(
        upstream,
        model_defaults,
    ));
    let uid = unsafe { libc::geteuid() };
    let control_auth = Arc::new(AuthenticatedControl::new(&identity_key, uid)?);
    let control = match ControlSocketServer::start(control_path, control_auth).await {
        Ok(control) => control,
        Err(error) => {
            let _ =
                state_repo.release_process(claimed.revision, pid, &chrono::Utc::now().to_rfc3339());
            return Err(error);
        }
    };
    let server = match GatewayLoopbackServer::start(
        GatewayServerConfig {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), claimed.value.stable_port),
            protocol_version: claimed.value.control_protocol_version,
            component_version: claimed.value.component_version.clone(),
            state_schema: claimed.value.state_schema,
            install_id: claimed.value.install_id.clone(),
            instance_id: claimed.value.instance_id.clone(),
            ready: true,
            max_body_bytes: 4 * 1024 * 1024,
            max_inflight: 32,
            max_inflight_per_binding: 4,
            max_queue: 64,
            max_queue_per_binding: 8,
            request_timeout: Duration::from_secs(15 * 60),
        },
        binding_service.clone(),
        handler,
        Arc::new(HealthProofKey::new(&identity_key)?),
        Arc::new(FileMetadataObserver::new(
            root.join("gateway-requests.jsonl"),
        )),
    )
    .await
    {
        Ok(server) => server,
        Err(error) => {
            control.shutdown().await?;
            let _ =
                state_repo.release_process(claimed.revision, pid, &chrono::Utc::now().to_rfc3339());
            return Err(error);
        }
    };
    let activity = server.activity();
    let idle_policy = SupervisorPolicy::new(
        3,
        Duration::from_millis(100),
        Duration::from_secs(2),
        Duration::from_secs(30),
    )?;
    let mut idle_check = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = control.wait_for_shutdown_request() => break,
            _ = tokio::signal::ctrl_c() => break,
            _ = idle_check.tick() => {
                let active_bindings = binding_service
                    .load()?
                    .value
                    .bindings
                    .iter()
                    .filter(|binding| binding_requires_gateway(binding))
                    .count();
                if idle_policy.should_idle_shutdown(
                    active_bindings,
                    activity.inflight_requests(),
                    activity.idle_for(),
                ) {
                    break;
                }
            }
        }
    }
    server.shutdown().await?;
    control.shutdown().await?;
    let current = state_repo.load()?;
    state_repo.release_process(current.revision, pid, &chrono::Utc::now().to_rfc3339())?;
    Ok(())
}

fn network_target_policy() -> localagentmanager_core::Result<Arc<dyn NetworkTargetPolicy>> {
    #[cfg(debug_assertions)]
    if let Some(value) = std::env::var_os("LAM_GATEWAY_TEST_UPSTREAM_ADDR") {
        let address = value.to_string_lossy().parse::<SocketAddr>().map_err(|_| {
            localagentmanager_core::AppError::new(
                "GATEWAY_TEST_UPSTREAM_INVALID",
                "debug upstream address is invalid",
            )
        })?;
        return Ok(Arc::new(DebugPinnedNetworkTarget(address)));
    }
    Ok(Arc::new(ProductionNetworkTargetPolicy))
}

#[cfg(debug_assertions)]
struct DebugPinnedNetworkTarget(SocketAddr);

#[cfg(debug_assertions)]
impl NetworkTargetPolicy for DebugPinnedNetworkTarget {
    fn validate_and_resolve<'a>(
        &'a self,
        _url: &'a url::Url,
    ) -> Pin<Box<dyn Future<Output = localagentmanager_core::Result<Vec<SocketAddr>>> + Send + 'a>>
    {
        Box::pin(async move { Ok(vec![self.0]) })
    }
}

fn required_path(name: &str) -> localagentmanager_core::Result<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).ok_or_else(|| {
        localagentmanager_core::AppError::new(
            "GATEWAY_ENVIRONMENT_INVALID",
            "required sidecar path is missing",
        )
    })
}

fn process_exists(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
