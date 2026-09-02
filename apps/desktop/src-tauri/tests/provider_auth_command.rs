#![cfg(unix)]
use localagentmanager_core::provider_auth_command::*;
use localagentmanager_core::provider_config_editor::{
    apply_projection, config_hash, ConfigProjectionSpec,
};
use localagentmanager_core::provider_v2::CodexProviderOptions;
use std::path::PathBuf;

fn command(executable: &str, args: &[&str]) -> AuthCommandSpec {
    AuthCommandSpec {
        executable: PathBuf::from(executable),
        args: args.iter().map(|v| (*v).into()).collect(),
        cwd: None,
        timeout_ms: 500,
        max_stdout_bytes: 128,
        cache_ttl_ms: 1_000,
    }
}

#[test]
fn command_returns_one_trimmed_token_and_passes_metacharacters_literally() {
    let marker = "token-$(touch SHOULD_NOT_EXIST);*";
    let approved = approve_auth_command(command("/usr/bin/printf", &["  %s  ", marker])).unwrap();
    let token = run_approved_auth_command(&approved).unwrap();
    assert!(token.with_exposed(|v| v == marker));
    assert!(!std::path::Path::new("SHOULD_NOT_EXIST").exists());
    assert!(!format!("{token:?}").contains(marker));
}

#[test]
fn command_rejects_bad_token_output_exit_and_timeout_without_leaking_output() {
    for (args, code) in [
        (vec![""], "AUTH_COMMAND_EMPTY"),
        (vec!["a\nb"], "AUTH_COMMAND_MULTILINE"),
        (
            vec!["xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"],
            "AUTH_COMMAND_OUTPUT_LIMIT",
        ),
    ] {
        let mut spec = command("/usr/bin/printf", &args);
        spec.max_stdout_bytes = 16;
        let error = run_approved_auth_command(&approve_auth_command(spec).unwrap()).unwrap_err();
        assert_eq!(error.code, code);
        assert!(!error.message.contains("xxxxxxxx"));
    }
    assert_eq!(
        run_approved_auth_command(&approve_auth_command(command("/usr/bin/false", &[])).unwrap())
            .unwrap_err()
            .code,
        "AUTH_COMMAND_EXIT"
    );
    let mut slow = command("/bin/sleep", &["1"]);
    slow.timeout_ms = 20;
    assert_eq!(
        run_approved_auth_command(&approve_auth_command(slow).unwrap())
            .unwrap_err()
            .code,
        "AUTH_COMMAND_TIMEOUT"
    );
}

#[test]
fn approval_rejects_missing_symlink_unsafe_cwd_and_executable_replacement() {
    assert_eq!(
        approve_auth_command(command("relative", &[]))
            .unwrap_err()
            .code,
        "AUTH_COMMAND_EXECUTABLE_POLICY"
    );
    assert_eq!(
        approve_auth_command(command("/missing/helper", &[]))
            .unwrap_err()
            .code,
        "AUTH_COMMAND_EXECUTABLE_MISSING"
    );
    let root = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/usr/bin/printf", root.path().join("helper")).unwrap();
    assert_eq!(
        approve_auth_command(command(
            root.path().join("helper").to_str().unwrap(),
            &["x"]
        ))
        .unwrap_err()
        .code,
        "AUTH_COMMAND_EXECUTABLE_POLICY"
    );
    let mut cwd = command("/usr/bin/printf", &["x"]);
    cwd.cwd = Some(PathBuf::from("relative"));
    assert_eq!(
        approve_auth_command(cwd).unwrap_err().code,
        "AUTH_COMMAND_CWD_POLICY"
    );

    let executable = root.path().join("approved-helper");
    std::fs::copy("/usr/bin/printf", &executable).unwrap();
    let approved = approve_auth_command(command(executable.to_str().unwrap(), &["x"])).unwrap();
    std::fs::write(&executable, b"replaced executable").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        run_approved_auth_command(&approved).unwrap_err().code,
        "AUTH_COMMAND_APPROVAL_STALE"
    );
}

#[test]
fn cache_reuses_then_refreshes_secret_and_helper_writes_only_token_line() {
    let approved = approve_auth_command(command("/usr/bin/printf", &["cache-token"])).unwrap();
    let mut cache = AuthCommandCache::new();
    assert!(cache
        .resolve(&approved, 100)
        .unwrap()
        .with_exposed(|v| v == "cache-token"));
    assert!(cache
        .resolve(&approved, 200)
        .unwrap()
        .with_exposed(|v| v == "cache-token"));
    cache.invalidate(&approved);
    let refreshed = cache.resolve(&approved, 201).unwrap();
    let mut stdout = Vec::new();
    write_helper_token(refreshed, &mut stdout).unwrap();
    assert_eq!(stdout, b"cache-token\n");

    let auth = codex_auth_for_approved_command(&approved);
    let projection = apply_projection(
        "",
        &config_hash(b""),
        &ConfigProjectionSpec {
            provider_id: "direct-helper".into(),
            model: "model-a".into(),
            display_name: "Direct helper".into(),
            base_url: "https://api.example.test/v1".into(),
            auth,
            codex: CodexProviderOptions::default(),
            gateway: false,
            model_catalog_path: "/tmp/direct-helper/models.json".into(),
            model_context_window: None,
            model_auto_compact_token_limit: None,
            reasoning_effort: None,
        },
    )
    .unwrap();
    assert!(projection
        .contents
        .contains("[model_providers.direct-helper.auth]"));
    assert!(projection
        .contents
        .contains("command = \"/usr/bin/printf\""));
    assert!(!projection.contents.contains("env_key"));
}
