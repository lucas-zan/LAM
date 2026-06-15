use localagentmanager_core::{
    attach_provider_to_profile, build_resume_command, create_account_plan, create_provider,
    create_relay_plan, delete_provider, execute_attach_provider_to_profile, execute_create_account,
    execute_create_relay, execute_rename_account, execute_sync, get_profile_quota, list_accounts,
    list_cached_accounts, list_cached_quotas, list_providers, list_sessions,
    plan_attach_provider_to_profile, refresh_all_quotas, relay_resume_session, rename_account_plan,
    sync_plan, terminal_applescript, AttachProviderRequest, CreateAccountRequest,
    CreateProviderRequest, CreateRelayRequest, RelayResumeRequest, RenameAccountRequest,
    ResumeCommandRequest, SecretInput, SyncRequest,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_home(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!("lam-test-{name}-{suffix}"));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn write_executable(path: &Path, body: &str) {
    write(path, body);
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn seed_codex_home(home: &Path, name: &str) -> PathBuf {
    let profile = if name == "main" {
        home.join(".codex")
    } else {
        home.join(format!(".codex-{name}"))
    };
    fs::create_dir_all(profile.join("sessions/2026/06/01")).unwrap();
    write(&profile.join("auth.json"), r#"{"token":"secret"}"#);
    write(
        &profile.join("config.toml"),
        "model = \"gpt-5-codex\"\nmodel_provider = \"openai\"\n",
    );
    write(
        &profile.join("history.jsonl"),
        "{\"text\":\"do not merge\"}\n",
    );
    write(
        &profile.join("sessions/2026/06/01/session-a.jsonl"),
        "{\"session_id\":\"sid-a\",\"cwd\":\"/tmp/project one\",\"summary\":\"Build account scanner\",\"model\":\"gpt-5-codex\"}\n",
    );
    write(&profile.join("logs_2.sqlite"), "sqlite secret");
    write(&profile.join("state_5.sqlite"), "state sqlite secret");
    write(&profile.join("cache/blob"), "cache");
    write(&profile.join("tmp/state"), "tmp");
    write(&profile.join("installation_id"), "install");
    profile
}

#[test]
fn static_fake_home_fixture_scans_expected_profiles() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixture = repo_root.join(".fake-home");

    let accounts = list_accounts(&fixture).unwrap();
    assert!(accounts.iter().any(|a| a.id == "main"));
    assert!(accounts.iter().any(|a| a.id == "a" && a.managed));
    assert!(accounts.iter().any(|a| a.id == "b"));
    assert!(accounts.iter().any(|a| a.id == "b-relay-a" && a.is_relay));

    let sessions = list_sessions(&fixture, "a").unwrap();
    assert!(sessions.iter().any(|s| s.id == "sid-a"));
    assert!(sessions.iter().any(|s| s.id == "broken"));
    assert!(sessions.iter().any(|s| s.id == "empty"));

    let plan = sync_plan(
        &fixture,
        &SyncRequest {
            from_profile_id: "a".into(),
            to_profile_id: "b-relay-a".into(),
            sync_sessions: true,
            backup_target_sessions: true,
            sidecar_backup_history: false,
        },
    )
    .unwrap();
    assert!(plan.blocked_files.iter().any(|p| p == "auth.json"));
    assert!(plan.blocked_files.iter().any(|p| p == "state_5.sqlite"));
    assert!(plan.policy_blocked_files.iter().any(|p| p == "*.sqlite*"));
}

#[test]
fn scans_accounts_and_sessions_without_reading_auth() {
    let home = temp_home("scan");
    seed_codex_home(&home, "main");
    let luna = seed_codex_home(&home, "luna");
    write(
        &luna.join(".managed-by-codex-session-manager.json"),
        "{\"accountName\":\"luna\"}\n",
    );

    let accounts = list_accounts(&home).unwrap();
    assert_eq!(accounts.len(), 2);
    assert!(accounts.iter().any(|a| a.id == "main" && a.has_auth));
    assert!(accounts.iter().any(|a| a.id == "luna" && a.managed));
    let luna_account = accounts.iter().find(|a| a.id == "luna").unwrap();
    assert_eq!(luna_account.provider_id.as_deref(), Some("openai"));
    assert_eq!(luna_account.model.as_deref(), Some("gpt-5-codex"));

    let sessions = list_sessions(&home, "luna").unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "sid-a");
    assert_eq!(sessions[0].cwd.as_deref(), Some("/tmp/project one"));
}

#[test]
fn parses_session_edge_cases_without_crashing() {
    let home = temp_home("session-edges");
    let profile = seed_codex_home(&home, "a");
    write(
        &profile.join("sessions/2026/06/01/json-session.json"),
        "{\"sessionId\":\"json-id\",\"working_directory\":\"/tmp/json\",\"title\":\"JSON title\"}",
    );
    write(&profile.join("sessions/2026/06/01/empty.jsonl"), "");
    write(
        &profile.join("sessions/2026/06/01/broken.jsonl"),
        "{not-json}\n{\"summary\":\"still parse fallback text\"}\n",
    );
    write(
        &profile.join("sessions/2026/06/01/no-cwd.jsonl"),
        "{\"session_id\":\"no-cwd\",\"summary\":\"No cwd here\"}\n",
    );

    let sessions = list_sessions(&home, "a").unwrap();
    assert!(sessions.iter().any(|s| s.id == "json-id"));
    assert!(sessions.iter().any(|s| s.id == "empty"));
    assert!(sessions.iter().any(|s| s.id == "broken"));
    let no_cwd = sessions.iter().find(|s| s.id == "no-cwd").unwrap();
    assert_eq!(no_cwd.cwd, None);
}

#[test]
fn parses_session_summary_with_multibyte_utf8_without_panicking() {
    let home = temp_home("session-utf8");
    let profile = seed_codex_home(&home, "a");
    let long_chinese = "按你这次澄清的语义已经改好了：直接从 medbench_request_dedupe 命中 done 一律等待 request_dedupe_ready_delay_sec 其他路径新请求没命中 dedupe 命中 running 后等生成完成 内存版复用 这些内容还要再核对一遍避免截断时按字节切开多字节字符导致 panic";
    write(
        &profile.join("sessions/2026/06/01/utf8.jsonl"),
        &format!(r#"{{"session_id":"utf8-session","summary":"{long_chinese}"}}"#),
    );

    let sessions = list_sessions(&home, "a").unwrap();
    let utf8 = sessions.iter().find(|s| s.id == "utf8-session").unwrap();
    assert!(utf8.summary.as_ref().is_some_and(|s| s.contains('按')));
    assert!(utf8
        .summary
        .as_ref()
        .is_some_and(|s| s.ends_with("...") || s.chars().count() <= 240));
}

#[test]
fn parses_rollout_session_meta_id_from_file_head_for_resume() {
    let home = temp_home("session-rollout-id");
    let profile = seed_codex_home(&home, "a");
    let session_id = "019e8c4b-cb7f-7851-9557-b903a39a6c4f";
    let path = profile.join(format!(
        "sessions/2026/06/03/rollout-2026-06-03T15-03-58-{session_id}.jsonl"
    ));
    let large_tail = "x".repeat(300 * 1024);
    write(
        &path,
        &format!(
            "{{\"timestamp\":\"2026-06-03T07:03:58.468Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session_id}\",\"cwd\":\"/tmp/project\",\"model_provider\":\"openai\",\"model\":\"gpt-5-codex\"}}}}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"note\",\"message\":\"{large_tail}\"}}}}\n"
        ),
    );

    let sessions = list_sessions(&home, "a").unwrap();
    let rollout = sessions
        .iter()
        .find(|session| session.path == path)
        .unwrap();
    assert_eq!(rollout.id, session_id);
}

#[test]
fn creates_managed_account_with_plan_and_safe_wrapper() {
    let home = temp_home("create-account");
    let req = CreateAccountRequest {
        name: "luna".into(),
        copy_config_from: None,
        overwrite_wrapper: false,
    };
    let plan = create_account_plan(&home, &req).unwrap();
    assert!(plan.operations.iter().any(|op| op.contains(".codex-luna")));
    assert!(!home.join(".codex-luna").exists());

    let result = execute_create_account(&home, &req).unwrap();
    assert!(result.home_path.exists());
    assert!(result.wrapper_path.exists());
    assert!(result
        .home_path
        .join(".managed-by-agent-workspace.json")
        .exists());
    assert!(!result.home_path.join("auth.json").exists());
    let wrapper = fs::read_to_string(result.wrapper_path).unwrap();
    assert!(wrapper.contains("export CODEX_HOME=\"$HOME/.codex-luna\""));
    assert!(wrapper.contains("exec \"$CODEX_BIN\" \"$@\""));
}

#[test]
fn renames_managed_account_home_wrapper_and_metadata() {
    let home = temp_home("rename-account");
    execute_create_account(
        &home,
        &CreateAccountRequest {
            name: "b".into(),
            copy_config_from: None,
            overwrite_wrapper: false,
        },
    )
    .unwrap();
    write(&home.join(".codex-b/auth.json"), r#"{"token":"secret"}"#);

    let rename = RenameAccountRequest {
        from_profile_id: "b".into(),
        to_name: "liming".into(),
        overwrite_wrapper: false,
    };
    let plan = rename_account_plan(&home, &rename).unwrap();
    assert!(plan
        .operations
        .iter()
        .any(|op| op.contains(".codex-b") && op.contains(".codex-liming")));
    assert!(plan
        .operations
        .iter()
        .any(|op| op.contains("codex-b") && op.contains("codex-liming")));
    assert!(plan.blocked.iter().any(|item| item == "auth.json"));

    let result = execute_rename_account(&home, &rename).unwrap();
    assert_eq!(result.profile_id, "liming");
    assert!(!home.join(".codex-b").exists());
    assert!(home.join(".codex-liming").exists());
    assert!(home.join(".codex-liming/auth.json").exists());
    assert!(!home.join("bin/codex-b").exists());
    assert!(home.join("bin/codex-liming").exists());

    let wrapper = fs::read_to_string(result.wrapper_path).unwrap();
    assert!(wrapper.contains("export CODEX_HOME=\"$HOME/.codex-liming\""));
    assert!(!wrapper.contains(".codex-b"));

    let marker =
        fs::read_to_string(home.join(".codex-liming/.managed-by-agent-workspace.json")).unwrap();
    assert!(marker.contains("\"accountName\": \"liming\""));
    assert!(marker.contains(".codex-liming"));
    assert!(marker.contains("codex-liming"));

    let accounts = list_accounts(&home).unwrap();
    assert!(!accounts.iter().any(|account| account.id == "b"));
    assert!(accounts
        .iter()
        .any(|account| account.id == "liming" && account.managed));
}

#[test]
fn rename_account_blocks_unsafe_targets() {
    let home = temp_home("rename-blocks");
    execute_create_account(
        &home,
        &CreateAccountRequest {
            name: "b".into(),
            copy_config_from: None,
            overwrite_wrapper: false,
        },
    )
    .unwrap();
    seed_codex_home(&home, "liming");
    seed_codex_home(&home, "main");

    let existing_home = rename_account_plan(
        &home,
        &RenameAccountRequest {
            from_profile_id: "b".into(),
            to_name: "liming".into(),
            overwrite_wrapper: false,
        },
    )
    .unwrap_err();
    assert_eq!(existing_home.code, "TARGET_ACCOUNT_ALREADY_EXISTS");

    let main_blocked = rename_account_plan(
        &home,
        &RenameAccountRequest {
            from_profile_id: "main".into(),
            to_name: "primary".into(),
            overwrite_wrapper: false,
        },
    )
    .unwrap_err();
    assert_eq!(main_blocked.code, "MAIN_ACCOUNT_RENAME_BLOCKED");
}

#[test]
fn rename_account_blocks_wrapper_conflict_without_overwrite() {
    let home = temp_home("rename-wrapper-conflict");
    execute_create_account(
        &home,
        &CreateAccountRequest {
            name: "b".into(),
            copy_config_from: None,
            overwrite_wrapper: false,
        },
    )
    .unwrap();
    write_executable(
        &home.join("bin/codex-liming"),
        "#!/usr/bin/env bash\nexit 0\n",
    );

    let err = rename_account_plan(
        &home,
        &RenameAccountRequest {
            from_profile_id: "b".into(),
            to_name: "liming".into(),
            overwrite_wrapper: false,
        },
    )
    .unwrap_err();
    assert_eq!(err.code, "WRAPPER_ALREADY_EXISTS");

    let plan = rename_account_plan(
        &home,
        &RenameAccountRequest {
            from_profile_id: "b".into(),
            to_name: "liming".into(),
            overwrite_wrapper: true,
        },
    )
    .unwrap();
    assert!(plan
        .warnings
        .iter()
        .any(|warning| warning.contains("wrapper")));
}

#[test]
fn creates_relay_without_touching_runtime_profile() {
    let home = temp_home("relay");
    let runtime = seed_codex_home(&home, "b");
    let before = fs::read_to_string(runtime.join("history.jsonl")).unwrap();
    seed_codex_home(&home, "a");

    let req = CreateRelayRequest {
        runtime_profile_id: "b".into(),
        source_profile_id: "a".into(),
        name: None,
        provider_policy: "inherit_runtime".into(),
        overwrite_wrapper: false,
    };
    let plan = create_relay_plan(&home, &req).unwrap();
    assert!(plan
        .operations
        .iter()
        .any(|op| op.contains(".codex-b-relay-a")));
    let result = execute_create_relay(&home, &req).unwrap();

    assert!(result.home_path.ends_with(".codex-b-relay-a"));
    assert!(result
        .home_path
        .join(".managed-by-agent-workspace.json")
        .exists());
    assert_eq!(
        fs::read_to_string(runtime.join("history.jsonl")).unwrap(),
        before
    );
}

#[test]
fn sync_plan_is_dry_run_and_execute_blocks_sensitive_files() {
    let home = temp_home("sync");
    let source = seed_codex_home(&home, "a");
    let target = seed_codex_home(&home, "b-relay-a");
    write(
        &target.join("config.toml"),
        "model = \"gpt-5.4\"\nmodel_provider = \"company-proxy\"\n",
    );
    write(
        &source.join("sessions/2026/06/01/extra.jsonl"),
        "{\"session_id\":\"extra\"}\n",
    );

    let req = SyncRequest {
        from_profile_id: "a".into(),
        to_profile_id: "b-relay-a".into(),
        sync_sessions: true,
        backup_target_sessions: true,
        sidecar_backup_history: false,
    };
    let plan = sync_plan(&home, &req).unwrap();
    assert!(plan.operations.iter().any(|op| op.kind == "backup_dir"));
    assert!(plan.operations.iter().any(|op| op.kind == "copy_file"));
    assert!(plan.blocked_files.iter().any(|p| p == "auth.json"));
    assert!(plan.blocked_files.iter().any(|p| p == "config.toml"));
    assert!(plan.blocked_files.iter().any(|p| p == "state_5.sqlite"));
    assert!(plan.blocked_files.iter().any(|p| p == "installation_id"));
    assert!(plan.policy_blocked_files.iter().any(|p| p == "*.sqlite*"));
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("Provider mismatch")));
    assert!(!home
        .join(".codex-b-relay-a/sessions/2026/06/01/extra.jsonl")
        .exists());

    let result = execute_sync(&home, &req).unwrap();
    assert!(result.manifest_path.exists());
    let backup_path = result.backup_path.unwrap();
    assert!(backup_path.exists());
    let backup_name = backup_path.file_name().unwrap().to_string_lossy();
    assert!(backup_name.starts_with("sessions.backup."));
    assert_eq!("sessions.backup.YYYYMMDD-HHMMSS".len(), backup_name.len());
    let manifest_name = result.manifest_path.file_name().unwrap().to_string_lossy();
    assert!(manifest_name.ends_with(".json"));
    assert_ne!(manifest_name.as_ref(), "0.json");
    let manifest = fs::read_to_string(&result.manifest_path).unwrap();
    assert!(manifest.contains("\"operations\""));
    assert!(manifest.contains("\"policyBlockedFiles\""));
    assert!(home
        .join(".codex-b-relay-a/sessions/2026/06/01/extra.jsonl")
        .exists());
    assert!(
        !home.join(".codex-b-relay-a/auth.json").exists()
            || fs::read_to_string(home.join(".codex-b-relay-a/auth.json"))
                .unwrap()
                .contains("secret")
    );
    assert!(!home.join(".codex-b-relay-a/history.from-a.jsonl").exists());
}

#[test]
fn relay_resume_copies_missing_session_and_builds_target_resume() {
    let home = temp_home("relay-resume-copy");
    let source = seed_codex_home(&home, "a");
    seed_codex_home(&home, "b");
    let source_session = source.join("sessions/2026/06/01/relay.jsonl");
    write(
        &source_session,
        "{\"session_id\":\"relay-sid\",\"cwd\":\"/tmp/relay\",\"summary\":\"continue\"}\n",
    );
    let target_session = home.join(".codex-b/sessions/2026/06/01/relay.jsonl");
    assert!(!target_session.exists());

    let result = relay_resume_session(
        &home,
        &RelayResumeRequest {
            from_profile_id: "a".into(),
            to_profile_id: "b".into(),
            session_id: "relay-sid".into(),
            cwd: Some("/tmp/relay".into()),
            diverged_strategy: None,
        },
    )
    .unwrap();

    assert_eq!(result.action, "copied");
    assert!(target_session.exists());
    assert_eq!(
        fs::read_to_string(&target_session).unwrap(),
        fs::read_to_string(&source_session).unwrap()
    );
    assert!(result.resume.command.contains("CODEX_HOME="));
    assert!(result.resume.command.contains(".codex-b"));
    assert!(result.resume.command.contains("codex resume"));
    assert!(result.resume.command.contains("relay-sid"));
    assert_eq!(result.backup_path, None);
}

#[test]
fn relay_resume_extends_target_when_target_is_source_prefix() {
    let home = temp_home("relay-resume-prefix");
    let source = seed_codex_home(&home, "a");
    let target = seed_codex_home(&home, "b");
    let rel = "sessions/2026/06/01/relay.jsonl";
    let first = "{\"session_id\":\"relay-sid\",\"cwd\":\"/tmp/relay\"}\n";
    let second = "{\"type\":\"response\",\"text\":\"new from source\"}\n";
    write(&source.join(rel), &format!("{first}{second}"));
    write(&target.join(rel), first);

    let result = relay_resume_session(
        &home,
        &RelayResumeRequest {
            from_profile_id: "a".into(),
            to_profile_id: "b".into(),
            session_id: "relay-sid".into(),
            cwd: Some("/tmp/relay".into()),
            diverged_strategy: None,
        },
    )
    .unwrap();

    assert_eq!(result.action, "extended");
    assert_eq!(
        fs::read_to_string(target.join(rel)).unwrap(),
        format!("{first}{second}")
    );
    assert!(result.backup_path.is_some());
    assert!(result.backup_path.unwrap().exists());
}

#[test]
fn relay_resume_skips_when_target_already_contains_source() {
    let home = temp_home("relay-resume-skip");
    let source = seed_codex_home(&home, "a");
    let target = seed_codex_home(&home, "b");
    let rel = "sessions/2026/06/01/relay.jsonl";
    let first = "{\"session_id\":\"relay-sid\",\"cwd\":\"/tmp/relay\"}\n";
    let second = "{\"type\":\"response\",\"text\":\"target is ahead\"}\n";
    write(&source.join(rel), first);
    write(&target.join(rel), &format!("{first}{second}"));

    let result = relay_resume_session(
        &home,
        &RelayResumeRequest {
            from_profile_id: "a".into(),
            to_profile_id: "b".into(),
            session_id: "relay-sid".into(),
            cwd: Some("/tmp/relay".into()),
            diverged_strategy: None,
        },
    )
    .unwrap();

    assert_eq!(result.action, "already_current");
    assert_eq!(
        fs::read_to_string(target.join(rel)).unwrap(),
        format!("{first}{second}")
    );
    assert_eq!(result.backup_path, None);
}

#[test]
fn relay_resume_rejects_diverged_session_and_keeps_backup() {
    let home = temp_home("relay-resume-conflict");
    let source = seed_codex_home(&home, "a");
    let target = seed_codex_home(&home, "b");
    let rel = "sessions/2026/06/01/relay.jsonl";
    let first = "{\"session_id\":\"relay-sid\",\"cwd\":\"/tmp/relay\"}\n";
    write(
        &source.join(rel),
        &format!("{first}{{\"source\":\"branch\"}}\n"),
    );
    write(
        &target.join(rel),
        &format!("{first}{{\"target\":\"branch\"}}\n"),
    );

    let err = relay_resume_session(
        &home,
        &RelayResumeRequest {
            from_profile_id: "a".into(),
            to_profile_id: "b".into(),
            session_id: "relay-sid".into(),
            cwd: Some("/tmp/relay".into()),
            diverged_strategy: None,
        },
    )
    .unwrap_err();

    assert_eq!(err.code, "SESSION_DIVERGED");
    assert!(target
        .join("sessions/2026/06/01")
        .read_dir()
        .unwrap()
        .any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("relay.jsonl.backup.")));
    assert_eq!(
        fs::read_to_string(target.join(rel)).unwrap(),
        format!("{first}{{\"target\":\"branch\"}}\n")
    );
}

