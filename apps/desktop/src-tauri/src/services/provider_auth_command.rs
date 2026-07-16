use super::error::{AppError, Result};
use super::provider_credentials::{DirectCodexAuth, SecretValue};
use super::storage::{StoreSnapshot, VersionedFileStore};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthCommandSpec {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub timeout_ms: u64,
    pub max_stdout_bytes: usize,
    pub cache_ttl_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovedAuthCommand {
    spec: AuthCommandSpec,
    executable_hash: String,
    approval_fingerprint: String,
    #[serde(default)]
    approval_mac: String,
}

impl ApprovedAuthCommand {
    pub fn spec(&self) -> &AuthCommandSpec {
        &self.spec
    }
    pub fn approval_fingerprint(&self) -> &str {
        &self.approval_fingerprint
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthCommandApprovalCollection {
    pub approvals: Vec<ApprovedAuthCommand>,
}

#[derive(Clone)]
pub struct AuthCommandApprovalRepository {
    store: VersionedFileStore<AuthCommandApprovalCollection>,
}

impl AuthCommandApprovalRepository {
    pub fn new(store: VersionedFileStore<AuthCommandApprovalCollection>) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<StoreSnapshot<AuthCommandApprovalCollection>> {
        self.store.load_or_default()
    }

    pub fn approve(
        &self,
        expected_revision: u64,
        spec: AuthCommandSpec,
        identity_key: &[u8; 32],
    ) -> Result<StoreSnapshot<AuthCommandApprovalCollection>> {
        let approved = approve_auth_command_with_key(spec, identity_key)?;
        let mut snapshot = self.load()?;
        if snapshot.revision != expected_revision {
            return Err(AppError::new(
                "AUTH_COMMAND_APPROVAL_CONFLICT",
                "auth command approval store revision changed",
            ));
        }
        snapshot
            .value
            .approvals
            .retain(|item| item.approval_fingerprint != approved.approval_fingerprint);
        snapshot.value.approvals.push(approved);
        snapshot
            .value
            .approvals
            .sort_by(|a, b| a.approval_fingerprint.cmp(&b.approval_fingerprint));
        self.store
            .compare_and_swap(expected_revision, &snapshot.value)
    }

    pub fn resolve(
        &self,
        approval_id: &str,
        identity_key: &[u8; 32],
    ) -> Result<ApprovedAuthCommand> {
        let snapshot = self.load()?;
        let approved = snapshot
            .value
            .approvals
            .into_iter()
            .find(|item| item.approval_fingerprint == approval_id)
            .ok_or_else(|| {
                AppError::new(
                    "AUTH_COMMAND_APPROVAL_NOT_FOUND",
                    "auth command approval was not found",
                )
            })?;
        verify_approval_mac(&approved, identity_key)?;
        Ok(approved)
    }
}

pub fn codex_auth_for_approved_command(approved: &ApprovedAuthCommand) -> DirectCodexAuth {
    DirectCodexAuth::AuthCommand {
        approval_id: approved.approval_fingerprint.clone(),
        command: approved.spec.executable.to_string_lossy().into_owned(),
        args: approved.spec.args.clone(),
    }
}

pub fn approve_auth_command(spec: AuthCommandSpec) -> Result<ApprovedAuthCommand> {
    build_approved_auth_command(spec, None)
}

pub fn approve_auth_command_with_key(
    spec: AuthCommandSpec,
    identity_key: &[u8; 32],
) -> Result<ApprovedAuthCommand> {
    build_approved_auth_command(spec, Some(identity_key))
}

fn build_approved_auth_command(
    spec: AuthCommandSpec,
    identity_key: Option<&[u8; 32]>,
) -> Result<ApprovedAuthCommand> {
    validate_spec(&spec)?;
    let executable_hash = file_hash(&spec.executable)?;
    let mut digest = Sha256::new();
    digest.update(executable_hash.as_bytes());
    digest.update(spec.executable.as_os_str().as_encoded_bytes());
    for arg in &spec.args {
        digest.update([0]);
        digest.update(arg.as_bytes());
    }
    if let Some(cwd) = &spec.cwd {
        digest.update([1]);
        digest.update(cwd.as_os_str().as_encoded_bytes());
    }
    digest.update(spec.timeout_ms.to_le_bytes());
    digest.update(spec.max_stdout_bytes.to_le_bytes());
    let approval_fingerprint = hex::encode(digest.finalize());
    let approval_mac = identity_key
        .map(|key| approval_mac(key, &approval_fingerprint))
        .transpose()?
        .unwrap_or_default();
    Ok(ApprovedAuthCommand {
        spec,
        executable_hash,
        approval_fingerprint,
        approval_mac,
    })
}

fn approval_mac(identity_key: &[u8; 32], fingerprint: &str) -> Result<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(identity_key).map_err(|_| {
        AppError::new(
            "AUTH_COMMAND_APPROVAL_KEY_INVALID",
            "auth command approval key is invalid",
        )
    })?;
    mac.update(b"lam.auth-command-approval.v1\0");
    mac.update(fingerprint.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn verify_approval_mac(approved: &ApprovedAuthCommand, identity_key: &[u8; 32]) -> Result<()> {
    let rebuilt = build_approved_auth_command(approved.spec.clone(), Some(identity_key))
        .map_err(|_| approval_tampered())?;
    if rebuilt.executable_hash != approved.executable_hash
        || rebuilt.approval_fingerprint != approved.approval_fingerprint
        || rebuilt.approval_mac != approved.approval_mac
    {
        return Err(approval_tampered());
    }
    let provided = hex::decode(&approved.approval_mac).map_err(|_| approval_tampered())?;
    let mut mac = Hmac::<Sha256>::new_from_slice(identity_key).map_err(|_| {
        AppError::new(
            "AUTH_COMMAND_APPROVAL_KEY_INVALID",
            "auth command approval key is invalid",
        )
    })?;
    mac.update(b"lam.auth-command-approval.v1\0");
    mac.update(approved.approval_fingerprint.as_bytes());
    mac.verify_slice(&provided).map_err(|_| approval_tampered())
}

fn approval_tampered() -> AppError {
    AppError::new(
        "AUTH_COMMAND_APPROVAL_TAMPERED",
        "auth command approval integrity verification failed",
    )
}

fn validate_spec(spec: &AuthCommandSpec) -> Result<()> {
    if !spec.executable.is_absolute() || spec.timeout_ms == 0 || spec.max_stdout_bytes == 0 {
        return Err(AppError::new(
            "AUTH_COMMAND_EXECUTABLE_POLICY",
            "command policy is invalid",
        ));
    }
    let link = fs::symlink_metadata(&spec.executable).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::new(
                "AUTH_COMMAND_EXECUTABLE_MISSING",
                "approved executable is missing",
            )
        } else {
            AppError::new(
                "AUTH_COMMAND_EXECUTABLE_POLICY",
                "executable metadata is unavailable",
            )
        }
    })?;
    if link.file_type().is_symlink() || !link.is_file() {
        return Err(AppError::new(
            "AUTH_COMMAND_EXECUTABLE_POLICY",
            "executable must be a regular non-symlink file",
        ));
    }
    #[cfg(unix)]
    {
        if link.permissions().mode() & 0o022 != 0 || link.permissions().mode() & 0o111 == 0 {
            return Err(AppError::new(
                "AUTH_COMMAND_EXECUTABLE_POLICY",
                "executable permissions are unsafe",
            ));
        }
        let uid = unsafe { libc::geteuid() };
        if link.uid() != uid && link.uid() != 0 {
            return Err(AppError::new(
                "AUTH_COMMAND_EXECUTABLE_POLICY",
                "executable owner is not trusted",
            ));
        }
    }
    if spec.cwd.is_some() {
        return Err(AppError::new(
            "AUTH_COMMAND_CWD_POLICY",
            "auth commands always run in a private temporary directory",
        ));
    }
    Ok(())
}

fn file_hash(path: &Path) -> Result<String> {
    fs::read(path)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .map_err(|_| {
            AppError::new(
                "AUTH_COMMAND_EXECUTABLE_MISSING",
                "approved executable is unavailable",
            )
        })
}

pub fn run_approved_auth_command(approved: &ApprovedAuthCommand) -> Result<SecretValue> {
    validate_spec(&approved.spec)?;
    if file_hash(&approved.spec.executable)? != approved.executable_hash {
        return Err(AppError::new(
            "AUTH_COMMAND_APPROVAL_STALE",
            "approved executable changed",
        ));
    }
    let private_directory = PrivateCommandDirectory::create()?;
    let mut command = Command::new(&approved.spec.executable);
    command
        .args(&approved.spec.args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", private_directory.path())
        .env("TMPDIR", private_directory.path())
        .current_dir(private_directory.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    let mut child = command
        .spawn()
        .map_err(|_| AppError::new("AUTH_COMMAND_SPAWN", "approved command could not start"))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdout_limit = approved.spec.max_stdout_bytes;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, stdout_limit));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, 4096));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < Duration::from_millis(approved.spec.timeout_ms) => {
                thread::sleep(Duration::from_millis(2))
            }
            Ok(None) => {
                terminate_process_group(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(AppError::new(
                    "AUTH_COMMAND_TIMEOUT",
                    "approved command timed out",
                ));
            }
            Err(_) => {
                terminate_process_group(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(AppError::new(
                    "AUTH_COMMAND_WAIT",
                    "approved command could not be reaped",
                ));
            }
        }
    };
    let output = stdout_reader.join().map_err(|_| {
        AppError::new("AUTH_COMMAND_WAIT", "approved command output reader failed")
    })??;
    let _ = stderr_reader.join();
    if !status.success() {
        return Err(AppError::new(
            "AUTH_COMMAND_EXIT",
            "approved command returned a nonzero exit status",
        ));
    }
    if output.len() > approved.spec.max_stdout_bytes {
        return Err(AppError::new(
            "AUTH_COMMAND_OUTPUT_LIMIT",
            "approved command output exceeded its limit",
        ));
    }
    let raw = String::from_utf8(output).map_err(|_| {
        AppError::new(
            "AUTH_COMMAND_ENCODING",
            "approved command output is not UTF-8",
        )
    })?;
    let token = raw.trim();
    if token.is_empty() {
        return Err(AppError::new(
            "AUTH_COMMAND_EMPTY",
            "approved command returned no token",
        ));
    }
    if token.contains(['\r', '\n', '\0']) {
        return Err(AppError::new(
            "AUTH_COMMAND_MULTILINE",
            "approved command returned an invalid token shape",
        ));
    }
    Ok(SecretValue::new(token.to_string()))
}

