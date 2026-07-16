use super::error::{AppError, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const DEFAULT_MAX_BYTES: u64 = 16 * 1024 * 1024;
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicWriteFault {
    Serialize,
    TempCreate,
    TempSync,
    Rename,
}

#[derive(Debug, Clone)]
pub struct StoreOptions {
    pub max_bytes: u64,
}

impl Default for StoreOptions {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreSnapshot<T> {
    pub schema_version: u32,
    pub revision: u64,
    pub value: T,
    pub exists: bool,
}

#[derive(Debug, Clone)]
pub struct InstallationLock {
    path: PathBuf,
    timeout: Duration,
}

impl InstallationLock {
    pub fn new(path: PathBuf, timeout: Duration) -> Self {
        Self { path, timeout }
    }

    pub fn acquire_shared(&self) -> Result<InstallationLockGuard> {
        self.acquire(false)
    }

    pub fn acquire_exclusive(&self) -> Result<InstallationLockGuard> {
        self.acquire(true)
    }

    fn acquire(&self, exclusive: bool) -> Result<InstallationLockGuard> {
        ensure_private_parent(&self.path)?;
        reject_symlink(&self.path)?;
        let file = open_private_lock(&self.path)?;
        let started = Instant::now();
        loop {
            match try_flock(&file, exclusive) {
                Ok(true) => {
                    return Ok(InstallationLockGuard {
                        file,
                        path: self.path.clone(),
                    })
                }
                Ok(false) if started.elapsed() < self.timeout => {
                    thread::sleep(LOCK_POLL_INTERVAL);
                }
                Ok(false) => {
                    return Err(AppError {
                        code: "STORE_LOCK_TIMEOUT".into(),
                        message: "timed out waiting for the Provider Hub installation lock".into(),
                        recoverable: true,
                        details: Some(json!({ "timeoutMs": self.timeout.as_millis() })),
                    });
                }
                Err(error) => return Err(store_io("acquire installation lock", error)),
            }
        }
    }
}

pub struct InstallationLockGuard {
    file: File,
    path: PathBuf,
}

impl Drop for InstallationLockGuard {
    fn drop(&mut self) {
        unlock(&self.file);
    }
}

pub struct VersionedFileStore<T> {
    path: PathBuf,
    lock: InstallationLock,
    schema_version: u32,
    options: StoreOptions,
    marker: PhantomData<T>,
}

impl<T> Clone for VersionedFileStore<T> {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            lock: self.lock.clone(),
            schema_version: self.schema_version,
            options: self.options.clone(),
            marker: PhantomData,
        }
    }
}