#[test]
fn relay_resume_diverged_prefer_source_replaces_target_with_backup() {
    let home = temp_home("relay-resume-prefer-source");
    let source = seed_codex_home(&home, "a");
    let target = seed_codex_home(&home, "b");
    let rel = "sessions/2026/06/01/relay.jsonl";
    let first = "{\"session_id\":\"relay-sid\",\"cwd\":\"/tmp/relay\"}\n";
    let source_body = format!("{first}{{\"source\":\"branch\"}}\n");
    write(&source.join(rel), &source_body);
    write(
        &target.join(rel),
        &format!("{first}{{\"target\":\"branch\"}}\n"),
    );

    let result = relay_resume_session(
        &home,
        &RelayResumeRequest {
            from_profile_id: "a".into(),
            to_profile_id: "b".into(),
            session_id: "relay-sid".into(),
            cwd: Some("/tmp/relay".into()),
            diverged_strategy: Some("prefer_source".into()),
        },
    )
    .unwrap();

    assert_eq!(result.action, "prefer_source");
    assert_eq!(fs::read_to_string(target.join(rel)).unwrap(), source_body);
    assert!(result.backup_path.is_some());
}

#[test]
fn relay_resume_diverged_prefer_target_keeps_target_and_copies_source_fork() {
    let home = temp_home("relay-resume-prefer-target");
    let source = seed_codex_home(&home, "a");
    let target = seed_codex_home(&home, "b");
    let rel = "sessions/2026/06/01/relay.jsonl";
    let first = "{\"session_id\":\"relay-sid\",\"cwd\":\"/tmp/relay\"}\n";
    let target_body = format!("{first}{{\"target\":\"branch\"}}\n");
    write(
        &source.join(rel),
        &format!("{first}{{\"source\":\"branch\"}}\n"),
    );
    write(&target.join(rel), &target_body);

    let result = relay_resume_session(
        &home,
        &RelayResumeRequest {
            from_profile_id: "a".into(),
            to_profile_id: "b".into(),
            session_id: "relay-sid".into(),
            cwd: Some("/tmp/relay".into()),
            diverged_strategy: Some("prefer_target".into()),
        },
    )
    .unwrap();

    assert_eq!(result.action, "prefer_target");
    assert_eq!(fs::read_to_string(target.join(rel)).unwrap(), target_body);
    assert!(result.fork_path.unwrap().exists());
}

