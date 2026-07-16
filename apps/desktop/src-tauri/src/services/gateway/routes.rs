use super::catalog::build_codex_model_catalog;
use super::server::{
    GatewayHttpRequest, GatewayHttpResponse, GatewayRouteHandler, RequestUsageMetadata,
};
use super::upstream::{GatewayCancellation, SecureUpstreamClient, UpstreamRequest};
use crate::services::adapters::deepseek::DeepSeekCompatibilityPreset;
use crate::services::adapters::nonstream::{convert_nonstream_response, DeterministicContext};
use crate::services::adapters::protocol::{
    extract_responses_usage, parse_responses_passthrough, parse_responses_request,
    ChatCompletionResponse,
};
use crate::services::adapters::registry::{
    AdapterExchange, AdapterRegistry, AdapterRequirement, AdapterVersion,
    ResponsesToChatCompletionsAdapter, WireProtocol,
};
use crate::services::adapters::request::translate_responses_request;
use crate::services::adapters::sse::{ResponsesStreamEvent, StreamingAdapter};
use crate::services::error::{AppError, Result};
use crate::services::provider_v2::{AdapterConfig, ProviderProtocol};
use axum::body::{Body, Bytes};
use chrono::Utc;
use serde_json::json;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

enum Compatibility {
    Generic,
    DeepSeek(DeepSeekCompatibilityPreset),
}

struct BudgetedFrame {
    bytes: Bytes,
    _budget: OwnedSemaphorePermit,
}

impl Compatibility {
    fn policy(&self) -> &crate::services::adapters::request::CompatibilityPolicy {
        match self {
            Self::Generic => generic_compatibility_policy(),
            Self::DeepSeek(preset) => &preset.policy,
        }
    }

    fn captures_reasoning(&self) -> bool {
        matches!(self, Self::DeepSeek(preset) if preset.policy.requires_reasoning_for_tool_calls)
    }
}

fn generic_compatibility_policy() -> &'static crate::services::adapters::request::CompatibilityPolicy
{
    static POLICY: std::sync::OnceLock<crate::services::adapters::request::CompatibilityPolicy> =
        std::sync::OnceLock::new();
    POLICY.get_or_init(
        crate::services::adapters::request::CompatibilityPolicy::generic_openai_compatible,
    )
}

pub struct GatewayRouteComposer {
    upstream: Arc<SecureUpstreamClient>,
    adapters: Arc<AdapterRegistry>,
}

impl GatewayRouteComposer {
    pub fn new(upstream: Arc<SecureUpstreamClient>) -> Self {
        let mut adapters = AdapterRegistry::new();
        for policy in [
            "generic-openai-compatible-v1",
            "deepseek-chat-completions-v1",
        ] {
            adapters
                .register(Arc::new(ResponsesToChatCompletionsAdapter::new(policy)))
                .expect("built-in adapter descriptor must be valid");
        }
        Self {
            upstream,
            adapters: Arc::new(adapters),
        }
    }

    async fn responses(&self, request: GatewayHttpRequest) -> Result<GatewayHttpResponse> {
        match request.binding.provider.protocol {
            ProviderProtocol::ChatCompletions => self.adapted_responses(request).await,
            ProviderProtocol::Responses if request.binding.provider.codex.route_via_gateway => {
                self.passthrough_responses(request).await
            }
            ProviderProtocol::Responses => Ok(error_response(
                400,
                "GATEWAY_ROUTE_UNSUPPORTED",
                "Responses Provider is not configured for Gateway routing",
            )),
        }
    }

