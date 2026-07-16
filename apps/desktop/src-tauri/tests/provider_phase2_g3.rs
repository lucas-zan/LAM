use localagentmanager_core::adapters::deepseek::MAX_REASONING_BYTES;
use localagentmanager_core::adapters::protocol::{
    MAX_EVENT_CHANNEL_BYTES, MAX_EVENT_CHANNEL_CAPACITY, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES,
    MAX_SSE_FRAME_BYTES, MAX_TOOLS, MAX_TOOL_ARGUMENT_BYTES,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    adapter_id: String,
    adapter_version: String,
    compatibility_profiles: Vec<String>,
    official_sources: Vec<String>,
    limits: Limits,
    artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    request_bytes: usize,
    response_bytes: usize,
    sse_frame_bytes: usize,
    tool_arguments_bytes: usize,
    reasoning_bytes: usize,
    event_capacity: usize,
    event_bytes: usize,
    max_tools: usize,
}

#[derive(Deserialize)]
struct Artifact {
    path: String,
    sha256: String,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/provider-adapter")
}

#[test]
fn phase2_fixture_manifest_pins_sources_profiles_limits_and_checksums() {
    let manifest: Manifest =
        serde_json::from_str(&fs::read_to_string(root().join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.adapter_id, "responses-to-chat-completions");
    assert_eq!(manifest.adapter_version, "1.0.0");
    assert_eq!(
        manifest.compatibility_profiles,
        [
            "generic-openai-compatible-v1",
            "deepseek-chat-completions-v1"
        ]
    );
    assert!(
        manifest
            .official_sources
            .iter()
            .any(|source| source
                .contains("developers.openai.com/api/docs/guides/streaming-responses"))
    );
    assert!(manifest
        .official_sources
        .iter()
        .any(|source| source.contains("api-docs.deepseek.com/guides/thinking_mode")));
    assert_eq!(manifest.limits.request_bytes, MAX_REQUEST_BYTES);
    assert_eq!(manifest.limits.response_bytes, MAX_RESPONSE_BYTES);
    assert_eq!(manifest.limits.sse_frame_bytes, MAX_SSE_FRAME_BYTES);
    assert_eq!(
        manifest.limits.tool_arguments_bytes,
        MAX_TOOL_ARGUMENT_BYTES
    );
    assert_eq!(manifest.limits.reasoning_bytes, MAX_REASONING_BYTES);
    assert_eq!(manifest.limits.event_capacity, MAX_EVENT_CHANNEL_CAPACITY);
    assert_eq!(manifest.limits.event_bytes, MAX_EVENT_CHANNEL_BYTES);
    assert_eq!(manifest.limits.max_tools, MAX_TOOLS);
    assert_eq!(manifest.artifacts.len(), 2);
    for artifact in manifest.artifacts {
        let bytes = fs::read(root().join(&artifact.path)).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), artifact.sha256);
        let text = String::from_utf8(bytes).unwrap();
        for forbidden in ["sk-", "Bearer ", "authorization", "api_key", "PRIVATE KEY"] {
            assert!(
                !text.contains(forbidden),
                "{} contains {forbidden}",
                artifact.path
            );
        }
    }
}

#[test]
fn phase2_gate_has_no_http_gateway_or_ui_fixture_dependency() {
    let manifest = fs::read_to_string(root().join("manifest.json")).unwrap();
    assert!(!manifest.contains("127.0.0.1"));
    assert!(!manifest.contains("reqwest"));
    assert!(!manifest.contains("GatewayBinding"));
    assert!(!manifest.contains("weekly"));
}