#[test]
fn relay_resume_diverged_summarize_fork_writes_target_handoff_without_overwrite() {
    let home = temp_home("relay-resume-summarize-fork");
    let source = seed_codex_home(&home, "a");
    let target = seed_codex_home(&home, "b");
    let rel = "sessions/2026/06/01/relay.jsonl";
    let first = "{\"session_id\":\"relay-sid\",\"cwd\":\"/tmp/relay\"}\n";
    let target_body = format!("{first}{{\"target\":\"branch\"}}\n");
    write(
        &source.join(rel),
        &format!("{first}{{\"source\":\"branch\"}}\n"),
    );
    write(&target.join(rel), &target_body);

    let result = relay_resume_session(
        &home,
        &RelayResumeRequest {
            from_profile_id: "a".into(),
            to_profile_id: "b".into(),
            session_id: "relay-sid".into(),
            cwd: Some("/tmp/relay".into()),
            diverged_strategy: Some("summarize_fork_with_target_account".into()),
        },
    )
    .unwrap();

    assert_eq!(result.action, "summarize_fork_with_target_account");
    assert_eq!(fs::read_to_string(target.join(rel)).unwrap(), target_body);
    let handoff = result.handoff_path.unwrap();
    assert!(handoff.exists());
    let body = fs::read_to_string(handoff).unwrap();
    assert!(body.contains("Target account b"));
    assert!(body.contains("relay-sid"));
    assert!(result.resume.command.contains("codex exec resume"));
    assert!(result.resume.command.contains(".codex-b"));
}

