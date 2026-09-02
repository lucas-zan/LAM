use localagentmanager_core::{
    add_pat_account, add_session_profile_account, delete_account, export_cpa_credentials,
    list_accounts, list_cached_accounts, read_pat_metadata, switch_to_pat_account,
    update_account_note, update_pat_session_auth, AccountNoteUpdate, AddPatAccountRequest,
    AddSessionProfileAccountRequest, DeleteAccountRequest,
};
use tempfile::TempDir;

const TEST_B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn test_base64url(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        let b0 = bytes[index];
        let b1 = bytes.get(index + 1).copied();
        let b2 = bytes.get(index + 2).copied();
        out.push(TEST_B64URL[(b0 >> 2) as usize] as char);
        out.push(
            TEST_B64URL[(((b0 & 0b0000_0011) << 4) | (b1.unwrap_or(0) >> 4)) as usize] as char,
        );
        if let Some(b1) = b1 {
            out.push(
                TEST_B64URL[(((b1 & 0b0000_1111) << 2) | (b2.unwrap_or(0) >> 6)) as usize] as char,
            );
        }
        if let Some(b2) = b2 {
            out.push(TEST_B64URL[(b2 & 0b0011_1111) as usize] as char);
        }
        index += 3;
    }
    out
}

fn test_jwt(payload: &serde_json::Value) -> String {
    format!(
        "{}.{}.{}",
        test_base64url(br#"{"alg":"RS256","typ":"JWT","kid":"test"}"#),
        test_base64url(serde_json::to_string(payload).unwrap().as_bytes()),
        test_base64url(b"test-signature")
    )
}

fn decode_test_jwt_payload(token: &str) -> serde_json::Value {
    let payload = token.split('.').nth(1).unwrap();
    let mut bytes = Vec::new();
    let mut buffer = 0_u32;
    let mut bits = 0_u8;
    for byte in payload.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => continue,
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    serde_json::from_slice(&bytes).unwrap()
}

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

    // Verify: uploaded auth and runtime auth files created separately
    let uploaded_auth_path = home.join(".codex-test-pat-account/auth-f.json");
    assert!(uploaded_auth_path.exists());
    let uploaded_auth_content = std::fs::read_to_string(&uploaded_auth_path).unwrap();
    let uploaded_auth: serde_json::Value = serde_json::from_str(&uploaded_auth_content).unwrap();
    assert_eq!(uploaded_auth, serde_json::Value::Object(auth_json.clone()));
    assert!(uploaded_auth.get("personal_access_token").is_none());

    let auth_path = home.join(".codex-test-pat-account/auth.json");
    assert!(auth_path.exists());
    let auth_content = std::fs::read_to_string(&auth_path).unwrap();
    let stored_auth: serde_json::Value = serde_json::from_str(&auth_content).unwrap();
    assert_eq!(
        stored_auth,
        serde_json::json!({
            "OPENAI_API_KEY": null,
            "personal_access_token": "pat-test-token"
        })
    );
    assert_eq!(stored_auth.as_object().unwrap().len(), 2);

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
    assert_ne!(target_auth, uploaded_auth);
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
fn test_export_cpa_credentials_merges_runtime_and_uploaded_auth() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    let auth_json = serde_json::json!({
        "tokens": {
            "id_token": "id-test",
            "access_token": "at-test",
            "refresh_token": "rt-test",
            "account_id": "account-test"
        },
        "last_refresh": "2026-06-24T00:00:00+00:00",
        "email": "user@example.com",
        "expired": "2030-12-31T10:00:00+08:00",
        "type": "codex",
        "websockets": true
    })
    .as_object()
    .unwrap()
    .clone();

    add_pat_account(
        home,
        &AddPatAccountRequest {
            account_id: "test-pat-export".to_string(),
            auth_json,
            personal_access_token: Some("pat-test-token".to_string()),
            token_expiration: None,
        },
    )
    .unwrap();

    let export = export_cpa_credentials(home, "test-pat-export").unwrap();
    assert_eq!(export.file_name, "test-pat-export-cpa.json");
    assert_eq!(export.content["id_token"], "id-test");
    assert_eq!(export.content["access_token"], "at-test");
    assert_eq!(export.content["refresh_token"], "rt-test");
    assert_eq!(export.content["account_id"], "account-test");
    assert_eq!(export.content["last_refresh"], "2026-06-24T00:00:00+00:00");
    assert_eq!(export.content["email"], "user@example.com");
    assert_eq!(export.content["expired"], "2030-12-31T10:00:00+08:00");
    assert_eq!(
        export.content["headers"]["authorization"],
        "Bearer pat-test-token"
    );
    assert_eq!(export.content["type"], "codex");
    assert_eq!(export.content["websockets"], true);
}

