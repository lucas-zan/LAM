use localagentmanager_core::gateway::binding::{GatewayBindingCollection, GatewayBindingService};
use localagentmanager_core::gateway::identity::load_or_create_system_install_identity;
use localagentmanager_core::gateway::launcher::{
    CodexLaunchRequest, CodexLauncher, DirectCodexLauncher, GatewayReadiness, InstallManifest,
    InstallManifestVerifier, MacCodeSignIdentityVerifier, VerifiedInstallation,
};
use localagentmanager_core::gateway::listener_handoff::configure_listener_handoff;
use localagentmanager_core::gateway::recovery::SystemGatewayProcessControl;
use localagentmanager_core::gateway::server::{HealthDocument, HealthProofKey};
use localagentmanager_core::gateway::sidecar::{
    gateway_control_socket_path, AuthenticatedControl, ControlCommand, ControlSocketClient,
    GatewayRuntimeState, GatewayStateRepository, RestartDecision, SupervisorPolicy,
};
use localagentmanager_core::gateway::supervisor::{
    inspect_gateway_claim, reconcile_gateway_claim, GatewayReconcileContext,
    GatewayReconcileOutcome, GatewayStartReservation, GatewaySupervisorMachine,
    GatewaySupervisorObservation, SystemGatewayIdentityProbe,
};
use localagentmanager_core::provider_binding::{
    ProfileBindingCollection, ProfileBindingRepository, ProfileProviderBinding, RouteKind,
};
use localagentmanager_core::provider_config_editor::validate_managed_projection;
use localagentmanager_core::provider_credentials::{CredentialSource, UpstreamAuth};
use localagentmanager_core::provider_keychain::{KeychainCredentialService, SystemKeychainBackend};
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use std::fs;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{}", error.code);
            std::process::exit(1);
        }
    }
}

