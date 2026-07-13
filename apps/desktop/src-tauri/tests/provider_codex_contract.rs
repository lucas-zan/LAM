use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

const SCENARIO_IDS: &[&str] = &[
    "auth-helper-empty",
    "auth-helper-trim",
    "auth-refresh-401",
    "disconnect",
    "function-tool",
    "malformed-sse",
    "resume",
    "retry-429",
    "retry-500",
    "route-inventory",
    "text-non-stream",
    "text-stream",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContractManifest {
    schema_version: u32,
    target: ContractTarget,
    support_policy: SupportPolicy,
    official_sources: Vec<String>,
    state_mode: String,
    required_route_set: Vec<String>,
    observed_route_set: Vec<String>,
    volatile_fields: Vec<String>,
    scenarios: Vec<Scenario>,
    artifacts: Vec<Artifact>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContractTarget {
    codex_version: String,
    platform: String,
    architecture: String,
    os_version: String,
    capture_date: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SupportPolicy {
    kind: String,
    versions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Scenario {
    id: String,
    status: String,
    runs: u32,
    deterministic: bool,
    evidence: String,
    artifacts: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Artifact {
    path: String,
    bytes: u64,
    fnv1a64: String,
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex-gateway-contract")
}

fn read_fixture(relative: &str) -> String {
    fs::read_to_string(fixture_root().join(relative))
        .unwrap_or_else(|error| panic!("failed to read fixture {relative}: {error}"))
}

fn read_json(relative: &str) -> Value {
    serde_json::from_str(&read_fixture(relative))
        .unwrap_or_else(|error| panic!("failed to parse fixture {relative}: {error}"))
}

fn manifest() -> ContractManifest {
    serde_json::from_str(&read_fixture("manifest.json")).expect("manifest must be valid JSON")
}

fn fnv1a64(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

fn string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap())
        .collect()
}

#[test]
fn manifest_pins_the_complete_exact_tested_contract() {
    let manifest = manifest();
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.target.codex_version, "0.144.1");
    assert_eq!(manifest.target.platform, "darwin");
    assert_eq!(manifest.target.architecture, "arm64");
    assert_eq!(manifest.target.os_version, "15.6");
    assert_eq!(manifest.target.capture_date, "2026-07-10");
    assert_eq!(manifest.support_policy.kind, "exact-tested");
    assert_eq!(manifest.support_policy.versions, ["0.144.1"]);
    assert!(manifest
        .official_sources
        .iter()
        .any(|source| { source.ends_with("/config-file/config-advanced#custom-model-providers") }));
    assert!(manifest
        .official_sources
        .iter()
        .any(|source| source.ends_with("/api/docs/guides/streaming-responses")));
    assert!(matches!(
        manifest.state_mode.as_str(),
        "full-input" | "previous-response-id" | "state-route" | "unsupported"
    ));
    assert!(!manifest.required_route_set.is_empty());
    assert!(!manifest.observed_route_set.is_empty());
    assert!(!manifest.volatile_fields.is_empty());

    let expected = SCENARIO_IDS.iter().copied().collect::<BTreeSet<_>>();
    let actual = manifest
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), manifest.scenarios.len());

    for scenario in manifest.scenarios {
        assert!(matches!(
            scenario.status.as_str(),
            "captured" | "unsupported"
        ));
        assert!(scenario.runs > 0);
        if scenario.deterministic && scenario.status == "captured" {
            assert!(scenario.runs >= 2);
        }
        assert!(!scenario.evidence.trim().is_empty());
        if scenario.status == "captured" {
            assert!(!scenario.artifacts.is_empty());
        }
    }
}

#[test]
fn artifacts_are_immutable_sanitized_and_manifest_owned() {
    let manifest = manifest();
    let declared = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.path.as_str())
        .collect::<BTreeSet<_>>();
    let referenced = manifest
        .scenarios
        .iter()
        .flat_map(|scenario| scenario.artifacts.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    assert_eq!(declared, referenced);
    assert_eq!(declared.len(), manifest.artifacts.len());

    for artifact in manifest.artifacts {
        assert!(Path::new(&artifact.path)
            .components()
            .all(|component| matches!(component, Component::Normal(_))));
        let bytes = fs::read(fixture_root().join(&artifact.path)).unwrap();
        assert_eq!(
            artifact.bytes,
            bytes.len() as u64,
            "length: {}",
            artifact.path
        );
        assert_eq!(artifact.fnv1a64, fnv1a64(&bytes), "hash: {}", artifact.path);

        let body = String::from_utf8_lossy(&bytes);
        for forbidden in [
            "FIXTURE_AUTH_TOKEN",
            "Bearer ",
            "sk-",
            "ghp_",
            "/Users/",
            "/home/",
            "C:\\Users\\",
            "zhanhd",
        ] {
            assert!(
                !body.contains(forbidden),
                "{} contains {forbidden}",
                artifact.path
            );
        }
        assert!(
            !body.contains('@'),
            "{} contains an email-like value",
            artifact.path
        );
    }
}

#[test]
fn normal_path_fixtures_define_request_stream_tool_and_resume_contracts() {
    let request = read_json("normal/text-stream-request.json");
    assert_eq!(request["method"], "POST");
    assert_eq!(request["path"], "/v1/responses");
    assert_eq!(request["headers"]["authorization"], "<redacted>");
    assert_eq!(request["body"]["model"], "fixture-model");
    assert_eq!(request["body"]["stream"], true);
    assert!(request["body"]["input"].as_array().is_some());
    assert!(request["body"]["tools"].as_array().is_some());

    let stream = read_json("normal/text-stream-events.json");
    let event_types = stream
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(event_types.first(), Some(&"response.created"));
    assert!(event_types.contains(&"response.output_text.delta"));
    assert_eq!(event_types.last(), Some(&"response.completed"));

    let tool_initial = read_json("normal/function-tool-initial-request.json");
    let tool_followup = read_json("normal/function-tool-followup-request.json");
    assert!(tool_initial["body"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| { tool["name"] == "exec_command" || tool["name"] == "shell" }));
    assert!(tool_followup["body"]["input"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| { item["type"] == "function_call_output" }));

    let resume = read_json("normal/resume-request.json");
    assert_eq!(resume["method"], "POST");
    assert_eq!(resume["body"]["model"], "fixture-model");
    assert!(
        resume["body"].get("previous_response_id").is_some()
            || resume["body"]["input"].as_array().is_some()
    );
}

#[test]
fn route_auth_retry_and_disconnect_observations_are_complete() {
    let manifest = manifest();
    let routes = read_json("observations/route-inventory.json");
    assert_eq!(
        string_array(&routes["observed"]),
        manifest.observed_route_set
    );
    assert!(string_array(&routes["observed"]).contains(&"GET /v1/models"));
    assert!(string_array(&routes["notObserved"]).contains(&"GET /v1/responses/{response_id}"));

    let auth = read_json("observations/auth-helper.json");
    assert_eq!(auth["trim"]["helperInvocations"], 2);
    assert_eq!(auth["trim"]["stdinBytes"], 0);
    assert_eq!(auth["trim"]["authorizationHeader"], "<redacted>");
    assert_eq!(auth["empty"]["authorizationHeader"], "<invalid>");
    assert_eq!(auth["empty"]["httpRequests"], 3);
    assert_eq!(auth["empty"]["outcome"], "success");
    assert_eq!(auth["empty"]["reportsEmptyToken"], true);
    assert_eq!(auth["refresh401"]["httpRequests"], 2);
    assert_eq!(auth["refresh401"]["outcome"], "success");

    let retries = read_json("observations/retries.json");
    assert_eq!(retries["status429"]["requestAttempts"], 1);
    assert_eq!(retries["status500"]["requestAttempts"], 30);
    assert_eq!(retries["malformedSse"]["requestAttempts"], 6);
    assert_eq!(retries["status429"]["outcome"], "codex-error");
    assert_eq!(retries["status500"]["outcome"], "codex-error");
    assert_eq!(retries["malformedSse"]["outcome"], "codex-error");

    let disconnect = read_json("observations/disconnect.json");
    assert_eq!(disconnect["clientCloseObserved"], true);
    assert_eq!(disconnect["codexExit"], "timeout-cancelled");
}
