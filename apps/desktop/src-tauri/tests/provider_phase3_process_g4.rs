#![cfg(target_os = "macos")]

use localagentmanager_core::gateway::binding::{GatewayBindingCollection, GatewayBindingService};
use localagentmanager_core::gateway::launcher::{InstallManifest, InstalledComponent};
use localagentmanager_core::gateway::sidecar::{GatewayRuntimeState, GatewayStateRepository};
use localagentmanager_core::provider_attach_transaction::{
    AttachJournalCollection, AttachTransactionCoordinator,
};
use localagentmanager_core::provider_binding::{ProfileBindingCollection, RouteKind};
use localagentmanager_core::provider_credentials::{CredentialSource, UpstreamAuth};
use localagentmanager_core::provider_keychain::{
    KeychainBackend, KeychainCredentialReference, KeychainCredentialService,
};
use localagentmanager_core::provider_planner::{
    plan_profile_attach, plan_provider_route, AdapterCatalog, AttachPlanContext, DryRunRegistry,
    GatewayPlanContext, RoutePlanInput,
};
use localagentmanager_core::provider_v2::{
    build_provider, AdapterConfig, CodexProviderOptions, ProviderCollection, ProviderInput,
    ProviderModel, ProviderProtocol,
};
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use localagentmanager_core::{AppError, Result, SecretValue};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct MemoryKeychain(Mutex<BTreeMap<String, String>>);

impl KeychainBackend for MemoryKeychain {
    fn write(&self, reference: &KeychainCredentialReference, secret: &SecretValue) -> Result<()> {
        self.0.lock().unwrap().insert(
            reference.account.clone(),
            secret.with_exposed(str::to_owned),
        );
        Ok(())
    }

    fn read(&self, reference: &KeychainCredentialReference) -> Result<SecretValue> {
        self.0
            .lock()
            .unwrap()
            .get(&reference.account)
            .cloned()
            .map(SecretValue::from_sensitive)
            .ok_or_else(|| AppError::new("KEYCHAIN_ITEM_NOT_FOUND", "missing"))
    }

    fn delete(&self, reference: &KeychainCredentialReference) -> Result<()> {
        self.0.lock().unwrap().remove(&reference.account);
        Ok(())
    }
}