#[test]
fn resume_command_is_escaped_and_has_no_arbitrary_shell_input() {
    let home = temp_home("resume");
    seed_codex_home(&home, "a");
    let command = build_resume_command(
        &home,
        &ResumeCommandRequest {
            profile_id: "a".into(),
            session_id: Some("sid-'a".into()),
            cwd: Some("/tmp/project 'one'".into()),
        },
    )
    .unwrap();

    assert!(command.command.contains("CODEX_HOME="));
    assert!(command.command.contains("codex resume"));
    assert!(command.command.contains("'sid-'\\''a'"));
    assert!(command.command.contains("'/tmp/project '\\''one'\\'''"));
    assert!(command.side_effects.iter().any(|s| s.contains(".codex-a")));

    let script = terminal_applescript(&command.command);
    assert!(script.contains("tell application \"Terminal\""));
    assert!(script.contains("do script"));
}

#[test]
fn accounts_cache_roundtrip_and_fast_read() {
    let home = temp_home("accounts-cache");
    seed_codex_home(&home, "a");
    seed_codex_home(&home, "b");

    assert!(list_cached_accounts(&home).unwrap().is_empty());

    let scanned = list_accounts(&home).unwrap();
    assert_eq!(scanned.len(), 2);

    let cached = list_cached_accounts(&home).unwrap();
    assert_eq!(cached.len(), 2);
    assert!(cached.iter().any(|account| account.id == "a"));
    assert!(cached.iter().any(|account| account.id == "b"));
}