fn run() -> localagentmanager_core::Result<i32> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() < 3 || arguments[0] != "codex" || arguments[1] != "--profile" {
        return Err(localagentmanager_core::AppError::new(
            "LAM_ARGUMENTS_INVALID",
            "usage: lam codex --profile <id> [-- <codex args>]",
        ));
    }
    let profile_id = arguments[2].clone();
    let codex_args = arguments
        .get(3..)
        .unwrap_or_default()
        .strip_prefix(&["--".to_owned()])
        .unwrap_or(arguments.get(3..).unwrap_or_default())
        .to_vec();
    let codex_args = localagentmanager_core::gateway::launch_planner::apply_permission_args(
        codex_args,
        localagentmanager_core::resolve_home_root()
            .map(|home| localagentmanager_core::codex_launch_permission_preset(&home))
            .unwrap_or_default(),
    );
    let root = provider_hub_root()?;
    ensure_private_directory(&root)?;
    localagentmanager_core::recover_provider_transactions_at_root_service_v2(
        &root,
        chrono::Utc::now().timestamp_millis().max(0) as u64,
    )?;
    if std::env::var_os("LAM_PROVIDER_HUB_ROOT").is_none() {
        let home = localagentmanager_core::resolve_home_root()?;
        localagentmanager_core::migrate_native_responses_bindings_service_v2(
            &home,
            chrono::Utc::now().timestamp_millis().max(0) as u64,
        )?;
    }
    let lock = InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(5));
    let bindings =
        ProfileBindingRepository::new(VersionedFileStore::<ProfileBindingCollection>::new(
            root.join("bindings.json"),
            lock.clone(),
            1,
            StoreOptions::default(),
        ));
    let binding = bindings
        .load()?
        .value
        .bindings
        .into_iter()
        .find(|binding| binding.profile_id == profile_id);
    let (route_kind, codex_home) = if let Some(binding) = binding.as_ref() {
        validate_projection(binding)?;
        (
            binding.route_kind,
            Path::new(&binding.config_projection.config_path)
                .parent()
                .ok_or_else(|| {
                    localagentmanager_core::AppError::new(
                        "CODEX_PROFILE_HOME_INVALID",
                        "profile config path has no parent",
                    )
                })?
                .to_path_buf(),
        )
    } else {
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            localagentmanager_core::AppError::new("HOME_MISSING", "HOME is unavailable")
        })?;
        let account = localagentmanager_core::list_accounts(&home)?
            .into_iter()
            .find(|account| account.id == profile_id)
            .ok_or_else(|| {
                localagentmanager_core::AppError::new(
                    "PROFILE_NOT_FOUND",
                    "Codex profile was not found",
                )
            })?;
        (RouteKind::Direct, account.codex_home)
    };
    let codex_executable = resolve_codex_executable()?;
    let launch_request = CodexLaunchRequest {
        profile_id,
        route_kind,
        codex_home,
        cwd: std::env::current_dir().map_err(|_| {
            localagentmanager_core::AppError::new(
                "CODEX_LAUNCH_CWD_INVALID",
                "current directory is unavailable",
            )
        })?,
        args: codex_args,
        codex_executable: Some(codex_executable),
    };
    if route_kind == RouteKind::Direct {
        return Ok(DirectCodexLauncher::run(launch_request)?.exit_code);
    }

    let (install_root, manifest_path) = install_paths()?;
    let manifest: InstallManifest =
        serde_json::from_slice(&fs::read(manifest_path).map_err(|_| {
            localagentmanager_core::AppError::new(
                "GATEWAY_MANIFEST_MISSING",
                "installation manifest is unavailable",
            )
        })?)
        .map_err(|_| {
            localagentmanager_core::AppError::new(
                "GATEWAY_MANIFEST_INVALID",
                "installation manifest is invalid",
            )
        })?;
    let installation = Arc::new(
        InstallManifestVerifier::new(install_root, Arc::new(MacCodeSignIdentityVerifier))
            .verify(manifest)?,
    );
    let timeout_seconds = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| localagentmanager_core::gateway_first_response_timeout_seconds(&home))
        .unwrap_or(localagentmanager_core::DEFAULT_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS);
    let readiness: Arc<dyn GatewayReadiness> = Arc::new(PackagedReadiness::new(
        root,
        lock,
        installation.clone(),
        timeout_seconds,
    )?);
    let launcher = CodexLauncher::new_shared(installation, readiness);
    Ok(launcher.run(launch_request)?.exit_code)
}

struct PackagedReadiness {
    root: PathBuf,
    state: GatewayStateRepository,
    bindings: GatewayBindingService<SystemKeychainBackend>,
    installation: Arc<VerifiedInstallation>,
    identity_key: [u8; 32],
    control_path: PathBuf,
    first_response_timeout_seconds: u64,
    owned_child: Mutex<Option<Child>>,
}

enum GatewayPreparation {
    Healthy,
    ReadyToStart(GatewayStartReservation),
}

impl PackagedReadiness {
    fn new(
        root: PathBuf,
        lock: InstallationLock,
        installation: Arc<VerifiedInstallation>,
        first_response_timeout_seconds: u64,
    ) -> localagentmanager_core::Result<Self> {
        let state = GatewayStateRepository::new(VersionedFileStore::<GatewayRuntimeState>::new(
            root.join("gateway-state.json"),
            lock.clone(),
            1,
            StoreOptions {
                max_bytes: 1024 * 1024,
            },
        ));
        let snapshot = state.load()?;
        let snapshot = if snapshot.exists {
            snapshot
        } else {
            state.initialize(
                snapshot.revision,
                choose_stable_port()?,
                "0.2.1",
                1,
                1,
                &chrono::Utc::now().to_rfc3339(),
            )?
        };
        let identity_key = load_or_create_system_install_identity(&snapshot.value.install_id)?;
        let temp_root = std::env::var_os("DARWIN_USER_TEMP_DIR")
            .or_else(|| std::env::var_os("TMPDIR"))
            .map(PathBuf::from)
            .ok_or_else(|| {
                localagentmanager_core::AppError::new(
                    "GATEWAY_CONTROL_PATH_UNSAFE",
                    "Darwin user temp directory is unavailable",
                )
            })?;
        let control_path = gateway_control_socket_path(&temp_root, &snapshot.value.install_id);
        let control_parent = control_path.parent().ok_or_else(|| {
            localagentmanager_core::AppError::new(
                "GATEWAY_CONTROL_PATH_UNSAFE",
                "Gateway control path has no parent",
            )
        })?;
        ensure_private_directory(&control_parent)?;
        Ok(Self {
            root: root.clone(),
            state,
            bindings: GatewayBindingService::new(
                VersionedFileStore::<GatewayBindingCollection>::new(
                    root.join("gateway-bindings.json"),
                    lock,
                    1,
                    StoreOptions::default(),
                ),
                KeychainCredentialService::new(Arc::new(SystemKeychainBackend)),
            ),
            installation,
            identity_key,
            control_path,
            first_response_timeout_seconds,
            owned_child: Mutex::new(None),
        })
    }