#[test]
fn test_export_cpa_credentials_accepts_chatgpt_session_json() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    let auth_json = serde_json::json!({
        "user": {"email": "session@example.com"},
        "expires": "2030-12-31T10:00:00+08:00",
        "accessToken": "at-session",
        "refreshToken": "rt-session",
        "idToken": "id-session",
        "accountId": "account-session",
        "lastRefresh": "2026-06-24T00:00:00+00:00",
        "chatgptPlanType": "team"
    })
    .as_object()
    .unwrap()
    .clone();

    add_pat_account(
        home,
        &AddPatAccountRequest {
            account_id: "test-session-export".to_string(),
            auth_json,
            personal_access_token: Some("pat-session-token".to_string()),
            token_expiration: None,
        },
    )
    .unwrap();

    let export = export_cpa_credentials(home, "test-session-export").unwrap();
    assert_eq!(export.content["id_token"], "id-session");
    assert_eq!(export.content["access_token"], "at-session");
    assert_eq!(export.content["refresh_token"], "rt-session");
    assert_eq!(export.content["account_id"], "account-session");
    assert_eq!(export.content["last_refresh"], "2026-06-24T00:00:00+00:00");
    assert_eq!(export.content["email"], "session@example.com");
    assert_eq!(export.content["expired"], "2030-12-31T10:00:00+08:00");
    assert_eq!(export.content["chatgpt_plan_type"], "team");
    assert_eq!(
        export.content["headers"]["authorization"],
        "Bearer pat-session-token"
    );
    assert_eq!(export.content["type"], "codex");
    assert_eq!(export.content["websockets"], true);
}

#[test]
fn test_update_pat_session_auth_replaces_uploaded_auth_only() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    add_pat_account(
        home,
        &AddPatAccountRequest {
            account_id: "test-update-session".to_string(),
            auth_json: serde_json::json!({"accessToken": "old"})
                .as_object()
                .unwrap()
                .clone(),
            personal_access_token: Some("pat-keep".to_string()),
            token_expiration: None,
        },
    )
    .unwrap();

    update_pat_session_auth(
        home,
        "test-update-session",
        serde_json::json!({
            "accessToken": "new",
            "idToken": "id-new",
            "user": {"email": "new@example.com"}
        })
        .as_object()
        .unwrap()
        .clone(),
    )
    .unwrap();

    let account_home = home.join(".codex-test-update-session");
    let auth: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(account_home.join("auth.json")).unwrap())
            .unwrap();
    let auth_f: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(account_home.join("auth-f.json")).unwrap())
            .unwrap();
    assert_eq!(auth["personal_access_token"], "pat-keep");
    assert_eq!(auth_f["accessToken"], "new");
    assert_eq!(auth_f["idToken"], "id-new");
    assert_eq!(auth_f["user"]["email"], "new@example.com");
}

