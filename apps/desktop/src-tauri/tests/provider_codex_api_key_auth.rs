use localagentmanager_core::codex_api_key_auth::{
    inspect_codex_api_key, read_codex_api_key, write_codex_api_key,
};
use localagentmanager_core::provider_credentials::SecretValue;
use std::fs;

#[test]
fn native_api_key_file_is_private_redacted_and_replaceable() {
    let home = tempfile::tempdir().unwrap();
    let codex_home = home.path().join(".codex-work");
    let first = SecretValue::from_sensitive("sk-first-not-real".into());

    write_codex_api_key(&codex_home, &first).unwrap();

    let auth_path = codex_home.join("auth.json");
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&auth_path).unwrap()).unwrap();
    assert_eq!(json["auth_mode"], "apikey");
    assert_eq!(json["OPENAI_API_KEY"], "sk-first-not-real");
    assert!(inspect_codex_api_key(&codex_home).unwrap());
    let loaded = read_codex_api_key(&codex_home).unwrap();
    assert_eq!(loaded.with_exposed(str::to_owned), "sk-first-not-real");
    assert_eq!(format!("{loaded:?}"), "SecretValue([REDACTED])");

    write_codex_api_key(
        &codex_home,
        &SecretValue::from_sensitive("sk-second-not-real".into()),
    )
    .unwrap();
    let replaced = read_codex_api_key(&codex_home).unwrap();
    assert_eq!(replaced.with_exposed(str::to_owned), "sk-second-not-real");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&codex_home).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(auth_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn native_api_key_file_rejects_empty_missing_and_malformed_credentials() {
    let home = tempfile::tempdir().unwrap();
    let codex_home = home.path().join(".codex-work");
    assert_eq!(
        write_codex_api_key(&codex_home, &SecretValue::from_sensitive("  ".into()),)
            .unwrap_err()
            .code,
        "CODEX_API_KEY_EMPTY"
    );
    assert!(!inspect_codex_api_key(&codex_home).unwrap());
    assert_eq!(
        read_codex_api_key(&codex_home).unwrap_err().code,
        "CODEX_API_KEY_MISSING"
    );

    fs::create_dir_all(&codex_home).unwrap();
    fs::write(codex_home.join("auth.json"), r#"{"auth_mode":"chatgpt"}"#).unwrap();
    assert_eq!(
        read_codex_api_key(&codex_home).unwrap_err().code,
        "CODEX_API_KEY_AUTH_INVALID"
    );
}