#[test]
fn quota_snapshot_uses_unavailable_state_without_fake_realtime_values() {
    let home = temp_home("quota");
    seed_codex_home(&home, "a");
    let snapshot = get_profile_quota(&home, "a", false).unwrap();

    assert_eq!(snapshot.profile_id, "a");
    assert_eq!(snapshot.source, "usage_unavailable");
    assert!(snapshot.fetched_at > 0);
    assert_eq!(snapshot.activity_tokens, None);
    assert_eq!(snapshot.remaining_percent, None);
    assert_eq!(snapshot.reset_at, None);

    let refreshed = refresh_all_quotas(&home, None).unwrap();
    assert!(refreshed.snapshots.iter().any(|s| s.profile_id == "a"));
}

#[test]
fn quota_app_server_attempt_falls_back_without_hanging_or_faking_realtime() {
    let _guard = env_lock().lock().unwrap();
    let home = temp_home("quota-app-server");
    seed_codex_home(&home, "a");
    std::env::set_var("LAM_ENABLE_CODEX_APP_SERVER_QUOTA", "1");
    std::env::set_var("LAM_CODEX_BIN", "/definitely/not/codex");
    let snapshot = get_profile_quota(&home, "a", true).unwrap();
    std::env::remove_var("LAM_ENABLE_CODEX_APP_SERVER_QUOTA");
    std::env::remove_var("LAM_CODEX_BIN");

    assert_eq!(snapshot.source, "usage_unavailable");
    assert_eq!(snapshot.remaining_percent, None);
    assert!(list_cached_quotas(&home, Some(vec!["a".into()]))
        .unwrap()
        .is_empty());
    assert!(snapshot
        .alerts
        .iter()
        .any(|alert| alert.contains("app-server quota unavailable")));
}

