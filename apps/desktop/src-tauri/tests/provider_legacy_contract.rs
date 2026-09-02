use localagentmanager_core::{
    list_providers, AttachProviderRequest, AttachProviderResult, CodexAccount,
    CreateProviderRequest, OperationPlan, ProviderProfile,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tempfile::TempDir;

const FIXTURE_IDS: &[&str] = &[
    "account.cache.current",
    "account.metadata.current",
    "config.auth-conflict-placeholder",
    "config.crlf-unicode",
    "config.legacy-flat",
    "config.official-provider",
    "config.top-level-only",
    "dto.attach-provider-request",
    "dto.attach-provider-result",
    "dto.create-provider-request-backend",
    "dto.create-provider-request",
    "dto.operation-plan",
    "dto.provider-profile-view",
    "provider-store.empty",
    "provider-store.future-version",
    "provider-store.legacy-openai",
    "provider-store.legacy-responses",
    "provider-store.malformed",
    "provider-store.missing",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureManifest {
    schema_version: u32,
    fixtures: Vec<FixtureEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureEntry {
    id: String,
    path: Option<String>,
    kind: String,
    origin: String,
    schema: String,
    expected_migration: String,
    write_back_allowed: bool,
    bytes: Option<u64>,
    fnv1a64: Option<String>,
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy-provider-contract")
}

fn fixture_path(relative: &str) -> PathBuf {
    fixture_root().join(relative)
}

fn read_fixture(relative: &str) -> String {
    fs::read_to_string(fixture_path(relative))
        .unwrap_or_else(|error| panic!("failed to read fixture {relative}: {error}"))
}

fn read_json(relative: &str) -> Value {
    serde_json::from_str(&read_fixture(relative))
        .unwrap_or_else(|error| panic!("failed to parse fixture {relative}: {error}"))
}

fn manifest() -> FixtureManifest {
    serde_json::from_str(&read_fixture("manifest.json")).expect("manifest must be valid JSON")
}

fn fnv1a64(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

fn assert_round_trip<T>(relative: &str)
where
    T: DeserializeOwned + Serialize,
{
    let value = read_json(relative);
    let contract: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(contract).unwrap(), value);
}

fn provider_home(relative: Option<&str>) -> TempDir {
    let home = tempfile::tempdir().unwrap();
    if let Some(relative) = relative {
        let store_dir = home.path().join(".lam/config");
        fs::create_dir_all(&store_dir).unwrap();
        fs::copy(fixture_path(relative), store_dir.join("providers.json")).unwrap();
    }
    home
}

#[test]
fn fixture_manifest_is_complete_immutable_and_sanitized() {
    let manifest = manifest();
    assert_eq!(manifest.schema_version, 1);

    let expected = FIXTURE_IDS.iter().copied().collect::<BTreeSet<_>>();
    let actual = manifest
        .fixtures
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    assert_eq!(
        actual.len(),
        manifest.fixtures.len(),
        "fixture IDs must be unique"
    );

    let mut paths = BTreeSet::new();
    for entry in manifest.fixtures {
        assert!(!entry.kind.trim().is_empty());
        assert!(!entry.origin.trim().is_empty());
        assert!(!entry.schema.trim().is_empty());
        assert!(!entry.expected_migration.trim().is_empty());
        assert!(
            !entry.write_back_allowed,
            "legacy reads must not write back"
        );

        let Some(relative) = entry.path else {
            assert_eq!(entry.id, "provider-store.missing");
            assert!(entry.bytes.is_none());
            assert!(entry.fnv1a64.is_none());
            continue;
        };
        assert!(
            paths.insert(relative.clone()),
            "fixture paths must be unique"
        );
        assert!(Path::new(&relative)
            .components()
            .all(|component| { matches!(component, Component::Normal(_)) }));

        let bytes = fs::read(fixture_path(&relative)).unwrap();
        assert_eq!(entry.bytes, Some(bytes.len() as u64), "length: {relative}");
        assert_eq!(entry.fnv1a64.as_deref(), Some(fnv1a64(&bytes).as_str()));

        let body = String::from_utf8_lossy(&bytes);
        for forbidden in [
            "sk-",
            "Bearer ",
            "ghp_",
            "/Users/",
            "/home/",
            "C:\\Users\\",
            "zhanhd",
        ] {
            assert!(!body.contains(forbidden), "{relative} contains {forbidden}");
        }
        assert!(
            !body.contains('@'),
            "{relative} must not contain an email address"
        );
    }
}

#[test]
fn legacy_provider_store_reads_are_frozen_and_side_effect_free() {
    for (relative, expected_id, expected_wire_api) in [
        (
            "provider-store/legacy-wire-openai.json",
            "legacy-chat-service",
            "openai",
        ),
        (
            "provider-store/legacy-wire-responses.json",
            "legacy-responses-service",
            "responses",
        ),
    ] {
        let home = provider_home(Some(relative));
        let path = home.path().join(".lam/config/providers.json");
        let before = fs::read(&path).unwrap();
        let modified_before = fs::metadata(&path).unwrap().modified().unwrap();

        let providers = list_providers(home.path()).unwrap();

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, expected_id);
        assert_eq!(providers[0].wire_api, expected_wire_api);
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            fs::metadata(&path).unwrap().modified().unwrap(),
            modified_before
        );
    }

    let empty = provider_home(Some("provider-store/empty.json"));
    assert!(list_providers(empty.path()).unwrap().is_empty());

    let missing = provider_home(None);
    let missing_path = missing.path().join(".lam/config/providers.json");
    assert!(list_providers(missing.path()).unwrap().is_empty());
    assert!(
        !missing_path.exists(),
        "a read must not create the missing store"
    );
}

#[test]
fn invalid_provider_store_inputs_keep_the_current_error_contract() {
    for relative in [
        "provider-store/malformed.json",
        "provider-store/future-version.json",
    ] {
        let home = provider_home(Some(relative));
        let error = list_providers(home.path()).unwrap_err();
        assert_eq!(error.code, "PROVIDER_STORE_INVALID", "fixture: {relative}");
    }
}

#[test]
fn legacy_config_fixtures_capture_parseable_and_raw_user_input() {
    for relative in [
        "config/top-level-only.toml",
        "config/official-provider.toml",
        "config/legacy-flat.toml",
        "config/auth-conflict-placeholder.toml",
    ] {
        read_fixture(relative)
            .parse::<toml::Value>()
            .unwrap_or_else(|error| panic!("invalid TOML fixture {relative}: {error}"));
    }

    let official = read_fixture("config/official-provider.toml");
    assert!(official.contains("[model_providers.\"company.proxy\"]"));
    assert!(official.contains("# preserve this trailing comment"));
    assert!(official.contains("[mcp_servers.fixture_tool]"));
    assert!(official.contains("示例兼容服务"));

    let legacy = read_fixture("config/legacy-flat.toml");
    assert!(legacy.contains("provider_base_url"));
    assert!(legacy.contains("provider_wire_api = \"openai\""));

    let crlf = read_json("config/crlf-unicode.json");
    let contents = crlf["contents"].as_str().unwrap();
    assert!(contents.contains("\r\n"));
    assert!(!contents.replace("\r\n", "").contains('\n'));
    assert!(contents.contains("示例兼容服务"));
}

#[test]
fn current_rust_dto_camel_case_shapes_round_trip_exactly() {
    assert_round_trip::<ProviderProfile>("dto/provider-profile-view.json");
    assert_round_trip::<CreateProviderRequest>("dto/create-provider-request-backend.json");
    assert_round_trip::<AttachProviderRequest>("dto/attach-provider-request.json");
    assert_round_trip::<AttachProviderResult>("dto/attach-provider-result.json");
    assert_round_trip::<OperationPlan>("dto/operation-plan.json");

    let cache = read_json("account/accounts-cache.json");
    let account: CodexAccount = serde_json::from_value(cache["accounts"][0].clone()).unwrap();
    assert_eq!(serde_json::to_value(account).unwrap(), cache["accounts"][0]);

    let metadata = read_json("account/managed-account-metadata.json");
    assert_eq!(metadata["managedBy"], "LAM");
    assert_eq!(metadata["accountName"], "fixture-profile");
}

#[test]
fn frontend_and_backend_env_secret_field_casing_drift_is_explicit() {
    let frontend_request = read_json("dto/create-provider-request.json");
    let error = serde_json::from_value::<CreateProviderRequest>(frontend_request).unwrap_err();
    assert!(error.to_string().contains("env_key"));

    let backend_request = read_json("dto/create-provider-request-backend.json");
    let request: CreateProviderRequest = serde_json::from_value(backend_request).unwrap();
    assert_eq!(
        serde_json::to_value(request).unwrap()["secret"]["env_key"],
        "EXAMPLE_RESPONSES_API_KEY"
    );
}
