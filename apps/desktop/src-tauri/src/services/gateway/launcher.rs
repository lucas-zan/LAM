use crate::services::error::{AppError, Result};
use crate::services::provider_binding::RouteKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

pub const INSTALL_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallManifest {
    pub schema_version: u32,
    pub components: Vec<InstalledComponent>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledComponent {
    pub name: String,
    pub relative_path: String,
    pub version: String,
    pub sha256: String,
    pub protocol_version: u32,
    pub state_schema: u32,
    pub platform: String,
    pub architecture: String,
    pub package_identity: String,
}

pub trait ComponentIdentityVerifier: Send + Sync {
    fn verify(&self, path: &Path, expected_identity: &str) -> Result<()>;
}

pub struct MacCodeSignIdentityVerifier;

impl ComponentIdentityVerifier for MacCodeSignIdentityVerifier {
    fn verify(&self, path: &Path, expected_identity: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let output = Command::new("/usr/bin/codesign")
                .args(["-dv", "--verbose=4"])
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .map_err(|_| component_integrity_failed())?;
            let details = String::from_utf8_lossy(&output.stderr);
            let identity_matches = if expected_identity == "adhoc" {
                details.lines().any(|line| line == "Signature=adhoc")
            } else {
                details
                    .lines()
                    .any(|line| line == format!("TeamIdentifier={expected_identity}"))
            };
            if !output.status.success() || !identity_matches {
                return Err(component_integrity_failed());
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (path, expected_identity);
            Err(unsupported_platform())
        }
    }
}

pub struct InstallManifestVerifier {
    install_root: PathBuf,
    identity: Arc<dyn ComponentIdentityVerifier>,
}

impl InstallManifestVerifier {
    pub fn new(install_root: PathBuf, identity: Arc<dyn ComponentIdentityVerifier>) -> Self {
        Self {
            install_root,
            identity,
        }
    }

    pub fn verify(&self, manifest: InstallManifest) -> Result<VerifiedInstallation> {
        if manifest.schema_version != INSTALL_MANIFEST_SCHEMA_VERSION {
            return Err(AppError::new(
                "GATEWAY_MANIFEST_VERSION_MISMATCH",
                "installation manifest schema is incompatible",
            ));
        }
        let root = fs::canonicalize(&self.install_root).map_err(|_| {
            AppError::new(
                "GATEWAY_INSTALL_ROOT_INVALID",
                "installation root is unavailable",
            )
        })?;
        let mut components = BTreeMap::new();
        for component in manifest.components {
            validate_component_contract(&component)?;
            if components.contains_key(&component.name) {
                return Err(AppError::new(
                    "GATEWAY_COMPONENT_DUPLICATE",
                    "installation manifest contains a duplicate component",
                ));
            }
            let relative = Path::new(&component.relative_path);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_)))
            {
                return Err(component_path_invalid());
            }
            let candidate = root.join(relative);
            let metadata =
                fs::symlink_metadata(&candidate).map_err(|_| component_path_invalid())?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(component_path_invalid());
            }
            let canonical = fs::canonicalize(&candidate).map_err(|_| component_path_invalid())?;
            if !canonical.starts_with(&root) {
                return Err(component_path_invalid());
            }
            #[cfg(unix)]
            if metadata.permissions().mode() & 0o111 == 0
                || metadata.permissions().mode() & 0o022 != 0
                || metadata.uid() != unsafe { libc::geteuid() }
            {
                return Err(component_integrity_failed());
            }
            let bytes = fs::read(&canonical).map_err(|_| component_integrity_failed())?;
            if hex::encode(Sha256::digest(bytes)) != component.sha256 {
                return Err(component_integrity_failed());
            }
            self.identity
                .verify(&canonical, &component.package_identity)?;
            components.insert(
                component.name.clone(),
                VerifiedComponent {
                    path: canonical,
                    metadata: component,
                },
            );
        }
        for required in ["launcher", "gateway", "auth-helper"] {
            if !components.contains_key(required) {
                return Err(AppError::new(
                    "GATEWAY_COMPONENT_NOT_FOUND",
                    "installation manifest is missing a required component",
                ));
            }
        }
        Ok(VerifiedInstallation { root, components })
    }
}