    async fn adapted_responses(&self, request: GatewayHttpRequest) -> Result<GatewayHttpResponse> {
        let parsed = match parse_responses_request(&request.body) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Ok(error_response(
                    400,
                    "ADAPTER_INVALID_REQUEST",
                    &error.message,
                ))
            }
        };
        if !request
            .binding
            .provider
            .models
            .iter()
            .any(|model| model.id == parsed.model)
        {
            return Ok(error_response(
                400,
                "ADAPTER_MODEL_NOT_ALLOWED",
                "requested model is not allowed by the Provider binding",
            ));
        }
        let requested_model = parsed.model.clone();
        let compatibility = match request.binding.provider.compatibility_profile.as_deref() {
            None | Some("openai_chat_completions") => Compatibility::Generic,
            Some("deepseek_chat_completions") if parsed.reasoning.is_some() => {
                if !parsed.tools.is_empty() {
                    return Ok(error_response(
                        400,
                        "ADAPTER_REASONING_HISTORY_UNREPRESENTABLE",
                        "Codex 0.144.1 full-history tool follow-up cannot carry required Provider reasoning metadata",
                    ));
                }
                Compatibility::DeepSeek(DeepSeekCompatibilityPreset::thinking_enabled())
            }
            Some("deepseek_chat_completions") => {
                Compatibility::DeepSeek(DeepSeekCompatibilityPreset::thinking_disabled())
            }
            Some(_) => {
                return Ok(error_response(
                    400,
                    "ADAPTER_COMPATIBILITY_PROFILE_UNSUPPORTED",
                    "approved compatibility profile is unavailable",
                ))
            }
        };
        let adapter_id = match &request.binding.provider.adapter {
            AdapterConfig::Local { adapter_id, .. } => adapter_id,
            _ => {
                return Ok(error_response(
                    400,
                    "ADAPTER_ROUTE_UNSUPPORTED",
                    "approved adapter is unavailable",
                ))
            }
        };
        let adapter = match self.adapters.resolve(&AdapterRequirement {
            id: adapter_id.clone(),
            minimum_version: AdapterVersion::new(1, 0, 0),
            source: WireProtocol::Responses,
            target: WireProtocol::ChatCompletions,
            compatibility_policy: compatibility.policy().id.clone(),
        }) {
            Ok(adapter) => adapter,
            Err(_) => {
                return Ok(error_response(
                    400,
                    "ADAPTER_REGISTRY_RESOLUTION_FAILED",
                    "approved adapter version or policy is unavailable",
                ))
            }
        };
        let mut exchange = match adapter.begin_exchange() {
            Ok(exchange) => exchange,
            Err(_) => {
                return Ok(error_response(
                    500,
                    "ADAPTER_EXCHANGE_FAILED",
                    "adapter exchange could not be started",
                ))
            }
        };
        let mut translated =
            match translate_responses_request(&parsed, &requested_model, compatibility.policy()) {
                Ok(translated) => translated,
                Err(error) => {
                    return Ok(error_response(
                        400,
                        request_error_code(error.code),
                        &error.message,
                    ))
                }
            };
        if let Compatibility::DeepSeek(preset) = &compatibility {
            if let Err(error) = preset.apply_request(&parsed, &mut translated) {
                return Ok(error_response(400, error.stable_code(), &error.message));
            }
        }
        exchange
            .push_fragment("request_translated")
            .map_err(|_| AppError::new("ADAPTER_EXCHANGE_FAILED", "adapter exchange failed"))?;
        let controlled_path = match &request.binding.provider.adapter {
            AdapterConfig::Local {
                adapter_id,
                upstream_path,
            } if adapter_id == "responses_to_chat_completions" => upstream_path.clone(),
            _ => {
                return Ok(error_response(
                    400,
                    "ADAPTER_ROUTE_UNSUPPORTED",
                    "approved adapter is unavailable",
                ))
            }
        };
        let body = serde_json::to_vec(&translated).map_err(|_| {
            AppError::new(
                "ADAPTER_SERIALIZATION_FAILED",
                "translated request could not be serialized",
            )
        })?;
        let cancellation = GatewayCancellation::new();
        let upstream_request = UpstreamRequest {
            base_url: request.binding.provider.base_url.clone(),
            controlled_path,
            auth: request.binding.provider.upstream_auth.clone(),
            body,
            content_type: "application/json".into(),
            cancellation: cancellation.clone(),
        };
        if parsed.stream {
            self.streaming(
                upstream_request,
                cancellation,
                exchange,
                compatibility.captures_reasoning(),
                requested_model,
            )
            .await
        } else {
            self.nonstream(upstream_request, exchange, requested_model)
                .await
        }
    }

    async fn passthrough_responses(
        &self,
        request: GatewayHttpRequest,
    ) -> Result<GatewayHttpResponse> {
        let parsed = match parse_responses_passthrough(&request.body) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Ok(error_response(
                    400,
                    "GATEWAY_INVALID_REQUEST",
                    &error.message,
                ))
            }
        };
        if !request
            .binding
            .provider
            .models
            .iter()
            .any(|model| model.id == parsed.model)
        {
            return Ok(error_response(
                400,
                "GATEWAY_MODEL_NOT_ALLOWED",
                "requested model is not allowed by the Provider binding",
            ));
        }
        if parsed.store || parsed.previous_response_id.is_some() {
            return Ok(error_response(
                400,
                "GATEWAY_UNSUPPORTED_FIELD",
                "Gateway Responses routing does not support server-side response state",
            ));
        }
        if parsed.input_empty {
            return Ok(error_response(
                400,
                "GATEWAY_INVALID_REQUEST",
                "Responses input must not be empty",
            ));
        }
        let stream = parsed.stream;
        let cancellation = GatewayCancellation::new();
        let upstream_request = UpstreamRequest {
            base_url: request.binding.provider.base_url,
            controlled_path: "/responses".into(),
            auth: request.binding.provider.upstream_auth,
            body: request.body,
            content_type: "application/json".into(),
            cancellation: cancellation.clone(),
        };
        if stream {
            self.passthrough_stream(upstream_request, cancellation)
                .await
        } else {
            self.passthrough_nonstream(upstream_request).await
        }
    }

    async fn passthrough_nonstream(
        &self,
        upstream_request: UpstreamRequest,
    ) -> Result<GatewayHttpResponse> {
        let response = match self.upstream.send(upstream_request).await {
            Ok(response) => response,
            Err(error) => return Ok(error_response(502, &error.code, &error.message)),
        };
        let content_type = response
            .content_type
            .unwrap_or_else(|| "application/octet-stream".into());
        let usage = if (200..300).contains(&response.status) {
            serde_json::from_slice::<serde_json::Value>(&response.body)
                .ok()
                .as_ref()
                .and_then(extract_responses_usage)
                .map(|usage| RequestUsageMetadata {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    total_tokens: usage.total_tokens,
                })
        } else {
            None
        };
        Ok(
            GatewayHttpResponse::from_body(
                response.status,
                content_type,
                Body::from(response.body),
            )
            .with_metrics(usage, response.attempts),
        )
    }

    async fn passthrough_stream(
        &self,
        upstream_request: UpstreamRequest,
        cancellation: GatewayCancellation,
    ) -> Result<GatewayHttpResponse> {
        let mut upstream = match self.upstream.open_stream(upstream_request).await {
            Ok(response) => response,
            Err(error) => return Ok(error_response(502, &error.code, &error.message)),
        };
        let content_type = upstream
            .content_type
            .clone()
            .unwrap_or_else(|| "application/octet-stream".into());
        if (200..300).contains(&upstream.status) && !content_type.starts_with("text/event-stream") {
            cancellation.cancel();
            return Ok(error_response(
                502,
                "GATEWAY_UPSTREAM_CONTENT_TYPE_INVALID",
                "upstream Responses stream content type is invalid",
            ));
        }
        let status = upstream.status;
        let attempts = upstream.attempts;
        let (sender, receiver) = mpsc::channel::<std::result::Result<BudgetedFrame, Infallible>>(
            crate::services::adapters::protocol::MAX_EVENT_CHANNEL_CAPACITY,
        );
        let byte_budget = Arc::new(Semaphore::new(
            crate::services::adapters::protocol::MAX_EVENT_CHANNEL_BYTES,
        ));
        tokio::spawn(async move {
            loop {
                match upstream.next_chunk().await {
                    Ok(Some(chunk)) => {
                        if send_passthrough_bytes(&sender, &byte_budget, &chunk)
                            .await
                            .is_err()
                        {
                            cancellation.cancel();
                            return;
                        }
                    }
                    Ok(None) => return,
                    Err(_) => {
                        cancellation.cancel();
                        return;
                    }
                }
            }
        });
        Ok(GatewayHttpResponse::from_body(
            status,
            content_type,
            Body::from_stream(
                ReceiverStream::new(receiver).map(|item| item.map(|frame| frame.bytes)),
            ),
        )
        .with_metrics(None, attempts))
    }

    async fn nonstream(
        &self,
        upstream_request: UpstreamRequest,
        mut exchange: Box<dyn AdapterExchange>,
        requested_model: String,
    ) -> Result<GatewayHttpResponse> {
        let response = match self.upstream.send(upstream_request).await {
            Ok(response) => response,
            Err(error) => {
                let _ = exchange.cancel();
                return Ok(error_response(502, &error.code, &error.message));
            }
        };
        let attempts = response.attempts;
        if !(200..300).contains(&response.status) {
            let _ = exchange.cancel();
            return Ok(error_response(
                response.status,
                "UPSTREAM_HTTP_ERROR",
                "upstream rejected the translated request",
            )
            .with_metrics(None, attempts));
        }
        let chat: ChatCompletionResponse = match serde_json::from_slice(&response.body) {
            Ok(chat) => chat,
            Err(_) => {
                let _ = exchange.cancel();
                return Ok(error_response(
                    502,
                    "ADAPTER_UPSTREAM_JSON_INVALID",
                    "upstream response was not valid Chat Completions JSON",
                ));
            }
        };
        let usage = chat.usage.as_ref().map(|usage| RequestUsageMetadata {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        });
        let response_id = format!("resp_{}", Uuid::new_v4().simple());
        let mut context = DeterministicContext::new(Utc::now().timestamp(), response_id);
        let converted = match convert_nonstream_response(&chat, &requested_model, &mut context) {
            Ok(converted) => converted,
            Err(error) => {
                let _ = exchange.cancel();
                return Ok(error_response(
                    502,
                    "ADAPTER_UPSTREAM_RESPONSE_INVALID",
                    &error.message,
                ));
            }
        };
        exchange
            .push_fragment("response_converted")
            .and_then(|_| exchange.finish())
            .map_err(|_| AppError::new("ADAPTER_EXCHANGE_FAILED", "adapter exchange failed"))?;
        Ok(GatewayHttpResponse::json(
            200,
            serde_json::to_value(converted).map_err(|_| {
                AppError::new(
                    "ADAPTER_SERIALIZATION_FAILED",
                    "Responses output could not be serialized",
                )
            })?,
        )
        .with_metrics(usage, attempts))
    }

    async fn streaming(
        &self,
        upstream_request: UpstreamRequest,
        cancellation: GatewayCancellation,
        mut exchange: Box<dyn AdapterExchange>,
        capture_reasoning: bool,
        requested_model: String,
    ) -> Result<GatewayHttpResponse> {
        let mut upstream = match self.upstream.open_stream(upstream_request).await {
            Ok(response) => response,
            Err(error) => {
                let _ = exchange.cancel();
                return Ok(error_response(502, &error.code, &error.message));
            }
        };
        let attempts = upstream.attempts;
        if !(200..300).contains(&upstream.status) {
            let _ = exchange.cancel();
            return Ok(error_response(
                upstream.status,
                "UPSTREAM_HTTP_ERROR",
                "upstream rejected the translated stream",
            )
            .with_metrics(None, attempts));
        }
        if !upstream
            .content_type
            .as_deref()
            .is_some_and(|value| value.starts_with("text/event-stream"))
        {
            let _ = exchange.cancel();
            return Ok(error_response(
                502,
                "ADAPTER_UPSTREAM_CONTENT_TYPE_INVALID",
                "upstream stream content type is invalid",
            ));
        }
        let (sender, receiver) = mpsc::channel::<std::result::Result<BudgetedFrame, Infallible>>(
            crate::services::adapters::protocol::MAX_EVENT_CHANNEL_CAPACITY,
        );
        let byte_budget = Arc::new(Semaphore::new(
            crate::services::adapters::protocol::MAX_EVENT_CHANNEL_BYTES,
        ));
        tokio::spawn(async move {
            let mut adapter = StreamingAdapter::new(
                format!("resp_{}", Uuid::new_v4().simple()),
                requested_model,
                Utc::now().timestamp(),
            );
            if capture_reasoning {
                adapter.enable_reasoning_capture(
                    crate::services::adapters::deepseek::MAX_REASONING_BYTES,
                );
            }
            let initial = match adapter.start() {
                Ok(events) => events,
                Err(_) => {
                    let _ = exchange.cancel();
                    return;
                }
            };
            if send_events(&sender, &byte_budget, &initial).await.is_err() {
                cancellation.cancel();
                let _ = exchange.cancel();
                return;
            }
            loop {
                match upstream.next_chunk().await {
                    Ok(Some(chunk)) => match adapter.push_bytes(&chunk) {
                        Ok(events) => {
                            if send_events(&sender, &byte_budget, &events).await.is_err() {
                                cancellation.cancel();
                                let _ = exchange.cancel();
                                return;
                            }
                            let _ = exchange.push_fragment("stream_events_converted");
                        }
                        Err(_) => {
                            let _ =
                                send_events(&sender, &byte_budget, adapter.failure_events()).await;
                            let _ = exchange.cancel();
                            return;
                        }
                    },
                    Ok(None) => {
                        if adapter.end_of_stream().is_err() {
                            let _ =
                                send_events(&sender, &byte_budget, adapter.failure_events()).await;
                            let _ = exchange.cancel();
                        } else {
                            let _ = exchange.finish();
                        }
                        return;
                    }
                    Err(_) => {
                        cancellation.cancel();
                        let _ = send_events(&sender, &byte_budget, adapter.failure_events()).await;
                        let _ = exchange.cancel();
                        return;
                    }
                }
            }
        });
        Ok(GatewayHttpResponse::from_body(
            200,
            "text/event-stream",
            Body::from_stream(
                ReceiverStream::new(receiver).map(|item| item.map(|frame| frame.bytes)),
            ),
        )
        .with_metrics(None, attempts))
    }

    fn models(&self, request: GatewayHttpRequest) -> GatewayHttpResponse {
        let catalog = match build_codex_model_catalog(&request.binding.provider.models) {
            Ok(catalog) => catalog,
            Err(error) => return error_response(500, &error.code, &error.message),
        };
        match serde_json::to_value(catalog) {
            Ok(body) => GatewayHttpResponse::json(200, body),
            Err(_) => error_response(
                500,
                "CODEX_MODEL_CATALOG_SERIALIZATION_FAILED",
                "Codex model catalog could not be serialized",
            ),
        }
    }
}

