use localagentmanager_core::storage::{
    AtomicWriteFault, InstallationLock, StoreOptions, VersionedFileStore,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
struct TestPayload {
    items: Vec<String>,
}

fn store(root: &TempDir) -> VersionedFileStore<TestPayload> {
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    VersionedFileStore::new(
        root.path().join("state.json"),
        InstallationLock::new(
            root.path().join("provider-hub.lock"),
            Duration::from_millis(250),
        ),
        1,
        StoreOptions::default(),
    )
}

#[test]
fn versioned_store_loads_missing_and_commits_revision() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);

    let missing = store.load_or_default().unwrap();
    assert!(!missing.exists);
    assert_eq!(missing.schema_version, 1);
    assert_eq!(missing.revision, 0);
    assert_eq!(missing.value, TestPayload::default());
    assert!(!store.path().exists(), "read must not create the target");

    let first = store
        .compare_and_swap(
            missing.revision,
            &TestPayload {
                items: vec!["one".into()],
            },
        )
        .unwrap();
    assert!(first.exists);
    assert_eq!(first.revision, 1);
    assert_eq!(store.load_or_default().unwrap(), first);

    let body = fs::read_to_string(store.path()).unwrap();
    assert!(body.contains("\"schemaVersion\": 1"));
    assert!(body.contains("\"revision\": 1"));
    assert!(body.contains("\"items\""));
}

#[test]
fn versioned_store_rejects_stale_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let left = store.load_or_default().unwrap();
    let right = store.load_or_default().unwrap();

    store
        .compare_and_swap(
            left.revision,
            &TestPayload {
                items: vec!["winner".into()],
            },
        )
        .unwrap();
    let err = store
        .compare_and_swap(
            right.revision,
            &TestPayload {
                items: vec!["lost".into()],
            },
        )
        .unwrap_err();

    assert_eq!(err.code, "STORE_REVISION_CONFLICT");
    assert_eq!(store.load_or_default().unwrap().value.items, ["winner"]);
}

#[test]
fn versioned_store_serializes_processes_and_survives_atomic_write_failure() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    store
        .compare_and_swap(
            0,
            &TestPayload {
                items: vec!["base".into()],
            },
        )
        .unwrap();
    let before = fs::read(store.path()).unwrap();

    for fault in [
        AtomicWriteFault::Serialize,
        AtomicWriteFault::TempCreate,
        AtomicWriteFault::TempSync,
        AtomicWriteFault::Rename,
    ] {
        let err = store
            .compare_and_swap_with_fault(
                1,
                &TestPayload {
                    items: vec![format!("fault-{fault:?}")],
                },
                fault,
            )
            .unwrap_err();
        assert_eq!(err.code, "STORE_FAULT_INJECTED");
        assert_eq!(fs::read(store.path()).unwrap(), before);
        assert_eq!(store.load_or_default().unwrap().revision, 1);
    }

    let barrier = Arc::new(Barrier::new(3));
    let results = ["left", "right"].map(|value| {
        let writer = store.clone();
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            writer.compare_and_swap(
                1,
                &TestPayload {
                    items: vec![value.into()],
                },
            )
        })
    });
    barrier.wait();
    let results = results.map(|handle| handle.join().unwrap());
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .filter(|error| error.code == "STORE_REVISION_CONFLICT")
            .count(),
        1
    );
    assert_eq!(store.load_or_default().unwrap().revision, 2);
}

#[test]
fn versioned_store_times_out_on_live_lock_and_recovers_after_release() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let lock = InstallationLock::new(
        root.path().join("provider-hub.lock"),
        Duration::from_millis(40),
    );
    let guard = lock.acquire_exclusive().unwrap();
    let blocked = VersionedFileStore::<TestPayload>::new(
        root.path().join("state.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );

    let err = blocked
        .compare_and_swap(0, &TestPayload::default())
        .unwrap_err();
    assert_eq!(err.code, "STORE_LOCK_TIMEOUT");

    drop(guard);
    assert_eq!(
        blocked
            .compare_and_swap(0, &TestPayload::default())
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn versioned_store_rejects_future_schema_without_overwrite() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let future = b"{\n  \"schemaVersion\": 99,\n  \"revision\": 7,\n  \"items\": []\n}\n";
    fs::write(store.path(), future).unwrap();
    #[cfg(unix)]
    fs::set_permissions(store.path(), fs::Permissions::from_mode(0o600)).unwrap();

    let load = store.load_or_default().unwrap_err();
    assert_eq!(load.code, "STORE_FUTURE_SCHEMA");
    let commit = store
        .compare_and_swap(7, &TestPayload::default())
        .unwrap_err();
    assert_eq!(commit.code, "STORE_FUTURE_SCHEMA");
    assert_eq!(fs::read(store.path()).unwrap(), future);
}

#[test]
fn versioned_store_read_has_no_target_mtime_side_effect() {
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    store.compare_and_swap(0, &TestPayload::default()).unwrap();
    let before = fs::metadata(store.path()).unwrap().modified().unwrap();
    thread::sleep(Duration::from_millis(20));

    store.load_or_default().unwrap();

    let after = fs::metadata(store.path()).unwrap().modified().unwrap();
    assert_eq!(after, before);
}

#[test]
fn versioned_store_rejects_invalid_and_oversized_data() {
    let invalid_root = tempfile::tempdir().unwrap();
    let invalid_store = store(&invalid_root);
    fs::write(invalid_store.path(), b"not json\n").unwrap();
    #[cfg(unix)]
    fs::set_permissions(invalid_store.path(), fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        invalid_store.load_or_default().unwrap_err().code,
        "STORE_INVALID"
    );

    let oversized_root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(oversized_root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let oversized_store = VersionedFileStore::<TestPayload>::new(
        oversized_root.path().join("state.json"),
        InstallationLock::new(
            oversized_root.path().join("provider-hub.lock"),
            Duration::from_millis(250),
        ),
        1,
        StoreOptions { max_bytes: 32 },
    );
    let err = oversized_store
        .compare_and_swap(
            0,
            &TestPayload {
                items: vec!["larger-than-thirty-two-bytes".into()],
            },
        )
        .unwrap_err();
    assert_eq!(err.code, "STORE_TOO_LARGE");
    assert!(!oversized_store.path().exists());
}

#[cfg(unix)]
#[test]
fn versioned_store_rejects_symlink_target() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let real = root.path().join("real.json");
    fs::write(&real, "{}\n").unwrap();
    symlink(&real, root.path().join("state.json")).unwrap();

    let err = store(&root).load_or_default().unwrap_err();
    assert_eq!(err.code, "STORE_UNSAFE_PATH");
}