#[test]
fn quota_app_server_parses_primary_and_secondary_windows() {
    let _guard = env_lock().lock().unwrap();
    let home = temp_home("quota-app-server-success");
    seed_codex_home(&home, "a");
    let bin = home.join("fake-codex.sh");
    write_executable(
        &bin,
        r#"#!/usr/bin/env bash
if [ "$1" = "app-server" ]; then
  read _line1 || exit 1
  read _line2 || exit 1
  read _line3 || exit 1
  echo '{"jsonrpc":"2.0","id":1,"result":{"plan_type":"plus","primary":{"used_percent":42,"reset_at":"2026-06-02T10:00:00Z"},"secondary":{"used_percent":18,"reset_at":"2026-06-08T00:00:00Z"}}}'
  sleep 5
  exit 0
fi
exit 1
"#,
    );
    std::env::set_var("LAM_ENABLE_CODEX_APP_SERVER_QUOTA", "1");
    std::env::set_var("LAM_CODEX_BIN", bin.to_string_lossy().to_string());
    let snapshot = get_profile_quota(&home, "a", true).unwrap();
    std::env::remove_var("LAM_ENABLE_CODEX_APP_SERVER_QUOTA");
    std::env::remove_var("LAM_CODEX_BIN");

    assert_eq!(snapshot.source, "app_server_rate_limits");
    assert_eq!(snapshot.plan_type.as_deref(), Some("plus"));
    assert_eq!(snapshot.primary_used_percent, Some(42));
    assert_eq!(snapshot.secondary_used_percent, Some(18));
    assert_eq!(snapshot.remaining_percent, Some(58));
    assert_eq!(snapshot.reset_at.as_deref(), Some("2026-06-02T10:00:00Z"));
    assert_eq!(
        snapshot.secondary_reset_at.as_deref(),
        Some("2026-06-08T00:00:00Z")
    );

    let cached = list_cached_quotas(&home, Some(vec!["a".into()])).unwrap();
    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].source, "app_server_rate_limits");
    assert_eq!(cached[0].staleness, "cached");
    assert_eq!(cached[0].primary_used_percent, Some(42));
}