    async fn verify(&self, profile_id: &str) -> localagentmanager_core::Result<()> {
        let state = self.state.load()?.value;
        let binding = self.active_binding(profile_id)?;
        let token = self.test_or_keychain_gateway_token(profile_id, &binding.binding_id)?;
        let control = AuthenticatedControl::new(&self.identity_key, unsafe { libc::geteuid() })?;
        let nonce = rand::random::<u64>().max(1);
        let request = control.sign(nonce, ControlCommand::Readiness, serde_json::json!({}))?;
        let response = ControlSocketClient::send(&self.control_path, &control, &request).await?;
        if response.payload["ok"] != true {
            return Err(identity_mismatch());
        }
        let health_nonce = format!("{:032x}", rand::random::<u128>());
        let health: HealthDocument = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| identity_mismatch())?
            .get(format!("http://127.0.0.1:{}/healthz", state.stable_port))
            .header("authorization", format!("Bearer {token}"))
            .header("x-lam-health-nonce", &health_nonce)
            .send()
            .await
            .map_err(|_| identity_mismatch())?
            .error_for_status()
            .map_err(|_| identity_mismatch())?
            .json()
            .await
            .map_err(|_| identity_mismatch())?;
        let key = HealthProofKey::new(&self.identity_key)?;
        if !key.verify(&health_nonce, &health)
            || health.install_id != state.install_id
            || health.instance_id != state.instance_id
            || health.protocol_version != state.control_protocol_version
            || health.state_schema != state.state_schema
            || !health.ready
        {
            return Err(identity_mismatch());
        }
        Ok(())
    }

    async fn prepare_gateway(
        &self,
        profile_id: &str,
    ) -> localagentmanager_core::Result<GatewayPreparation> {
        let binding = self.active_binding(profile_id)?;
        let token = self.test_or_keychain_gateway_token(profile_id, &binding.binding_id)?;
        let probe =
            SystemGatewayIdentityProbe::new(token, &self.identity_key, Duration::from_secs(2))?;
        let executable = &self.installation.component("gateway")?.path;
        let control = SystemGatewayProcessControl;
        let mut machine = GatewaySupervisorMachine::new(3)?;

        for attempt in 1..=3 {
            let snapshot = self.state.load()?;
            let observation =
                inspect_gateway_claim(&control, &probe, &snapshot.value, executable, unsafe {
                    libc::geteuid()
                })
                .await?;
            if observation == GatewaySupervisorObservation::IdentityHealthy {
                return Ok(GatewayPreparation::Healthy);
            }
            let action = machine.observe(observation);
            let now = chrono::Utc::now().to_rfc3339();
            let outcome = reconcile_gateway_claim(
                action,
                observation,
                &snapshot,
                GatewayReconcileContext {
                    control: &control,
                    state: &self.state,
                    expected_executable: executable,
                    expected_uid: unsafe { libc::geteuid() },
                    control_path: &self.control_path,
                    now: &now,
                    termination_timeout: Duration::from_secs(2),
                },
            )?;
            if let GatewayReconcileOutcome::ReadyToStart(reservation) = outcome {
                return Ok(GatewayPreparation::ReadyToStart(reservation));
            }
            if attempt < 3 {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Err(localagentmanager_core::AppError::new(
            "GATEWAY_IDENTITY_UNAVAILABLE",
            "Existing Gateway identity could not be verified after bounded retries",
        ))
    }

    fn active_binding(
        &self,
        profile_id: &str,
    ) -> localagentmanager_core::Result<localagentmanager_core::gateway::binding::GatewayBinding>
    {
        self.bindings
            .load()?
            .value
            .bindings
            .into_iter()
            .find(|binding| binding.profile_id == profile_id && binding.revoked_at.is_none())
            .ok_or_else(|| {
                localagentmanager_core::AppError::new(
                    "GATEWAY_BINDING_NOT_FOUND",
                    "active Gateway binding was not found",
                )
            })
    }

    fn test_or_keychain_gateway_token(
        &self,
        profile_id: &str,
        binding_id: &str,
    ) -> localagentmanager_core::Result<String> {
        #[cfg(debug_assertions)]
        if let Some(token) = std::env::var_os("LAM_TEST_GATEWAY_TOKEN") {
            let token = token.to_string_lossy().into_owned();
            let snapshot = self
                .bindings
                .authenticate(&format!("Bearer {token}"), &chrono::Utc::now().to_rfc3339())?;
            if snapshot.profile_id != profile_id || snapshot.binding_id != binding_id {
                return Err(identity_mismatch());
            }
            return Ok(token);
        }
        self.bindings.token_for_helper(profile_id, binding_id)
    }

    fn start_sidecar(
        &self,
        profile_id: &str,
        reservation: &GatewayStartReservation,
    ) -> localagentmanager_core::Result<Child> {
        let binding = self.active_binding(profile_id)?;
        let mut command = Command::new(&self.installation.component("gateway")?.path);
        command
            .env_clear()
            .env("LAM_PROVIDER_HUB_ROOT", &self.root)
            .env("LAM_GATEWAY_CONTROL_SOCKET", &self.control_path)
            .env(
                localagentmanager_core::provider_runtime::GATEWAY_FIRST_RESPONSE_TIMEOUT_ENV,
                self.first_response_timeout_seconds.to_string(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        if let Some(home) = std::env::var_os("HOME") {
            let model_catalog = PathBuf::from(home).join(".codex").join("models_cache.json");
            if model_catalog.is_file() {
                command.env(
                    localagentmanager_core::provider_runtime::CODEX_MODEL_CATALOG_ENV,
                    model_catalog,
                );
            }
        }
        let source = match &binding.provider.upstream_auth {
            UpstreamAuth::Bearer { source } | UpstreamAuth::Header { source, .. } => Some(source),
            UpstreamAuth::None => None,
        };
        if let Some(CredentialSource::Env { env_key }) = source {
            let value = std::env::var_os(env_key).ok_or_else(|| {
                localagentmanager_core::AppError::new(
                    "PROVIDER_CREDENTIAL_MISSING",
                    "Provider credential environment is missing",
                )
            })?;
            command.env(env_key, value);
        }
        #[cfg(debug_assertions)]
        if let Some(value) = std::env::var_os("LAM_GATEWAY_TEST_UPSTREAM_ADDR") {
            command.env("LAM_GATEWAY_TEST_UPSTREAM_ADDR", value);
        }
        configure_listener_handoff(&mut command, reservation.listener())?;
        let mut child = command.spawn().map_err(|_| {
            localagentmanager_core::AppError::new(
                "GATEWAY_START_FAILED",
                "Gateway sidecar could not be launched",
            )
        })?;
        child
            .stdin
            .take()
            .ok_or_else(|| {
                localagentmanager_core::AppError::new(
                    "GATEWAY_BOOTSTRAP_FAILED",
                    "Gateway bootstrap pipe is unavailable",
                )
            })?
            .write_all(&self.identity_key)
            .map_err(|_| {
                localagentmanager_core::AppError::new(
                    "GATEWAY_BOOTSTRAP_FAILED",
                    "Gateway bootstrap identity could not be delivered",
                )
            })?;
        Ok(child)
    }

    fn retain_child(&self, mut child: Child) -> localagentmanager_core::Result<()> {
        let mut owned = match self.owned_child.lock() {
            Ok(owned) => owned,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(gateway_owner_failed());
            }
        };
        if owned.is_some() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(gateway_owner_failed());
        }
        *owned = Some(child);
        Ok(())
    }

    fn stop_owned_child(&self) -> localagentmanager_core::Result<()> {
        let mut child = self
            .owned_child
            .lock()
            .map_err(|_| gateway_owner_failed())?
            .take();
        let Some(mut child) = child.take() else {
            return Ok(());
        };
        let _ = self.request_shutdown();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if child
                .try_wait()
                .map_err(|_| gateway_owner_failed())?
                .is_some()
            {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        child.wait().map_err(|_| gateway_owner_failed())?;
        Ok(())
    }

    fn request_shutdown(&self) -> localagentmanager_core::Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| gateway_owner_failed())?;
        let control = AuthenticatedControl::new(&self.identity_key, unsafe { libc::geteuid() })?;
        let request = control.sign(u64::MAX, ControlCommand::Shutdown, serde_json::json!({}))?;
        let response = runtime.block_on(ControlSocketClient::send(
            &self.control_path,
            &control,
            &request,
        ))?;
        if response.payload["ok"] == true {
            Ok(())
        } else {
            Err(gateway_owner_failed())
        }
    }
}

