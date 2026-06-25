use localagentmanager_core::{
    add_pat_account, list_accounts, read_pat_metadata, switch_to_pat_account, AddPatAccountRequest,
};
use tempfile::TempDir;

#[test]
fn test_add_and_switch_pat_account() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    let auth_json = serde_json::json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": {
            "access_token": "at-test-token-123",
            "refresh_token": "rt-test-token-123",
            "account_id": "account-test-1234"
        },
        "last_refresh": "2026-06-24T00:00:00+00:00",
        "custom_field": "preserved"
    })
    .as_object()
    .unwrap()
    .clone();

    // Test: Upload auth.json as a new account
    let req = AddPatAccountRequest {
        account_id: "test-pat-account".to_string(),
        auth_json: auth_json.clone(),
        personal_access_token: Some("pat-test-token".to_string()),
        token_expiration: Some("2030-12-31T23:59:59+00:00".to_string()),
    };
    let result = add_pat_account(home, &req).unwrap();
    assert_eq!(result.account_id, "test-pat-account");

    // Verify: auth file created
    let auth_path = home.join(".codex-test-pat-account/auth.json");
    assert!(auth_path.exists());
    let auth_content = std::fs::read_to_string(&auth_path).unwrap();
    let stored_auth: serde_json::Value = serde_json::from_str(&auth_content).unwrap();
    let mut expected_auth = auth_json;
    expected_auth.insert("OPENAI_API_KEY".to_string(), serde_json::Value::Null);
    expected_auth.insert(
        "personal_access_token".to_string(),
        serde_json::Value::String("pat-test-token".to_string()),
    );
    assert_eq!(stored_auth, serde_json::Value::Object(expected_auth));

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
    assert_eq!(accounts[0].auth_mode, Some("uploaded".to_string()));
    assert!(!accounts[0].is_active_auth);

    // Test: Switch to PAT account
    std::fs::create_dir_all(home.join(".codex/sessions")).unwrap();
    std::fs::write(home.join(".codex/config.toml"), "model = \"gpt-5\"\n").unwrap();
    std::fs::write(home.join(".codex/sessions/keep.jsonl"), "{}\n").unwrap();
    switch_to_pat_account(home, "test-pat-account").unwrap();

    // Verify: auth.json copied to ~/.codex/
    let target_auth = home.join(".codex/auth.json");
    assert!(target_auth.exists());
    let target_content = std::fs::read_to_string(&target_auth).unwrap();
    let target_auth: serde_json::Value = serde_json::from_str(&target_content).unwrap();
    assert_eq!(target_auth, stored_auth);
    assert!(home.join(".codex/config.toml").exists());
    assert!(home.join(".codex/sessions/keep.jsonl").exists());

    let accounts = list_accounts(home).unwrap();
    assert!(
        accounts
            .iter()
            .find(|account| account.id == "test-pat-account")
            .unwrap()
            .is_active_auth
    );
}

#[test]
fn test_active_auth_requires_unique_account_id_suffix() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(
        home.join(".codex/auth.json"),
        r#"{"tokens":{"account_id":"active-1234"}}"#,
    )
    .unwrap();

    for account_id in ["first", "second"] {
        let auth_json = serde_json::json!({
            "tokens": {"account_id": format!("{account_id}-1234")}
        })
        .as_object()
        .unwrap()
        .clone();
        add_pat_account(
            home,
            &AddPatAccountRequest {
                account_id: account_id.to_string(),
                auth_json,
                personal_access_token: None,
                token_expiration: None,
            },
        )
        .unwrap();
    }

    assert!(list_accounts(home)
        .unwrap()
        .iter()
        .all(|account| !account.is_active_auth));
}

#[test]
fn test_main_auth_slot_cannot_be_switched() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(home.join(".codex/auth.json"), "{}").unwrap();

    let error = switch_to_pat_account(home, "main").unwrap_err();
    assert_eq!(error.code, "MAIN_AUTH_SLOT");
}

#[test]
fn test_add_duplicate_account_fails() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    let auth_json = serde_json::json!({
        "auth_mode": "chatgpt",
        "tokens": {"access_token": "at-test"}
    })
    .as_object()
    .unwrap()
    .clone();

    // Add first time - should succeed
    let req = AddPatAccountRequest {
        account_id: "dup".to_string(),
        auth_json: auth_json.clone(),
        personal_access_token: None,
        token_expiration: None,
    };
    add_pat_account(home, &req).unwrap();

    // Add again - should fail
    let req2 = AddPatAccountRequest {
        account_id: "dup".to_string(),
        auth_json,
        personal_access_token: None,
        token_expiration: None,
    };
    let result = add_pat_account(home, &req2);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));
}