#[test]
fn quota_app_server_failure_returns_cached_real_quota_when_available() {
    let _guard = env_lock().lock().unwrap();
    let home = temp_home("quota-cache-fallback");
    seed_codex_home(&home, "a");
    let bin = home.join("fake-codex.sh");
    write_executable(
        &bin,
        r#"#!/usr/bin/env bash
if [ "$1" = "app-server" ]; then
  if [ "$LAM_FAKE_CODEX_FAIL" = "1" ]; then
    echo 'offline' >&2
    exit 1
  fi
  read _line1 || exit 1
  read _line2 || exit 1
  read _line3 || exit 1
  echo '{"jsonrpc":"2.0","id":1,"result":{"plan_type":"plus","primary":{"used_percent":33,"reset_at":"2026-06-02T10:00:00Z"},"secondary":{"used_percent":22,"reset_at":"2026-06-08T00:00:00Z"}}}'
  sleep 5
  exit 0
fi
exit 1
"#,
    );
    std::env::set_var("LAM_ENABLE_CODEX_APP_SERVER_QUOTA", "1");
    std::env::set_var("LAM_CODEX_BIN", bin.to_string_lossy().to_string());
    let fresh = get_profile_quota(&home, "a", true).unwrap();
    std::env::set_var("LAM_FAKE_CODEX_FAIL", "1");
    let cached = get_profile_quota(&home, "a", true).unwrap();
    std::env::remove_var("LAM_ENABLE_CODEX_APP_SERVER_QUOTA");
    std::env::remove_var("LAM_CODEX_BIN");
    std::env::remove_var("LAM_FAKE_CODEX_FAIL");

    assert_eq!(fresh.source, "app_server_rate_limits");
    assert_eq!(cached.source, "app_server_rate_limits");
    assert_eq!(cached.staleness, "cached");
    assert_eq!(cached.primary_used_percent, Some(33));
    assert!(cached
        .alerts
        .iter()
        .any(|alert| alert.contains("app-server quota unavailable")));
}

#[test]
fn provider_crud_never_returns_or_persists_plaintext_secret() {
    let home = temp_home("provider");
    let provider = create_provider(
        &home,
        &CreateProviderRequest {
            id: "openai-alt".into(),
            name: "OpenAI Alt".into(),
            base_url: "https://api.openai.com/v1".into(),
            wire_api: "openai".into(),
            default_model: "gpt-5-codex".into(),
            env_key: Some("OPENAI_ALT_API_KEY".into()),
            secret: Some(SecretInput::Env {
                env_key: "OPENAI_ALT_API_KEY".into(),
            }),
        },
    )
    .unwrap();

    assert_eq!(provider.id, "openai-alt");
    assert_eq!(provider.secret_storage, "env");
    assert!(!format!("{provider:?}").contains("sk-secret"));

    let providers = list_providers(&home).unwrap();
    assert_eq!(providers.len(), 1);
    assert!(!format!("{providers:?}").contains("sk-secret"));
    let store = fs::read_to_string(home.join(".config/agent-workspace/providers.json")).unwrap();
    assert!(store.contains("OPENAI_ALT_API_KEY"));
    assert!(!store.contains("sk-secret"));

    let deleted = delete_provider(&home, "openai-alt").unwrap();
    assert!(deleted);
}