impl GatewayReadiness for PackagedReadiness {
    fn ensure_ready(&self, profile_id: &str) -> localagentmanager_core::Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| {
                localagentmanager_core::AppError::new(
                    "GATEWAY_RUNTIME_FAILED",
                    "Gateway readiness runtime failed",
                )
            })?;
        let mut reservation = match runtime.block_on(self.prepare_gateway(profile_id))? {
            GatewayPreparation::Healthy => return Ok(()),
            GatewayPreparation::ReadyToStart(reservation) => reservation,
        };
        let policy = SupervisorPolicy::new(
            3,
            Duration::from_millis(100),
            Duration::from_secs(2),
            Duration::from_secs(30),
        )?;
        for failure_count in 1..=4 {
            let mut child = self.start_sidecar(profile_id, &reservation)?;
            drop(reservation);
            for _ in 0..50 {
                if runtime.block_on(self.verify(profile_id)).is_ok() {
                    return self.retain_child(child);
                }
                if child
                    .try_wait()
                    .map_err(|_| {
                        localagentmanager_core::AppError::new(
                            "GATEWAY_SUPERVISOR_FAILED",
                            "Gateway process status could not be observed",
                        )
                    })?
                    .is_some()
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.wait();
            }
            match policy.restart_decision(failure_count, rand::random()) {
                RestartDecision::RestartAfter(delay) => {
                    std::thread::sleep(delay);
                    reservation = match runtime.block_on(self.prepare_gateway(profile_id))? {
                        GatewayPreparation::Healthy => return Ok(()),
                        GatewayPreparation::ReadyToStart(reservation) => reservation,
                    };
                }
                RestartDecision::Failed => break,
            }
        }
        Err(localagentmanager_core::AppError::new(
            "GATEWAY_HEALTH_FAILED",
            "Gateway did not become ready within its bounded deadline",
        ))
    }

    fn shutdown(&self) -> localagentmanager_core::Result<()> {
        self.stop_owned_child()
    }
}