#[test]
fn packaged_layout_runs_real_launcher_sidecar_helper_codex_and_upstream_processes() {
    let root = tempfile::Builder::new()
        .prefix("lam-g4-")
        .tempdir_in("/tmp")
        .unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let hub = root.path().join("provider-hub");
    let profile = root.path().join("profile");
    let install = root.path().join("install");
    let macos = install.join("MacOS");
    fs::create_dir_all(&hub).unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::create_dir_all(&macos).unwrap();
    for directory in [&hub, &profile, &install, &macos] {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
    }

    let node = canonical_program("node");
    let upstream_script = root.path().join("fake-upstream.mjs");
    let upstream_port_file = root.path().join("upstream.port");
    write_executable(
        &upstream_script,
        &format!(
            "#!{}\n{}",
            node.display(),
            r#"import http from 'node:http';
import fs from 'node:fs';
const portFile = process.argv[2];
const server = http.createServer((req, res) => {
  let wire = '';
  req.on('data', chunk => wire += chunk);
  req.on('end', () => {
    const body = JSON.parse(wire);
    if (body.stream) {
      res.writeHead(200, {'content-type':'text/event-stream'});
      res.end('data: {"id":"stream","object":"chat.completion.chunk","created":1,"model":"deepseek-chat","choices":[{"index":0,"delta":{"content":"stream-process-ok"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n');
      return;
    }
    const hasToolResult = body.messages.some(message => message.role === 'tool');
    const message = hasToolResult
      ? {role:'assistant', content:'tool-process-ok'}
      : body.tools?.length
        ? {role:'assistant', content:null, tool_calls:[{id:'call-process',type:'function',function:{name:'lookup',arguments:'{"q":"x"}'}}]}
        : {role:'assistant', content:'text-process-ok'};
    res.writeHead(200, {'content-type':'application/json'});
    res.end(JSON.stringify({id:'chat',object:'chat.completion',created:1,model:'deepseek-chat',choices:[{index:0,message,finish_reason:message.tool_calls?'tool_calls':'stop'}]}));
  });
});
server.listen(0, '127.0.0.1', () => fs.writeFileSync(portFile, String(server.address().port), {mode:0o600}));
"#
        ),
    );
    let mut upstream = Command::new(&node)
        .arg(&upstream_script)
        .arg(&upstream_port_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let upstream_port = wait_for_port_file(&upstream_port_file, &mut upstream);
    let upstream_addr = format!("127.0.0.1:{upstream_port}");

    let stable_port = reserve_private_port();
    let lock = InstallationLock::new(hub.join("provider-hub.lock"), Duration::from_secs(5));
    let state_store = VersionedFileStore::<GatewayRuntimeState>::new(
        hub.join("gateway-state.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    let state_repo = GatewayStateRepository::new(state_store);
    let _state = state_repo
        .initialize(0, stable_port, "0.2.1", 1, 1, "2026-07-14T00:00:00Z")
        .unwrap();
    let identity = [31_u8; 32];

    let binaries = [
        ("lam", env!("CARGO_BIN_EXE_lam")),
        (
            "lam-provider-gateway",
            env!("CARGO_BIN_EXE_lam-provider-gateway"),
        ),
        ("lam-auth-helper", env!("CARGO_BIN_EXE_lam-auth-helper")),
    ];
    let mut components = Vec::new();
    for (name, source) in binaries {
        let destination = macos.join(name);
        fs::copy(source, &destination).unwrap();
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).unwrap();
        let status = Command::new("/usr/bin/codesign")
            .args(["--force", "--sign", "-"])
            .arg(&destination)
            .status()
            .unwrap();
        assert!(status.success());
        components.push(InstalledComponent {
            name: match name {
                "lam" => "launcher",
                "lam-provider-gateway" => "gateway",
                _ => "auth-helper",
            }
            .into(),
            relative_path: format!("MacOS/{name}"),
            version: "0.2.1".into(),
            sha256: hex::encode(Sha256::digest(fs::read(&destination).unwrap())),
            protocol_version: 1,
            state_schema: 1,
            platform: "macos".into(),
            architecture: "aarch64".into(),
            package_identity: "adhoc".into(),
        });
    }
    let manifest_path = install.join("provider-gateway-install-manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec(&InstallManifest {
            schema_version: 1,
            components,
        })
        .unwrap(),
    )
    .unwrap();
    fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o600)).unwrap();

    let provider = build_provider(
        ProviderInput {
            id: "process-g4".into(),
            name: "Process G4".into(),
            protocol: ProviderProtocol::ChatCompletions,
            base_url: format!("http://127.0.0.1:{upstream_port}/v1"),
            default_model: "deepseek-chat".into(),
            models: vec![ProviderModel {
                id: "deepseek-chat".into(),
                label: "DeepSeek Chat".into(),
                capabilities: None,
            }],
            upstream_auth: UpstreamAuth::Bearer {
                source: CredentialSource::Env {
                    env_key: "PROCESS_G4_UPSTREAM_KEY".into(),
                },
            },
            adapter: AdapterConfig::Local {
                adapter_id: "responses_to_chat_completions".into(),
                upstream_path: "/chat/completions".into(),
            },
            compatibility_profile: Some("deepseek_chat_completions".into()),
            codex: CodexProviderOptions::default(),
        },
        "2026-07-14T00:00:00Z",
    )
    .unwrap();
    let provider_store = VersionedFileStore::<ProviderCollection>::new(
        hub.join("providers.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    provider_store
        .compare_and_swap(
            0,
            &ProviderCollection {
                providers: vec![provider.clone()],
            },
        )
        .unwrap();
    let binding_store = VersionedFileStore::<ProfileBindingCollection>::new(
        hub.join("bindings.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    let journal_store = VersionedFileStore::<AttachJournalCollection>::new(
        hub.join("attach-journal.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    let gateway = Arc::new(GatewayBindingService::new(
        VersionedFileStore::<GatewayBindingCollection>::new(
            hub.join("gateway-bindings.json"),
            lock.clone(),
            1,
            StoreOptions::default(),
        ),
        KeychainCredentialService::new(Arc::new(MemoryKeychain::default())),
    ));
    let config = profile.join("config.toml");
    fs::write(&config, "# process G4\n").unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let route = plan_provider_route(RoutePlanInput {
        provider,
        selected_model: "deepseek-chat".into(),
        adapters: AdapterCatalog::standard(),
    });
    assert_eq!(route.route_kind, RouteKind::Gateway);
    let plan = plan_profile_attach(
        route,
        AttachPlanContext {
            profile_id: "profile-process".into(),
            provider_store_revision: 1,
            config_path: config.to_string_lossy().into_owned(),
            expected_binding_revision: None,
            binding_store_revision: 0,
            source_config_hash: localagentmanager_core::provider_config_editor::config_hash(
                &fs::read(&config).unwrap(),
            ),
            binding_drifted: false,
            credential_ready: true,
            gateway: GatewayPlanContext {
                base_url: format!("http://127.0.0.1:{stable_port}/v1"),
                available: true,
                endpoint_version: 1,
            },
            planner_options: BTreeMap::new(),
            auth_helper_path: macos.join("lam-auth-helper").to_string_lossy().into_owned(),
            provider_hub_root: hub.to_string_lossy().into_owned(),
            gateway_binding_id: Some("10000000-0000-4000-8000-000000000004".into()),
        },
    );
    let coordinator = AttachTransactionCoordinator::new(
        lock,
        provider_store,
        binding_store,
        journal_store,
        gateway.clone(),
        1,
    );
    let mut registry = DryRunRegistry::new(60_000, 8);
    let ticket = registry.issue(&plan, 1_000);
    coordinator
        .execute_attach(&mut registry, &ticket, &plan, 1_001, None)
        .unwrap();
    let gateway_token = gateway
        .token_for_helper("profile-process", "10000000-0000-4000-8000-000000000004")
        .unwrap();

    let fake_codex = root.path().join("fake-codex.mjs");
    let process_output = root.path().join("process-output.json");
    write_executable(
        &fake_codex,
        &format!(
            "#!{}\n{}",
            node.display(),
            r#"import fs from 'node:fs';
import {spawnSync} from 'node:child_process';
const config = fs.readFileSync(`${process.env.CODEX_HOME}/config.toml`, 'utf8');
const base = config.match(/base_url = "([^"]+)"/)[1];
const command = config.match(/command = "([^"]+)"/)[1];
const argsBody = config.match(/args = \[([\s\S]*?)\]/)[1];
const args = [...argsBody.matchAll(/"((?:\\.|[^"])*)"/g)].map(match => JSON.parse(`"${match[1]}"`));
const auth = spawnSync(command, args, {encoding:'utf8', env:{LAM_TEST_GATEWAY_TOKEN:process.env.LAM_TEST_GATEWAY_TOKEN}});
if (auth.status !== 0) throw new Error(`auth failed ${auth.status}`);
const headers = {'authorization':`Bearer ${auth.stdout.trim()}`,'content-type':'application/json'};
const models = await fetch(`${base}/models`, {headers}).then(response => response.json());
const call = body => fetch(`${base}/responses`, {method:'POST',headers,body:JSON.stringify(body)}).then(async response => ({status:response.status,text:await response.text()}));
const text = await call({model:'deepseek-chat',input:'text',stream:false});
const stream = await call({model:'deepseek-chat',input:'stream',stream:true});
const tool = await call({model:'deepseek-chat',input:'tool',stream:false,tools:[{type:'function',name:'lookup',parameters:{type:'object'}}]});
const toolBody = JSON.parse(tool.text);
const followup = await call({model:'deepseek-chat',stream:false,input:[{type:'message',role:'user',content:[{type:'input_text',text:'tool'}]},{type:'function_call',call_id:'call-process',name:'lookup',arguments:'{"q":"x"}'},{type:'function_call_output',call_id:'call-process',output:'result'}]});
const resume = await call({model:'deepseek-chat',stream:false,input:[{type:'message',role:'user',content:[{type:'input_text',text:'old'}]},{type:'message',role:'assistant',content:[{type:'output_text',text:'prior'}]},{type:'message',role:'user',content:[{type:'input_text',text:'resume'}]}]});
fs.writeFileSync(process.argv[2], JSON.stringify({models,text,stream,tool:toolBody,followup,resume}), {mode:0o600});
"#
        ),
    );

    let status = Command::new(macos.join("lam"))
        .args(["codex", "--profile", "profile-process", "--"])
        .arg(&process_output)
        .env_clear()
        .env("HOME", root.path())
        .env("TMPDIR", "/tmp")
        .env("LAM_PROVIDER_HUB_ROOT", &hub)
        .env("LAM_INSTALL_ROOT", &install)
        .env("LAM_INSTALL_MANIFEST", &manifest_path)
        .env("LAM_CODEX_EXECUTABLE", &fake_codex)
        .env("LAM_GATEWAY_TEST_UPSTREAM_ADDR", &upstream_addr)
        .env("LAM_TEST_GATEWAY_TOKEN", &gateway_token)
        .env("LAM_TEST_INSTALL_IDENTITY_KEY", hex::encode(identity))
        .env("PROCESS_G4_UPSTREAM_KEY", "PROCESS_G4_SECRET")
        .status()
        .unwrap();
    assert!(status.success());
    let output: serde_json::Value =
        serde_json::from_slice(&fs::read(&process_output).unwrap()).unwrap();
    assert_eq!(output["models"]["models"][0]["slug"], "deepseek-chat");
    assert!(output["models"]["models"][0].get("id").is_none());
    assert!(output["text"]["text"]
        .as_str()
        .unwrap()
        .contains("text-process-ok"));
    assert!(output["stream"]["text"]
        .as_str()
        .unwrap()
        .contains("stream-process-ok"));
    assert_eq!(output["tool"]["output"][0]["call_id"], "call-process");
    assert!(output["followup"]["text"]
        .as_str()
        .unwrap()
        .contains("tool-process-ok"));
    assert!(output["resume"]["text"]
        .as_str()
        .unwrap()
        .contains("text-process-ok"));

    if let Some(pid) = state_repo.load().unwrap().value.process_id {
        unsafe { libc::kill(pid as i32, libc::SIGINT) };
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && state_repo.load().unwrap().value.process_id.is_some() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let _ = upstream.kill();
    let _ = upstream.wait();
    assert!(!read_text_tree(root.path()).contains("PROCESS_G4_SECRET"));
    assert!(!read_text_tree(root.path()).contains(&gateway_token));
}

fn canonical_program(name: &str) -> PathBuf {
    let output = Command::new("/usr/bin/which").arg(name).output().unwrap();
    assert!(output.status.success());
    fs::canonicalize(String::from_utf8(output.stdout).unwrap().trim()).unwrap()
}

fn write_executable(path: &Path, body: &str) {
    let mut file = fs::File::create(path).unwrap();
    file.write_all(body.as_bytes()).unwrap();
    file.sync_all().unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn wait_for_port_file(path: &Path, child: &mut Child) -> u16 {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(value) = fs::read_to_string(path) {
            return value.parse().unwrap();
        }
        assert!(child.try_wait().unwrap().is_none(), "fake upstream exited");
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("fake upstream did not publish a port");
}

fn reserve_private_port() -> u16 {
    loop {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        if port >= 49_152 {
            return port;
        }
    }
}

fn read_text_tree(root: &Path) -> String {
    let mut output = String::new();
    for entry in fs::read_dir(root).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            output.push_str(&read_text_tree(&path));
        } else if let Ok(value) = fs::read_to_string(path) {
            output.push_str(&value);
        }
    }
    output
}