#[test]
fn keychain_secret_failure_does_not_write_provider_metadata() {
    let home = temp_home("keychain-failure");
    let err = create_provider(
        &home,
        &CreateProviderRequest {
            id: "keychain-provider".into(),
            name: "Keychain Provider".into(),
            base_url: "https://proxy.example.test/v1".into(),
            wire_api: "openai".into(),
            default_model: "gpt-5-codex".into(),
            env_key: None,
            secret: Some(SecretInput::Keychain { secret: "".into() }),
        },
    )
    .unwrap_err();

    assert_eq!(err.code, "PROVIDER_SECRET_EMPTY");
    assert!(list_providers(&home).unwrap().is_empty());
}

#[test]
fn attach_provider_writes_reference_and_backs_up_config_without_secret() {
    let home = temp_home("attach-provider");
    let profile = seed_codex_home(&home, "a");
    create_provider(
        &home,
        &CreateProviderRequest {
            id: "company-proxy".into(),
            name: "Company Proxy".into(),
            base_url: "https://proxy.example.test/v1".into(),
            wire_api: "openai".into(),
            default_model: "gpt-5.4".into(),
            env_key: Some("COMPANY_PROXY_API_KEY".into()),
            secret: Some(SecretInput::Env {
                env_key: "COMPANY_PROXY_API_KEY".into(),
            }),
        },
    )
    .unwrap();

    let req = AttachProviderRequest {
        profile_id: "a".into(),
        provider_id: "company-proxy".into(),
        model: Some("gpt-5.4".into()),
    };
    let plan = plan_attach_provider_to_profile(&home, &req).unwrap();
    assert!(plan
        .operations
        .iter()
        .any(|op| op.contains("backup config.toml")));
    assert!(plan.blocked.iter().any(|item| item == "api_key"));
    let result = attach_provider_to_profile(&home, &req).unwrap();
    assert!(result.config_path.ends_with("config.toml"));
    assert!(result.backup_path.exists());
    assert_eq!(result.provider_id, "company-proxy");

    let config = fs::read_to_string(profile.join("config.toml")).unwrap();
    assert!(config.contains("model_provider = \"company-proxy\""));
    assert!(config.contains("model = \"gpt-5.4\""));
    assert!(config.contains("env_key = \"COMPANY_PROXY_API_KEY\""));
    assert!(!config.contains("sk-"));
    execute_attach_provider_to_profile(&home, &req).unwrap();
}

#[test]
fn provider_delete_is_blocked_while_profile_uses_it() {
    let home = temp_home("provider-delete-blocked");
    seed_codex_home(&home, "a");
    create_provider(
        &home,
        &CreateProviderRequest {
            id: "company-proxy".into(),
            name: "Company Proxy".into(),
            base_url: "https://proxy.example.test/v1".into(),
            wire_api: "openai".into(),
            default_model: "gpt-5.4".into(),
            env_key: Some("COMPANY_PROXY_API_KEY".into()),
            secret: Some(SecretInput::Env {
                env_key: "COMPANY_PROXY_API_KEY".into(),
            }),
        },
    )
    .unwrap();
    attach_provider_to_profile(
        &home,
        &AttachProviderRequest {
            profile_id: "a".into(),
            provider_id: "company-proxy".into(),
            model: Some("gpt-5.4".into()),
        },
    )
    .unwrap();

    let err = delete_provider(&home, "company-proxy").unwrap_err();
    assert_eq!(err.code, "PROVIDER_IN_USE");
    assert!(format!("{:?}", err.details).contains("a"));
}

#[test]
fn sessions_report_original_current_provider_and_mismatch() {
    let home = temp_home("session-provider-mismatch");
    let profile = seed_codex_home(&home, "a");
    write(
        &profile.join("sessions/2026/06/01/provider-session.jsonl"),
        "{\"session_id\":\"provider-session\",\"cwd\":\"/tmp/project\",\"summary\":\"Provider mismatch\",\"model\":\"gpt-5-codex\",\"provider_id\":\"openai\"}\n",
    );
    write(
        &profile.join("config.toml"),
        "model = \"gpt-5.4\"\nmodel_provider = \"company-proxy\"\n",
    );

    let sessions = list_sessions(&home, "a").unwrap();
    let session = sessions
        .iter()
        .find(|session| session.id == "provider-session")
        .unwrap();
    assert_eq!(session.original_provider_id.as_deref(), Some("openai"));
    assert_eq!(
        session.current_provider_id.as_deref(),
        Some("company-proxy")
    );
    assert!(session.provider_mismatch);
}
