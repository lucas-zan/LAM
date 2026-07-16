use localagentmanager_core::adapters::registry::*;
use std::sync::{Arc, Mutex};

struct FakeAdapter {
    descriptor: AdapterDescriptor,
    ids: Arc<Mutex<u64>>,
}

impl ProtocolAdapter for FakeAdapter {
    fn descriptor(&self) -> &AdapterDescriptor {
        &self.descriptor
    }

    fn begin_exchange(&self) -> Result<Box<dyn AdapterExchange>, AdapterRegistryError> {
        let mut ids = self.ids.lock().unwrap();
        *ids += 1;
        Ok(Box::new(LifecycleExchange::new(format!("resp-{ids}"), 42)))
    }
}

fn fake(id: &str) -> Arc<dyn ProtocolAdapter> {
    Arc::new(FakeAdapter {
        descriptor: AdapterDescriptor {
            id: id.into(),
            version: AdapterVersion::new(1, 0, 0),
            source: WireProtocol::Responses,
            target: WireProtocol::ChatCompletions,
            compatibility_policy: "generic-v1".into(),
        },
        ids: Arc::new(Mutex::new(0)),
    })
}

#[test]
fn registry_validates_identity_protocol_version_and_policy() {
    let mut registry = AdapterRegistry::new();
    registry.register(fake("responses-to-chat")).unwrap();
    assert_eq!(registry.len(), 1);
    let adapter = registry
        .resolve(&AdapterRequirement {
            id: "responses-to-chat".into(),
            minimum_version: AdapterVersion::new(1, 0, 0),
            source: WireProtocol::Responses,
            target: WireProtocol::ChatCompletions,
            compatibility_policy: "generic-v1".into(),
        })
        .unwrap();
    assert_eq!(adapter.descriptor().id, "responses-to-chat");

    assert_eq!(
        registry
            .register(fake("responses-to-chat"))
            .unwrap_err()
            .code,
        AdapterRegistryErrorCode::DuplicateAdapter
    );
    for requirement in [
        AdapterRequirement {
            id: "missing".into(),
            minimum_version: AdapterVersion::new(1, 0, 0),
            source: WireProtocol::Responses,
            target: WireProtocol::ChatCompletions,
            compatibility_policy: "generic-v1".into(),
        },
        AdapterRequirement {
            id: "responses-to-chat".into(),
            minimum_version: AdapterVersion::new(2, 0, 0),
            source: WireProtocol::Responses,
            target: WireProtocol::ChatCompletions,
            compatibility_policy: "generic-v1".into(),
        },
        AdapterRequirement {
            id: "responses-to-chat".into(),
            minimum_version: AdapterVersion::new(1, 0, 0),
            source: WireProtocol::ChatCompletions,
            target: WireProtocol::Responses,
            compatibility_policy: "generic-v1".into(),
        },
        AdapterRequirement {
            id: "responses-to-chat".into(),
            minimum_version: AdapterVersion::new(1, 0, 0),
            source: WireProtocol::Responses,
            target: WireProtocol::ChatCompletions,
            compatibility_policy: "deepseek-v1".into(),
        },
    ] {
        assert!(registry.resolve(&requirement).is_err());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_exchanges_do_not_share_ids_fragments_or_terminal_state() {
    let adapter = fake("responses-to-chat");
    let mut tasks = Vec::new();
    for label in ["alpha", "beta", "gamma"] {
        let adapter = adapter.clone();
        tasks.push(tokio::spawn(async move {
            let mut exchange = adapter.begin_exchange().unwrap();
            exchange.push_fragment(label).unwrap();
            exchange.finish().unwrap();
            (
                exchange.response_id().to_owned(),
                exchange.fragments().to_vec(),
                exchange.state(),
            )
        }));
    }
    let mut outputs = Vec::new();
    for task in tasks {
        outputs.push(task.await.unwrap());
    }
    outputs.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        outputs.iter().map(|value| &value.0).collect::<Vec<_>>(),
        ["resp-1", "resp-2", "resp-3"]
    );
    assert_eq!(outputs[0].1, ["alpha"]);
    assert_eq!(outputs[1].1, ["beta"]);
    assert_eq!(outputs[2].1, ["gamma"]);
    assert!(outputs
        .iter()
        .all(|value| value.2 == ExchangeState::Completed));
}

#[test]
fn exchange_lifecycle_has_one_terminal_and_rejects_post_terminal_work() {
    let mut completed = LifecycleExchange::new("resp-complete".into(), 7);
    completed.push_fragment("one").unwrap();
    completed.finish().unwrap();
    assert_eq!(
        completed.finish().unwrap_err().code,
        AdapterRegistryErrorCode::AlreadyTerminal
    );
    assert_eq!(
        completed.cancel().unwrap_err().code,
        AdapterRegistryErrorCode::AlreadyTerminal
    );
    assert_eq!(
        completed.push_fragment("two").unwrap_err().code,
        AdapterRegistryErrorCode::AlreadyTerminal
    );

    let mut cancelled = LifecycleExchange::new("resp-cancel".into(), 8);
    cancelled.cancel().unwrap();
    assert_eq!(cancelled.state(), ExchangeState::Cancelled);
}