impl<T> VersionedFileStore<T>
where
    T: Clone + Default + DeserializeOwned + Serialize,
{
    pub fn new(
        path: PathBuf,
        lock: InstallationLock,
        schema_version: u32,
        options: StoreOptions,
    ) -> Self {
        Self {
            path,
            lock,
            schema_version,
            options,
            marker: PhantomData,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_or_default(&self) -> Result<StoreSnapshot<T>> {
        let _guard = self.lock.acquire_shared()?;
        self.read_unlocked()
    }

    pub fn compare_and_swap(&self, expected_revision: u64, value: &T) -> Result<StoreSnapshot<T>> {
        self.commit(expected_revision, value, None)
    }

    pub fn compare_and_swap_with_fault(
        &self,
        expected_revision: u64,
        value: &T,
        fault: AtomicWriteFault,
    ) -> Result<StoreSnapshot<T>> {
        self.commit(expected_revision, value, Some(fault))
    }

    fn commit(
        &self,
        expected_revision: u64,
        value: &T,
        fault: Option<AtomicWriteFault>,
    ) -> Result<StoreSnapshot<T>> {
        let guard = self.lock.acquire_exclusive()?;
        self.compare_and_swap_locked(&guard, expected_revision, value, fault)
    }

    pub(crate) fn load_locked(&self, guard: &InstallationLockGuard) -> Result<StoreSnapshot<T>> {
        self.ensure_guard(guard)?;
        self.read_unlocked()
    }

    pub(crate) fn compare_and_swap_locked(
        &self,
        guard: &InstallationLockGuard,
        expected_revision: u64,
        value: &T,
        fault: Option<AtomicWriteFault>,
    ) -> Result<StoreSnapshot<T>> {
        self.ensure_guard(guard)?;
        let current = self.read_unlocked()?;
        ensure_revision(expected_revision, current.revision)?;
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| AppError::new("STORE_REVISION_OVERFLOW", "store revision overflow"))?;
        let body = serialize_envelope(self.schema_version, next_revision, value, fault)?;
        if body.len() as u64 > self.options.max_bytes {
            return Err(AppError::new(
                "STORE_TOO_LARGE",
                "serialized store exceeds its size limit",
            ));
        }
        atomic_replace(&self.path, &body, fault)?;
        Ok(StoreSnapshot {
            schema_version: self.schema_version,
            revision: next_revision,
            value: value.clone(),
            exists: true,
        })
    }

    fn ensure_guard(&self, guard: &InstallationLockGuard) -> Result<()> {
        if guard.path == self.lock.path {
            Ok(())
        } else {
            Err(AppError::new(
                "STORE_LOCK_MISMATCH",
                "store operation used a different installation lock",
            ))
        }
    }

    fn read_unlocked(&self) -> Result<StoreSnapshot<T>> {
        reject_symlink(&self.path)?;
        let Some(bytes) = read_bounded(&self.path, self.options.max_bytes)? else {
            return Ok(StoreSnapshot {
                schema_version: self.schema_version,
                revision: 0,
                value: T::default(),
                exists: false,
            });
        };
        let metadata: EnvelopeMetadata = serde_json::from_slice(&bytes)
            .map_err(|error| AppError::new("STORE_INVALID", error.to_string()))?;
        ensure_schema(metadata.schema_version, self.schema_version)?;
        let envelope: VersionedEnvelope<T> = serde_json::from_slice(&bytes)
            .map_err(|error| AppError::new("STORE_INVALID", error.to_string()))?;
        Ok(StoreSnapshot {
            schema_version: envelope.schema_version,
            revision: envelope.revision,
            value: envelope.value,
            exists: true,
        })
    }
}

#[derive(Deserialize)]
struct EnvelopeMetadata {
    #[serde(rename = "schemaVersion", alias = "schema_version")]
    schema_version: u32,
    #[serde(rename = "revision")]
    _revision: u64,
}

#[derive(Deserialize)]
struct VersionedEnvelope<T> {
    #[serde(rename = "schemaVersion", alias = "schema_version")]
    schema_version: u32,
    revision: u64,
    #[serde(flatten)]
    value: T,
}

#[derive(Serialize)]
struct VersionedEnvelopeRef<'a, T> {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    revision: u64,
    #[serde(flatten)]
    value: &'a T,
}

fn serialize_envelope<T: Serialize>(
    schema_version: u32,
    revision: u64,
    value: &T,
    fault: Option<AtomicWriteFault>,
) -> Result<Vec<u8>> {
    inject_fault(fault, AtomicWriteFault::Serialize)?;
    let envelope = VersionedEnvelopeRef {
        schema_version,
        revision,
        value,
    };
    let mut body = serde_json::to_vec_pretty(&envelope)
        .map_err(|error| AppError::new("STORE_SERIALIZE", error.to_string()))?;
    body.push(b'\n');
    Ok(body)
}