pub struct VerifiedInstallation {
    root: PathBuf,
    components: BTreeMap<String, VerifiedComponent>,
}

impl fmt::Debug for VerifiedInstallation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedInstallation")
            .field("root", &self.root)
            .field(
                "component_names",
                &self.components.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl VerifiedInstallation {
    pub fn component(&self, name: &str) -> Result<&VerifiedComponent> {
        self.components.get(name).ok_or_else(|| {
            AppError::new(
                "GATEWAY_COMPONENT_NOT_FOUND",
                "installed component was not found",
            )
        })
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedComponent {
    pub path: PathBuf,
    pub metadata: InstalledComponent,
}

pub trait GatewayReadiness: Send + Sync {
    fn ensure_ready(&self, profile_id: &str) -> Result<()>;

    fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CodexLaunchRequest {
    pub profile_id: String,
    pub route_kind: RouteKind,
    pub codex_home: PathBuf,
    pub cwd: PathBuf,
    pub args: Vec<String>,
    pub codex_executable: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexLaunchOutcome {
    pub exit_code: i32,
}

pub struct CodexLauncher {
    installation: Arc<VerifiedInstallation>,
    readiness: Arc<dyn GatewayReadiness>,
}

pub struct DirectCodexLauncher;

impl DirectCodexLauncher {
    pub fn run(request: CodexLaunchRequest) -> Result<CodexLaunchOutcome> {
        if request.route_kind != RouteKind::Direct {
            return Err(AppError::new(
                "CODEX_DIRECT_ROUTE_REQUIRED",
                "Direct Codex launcher received a Gateway route",
            ));
        }
        validate_launch_request(&request)?;
        let executable = request.codex_executable.clone().ok_or_else(|| {
            AppError::new(
                "CODEX_EXECUTABLE_REQUIRED",
                "Direct Codex launch requires an explicit executable",
            )
        })?;
        run_codex_process(request, executable)
    }
}

impl CodexLauncher {
    pub fn new(installation: VerifiedInstallation, readiness: Arc<dyn GatewayReadiness>) -> Self {
        Self {
            installation: Arc::new(installation),
            readiness,
        }
    }

    pub fn new_shared(
        installation: Arc<VerifiedInstallation>,
        readiness: Arc<dyn GatewayReadiness>,
    ) -> Self {
        Self {
            installation,
            readiness,
        }
    }

    pub fn run(&self, request: CodexLaunchRequest) -> Result<CodexLaunchOutcome> {
        if request.route_kind != RouteKind::Gateway {
            return Err(AppError::new(
                "CODEX_GATEWAY_ROUTE_REQUIRED",
                "Verified Gateway launcher received a Direct route",
            ));
        }
        validate_launch_request(&request)?;
        self.readiness.ensure_ready(&request.profile_id)?;
        let outcome = self.run_codex(request);
        let shutdown = self.readiness.shutdown();
        match outcome {
            Err(error) => Err(error),
            Ok(outcome) => shutdown.map(|_| outcome),
        }
    }

    fn run_codex(&self, request: CodexLaunchRequest) -> Result<CodexLaunchOutcome> {
        let executable = match request.codex_executable {
            Some(ref path) => validate_external_executable(path)?,
            None => self.installation.component("codex")?.path.clone(),
        };
        run_codex_process(request, executable)
    }
}

fn validate_launch_request(request: &CodexLaunchRequest) -> Result<()> {
    validate_profile_id(&request.profile_id)?;
    validate_launch_directory(&request.codex_home, "CODEX_PROFILE_HOME_INVALID")?;
    validate_launch_directory(&request.cwd, "CODEX_LAUNCH_CWD_INVALID")?;
    Ok(())
}

fn run_codex_process(
    request: CodexLaunchRequest,
    executable: PathBuf,
) -> Result<CodexLaunchOutcome> {
    let executable = validate_external_executable(&executable)?;
    let mut command = Command::new(executable);
    command
        .args(&request.args)
        .current_dir(&request.cwd)
        .env_clear()
        .env("CODEX_HOME", &request.codex_home)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    for name in [
        "HOME", "LANG", "LC_ALL", "TERM", "TMPDIR", "NO_COLOR", "PATH",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(debug_assertions)]
    for name in ["LAM_TEST_GATEWAY_TOKEN", "LAM_TEST_INSTALL_IDENTITY_KEY"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let status = command
        .status()
        .map_err(|_| AppError::new("CODEX_LAUNCH_FAILED", "Codex process could not be launched"))?;
    #[cfg(unix)]
    let exit_code = status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(1);
    #[cfg(not(unix))]
    let exit_code = status.code().unwrap_or(1);
    Ok(CodexLaunchOutcome { exit_code })
}

fn validate_component_contract(component: &InstalledComponent) -> Result<()> {
    let expected_platform = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unsupported"
    };
    let expected_architecture = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unsupported"
    };
    if component.platform != expected_platform || component.architecture != expected_architecture {
        return Err(unsupported_platform());
    }
    if component.name.is_empty()
        || component.version.is_empty()
        || component.protocol_version == 0
        || component.state_schema == 0
        || component.sha256.len() != 64
        || component.package_identity.is_empty()
    {
        return Err(component_integrity_failed());
    }
    Ok(())
}

fn validate_launch_directory(path: &Path, code: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| AppError::new(code, "launch directory is unavailable"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::new(code, "launch directory is unsafe"));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o022 != 0 || metadata.uid() != unsafe { libc::geteuid() } {
        return Err(AppError::new(
            code,
            "launch directory is not owner-controlled",
        ));
    }
    Ok(())
}

fn validate_external_executable(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(AppError::new(
            "CODEX_EXECUTABLE_INVALID",
            "Codex executable path must be absolute",
        ));
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        AppError::new(
            "CODEX_EXECUTABLE_INVALID",
            "Codex executable is unavailable",
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::new(
            "CODEX_EXECUTABLE_INVALID",
            "Codex executable path is unsafe",
        ));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0 || metadata.permissions().mode() & 0o022 != 0 {
        return Err(AppError::new(
            "CODEX_EXECUTABLE_INVALID",
            "Codex executable permissions are unsafe",
        ));
    }
    fs::canonicalize(path).map_err(|_| {
        AppError::new(
            "CODEX_EXECUTABLE_INVALID",
            "Codex executable cannot be resolved",
        )
    })
}

pub fn resolve_external_codex_executable(path: &Path) -> Result<PathBuf> {
    if let Ok(executable) = validate_external_executable(path) {
        return Ok(executable);
    }
    let wrapper = fs::canonicalize(path).map_err(|_| codex_executable_invalid())?;
    let native = native_codex_candidate(&wrapper).ok_or_else(codex_executable_invalid)?;
    validate_external_executable(&native)
}

#[cfg(target_os = "macos")]
fn native_codex_candidate(wrapper: &Path) -> Option<PathBuf> {
    let (package, target) = match std::env::consts::ARCH {
        "aarch64" => ("codex-darwin-arm64", "aarch64-apple-darwin"),
        "x86_64" => ("codex-darwin-x64", "x86_64-apple-darwin"),
        _ => return None,
    };
    wrapper
        .ancestors()
        .find(|ancestor| {
            ancestor
                .file_name()
                .is_some_and(|name| name == "node_modules")
        })
        .map(|node_modules| {
            node_modules
                .join("@openai")
                .join(package)
                .join("vendor")
                .join(target)
                .join("bin/codex")
        })
}

#[cfg(not(target_os = "macos"))]
fn native_codex_candidate(_wrapper: &Path) -> Option<PathBuf> {
    None
}

fn codex_executable_invalid() -> AppError {
    AppError::new(
        "CODEX_EXECUTABLE_INVALID",
        "Codex native executable could not be resolved safely",
    )
}

fn validate_profile_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AppError::new(
            "CODEX_PROFILE_ID_INVALID",
            "Codex profile identifier is invalid",
        ));
    }
    Ok(())
}

fn component_path_invalid() -> AppError {
    AppError::new(
        "GATEWAY_COMPONENT_PATH_INVALID",
        "installed component path is invalid",
    )
}

fn component_integrity_failed() -> AppError {
    AppError::new(
        "GATEWAY_COMPONENT_INTEGRITY_FAILED",
        "installed component integrity verification failed",
    )
}

fn unsupported_platform() -> AppError {
    AppError::new(
        "UNSUPPORTED_REMOTE_PROVIDER_PLATFORM",
        "Gateway components require exact-tested macOS arm64",
    )
}