impl GatewayRouteHandler for GatewayRouteComposer {
    fn handle(
        &self,
        request: GatewayHttpRequest,
    ) -> Pin<Box<dyn Future<Output = Result<GatewayHttpResponse>> + Send + '_>> {
        Box::pin(async move {
            match request.path.as_str() {
                "/v1/models" => Ok(self.models(request)),
                "/v1/responses" => self.responses(request).await,
                _ => Ok(error_response(
                    404,
                    "GATEWAY_ROUTE_NOT_FOUND",
                    "Gateway route is not supported",
                )),
            }
        })
    }
}

async fn send_passthrough_bytes(
    sender: &mpsc::Sender<std::result::Result<BudgetedFrame, Infallible>>,
    byte_budget: &Arc<Semaphore>,
    bytes: &[u8],
) -> std::result::Result<(), ()> {
    for chunk in bytes.chunks(crate::services::adapters::protocol::MAX_EVENT_CHANNEL_BYTES) {
        let bytes = Bytes::copy_from_slice(chunk);
        let permits = u32::try_from(bytes.len()).map_err(|_| ())?;
        let budget = byte_budget
            .clone()
            .acquire_many_owned(permits)
            .await
            .map_err(|_| ())?;
        sender
            .send(Ok(BudgetedFrame {
                bytes,
                _budget: budget,
            }))
            .await
            .map_err(|_| ())?;
    }
    Ok(())
}