fn atomic_replace(path: &Path, body: &[u8], fault: Option<AtomicWriteFault>) -> Result<()> {
    ensure_private_parent(path)?;
    reject_symlink(path)?;
    inject_fault(fault, AtomicWriteFault::TempCreate)?;
    let temp_path = temporary_path(path)?;
    let result = write_and_commit_temp(path, &temp_path, body, fault);
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn write_and_commit_temp(
    target: &Path,
    temp_path: &Path,
    body: &[u8],
    fault: Option<AtomicWriteFault>,
) -> Result<()> {
    let mut temp = open_private_temp(temp_path)?;
    temp.write_all(body)
        .map_err(|error| store_io("write store temp file", error))?;
    inject_fault(fault, AtomicWriteFault::TempSync)?;
    temp.sync_all()
        .map_err(|error| store_io("sync store temp file", error))?;
    drop(temp);
    inject_fault(fault, AtomicWriteFault::Rename)?;
    fs::rename(temp_path, target).map_err(|error| store_io("replace store", error))?;
    sync_parent(target)
}

fn read_bounded(path: &Path, max_bytes: u64) -> Result<Option<Vec<u8>>> {
    let mut file = match open_read_no_follow(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(store_io("open store", error)),
    };
    let length = file
        .metadata()
        .map_err(|error| store_io("inspect store", error))?;
    validate_private_file(path, &length)?;
    let length = length.len();
    if length > max_bytes {
        return Err(AppError::new(
            "STORE_TOO_LARGE",
            "store exceeds its size limit",
        ));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| store_io("read store", error))?;
    Ok(Some(bytes))
}

fn ensure_schema(actual: u32, supported: u32) -> Result<()> {
    if actual > supported {
        return Err(AppError {
            code: "STORE_FUTURE_SCHEMA".into(),
            message: format!("store schema {actual} is newer than supported schema {supported}"),
            recoverable: false,
            details: Some(json!({ "actual": actual, "supported": supported })),
        });
    }
    if actual < supported {
        return Err(AppError {
            code: "STORE_SCHEMA_MIGRATION_REQUIRED".into(),
            message: format!("store schema {actual} requires migration to schema {supported}"),
            recoverable: true,
            details: Some(json!({ "actual": actual, "supported": supported })),
        });
    }
    Ok(())
}

fn ensure_revision(expected: u64, actual: u64) -> Result<()> {
    if expected == actual {
        return Ok(());
    }
    Err(AppError {
        code: "STORE_REVISION_CONFLICT".into(),
        message: format!("expected revision {expected}, found {actual}"),
        recoverable: true,
        details: Some(json!({ "expected": expected, "actual": actual })),
    })
}

fn inject_fault(actual: Option<AtomicWriteFault>, expected: AtomicWriteFault) -> Result<()> {
    if actual == Some(expected) {
        return Err(AppError::new(
            "STORE_FAULT_INJECTED",
            format!("injected fault before {expected:?}"),
        ));
    }
    Ok(())
}

fn temporary_path(target: &Path) -> Result<PathBuf> {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::new("STORE_UNSAFE_PATH", "store has no valid file name"))?;
    Ok(target.with_file_name(format!(".{name}.{}.tmp", Uuid::new_v4())))
}

fn ensure_private_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("STORE_UNSAFE_PATH", "store has no parent directory"))?;
    if !parent.exists() {
        fs::create_dir_all(parent).map_err(|error| store_io("create store directory", error))?;
        set_mode(parent, 0o700)?;
    }
    reject_symlink(parent)?;
    validate_private_directory(parent)
}

fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AppError::new(
            "STORE_UNSAFE_PATH",
            format!("symlink is not allowed: {}", path.display()),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(store_io("inspect store path", error)),
    }
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("STORE_UNSAFE_PATH", "store has no parent directory"))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| store_io("sync store directory", error))
}

fn store_io(action: &str, error: std::io::Error) -> AppError {
    AppError::new("STORE_IO", format!("{action}: {error}"))
}

#[cfg(unix)]
fn open_private_lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| store_io("open installation lock", error))?;
    let metadata = file
        .metadata()
        .map_err(|error| store_io("inspect installation lock", error))?;
    validate_private_file(path, &metadata)?;
    Ok(file)
}

#[cfg(unix)]
fn open_private_temp(path: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| store_io("create store temp file", error))
}

#[cfg(unix)]
fn open_read_no_follow(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| store_io("set private permissions", error))
}

#[cfg(unix)]
fn try_flock(file: &File, exclusive: bool) -> std::io::Result<bool> {
    let operation = if exclusive {
        libc::LOCK_EX
    } else {
        libc::LOCK_SH
    } | libc::LOCK_NB;
    let result = unsafe { libc::flock(file.as_raw_fd(), operation) };
    if result == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(unix)]
fn unlock(file: &File) {
    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_UN);
    }
}

#[cfg(unix)]
fn validate_private_file(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if !metadata.file_type().is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(AppError::new(
            "STORE_UNSAFE_PATH",
            format!(
                "store file is not private and owner-controlled: {}",
                path.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn validate_private_directory(path: &Path) -> Result<()> {
    let metadata =
        fs::metadata(path).map_err(|error| store_io("inspect store directory", error))?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(AppError::new(
            "STORE_UNSAFE_PATH",
            format!(
                "store directory is not private and owner-controlled: {}",
                path.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
compile_error!("Remote Provider versioned storage is supported only on Unix MVP targets");
