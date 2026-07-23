use super::catalog::{build_codex_model_catalog_with_defaults, CodexModelDefaultsCatalog};
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
use axum::http::StatusCode;
use chrono::Utc;
use serde_json::json;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
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

const MAX_RESPONSES_SSE_PENDING_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct ResponsesTerminalObserver {
    pending: Vec<u8>,
    terminal: bool,
}

impl ResponsesTerminalObserver {
    fn push(&mut self, bytes: &[u8]) {
        if self.terminal {
            return;
        }
        self.pending.extend_from_slice(bytes);
        while let Some((frame_end, separator_len)) = sse_frame_end(&self.pending) {
            let frame = self.pending[..frame_end].to_vec();
            self.pending.drain(..frame_end + separator_len);
            self.observe_frame(&frame);
        }
        if self.pending.len() > MAX_RESPONSES_SSE_PENDING_BYTES {
            self.pending.clear();
        }
    }

    fn observe_frame(&mut self, frame: &[u8]) {
        let frame = String::from_utf8_lossy(frame);
        let event = frame
            .lines()
            .find_map(|line| line.strip_prefix("event:").map(str::trim));
        let data_type = frame.lines().find_map(|line| {
            line.strip_prefix("data:")
                .map(str::trim)
                .and_then(|data| serde_json::from_str::<serde_json::Value>(data).ok())
                .and_then(|data| {
                    data.get("type")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
        });
        self.terminal = event.is_some_and(is_responses_terminal_event)
            || data_type
                .as_deref()
                .is_some_and(is_responses_terminal_event);
    }

    fn is_terminal(&self) -> bool {
        self.terminal
    }
}

fn sse_frame_end(bytes: &[u8]) -> Option<(usize, usize)> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| (index, 4))
        .or_else(|| {
            bytes
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|index| (index, 2))
        })
}

fn is_responses_terminal_event(event: &str) -> bool {
    matches!(
        event,
        "response.completed" | "response.failed" | "response.incomplete" | "error"
    )
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
    model_defaults: Arc<CodexModelDefaultsCatalog>,
}

impl GatewayRouteComposer {
    pub fn new(upstream: Arc<SecureUpstreamClient>) -> Self {
        Self::new_with_model_defaults(upstream, CodexModelDefaultsCatalog::default())
    }

    pub fn new_with_model_defaults(
        upstream: Arc<SecureUpstreamClient>,
        model_defaults: CodexModelDefaultsCatalog,
    ) -> Self {
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
            model_defaults: Arc::new(model_defaults),
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
            codex_headers: request.upstream_headers.clone(),
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
        let error_context = UpstreamErrorContext::new(
            &request.binding.provider.id,
            &request.binding.provider.base_url,
            &request.request_id,
        );
        let upstream_request = UpstreamRequest {
            base_url: request.binding.provider.base_url,
            controlled_path: "/responses".into(),
            auth: request.binding.provider.upstream_auth,
            body: request.body,
            content_type: "application/json".into(),
            codex_headers: request.upstream_headers,
            cancellation: cancellation.clone(),
        };
        if stream {
            self.passthrough_stream(upstream_request, cancellation, error_context)
                .await
        } else {
            self.passthrough_nonstream(upstream_request, error_context)
                .await
        }
    }

    async fn passthrough_nonstream(
        &self,
        upstream_request: UpstreamRequest,
        error_context: UpstreamErrorContext,
    ) -> Result<GatewayHttpResponse> {
        let response = match self.upstream.send(upstream_request).await {
            Ok(response) => response,
            Err(error) => return Ok(error_response(502, &error.code, &error.message)),
        };
        if !(200..300).contains(&response.status) {
            return Ok(upstream_http_error_response(
                response.status,
                response.content_type.as_deref(),
                &response.body,
                &error_context,
            )
            .with_retry_after(response.retry_after)
            .with_metrics(None, response.attempts)
            .with_upstream_metrics(response.status, response.first_byte_ms));
        }
        let content_type = response
            .content_type
            .unwrap_or_else(|| "application/octet-stream".into());
        let usage = serde_json::from_slice::<serde_json::Value>(&response.body)
            .ok()
            .as_ref()
            .and_then(extract_responses_usage)
            .map(|usage| RequestUsageMetadata {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                total_tokens: usage.total_tokens,
            });
        Ok(
            GatewayHttpResponse::from_body(
                response.status,
                content_type,
                Body::from(response.body),
            )
            .with_metrics(usage, response.attempts)
            .with_upstream_metrics(response.status, response.first_byte_ms),
        )
    }

