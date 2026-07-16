use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireProtocol {
    Responses,
    ChatCompletions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AdapterVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl AdapterVersion {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    fn satisfies(self, minimum: Self) -> bool {
        self.major == minimum.major && self >= minimum
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterDescriptor {
    pub id: String,
    pub version: AdapterVersion,
    pub source: WireProtocol,
    pub target: WireProtocol,
    pub compatibility_policy: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterRequirement {
    pub id: String,
    pub minimum_version: AdapterVersion,
    pub source: WireProtocol,
    pub target: WireProtocol,
    pub compatibility_policy: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterRegistryErrorCode {
    InvalidAdapter,
    DuplicateAdapter,
    AdapterNotFound,
    VersionMismatch,
    ProtocolMismatch,
    PolicyMismatch,
    AlreadyTerminal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterRegistryError {
    pub code: AdapterRegistryErrorCode,
    pub message: String,
}

impl AdapterRegistryError {
    fn new(code: AdapterRegistryErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub trait ProtocolAdapter: Send + Sync {
    fn descriptor(&self) -> &AdapterDescriptor;
    fn begin_exchange(&self) -> Result<Box<dyn AdapterExchange>, AdapterRegistryError>;
}

pub trait AdapterExchange: Send {
    fn response_id(&self) -> &str;
    fn created_at(&self) -> i64;
    fn state(&self) -> ExchangeState;
    fn fragments(&self) -> &[String];
    fn push_fragment(&mut self, fragment: &str) -> Result<(), AdapterRegistryError>;
    fn finish(&mut self) -> Result<(), AdapterRegistryError>;
    fn cancel(&mut self) -> Result<(), AdapterRegistryError>;
}

pub struct ResponsesToChatCompletionsAdapter {
    descriptor: AdapterDescriptor,
    next_exchange_id: AtomicU64,
}

impl ResponsesToChatCompletionsAdapter {
    pub fn new(compatibility_policy: impl Into<String>) -> Self {
        Self {
            descriptor: AdapterDescriptor {
                id: "responses_to_chat_completions".into(),
                version: AdapterVersion::new(1, 0, 0),
                source: WireProtocol::Responses,
                target: WireProtocol::ChatCompletions,
                compatibility_policy: compatibility_policy.into(),
            },
            next_exchange_id: AtomicU64::new(1),
        }
    }
}

impl ProtocolAdapter for ResponsesToChatCompletionsAdapter {
    fn descriptor(&self) -> &AdapterDescriptor {
        &self.descriptor
    }

    fn begin_exchange(&self) -> Result<Box<dyn AdapterExchange>, AdapterRegistryError> {
        let sequence = self.next_exchange_id.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(LifecycleExchange::new(
            format!("exchange-{sequence}"),
            chrono::Utc::now().timestamp(),
        )))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExchangeState {
    Open,
    Completed,
    Cancelled,
    Failed,
}

pub struct LifecycleExchange {
    response_id: String,
    created_at: i64,
    state: ExchangeState,
    fragments: Vec<String>,
}

impl LifecycleExchange {
    pub fn new(response_id: String, created_at: i64) -> Self {
        Self {
            response_id,
            created_at,
            state: ExchangeState::Open,
            fragments: Vec::new(),
        }
    }

    fn ensure_open(&self) -> Result<(), AdapterRegistryError> {
        if self.state == ExchangeState::Open {
            Ok(())
        } else {
            Err(AdapterRegistryError::new(
                AdapterRegistryErrorCode::AlreadyTerminal,
                "exchange already reached a terminal state",
            ))
        }
    }

    pub fn response_id(&self) -> &str {
        &self.response_id
    }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
    pub fn state(&self) -> ExchangeState {
        self.state
    }
    pub fn fragments(&self) -> &[String] {
        &self.fragments
    }
    pub fn push_fragment(&mut self, fragment: &str) -> Result<(), AdapterRegistryError> {
        <Self as AdapterExchange>::push_fragment(self, fragment)
    }
    pub fn finish(&mut self) -> Result<(), AdapterRegistryError> {
        <Self as AdapterExchange>::finish(self)
    }
    pub fn cancel(&mut self) -> Result<(), AdapterRegistryError> {
        <Self as AdapterExchange>::cancel(self)
    }
}

impl AdapterExchange for LifecycleExchange {
    fn response_id(&self) -> &str {
        &self.response_id
    }
    fn created_at(&self) -> i64 {
        self.created_at
    }
    fn state(&self) -> ExchangeState {
        self.state
    }
    fn fragments(&self) -> &[String] {
        &self.fragments
    }

    fn push_fragment(&mut self, fragment: &str) -> Result<(), AdapterRegistryError> {
        self.ensure_open()?;
        self.fragments.push(fragment.to_owned());
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AdapterRegistryError> {
        self.ensure_open()?;
        self.state = ExchangeState::Completed;
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AdapterRegistryError> {
        self.ensure_open()?;
        self.state = ExchangeState::Cancelled;
        Ok(())
    }
}

#[derive(Default)]
pub struct AdapterRegistry {
    adapters: BTreeMap<(String, String), Arc<dyn ProtocolAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.adapters.len()
    }
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    pub fn register(
        &mut self,
        adapter: Arc<dyn ProtocolAdapter>,
    ) -> Result<(), AdapterRegistryError> {
        let descriptor = adapter.descriptor();
        validate_descriptor(descriptor)?;
        let key = (
            descriptor.id.clone(),
            descriptor.compatibility_policy.clone(),
        );
        if self.adapters.contains_key(&key) {
            return Err(AdapterRegistryError::new(
                AdapterRegistryErrorCode::DuplicateAdapter,
                "adapter ID is already registered",
            ));
        }
        self.adapters.insert(key, adapter);
        Ok(())
    }

    pub fn resolve(
        &self,
        requirement: &AdapterRequirement,
    ) -> Result<Arc<dyn ProtocolAdapter>, AdapterRegistryError> {
        let key = (
            requirement.id.clone(),
            requirement.compatibility_policy.clone(),
        );
        let adapter = self.adapters.get(&key).cloned().ok_or_else(|| {
            AdapterRegistryError::new(
                AdapterRegistryErrorCode::AdapterNotFound,
                "adapter is not registered",
            )
        })?;
        let descriptor = adapter.descriptor();
        if !descriptor.version.satisfies(requirement.minimum_version) {
            return Err(AdapterRegistryError::new(
                AdapterRegistryErrorCode::VersionMismatch,
                "adapter version is incompatible",
            ));
        }
        if descriptor.source != requirement.source || descriptor.target != requirement.target {
            return Err(AdapterRegistryError::new(
                AdapterRegistryErrorCode::ProtocolMismatch,
                "adapter protocol pair does not match",
            ));
        }
        if descriptor.compatibility_policy != requirement.compatibility_policy {
            return Err(AdapterRegistryError::new(
                AdapterRegistryErrorCode::PolicyMismatch,
                "compatibility policy does not match",
            ));
        }
        Ok(adapter)
    }
}

fn validate_descriptor(descriptor: &AdapterDescriptor) -> Result<(), AdapterRegistryError> {
    let valid_id = !descriptor.id.is_empty()
        && descriptor.id.len() <= 64
        && descriptor
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if !valid_id || descriptor.compatibility_policy.trim().is_empty() {
        return Err(AdapterRegistryError::new(
            AdapterRegistryErrorCode::InvalidAdapter,
            "invalid adapter descriptor",
        ));
    }
    Ok(())
}
