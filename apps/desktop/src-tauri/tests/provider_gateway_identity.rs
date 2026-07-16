use localagentmanager_core::gateway::identity::{
    load_or_create_install_identity, InstallIdentityStore, StoredIdentity,
};
use localagentmanager_core::{AppError, Result};
use std::sync::Mutex;

#[derive(Clone)]
enum LoadOutcome {
    Found(Vec<u8>),
    Missing,
    Failed,
}

struct FakeStore {
    load: LoadOutcome,
    fail_store: bool,
    writes: Mutex<Vec<Vec<u8>>>,
}

impl FakeStore {
    fn new(load: LoadOutcome) -> Self {
        Self {
            load,
            fail_store: false,
            writes: Mutex::new(Vec::new()),
        }
    }

    fn failing_store() -> Self {
        Self {
            fail_store: true,
            ..Self::new(LoadOutcome::Missing)
        }
    }
}

impl InstallIdentityStore for FakeStore {
    fn load(&self, _install_id: &str) -> Result<StoredIdentity> {
        match &self.load {
            LoadOutcome::Found(bytes) => Ok(StoredIdentity::Found(bytes.clone())),
            LoadOutcome::Missing => Ok(StoredIdentity::Missing),
            LoadOutcome::Failed => Err(AppError::new("KEYCHAIN_UNAVAILABLE", "read failed")),
        }
    }

    fn store(&self, _install_id: &str, identity: &[u8]) -> Result<()> {
        if self.fail_store {
            return Err(AppError::new("KEYCHAIN_UNAVAILABLE", "write failed"));
        }
        self.writes.lock().unwrap().push(identity.to_vec());
        Ok(())
    }
}

#[test]
fn install_identity_returns_existing_bytes_without_writing() {
    let store = FakeStore::new(LoadOutcome::Found(vec![3; 32]));

    let identity = load_or_create_install_identity(&store, "install-a", || [9; 32]).unwrap();

    assert_eq!(identity, [3; 32]);
    assert!(store.writes.lock().unwrap().is_empty());
}

#[test]
fn install_identity_creates_once_only_when_explicitly_missing() {
    let store = FakeStore::new(LoadOutcome::Missing);

    let identity = load_or_create_install_identity(&store, "install-a", || [7; 32]).unwrap();

    assert_eq!(identity, [7; 32]);
    assert_eq!(store.writes.lock().unwrap().as_slice(), &[vec![7; 32]]);
}

#[test]
fn install_identity_read_error_never_rotates_or_writes() {
    let store = FakeStore::new(LoadOutcome::Failed);

    let error = load_or_create_install_identity(&store, "install-a", || [7; 32]).unwrap_err();

    assert_eq!(error.code, "KEYCHAIN_UNAVAILABLE");
    assert!(store.writes.lock().unwrap().is_empty());
}

#[test]
fn install_identity_rejects_invalid_length_without_writing() {
    let store = FakeStore::new(LoadOutcome::Found(vec![3; 31]));

    let error = load_or_create_install_identity(&store, "install-a", || [7; 32]).unwrap_err();

    assert_eq!(error.code, "GATEWAY_IDENTITY_KEY_INVALID");
    assert!(store.writes.lock().unwrap().is_empty());
}

#[test]
fn install_identity_write_failure_is_explicit() {
    let store = FakeStore::failing_store();

    let error = load_or_create_install_identity(&store, "install-a", || [7; 32]).unwrap_err();

    assert_eq!(error.code, "KEYCHAIN_UNAVAILABLE");
    assert!(store.writes.lock().unwrap().is_empty());
}