#[test]
fn test_add_pat_account_without_pat_keeps_single_auth_json() {
    for (suffix, personal_access_token) in [
        ("none", None),
        ("empty", Some("")),
        ("whitespace", Some("   ")),
    ] {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let auth_json = serde_json::json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "access_token": format!("at-{suffix}"),
                "refresh_token": format!("rt-{suffix}")
            },
            "custom_field": suffix
        })
        .as_object()
        .unwrap()
        .clone();

        let account_id = format!("test-pat-{suffix}");
        add_pat_account(
            home,
            &AddPatAccountRequest {
                account_id: account_id.clone(),
                auth_json: auth_json.clone(),
                personal_access_token: personal_access_token.map(str::to_string),
                token_expiration: None,
            },
        )
        .unwrap();

        let account_home = home.join(format!(".codex-{account_id}"));
        assert!(!account_home.join("auth-f.json").exists());
        let stored_auth: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(account_home.join("auth.json")).unwrap())
                .unwrap();
        assert_eq!(stored_auth, serde_json::Value::Object(auth_json));
    }
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
fn test_add_session_profile_account_creates_complete_profile_space() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    let session_json = serde_json::json!({
        "user": {"email": "profile@example.com"},
        "expires": "2030-12-31T10:00:00+08:00",
        "accessToken": "at-profile",
        "refreshToken": "rt-profile",
        "idToken": "id-profile",
        "accountId": "account-profile",
        "lastRefresh": "2026-06-24T00:00:00+00:00",
        "chatgptPlanType": "team"
    })
    .as_object()
    .unwrap()
    .clone();

    let result = add_session_profile_account(
        home,
        &AddSessionProfileAccountRequest {
            account_id: "profile-session".to_string(),
            session_json,
            overwrite_wrapper: false,
        },
    )
    .unwrap();

    let account_home = home.join(".codex-profile-session");
    assert_eq!(result.profile_id, "profile-session");
    assert_eq!(result.home_path, account_home);
    assert!(account_home.join("sessions").exists());
    assert!(account_home.join("config.toml").exists());
    assert!(account_home
        .join(".managed-by-agent-workspace.json")
        .exists());
    assert!(home.join("bin/codex-profile-session").exists());

    let auth: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(account_home.join("auth.json")).unwrap())
            .unwrap();
    assert_eq!(auth["auth_mode"], "chatgpt");
    assert_eq!(auth["OPENAI_API_KEY"], serde_json::Value::Null);
    assert_eq!(auth["tokens"]["access_token"], "at-profile");
    assert_eq!(auth["tokens"]["refresh_token"], "rt-profile");
    assert_eq!(auth["tokens"]["id_token"], "id-profile");
    assert_eq!(auth["tokens"]["account_id"], "account-profile");
    assert_eq!(auth["email"], "profile@example.com");
    assert_eq!(auth["expired"], "2030-12-31T10:00:00+08:00");
    assert_eq!(auth["last_refresh"], "2026-06-24T00:00:00+00:00");
    assert_eq!(auth["chatgpt_plan_type"], "team");
}

#[test]
fn test_add_session_profile_account_accepts_snake_case_session_json() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    add_session_profile_account(
        home,
        &AddSessionProfileAccountRequest {
            account_id: "snake-session".to_string(),
            session_json: serde_json::json!({
                "access_token": "at-snake",
                "id_token": "id-snake",
                "account_id": "account-snake"
            })
            .as_object()
            .unwrap()
            .clone(),
            overwrite_wrapper: false,
        },
    )
    .unwrap();

    let auth: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.join(".codex-snake-session/auth.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(auth["tokens"]["access_token"], "at-snake");
    assert_eq!(auth["tokens"]["id_token"], "id-snake");
    assert_eq!(auth["tokens"]["account_id"], "account-snake");
}

#[test]
fn test_add_session_profile_account_accepts_raw_web_session_json() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let access_token = test_jwt(&serde_json::json!({
        "exp": 1783934795_i64,
        "https://api.openai.com/profile": {
            "email": "web-session@example.com"
        },
        "https://api.openai.com/auth": {
            "chatgpt_account_id": "account-from-access",
            "chatgpt_user_id": "user-from-access",
            "organization_id": "org-from-access",
            "project_id": "project-from-access",
            "chatgpt_plan_type": "team"
        }
    }));

    add_session_profile_account(
        home,
        &AddSessionProfileAccountRequest {
            account_id: "web-session".to_string(),
            session_json: serde_json::json!({
                "user": {
                    "id": "user-from-session",
                    "email": "web-session@example.com"
                },
                "account": {
                    "id": "account-from-session",
                    "planType": "plus"
                },
                "expires": "2026-10-01T09:26:48.589Z",
                "accessToken": access_token,
                "sessionToken": "session-token-value",
                "authProvider": "openai"
            })
            .as_object()
            .unwrap()
            .clone(),
            overwrite_wrapper: false,
        },
    )
    .unwrap();

    let auth: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.join(".codex-web-session/auth.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        auth["tokens"]["access_token"].as_str(),
        Some(access_token.as_str())
    );
    assert_eq!(auth["tokens"]["refresh_token"], "session-token-value");
    assert_eq!(auth["tokens"]["account_id"], "account-from-session");
    assert_eq!(auth["email"], "web-session@example.com");
    assert_eq!(auth["expired"], "2026-10-01T09:26:48.589Z");
    assert_eq!(auth["chatgpt_plan_type"], "plus");

    let id_token = auth["tokens"]["id_token"].as_str().unwrap();
    assert_ne!(id_token, access_token);
    let id_payload = decode_test_jwt_payload(id_token);
    assert_eq!(id_payload["email"], "web-session@example.com");
    assert_eq!(
        id_payload["https://api.openai.com/auth"]["chatgpt_account_id"],
        "account-from-session"
    );
    assert_eq!(
        id_payload["https://api.openai.com/auth"]["account_id"],
        "account-from-session"
    );
    assert_eq!(
        id_payload["https://api.openai.com/auth"]["chatgpt_user_id"],
        "user-from-session"
    );
    assert_eq!(
        id_payload["https://api.openai.com/auth"]["chatgpt_plan_type"],
        "plus"
    );
}