    async fn passthrough_stream(
        &self,
        upstream_request: UpstreamRequest,
        cancellation: GatewayCancellation,
        error_context: UpstreamErrorContext,
    ) -> Result<GatewayHttpResponse> {
        let mut upstream = match self.upstream.open_stream(upstream_request).await {
            Ok(response) => response,
            Err(error) => return Ok(error_response(502, &error.code, &error.message)),
        };
        if !(200..300).contains(&upstream.status) {
            let status = upstream.status;
            let content_type = upstream.content_type.clone();
            let retry_after = upstream.retry_after.clone();
            let attempts = upstream.attempts;
            let first_byte_ms = upstream.first_byte_ms;
            let mut body = Vec::new();
            while let Some(chunk) = upstream.next_chunk().await? {
                body.extend_from_slice(&chunk);
            }
            return Ok(upstream_http_error_response(
                status,
                content_type.as_deref(),
                &body,
                &error_context,
            )
            .with_retry_after(retry_after)
            .with_metrics(None, attempts)
            .with_upstream_metrics(status, first_byte_ms));
        }
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
        let require_terminal_event = true;
        let attempts = upstream.attempts;
        let first_byte_ms = upstream.first_byte_ms;
        let (sender, receiver) = mpsc::channel::<std::result::Result<BudgetedFrame, Infallible>>(
            crate::services::adapters::protocol::MAX_EVENT_CHANNEL_CAPACITY,
        );
        let byte_budget = Arc::new(Semaphore::new(
            crate::services::adapters::protocol::MAX_EVENT_CHANNEL_BYTES,
        ));
        let downstream_cancellation = cancellation.clone();
        let (completion, completed) = oneshot::channel();
        tokio::spawn(async move {
            let _completion = completion;
            let mut terminal = ResponsesTerminalObserver::default();
            loop {
                match upstream.next_chunk().await {
                    Ok(Some(chunk)) => {
                        if require_terminal_event {
                            terminal.push(&chunk);
                        }
                        if send_passthrough_bytes(&sender, &byte_budget, &chunk)
                            .await
                            .is_err()
                        {
                            cancellation.cancel();
                            return;
                        }
                    }
                    Ok(None) => {
                        if require_terminal_event && !terminal.is_terminal() {
                            let error = responses_stream_error_frame(
                                "GATEWAY_UPSTREAM_STREAM_INCOMPLETE",
                                "upstream Responses stream ended before a terminal event",
                            );
                            let _ = send_passthrough_bytes(&sender, &byte_budget, &error).await;
                        }
                        return;
                    }
                    Err(error) => {
                        if require_terminal_event && !terminal.is_terminal() {
                            let frame = responses_stream_error_frame(
                                &error.code,
                                "upstream Responses stream failed before completion",
                            );
                            let _ = send_passthrough_bytes(&sender, &byte_budget, &frame).await;
                        }
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
        .with_metrics(None, attempts)
        .with_upstream_metrics(status, first_byte_ms)
        .with_stream_lifecycle(downstream_cancellation, completed)
        .streaming())
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
            .with_metrics(None, attempts)
            .with_upstream_metrics(response.status, response.first_byte_ms));
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
        .with_metrics(usage, attempts)
        .with_upstream_metrics(response.status, response.first_byte_ms))
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
        let upstream_status = upstream.status;
        let first_byte_ms = upstream.first_byte_ms;
        if !(200..300).contains(&upstream.status) {
            let _ = exchange.cancel();
            return Ok(error_response(
                upstream.status,
                "UPSTREAM_HTTP_ERROR",
                "upstream rejected the translated stream",
            )
            .with_metrics(None, attempts)
            .with_upstream_metrics(upstream_status, first_byte_ms));
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
        let downstream_cancellation = cancellation.clone();
        let (completion, completed) = oneshot::channel();
        tokio::spawn(async move {
            let _completion = completion;
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
        .with_metrics(None, attempts)
        .with_upstream_metrics(upstream_status, first_byte_ms)
        .with_stream_lifecycle(downstream_cancellation, completed)
        .streaming())
    }

    fn models(&self, request: GatewayHttpRequest) -> GatewayHttpResponse {
        let catalog = match build_codex_model_catalog_with_defaults(
            &request.binding.provider.models,
            &self.model_defaults,
        ) {
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

fn responses_stream_error_frame(code: &str, message: &str) -> Vec<u8> {
    let data = json!({
        "type": "error",
        "error": {
            "type": "gateway_error",
            "code": code,
            "message": message
        }
    });
    format!("event: error\ndata: {data}\n\n").into_bytes()
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
    .with_outcome_code(code)
}

const MAX_UPSTREAM_ERROR_DETAIL_CHARS: usize = 512;

struct UpstreamErrorContext {
    provider_id: String,
    upstream_host: String,
    request_id: String,
}

impl UpstreamErrorContext {
    fn new(provider_id: &str, base_url: &str, request_id: &str) -> Self {
        let upstream_host = url::Url::parse(base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| "configured upstream".into());
        Self {
            provider_id: provider_id.into(),
            upstream_host,
            request_id: request_id.into(),
        }
    }
}

fn upstream_http_error_response(
    status: u16,
    content_type: Option<&str>,
    body: &[u8],
    context: &UpstreamErrorContext,
) -> GatewayHttpResponse {
    let detail = upstream_error_detail(content_type, body);
    let message = upstream_error_message(status, &context.upstream_host, detail.as_deref());
    GatewayHttpResponse::json(
        status,
        json!({
            "error": {
                "type": "upstream_error",
                "source": "upstream",
                "code": upstream_error_code(status),
                "message": message,
                "upstreamStatus": status,
                "upstreamHost": context.upstream_host,
                "providerId": context.provider_id,
                "requestId": context.request_id,
                "retryable": upstream_status_is_retryable(status),
            }
        }),
    )
    .with_outcome_code(upstream_error_code(status))
}

fn upstream_error_detail(content_type: Option<&str>, body: &[u8]) -> Option<String> {
    let text = if content_type.is_some_and(|value| value.contains("json")) {
        json_error_message(body).unwrap_or_else(|| String::from_utf8_lossy(body).into_owned())
    } else {
        String::from_utf8_lossy(body).into_owned()
    };
    sanitize_upstream_detail(&text)
}

fn json_error_message(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .or_else(|| value.get("error"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn sanitize_upstream_detail(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    let mut bounded = normalized
        .chars()
        .take(MAX_UPSTREAM_ERROR_DETAIL_CHARS)
        .collect::<String>();
    if normalized.chars().count() > MAX_UPSTREAM_ERROR_DETAIL_CHARS {
        bounded.push('…');
    }
    Some(bounded)
}

fn upstream_error_message(status: u16, host: &str, detail: Option<&str>) -> String {
    let reason = StatusCode::from_u16(status)
        .ok()
        .and_then(|status| status.canonical_reason())
        .unwrap_or("Unknown Status");
    let prefix = format!("Remote API service {host} returned {status} {reason}");
    detail.map_or(prefix.clone(), |detail| format!("{prefix}: {detail}"))
}

fn upstream_error_code(status: u16) -> &'static str {
    match status {
        429 => "UPSTREAM_RATE_LIMITED",
        502 => "UPSTREAM_BAD_GATEWAY",
        503 => "UPSTREAM_SERVICE_UNAVAILABLE",
        504 => "UPSTREAM_GATEWAY_TIMEOUT",
        _ => "UPSTREAM_HTTP_ERROR",
    }
}

fn upstream_status_is_retryable(status: u16) -> bool {
    matches!(status, 429 | 502 | 503 | 504)
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

#[cfg(test)]
mod responses_terminal_tests {
    use super::ResponsesTerminalObserver;

    #[test]
    fn terminal_observer_handles_every_byte_boundary_and_data_type_fallback() {
        let wire = b"event: message\r\ndata: {\"type\":\"response.completed\"}\r\n\r\n";
        for split in 0..=wire.len() {
            let mut observer = ResponsesTerminalObserver::default();
            observer.push(&wire[..split]);
            observer.push(&wire[split..]);
            assert!(observer.is_terminal(), "split {split}");
        }
    }
}
