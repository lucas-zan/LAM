use localagentmanager_core::gateway::launcher::{
    resolve_external_codex_executable, CodexLaunchRequest, CodexLauncher,
    ComponentIdentityVerifier, DirectCodexLauncher, GatewayReadiness, InstallManifest,
    InstallManifestVerifier, InstalledComponent,
};
use localagentmanager_core::provider_binding::RouteKind;
use localagentmanager_core::{AppError, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
use std::os::unix::fs::symlink;

struct TestIdentity;
impl ComponentIdentityVerifier for TestIdentity {
    fn verify(&self, _path: &Path, expected_identity: &str) -> Result<()> {
        if expected_identity == "TEAM.TEST" {
            Ok(())
        } else {
            Err(AppError::new(
                "GATEWAY_COMPONENT_INTEGRITY_FAILED",
                "identity mismatch",
            ))
        }
    }
}

#[derive(Default)]
struct FakeReadiness {
    ensure_count: AtomicUsize,
    shutdown_count: AtomicUsize,
}

impl GatewayReadiness for FakeReadiness {
    fn ensure_ready(&self, _profile_id: &str) -> Result<()> {
        self.ensure_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        self.shutdown_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn hash(path: &Path) -> String {
    hex::encode(Sha256::digest(fs::read(path).unwrap()))
}

fn manifest(root: &Path, codex: &Path) -> InstallManifest {
    let component = |name: &str, path: &Path| InstalledComponent {
        name: name.into(),
        relative_path: path.strip_prefix(root).unwrap().to_string_lossy().into(),
        version: "0.2.1".into(),
        sha256: hash(path),
        protocol_version: 1,
        state_schema: 1,
        platform: "macos".into(),
        architecture: "aarch64".into(),
        package_identity: "TEAM.TEST".into(),
    };
    InstallManifest {
        schema_version: 1,
        components: vec![
            component("launcher", &root.join("bin/lam")),
            component("gateway", &root.join("bin/lam-provider-gateway")),
            component("auth-helper", &root.join("bin/lam-auth-helper")),
            component("codex", codex),
        ],
    }
}

fn installation(
    root: &Path,
) -> (
    localagentmanager_core::gateway::launcher::VerifiedInstallation,
    PathBuf,
) {
    fs::create_dir_all(root.join("bin")).unwrap();
    let output = root.join("launch-output.json");
    let codex = root.join("bin/codex");
    executable(&root.join("bin/lam"), "#!/bin/sh\nexit 0\n");
    executable(
        &root.join("bin/lam-provider-gateway"),
        "#!/bin/sh\nexit 0\n",
    );
    executable(&root.join("bin/lam-auth-helper"), "#!/bin/sh\nexit 0\n");
    executable(
        &codex,
        &format!(
            r#"#!/bin/sh
printf '{{"cwd":"%s","codexHome":"%s","arg1":"%s","arg2":"%s","path":"%s","rustLog":"%s"}}' "$PWD" "$CODEX_HOME" "$1" "$2" "$PATH" "${{RUST_LOG:-}}" > '{}'
exit 23
"#,
            output.display()
        ),
    );
    let manifest = manifest(root, &codex);
    let verified = InstallManifestVerifier::new(root.to_path_buf(), Arc::new(TestIdentity))
        .verify(manifest)
        .unwrap();
    (verified, output)
}

#[test]
fn manifest_resolves_only_integrity_checked_bundle_relative_components() {
    let root = tempfile::tempdir().unwrap();
    let (verified, _) = installation(root.path());
    let canonical_root = fs::canonicalize(root.path()).unwrap();
    for name in ["launcher", "gateway", "auth-helper", "codex"] {
        let component = verified.component(name).unwrap();
        assert!(component.path.is_absolute());
        assert!(component.path.starts_with(&canonical_root));
    }
    assert_eq!(
        verified.component("missing").unwrap_err().code,
        "GATEWAY_COMPONENT_NOT_FOUND"
    );
}

#[test]
fn manifest_rejects_tamper_traversal_version_platform_and_identity() {
    let root = tempfile::tempdir().unwrap();
    let (verified, _) = installation(root.path());
    let codex = verified.component("codex").unwrap().path.clone();
    executable(&codex, "#!/bin/sh\nexit 9\n");
    let canonical_root = fs::canonicalize(root.path()).unwrap();
    let manifest = manifest(&canonical_root, &codex);
    let mut tampered = manifest.clone();
    tampered.components[0].sha256 = "00".repeat(32);
    assert_eq!(
        InstallManifestVerifier::new(root.path().into(), Arc::new(TestIdentity))
            .verify(tampered)
            .unwrap_err()
            .code,
        "GATEWAY_COMPONENT_INTEGRITY_FAILED"
    );
    let mut traversal = manifest.clone();
    traversal.components[0].relative_path = "../outside".into();
    assert_eq!(
        InstallManifestVerifier::new(root.path().into(), Arc::new(TestIdentity))
            .verify(traversal)
            .unwrap_err()
            .code,
        "GATEWAY_COMPONENT_PATH_INVALID"
    );
    let mut platform = manifest.clone();
    platform.components[0].platform = "linux".into();
    assert_eq!(
        InstallManifestVerifier::new(root.path().into(), Arc::new(TestIdentity))
            .verify(platform)
            .unwrap_err()
            .code,
        "UNSUPPORTED_REMOTE_PROVIDER_PLATFORM"
    );
}

#[test]
fn launcher_preserves_args_cwd_codex_home_and_exit_code() {
    let root = tempfile::tempdir().unwrap();
    let (verified, output) = installation(root.path());
    let readiness = Arc::new(FakeReadiness::default());
    let launcher = CodexLauncher::new(verified, readiness.clone());
    let cwd = root.path().join("work");
    let codex_home = root.path().join("profile-home");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(&codex_home).unwrap();
    let outcome = launcher
        .run(CodexLaunchRequest {
            profile_id: "profile-a".into(),
            route_kind: RouteKind::Gateway,
            codex_home: codex_home.clone(),
            cwd: cwd.clone(),
            args: vec!["resume".into(), "session with space".into()],
            codex_executable: None,
        })
        .unwrap();
    assert_eq!(outcome.exit_code, 23);
    assert_eq!(readiness.ensure_count.load(Ordering::SeqCst), 1);
    assert_eq!(readiness.shutdown_count.load(Ordering::SeqCst), 1);
    let captured: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(
        captured["cwd"],
        fs::canonicalize(&cwd).unwrap().to_string_lossy().as_ref()
    );
    assert_eq!(captured["codexHome"], codex_home.to_string_lossy().as_ref());
    assert_eq!(captured["arg1"], "resume");
    assert_eq!(captured["arg2"], "session with space");
}

#[test]
fn direct_launcher_runs_external_codex_without_a_gateway_installation() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("direct-output.json");
    let codex = root.path().join("codex");
    executable(
        &codex,
        &format!(
            r#"#!/bin/sh
printf '{{"cwd":"%s","codexHome":"%s","arg1":"%s"}}' "$PWD" "$CODEX_HOME" "$1" > '{}'
exit 29
"#,
            output.display()
        ),
    );
    let cwd = root.path().join("work");
    let codex_home = root.path().join("profile-home");
    fs::create_dir(&cwd).unwrap();
    fs::create_dir(&codex_home).unwrap();

    let outcome = DirectCodexLauncher::run(CodexLaunchRequest {
        profile_id: "official-account".into(),
        route_kind: RouteKind::Direct,
        codex_home: codex_home.clone(),
        cwd: cwd.clone(),
        args: vec!["resume".into()],
        codex_executable: Some(codex),
    })
    .unwrap();

    assert_eq!(outcome.exit_code, 29);
    let captured: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(
        captured["cwd"],
        fs::canonicalize(&cwd).unwrap().to_string_lossy().as_ref()
    );
    assert_eq!(captured["codexHome"], codex_home.to_string_lossy().as_ref());
    assert_eq!(captured["arg1"], "resume");
}

#[test]
fn direct_launcher_rejects_gateway_routes() {
    let root = tempfile::tempdir().unwrap();
    let codex_home = root.path().join("profile-home");
    fs::create_dir(&codex_home).unwrap();

    let error = DirectCodexLauncher::run(CodexLaunchRequest {
        profile_id: "external-account".into(),
        route_kind: RouteKind::Gateway,
        codex_home,
        cwd: root.path().into(),
        args: Vec::new(),
        codex_executable: Some(root.path().join("codex")),
    })
    .unwrap_err();

    assert_eq!(error.code, "CODEX_DIRECT_ROUTE_REQUIRED");
}

#[test]
fn gateway_launcher_preserves_path_without_inheriting_unapproved_environment() {
    let root = tempfile::tempdir().unwrap();
    let (verified, output) = installation(root.path());
    let launcher = CodexLauncher::new(verified, Arc::new(FakeReadiness::default()));
    let home = root.path().join("home");
    fs::create_dir(&home).unwrap();

    launcher
        .run(CodexLaunchRequest {
            profile_id: "profile-a".into(),
            route_kind: RouteKind::Gateway,
            codex_home: home,
            cwd: root.path().into(),
            args: Vec::new(),
            codex_executable: None,
        })
        .unwrap();

    let captured: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(
        captured["path"].as_str(),
        std::env::var("PATH").ok().as_deref()
    );
    assert_eq!(captured["rustLog"], "");
}

#[cfg(target_os = "macos")]
#[test]
fn npm_codex_symlink_resolves_to_the_owner_controlled_native_binary() {
    let root = tempfile::tempdir().unwrap();
    let node_modules = root.path().join("node_modules");
    let wrapper = node_modules.join("@openai/codex/bin/codex.js");
    let command = root.path().join("bin/codex");
    let native =
        node_modules.join("@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex");
    fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
    fs::create_dir_all(command.parent().unwrap()).unwrap();
    fs::create_dir_all(native.parent().unwrap()).unwrap();
    executable(&wrapper, "#!/usr/bin/env node\n");
    executable(&native, "#!/bin/sh\nexit 0\n");
    symlink(&wrapper, &command).unwrap();

    assert_eq!(
        resolve_external_codex_executable(&command).unwrap(),
        fs::canonicalize(native).unwrap()
    );
}

#[test]
fn verified_gateway_launcher_rejects_direct_routes() {
    let root = tempfile::tempdir().unwrap();
    let (verified, output) = installation(root.path());
    let readiness = Arc::new(FakeReadiness::default());
    let launcher = CodexLauncher::new(verified, readiness.clone());
    let home = root.path().join("home");
    fs::create_dir(&home).unwrap();
    let error = launcher
        .run(CodexLaunchRequest {
            profile_id: "profile-a".into(),
            route_kind: RouteKind::Direct,
            codex_home: home,
            cwd: root.path().into(),
            args: vec![],
            codex_executable: None,
        })
        .unwrap_err();
    assert_eq!(error.code, "CODEX_GATEWAY_ROUTE_REQUIRED");
    assert_eq!(readiness.ensure_count.load(Ordering::SeqCst), 0);
    assert_eq!(readiness.shutdown_count.load(Ordering::SeqCst), 0);
    assert!(!output.exists());
}

#[test]
fn gateway_is_shutdown_when_codex_spawn_fails_after_readiness() {
    let root = tempfile::tempdir().unwrap();
    let (verified, _) = installation(root.path());
    let readiness = Arc::new(FakeReadiness::default());
    let codex = verified.component("codex").unwrap().path.clone();
    executable(&codex, "#!/definitely/missing\n");
    let home = root.path().join("home");
    fs::create_dir(&home).unwrap();
    let launcher = CodexLauncher::new(verified, readiness.clone());

    let error = launcher
        .run(CodexLaunchRequest {
            profile_id: "profile-a".into(),
            route_kind: RouteKind::Gateway,
            codex_home: home,
            cwd: root.path().into(),
            args: vec![],
            codex_executable: None,
        })
        .unwrap_err();

    assert_eq!(error.code, "CODEX_LAUNCH_FAILED");
    assert_eq!(readiness.ensure_count.load(Ordering::SeqCst), 1);
    assert_eq!(readiness.shutdown_count.load(Ordering::SeqCst), 1);
}
