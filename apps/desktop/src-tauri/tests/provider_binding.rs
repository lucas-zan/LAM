use localagentmanager_core::provider_binding::*;
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use std::collections::BTreeMap;
use std::fs;
use std::time::Duration;

fn repository(root: &tempfile::TempDir) -> ProfileBindingRepository {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    ProfileBindingRepository::new(VersionedFileStore::new(
        root.path().join("bindings.json"),
        InstallationLock::new(
            root.path().join("provider-hub.lock"),
            Duration::from_millis(250),
        ),
        1,
        StoreOptions::default(),
    ))
}

fn observation() -> ExistingConfigBinding {
    ExistingConfigBinding {
        provider_id: "provider-a".into(),
        selected_model: "model-a".into(),
        config_path: "/fixture/profile/config.toml".into(),
        config_hash: "hash-a".into(),
        auth_supported: true,
        manageable: true,
        ambiguous: false,
    }
}

#[test]
fn adoption_is_explicit_idempotent_and_used_by_reads_only_the_store() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(&root);
    let first = repo
        .adopt(
            0,
            "profile-a",
            observation(),
            7,
            "fingerprint-a",
            "2026-07-13T01:00:00Z",
        )
        .unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(
        first.value.bindings[0].config_projection.ownership,
        ProjectionOwnership::Adopted
    );
    assert!(first.value.bindings[0]
        .config_projection
        .managed_values
        .is_empty());
    let repeated = repo
        .adopt(
            1,
            "profile-a",
            observation(),
            7,
            "fingerprint-a",
            "2026-07-13T02:00:00Z",
        )
        .unwrap();
    assert_eq!(repeated.revision, 1, "idempotent adoption must not write");
    assert_eq!(repo.used_by("provider-a").unwrap(), ["profile-a"]);
}

#[test]
fn adoption_rejects_unknown_ambiguous_unmanageable_and_unsupported_auth() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(&root);
    for (mut value, code) in [
        (observation(), "PROFILE_CONFIG_AMBIGUOUS"),
        (observation(), "PROFILE_CONFIG_UNMANAGEABLE"),
        (observation(), "PROFILE_CONFIG_AUTH_UNSUPPORTED"),
    ] {
        match code {
            "PROFILE_CONFIG_AMBIGUOUS" => value.ambiguous = true,
            "PROFILE_CONFIG_UNMANAGEABLE" => value.manageable = false,
            _ => value.auth_supported = false,
        }
        assert_eq!(
            repo.adopt(0, "profile", value, 1, "fp", "2026-07-13T01:00:00Z")
                .unwrap_err()
                .code,
            code
        );
    }
}

#[test]
fn reconciliation_reports_managed_drift_and_stale_provider_deterministically() {
    let projection = ManagedConfigProjection {
        config_path: "/fixture/config.toml".into(),
        ownership: ProjectionOwnership::Managed,
        before_hash: "before".into(),
        applied_hash: "applied".into(),
        previous_values: BTreeMap::new(),
        managed_values: BTreeMap::from([("model".into(), "\"model-a\"".into())]),
        provider_table_created_by_lam: true,
    };
    let binding = ProfileProviderBinding::new_for_test(
        "profile",
        "provider-a",
        "model-a",
        projection,
        3,
        "fp-a",
    );
    assert_eq!(
        reconcile_binding(
            &binding,
            &BTreeMap::from([("model".into(), "\"model-a\"".into())]),
            3,
            "fp-a"
        ),
        BindingState::Managed
    );
    assert!(matches!(
        reconcile_binding(
            &binding,
            &BTreeMap::from([("model".into(), "\"changed\"".into())]),
            3,
            "fp-a"
        ),
        BindingState::Drifted { .. }
    ));
    assert_eq!(
        reconcile_binding(
            &binding,
            &binding.config_projection.managed_values,
            4,
            "fp-b"
        ),
        BindingState::StaleProvider
    );
}

#[test]
fn rename_detach_delete_guard_and_revision_conflict_have_no_side_effects() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(&root);
    repo.adopt(0, "old", observation(), 1, "fp", "2026-07-13T01:00:00Z")
        .unwrap();
    assert_eq!(
        repo.ensure_profile_can_delete("old").unwrap_err().code,
        "PROFILE_HAS_PROVIDER_BINDING"
    );
    let renamed = repo
        .rename_profile(1, "old", "new", 1, "2026-07-13T02:00:00Z")
        .unwrap();
    assert_eq!(renamed.value.bindings[0].profile_id, "new");
    let conflict = repo
        .rename_profile(2, "new", "other", 1, "2026-07-13T03:00:00Z")
        .unwrap_err();
    assert_eq!(conflict.code, "PROFILE_BINDING_CONFLICT");
    assert_eq!(repo.load().unwrap().value.bindings[0].profile_id, "new");
    repo.detach(2, "new", 2).unwrap();
    repo.ensure_profile_can_delete("new").unwrap();
    assert!(repo.used_by("provider-a").unwrap().is_empty());
}