fn gateway_owner_failed() -> localagentmanager_core::AppError {
    localagentmanager_core::AppError::new(
        "GATEWAY_OWNER_FAILED",
        "Gateway child lifecycle could not be completed",
    )
}

fn validate_projection(binding: &ProfileProviderBinding) -> localagentmanager_core::Result<()> {
    let source = fs::read_to_string(&binding.config_projection.config_path).map_err(|_| {
        localagentmanager_core::AppError::new(
            "CODEX_CONFIG_MISSING",
            "managed Codex config is unavailable",
        )
    })?;
    validate_managed_projection(
        &source,
        &binding.provider_id,
        &binding.config_projection.managed_values,
    )?;
    if binding.route_kind == RouteKind::Gateway && binding.gateway_binding_id.is_none() {
        return Err(localagentmanager_core::AppError::new(
            "GATEWAY_BINDING_NOT_FOUND",
            "profile has no Gateway binding reference",
        ));
    }
    Ok(())
}

fn provider_hub_root() -> localagentmanager_core::Result<PathBuf> {
    if let Some(root) = std::env::var_os("LAM_PROVIDER_HUB_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        localagentmanager_core::AppError::new("HOME_MISSING", "HOME is unavailable")
    })?;
    localagentmanager_core::provider_runtime::ProviderHubPaths::for_home(&home)
        .ensure_canonical_root()
}

