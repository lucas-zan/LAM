use localagentmanager_core::{
    add_pat_account, list_accounts, read_pat_metadata, switch_to_pat_account, AddPatAccountRequest,
    UploadedCredentials,
};
use tempfile::TempDir;

#[test]
fn test_add_and_switch_pat_account() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // Prepare credentials
    let mut headers = serde_json::Map::new();
    headers.insert(
        "authorization".to_string(),
        serde_json::Value::String("Bearer at-test-token-123".to_string()),
    );

    let creds = UploadedCredentials {
        access_token: "".to_string(),
        account_id: "test-pat-account".to_string(),
        disabled: false,
        email: "test@example.com".to_string(),
        expired: "2030-12-31T23:59:59+00:00".to_string(),
        headers: Some(headers),
        id_token: None,
        last_refresh: "2026-06-24T00:00:00+00:00".to_string(),
        refresh_token: None,
        credential_type: "codex".to_string(),
        websockets: true,
    };

    // Test: Add PAT account
    let req = AddPatAccountRequest { credentials: creds };
    let result = add_pat_account(home, &req).unwrap();
    assert_eq!(result.account_id, "test-pat-account");
    assert_eq!(result.email, "test@example.com");

    // Verify: auth file created
    let auth_path = home.join(".codex-test-pat-account/auth.json");
    assert!(auth_path.exists());
    let auth_content = std::fs::read_to_string(&auth_path).unwrap();
    assert!(auth_content.contains("personal_access_token"));
    assert!(auth_content.contains("at-test-token-123"));

    // Verify: metadata file created
    let metadata = read_pat_metadata(home, "test-pat-account")
        .unwrap()
        .unwrap();
    assert_eq!(
        metadata.token_expiration,
        Some("2030-12-31T23:59:59+00:00".to_string())
    );

    // Verify: appears in list_accounts
    let accounts = list_accounts(home).unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "test-pat-account");
    assert_eq!(accounts[0].auth_mode, Some("personal_token".to_string()));

    // Test: Switch to PAT account
    std::fs::create_dir_all(home.join(".codex/sessions")).unwrap();
    std::fs::write(home.join(".codex/config.toml"), "model = \"gpt-5\"\n").unwrap();
    std::fs::write(home.join(".codex/sessions/keep.jsonl"), "{}\n").unwrap();
    switch_to_pat_account(home, "test-pat-account").unwrap();

    // Verify: auth.json copied to ~/.codex/
    let target_auth = home.join(".codex/auth.json");
    assert!(target_auth.exists());
    let target_content = std::fs::read_to_string(&target_auth).unwrap();
    assert!(target_content.contains("at-test-token-123"));
    assert!(home.join(".codex/config.toml").exists());
    assert!(home.join(".codex/sessions/keep.jsonl").exists());
}

#[test]
fn test_add_duplicate_account_fails() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    let mut headers = serde_json::Map::new();
    headers.insert(
        "authorization".to_string(),
        serde_json::Value::String("Bearer at-test".to_string()),
    );

    let creds = UploadedCredentials {
        access_token: "".to_string(),
        account_id: "dup".to_string(),
        disabled: false,
        email: "test@example.com".to_string(),
        expired: "2030-12-31T23:59:59+00:00".to_string(),
        headers: Some(headers.clone()),
        id_token: None,
        last_refresh: "2026-06-24T00:00:00+00:00".to_string(),
        refresh_token: None,
        credential_type: "codex".to_string(),
        websockets: true,
    };

    // Add first time - should succeed
    let req = AddPatAccountRequest {
        credentials: creds.clone(),
    };
    add_pat_account(home, &req).unwrap();

    // Add again - should fail
    let req2 = AddPatAccountRequest { credentials: creds };
    let result = add_pat_account(home, &req2);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));
}