async fn send_events(
    sender: &mpsc::Sender<std::result::Result<BudgetedFrame, Infallible>>,
    byte_budget: &Arc<Semaphore>,
    events: &[ResponsesStreamEvent],
) -> std::result::Result<(), ()> {
    for event in events {
        let data = serde_json::to_string(event).map_err(|_| ())?;
        let frame = format!("event: {}\ndata: {data}\n\n", event.kind());
        let frame = Bytes::from(frame);
        let permits = u32::try_from(frame.len()).map_err(|_| ())?;
        if frame.len() > crate::services::adapters::protocol::MAX_EVENT_CHANNEL_BYTES {
            return Err(());
        }
        let budget = byte_budget
            .clone()
            .acquire_many_owned(permits)
            .await
            .map_err(|_| ())?;
        sender
            .send(Ok(BudgetedFrame {
                bytes: frame,
                _budget: budget,
            }))
            .await
            .map_err(|_| ())?;
    }
    Ok(())
}

fn error_response(status: u16, code: &str, message: &str) -> GatewayHttpResponse {
    GatewayHttpResponse::json(
        status,
        json!({
            "error": {
                "code": code,
                "message": message,
                "type": "gateway_error"
            }
        }),
    )
}

fn request_error_code(
    code: crate::services::adapters::request::AdapterRequestErrorCode,
) -> &'static str {
    use crate::services::adapters::request::AdapterRequestErrorCode::*;
    match code {
        ModelMismatch => "ADAPTER_MODEL_MISMATCH",
        ResponseStoreUnsupported => "ADAPTER_UNSUPPORTED_FIELD",
        UnsupportedInput => "ADAPTER_UNSUPPORTED_INPUT",
        UnsupportedTool => "ADAPTER_UNSUPPORTED_TOOL",
        UnsupportedParameter => "ADAPTER_UNSUPPORTED_PARAMETER",
        EmptyInput
        | InvalidRoleOrder
        | InvalidToolName
        | DuplicateCallId
        | UnknownCallId
        | ToolResultBeforeCall
        | ToolArgumentsInvalid
        | ToolArgumentsLimitExceeded
        | ReasoningHistoryRequired => "ADAPTER_INVALID_REQUEST",
    }
}