struct PrivateCommandDirectory(PathBuf);

impl PrivateCommandDirectory {
    fn create() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("lam-auth-command-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).map_err(|_| {
            AppError::new(
                "AUTH_COMMAND_TEMP_DIR",
                "private auth command directory could not be created",
            )
        })?;
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(|_| {
            AppError::new(
                "AUTH_COMMAND_TEMP_DIR",
                "private auth command directory permissions could not be secured",
            )
        })?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PrivateCommandDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut kept = Vec::with_capacity(limit.min(4096));
    let mut buffer = [0_u8; 4096];
    loop {
        let count = reader.read(&mut buffer).map_err(|_| {
            AppError::new(
                "AUTH_COMMAND_OUTPUT",
                "approved command output could not be read",
            )
        })?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_add(1).saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    Ok(kept)
}

fn terminate_process_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

struct CachedToken {
    token: SecretValue,
    expires_at: u64,
}
#[derive(Default)]
pub struct AuthCommandCache {
    values: BTreeMap<String, CachedToken>,
}
impl AuthCommandCache {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn resolve(&mut self, approved: &ApprovedAuthCommand, now_ms: u64) -> Result<&SecretValue> {
        let key = approved.approval_fingerprint.clone();
        let fresh = self.values.get(&key).is_some_and(|v| now_ms < v.expires_at);
        if !fresh {
            let token = run_approved_auth_command(approved)?;
            self.values.insert(
                key.clone(),
                CachedToken {
                    token,
                    expires_at: now_ms.saturating_add(approved.spec.cache_ttl_ms),
                },
            );
        }
        Ok(&self.values.get(&key).expect("cache inserted").token)
    }
    pub fn invalidate(&mut self, approved: &ApprovedAuthCommand) {
        self.values.remove(&approved.approval_fingerprint);
    }
}

pub fn write_helper_token(secret: &SecretValue, mut output: impl Write) -> Result<()> {
    secret
        .with_exposed(|token| output.write_all(token.as_bytes()))
        .map_err(|_| AppError::new("AUTH_HELPER_STDOUT", "token output failed"))?;
    output
        .write_all(b"\n")
        .map_err(|_| AppError::new("AUTH_HELPER_STDOUT", "token output failed"))
}