fn install_paths() -> localagentmanager_core::Result<(PathBuf, PathBuf)> {
    if let (Some(root), Some(manifest)) = (
        std::env::var_os("LAM_INSTALL_ROOT"),
        std::env::var_os("LAM_INSTALL_MANIFEST"),
    ) {
        return Ok((PathBuf::from(root), PathBuf::from(manifest)));
    }
    let executable = std::env::current_exe().map_err(|_| {
        localagentmanager_core::AppError::new(
            "GATEWAY_INSTALL_ROOT_INVALID",
            "launcher executable path is unavailable",
        )
    })?;
    #[cfg(debug_assertions)]
    {
        let root = executable.parent().ok_or_else(|| {
            localagentmanager_core::AppError::new(
                "GATEWAY_INSTALL_ROOT_INVALID",
                "development launcher directory is unavailable",
            )
        })?;
        return Ok((
            root.to_path_buf(),
            root.join("provider-gateway-install-manifest.json"),
        ));
    }
    #[cfg(not(debug_assertions))]
    let contents = executable.parent().and_then(Path::parent).ok_or_else(|| {
        localagentmanager_core::AppError::new(
            "GATEWAY_INSTALL_ROOT_INVALID",
            "app bundle layout is invalid",
        )
    })?;
    #[cfg(not(debug_assertions))]
    return Ok((
        contents.into(),
        contents.join("Resources/provider-gateway-install-manifest.json"),
    ));
}

fn resolve_codex_executable() -> localagentmanager_core::Result<PathBuf> {
    let path = if let Some(path) = std::env::var_os("LAM_CODEX_EXECUTABLE") {
        PathBuf::from(path)
    } else {
        let output = Command::new("/usr/bin/which")
            .arg("codex")
            .output()
            .map_err(|_| codex_not_installed())?;
        if !output.status.success() {
            return Err(codex_not_installed());
        }
        PathBuf::from(
            String::from_utf8(output.stdout)
                .map_err(|_| codex_not_installed())?
                .trim(),
        )
    };
    localagentmanager_core::gateway::launcher::resolve_external_codex_executable(&path)
}

fn codex_not_installed() -> localagentmanager_core::AppError {
    localagentmanager_core::AppError::new(
        "CODEX_EXECUTABLE_INVALID",
        "Codex executable is not installed",
    )
}

fn choose_stable_port() -> localagentmanager_core::Result<u16> {
    for port in 54_600..54_700 {
        if TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)).is_ok() {
            return Ok(port);
        }
    }
    Err(localagentmanager_core::AppError::new(
        "GATEWAY_PORT_UNAVAILABLE",
        "no stable Gateway port is available",
    ))
}

fn ensure_private_directory(path: &Path) -> localagentmanager_core::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn identity_mismatch() -> localagentmanager_core::AppError {
    localagentmanager_core::AppError::new(
        "GATEWAY_IDENTITY_MISMATCH",
        "Gateway listener identity could not be verified",
    )
}
