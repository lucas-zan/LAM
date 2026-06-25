use localagentmanager_core::{
    check_token_expiration, process_uploaded_credentials, read_pat_metadata, UploadedCredentials,
};
use tempfile::TempDir;

#[test]
fn test_pat_auth_end_to_end() {
    let temp = TempDir::new().unwrap();
    let home_root = temp.path();

    let creds = UploadedCredentials {
        access_token: "at-integration".to_string(),
        account_id: "id".to_string(),
        disabled: false,
        email: "test@example.com".to_string(),
        expired: "2030-12-31T23:59:59+00:00".to_string(),
        headers: None,
        id_token: None,
        last_refresh: "2026-06-24T00:00:00+00:00".to_string(),
        refresh_token: None,
        credential_type: "codex".to_string(),
        websockets: true,
        raw_auth_json: None,
    };

    std::fs::create_dir_all(home_root.join(".codex-test-profile/sessions")).unwrap();
    process_uploaded_credentials(home_root, "test-profile", &creds).unwrap();

    let metadata = read_pat_metadata(home_root, "test-profile")
        .unwrap()
        .unwrap();
    assert_eq!(metadata.auth_type, "oauth");

    let status = check_token_expiration(home_root, "test-profile").unwrap();
    assert!(!status.is_expired);
    assert_eq!(status.warning_level, "ok");
}

#[test]
fn test_pat_metadata_rejects_path_traversal() {
    let temp = TempDir::new().unwrap();
    let result = read_pat_metadata(temp.path(), "../outside");

    assert!(result.is_err());
}