#[test]
fn test_add_session_profile_account_rejects_missing_tokens_and_conflicts() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    let missing_tokens = add_session_profile_account(
        home,
        &AddSessionProfileAccountRequest {
            account_id: "bad-session".to_string(),
            session_json: serde_json::json!({"user": {"email": "missing@example.com"}})
                .as_object()
                .unwrap()
                .clone(),
            overwrite_wrapper: false,
        },
    )
    .unwrap_err();
    assert_eq!(missing_tokens.code, "INVALID_SESSION_JSON");

    std::fs::create_dir_all(home.join(".codex-conflict")).unwrap();
    let conflict = add_session_profile_account(
        home,
        &AddSessionProfileAccountRequest {
            account_id: "conflict".to_string(),
            session_json: serde_json::json!({"accessToken": "at-conflict"})
                .as_object()
                .unwrap()
                .clone(),
            overwrite_wrapper: false,
        },
    )
    .unwrap_err();
    assert_eq!(conflict.code, "ACCOUNT_EXISTS");
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

#[test]
fn test_delete_account_removes_profile_wrapper_note_and_cache() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    add_session_profile_account(
        home,
        &AddSessionProfileAccountRequest {
            account_id: "delete-me".to_string(),
            session_json: serde_json::json!({
                "accessToken": "at-delete",
                "idToken": "id-delete",
                "accountId": "account-delete"
            })
            .as_object()
            .unwrap()
            .clone(),
            overwrite_wrapper: false,
        },
    )
    .unwrap();
    update_account_note(
        home,
        &AccountNoteUpdate {
            profile_id: "delete-me".to_string(),
            renewal_date: Some("2030-01-01".to_string()),
            note: Some("remove this".to_string()),
        },
    )
    .unwrap();
    assert!(home.join(".codex-delete-me").exists());
    assert!(home.join("bin/codex-delete-me").exists());

    let result = delete_account(
        home,
        &DeleteAccountRequest {
            profile_id: "delete-me".to_string(),
        },
    )
    .unwrap();

    assert_eq!(result.profile_id, "delete-me");
    assert_eq!(result.removed_home_path, home.join(".codex-delete-me"));
    assert_eq!(
        result.removed_wrapper_path,
        Some(home.join("bin/codex-delete-me"))
    );
    assert!(!home.join(".codex-delete-me").exists());
    assert!(!home.join("bin/codex-delete-me").exists());
    assert!(list_accounts(home)
        .unwrap()
        .iter()
        .all(|account| account.id != "delete-me"));
    assert!(list_cached_accounts(home)
        .unwrap()
        .iter()
        .all(|account| account.id != "delete-me"));
    let notes: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.join(".lam/config/account-notes.json")).unwrap(),
    )
    .unwrap();
    assert!(notes["accounts"].get("delete-me").is_none());
}

#[test]
fn test_delete_last_account_rewrites_empty_cache() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    add_session_profile_account(
        home,
        &AddSessionProfileAccountRequest {
            account_id: "only".to_string(),
            session_json: serde_json::json!({
                "accessToken": "at-only",
                "idToken": "id-only",
                "accountId": "account-only"
            })
            .as_object()
            .unwrap()
            .clone(),
            overwrite_wrapper: false,
        },
    )
    .unwrap();
    assert_eq!(list_accounts(home).unwrap().len(), 1);
    assert_eq!(list_cached_accounts(home).unwrap().len(), 1);

    delete_account(
        home,
        &DeleteAccountRequest {
            profile_id: "only".to_string(),
        },
    )
    .unwrap();

    assert!(list_accounts(home).unwrap().is_empty());
    assert!(list_cached_accounts(home).unwrap().is_empty());
}

#[test]
fn test_delete_account_blocks_main_profile() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".codex/sessions")).unwrap();

    let error = delete_account(
        home,
        &DeleteAccountRequest {
            profile_id: "main".to_string(),
        },
    )
    .unwrap_err();

    assert_eq!(error.code, "MAIN_ACCOUNT_DELETE_BLOCKED");
    assert!(home.join(".codex").exists());
}
