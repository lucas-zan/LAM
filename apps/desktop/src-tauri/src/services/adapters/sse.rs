use super::nonstream::{
    map_usage, IncompleteDetails, ResponsesOutputContent, ResponsesOutputItem, ResponsesResponse,
    ResponsesStatus,
};
use super::protocol::{
    ChatChunk, ChatToolCallDelta, MAX_RESPONSE_BYTES, MAX_SSE_FRAME_BYTES, MAX_TOOL_ARGUMENT_BYTES,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamState {
    New,
    Open,
    ItemOpen,
    AwaitingDone,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamErrorCode {
    NotStarted,
    AlreadyStarted,
    AlreadyTerminal,
    MalformedSse,
    MalformedJson,
    FrameLimitExceeded,
    ProtocolMismatch,
    MissingDone,
    BackpressureOverflow,
    ResponseLimitExceeded,
    ToolArgumentsLimitExceeded,
    InvalidToolArguments,
    DuplicateToolCall,
    ReasoningLimitExceeded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamError {
    pub code: StreamErrorCode,
    pub message: String,
}

impl StreamError {
    fn new(code: StreamErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponsesStreamEvent {
    #[serde(rename = "response.created")]
    Created {
        sequence_number: u64,
        response: ResponsesResponse,
    },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        sequence_number: u64,
        response_id: String,
        output_index: usize,
        item: ResponsesOutputItem,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        sequence_number: u64,
        response_id: String,
        item_id: String,
        output_index: usize,
        content_index: usize,
        part: ResponsesOutputContent,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        sequence_number: u64,
        response_id: String,
        item_id: String,
        output_index: usize,
        content_index: usize,
        delta: String,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        sequence_number: u64,
        response_id: String,
        item_id: String,
        output_index: usize,
        content_index: usize,
        text: String,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        sequence_number: u64,
        response_id: String,
        item_id: String,
        output_index: usize,
        content_index: usize,
        part: ResponsesOutputContent,
    },
    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        sequence_number: u64,
        response_id: String,
        output_index: usize,
        item: ResponsesOutputItem,
    },
    #[serde(rename = "response.completed")]
    Completed {
        sequence_number: u64,
        response: ResponsesResponse,
    },
    #[serde(rename = "response.incomplete")]
    Incomplete {
        sequence_number: u64,
        response: ResponsesResponse,
    },
    #[serde(rename = "response.failed")]
    Failed {
        sequence_number: u64,
        response: ResponsesResponse,
    },
    #[serde(rename = "response.cancelled")]
    Cancelled {
        sequence_number: u64,
        response: ResponsesResponse,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        sequence_number: u64,
        response_id: String,
        item_id: String,
        output_index: usize,
        delta: String,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        sequence_number: u64,
        response_id: String,
        item_id: String,
        output_index: usize,
        name: String,
        arguments: String,
    },
}

impl ResponsesStreamEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Created { .. } => "response.created",
            Self::OutputItemAdded { .. } => "response.output_item.added",
            Self::ContentPartAdded { .. } => "response.content_part.added",
            Self::OutputTextDelta { .. } => "response.output_text.delta",
            Self::OutputTextDone { .. } => "response.output_text.done",
            Self::ContentPartDone { .. } => "response.content_part.done",
            Self::OutputItemDone { .. } => "response.output_item.done",
            Self::Completed { .. } => "response.completed",
            Self::Incomplete { .. } => "response.incomplete",
            Self::Failed { .. } => "response.failed",
            Self::Cancelled { .. } => "response.cancelled",
            Self::FunctionCallArgumentsDelta { .. } => "response.function_call_arguments.delta",
            Self::FunctionCallArgumentsDone { .. } => "response.function_call_arguments.done",
        }
    }

    pub fn response_id(&self) -> &str {
        match self {
            Self::Created { response, .. }
            | Self::Completed { response, .. }
            | Self::Incomplete { response, .. }
            | Self::Failed { response, .. }
            | Self::Cancelled { response, .. } => &response.id,
            Self::OutputItemAdded { response_id, .. }
            | Self::ContentPartAdded { response_id, .. }
            | Self::OutputTextDelta { response_id, .. }
            | Self::OutputTextDone { response_id, .. }
            | Self::ContentPartDone { response_id, .. }
            | Self::OutputItemDone { response_id, .. }
            | Self::FunctionCallArgumentsDelta { response_id, .. }
            | Self::FunctionCallArgumentsDone { response_id, .. } => response_id,
        }
    }

    pub fn text_delta(&self) -> Option<&str> {
        match self {
            Self::OutputTextDelta { delta, .. } => Some(delta),
            _ => None,
        }
    }
    pub fn completed_response(&self) -> Option<&ResponsesResponse> {
        match self {
            Self::Completed { response, .. } | Self::Incomplete { response, .. } => Some(response),
            _ => None,
        }
    }
    pub fn test_delta(response_id: &str, delta: &str) -> Self {
        Self::OutputTextDelta {
            sequence_number: 0,
            response_id: response_id.into(),
            item_id: "test-item".into(),
            output_index: 0,
            content_index: 0,
            delta: delta.into(),
        }
    }
}

pub struct BoundedEventQueue {
    events: VecDeque<ResponsesStreamEvent>,
    max_events: usize,
    max_bytes: usize,
    current_bytes: usize,
}

impl BoundedEventQueue {
    pub fn new(max_events: usize, max_bytes: usize) -> Self {
        Self {
            events: VecDeque::new(),
            max_events,
            max_bytes,
            current_bytes: 0,
        }
    }
    pub fn push(&mut self, event: ResponsesStreamEvent) -> Result<(), StreamError> {
        let bytes = serde_json::to_vec(&event)
            .map_err(|_| {
                StreamError::new(
                    StreamErrorCode::BackpressureOverflow,
                    "event cannot be measured",
                )
            })?
            .len();
        if self.events.len() >= self.max_events
            || self.current_bytes.saturating_add(bytes) > self.max_bytes
        {
            return Err(StreamError::new(
                StreamErrorCode::BackpressureOverflow,
                "bounded event queue is full",
            ));
        }
        self.current_bytes += bytes;
        self.events.push_back(event);
        Ok(())
    }
    pub fn pop(&mut self) -> Option<ResponsesStreamEvent> {
        let event = self.events.pop_front()?;
        self.current_bytes = self.current_bytes.saturating_sub(
            serde_json::to_vec(&event)
                .map(|value| value.len())
                .unwrap_or(0),
        );
        Some(event)
    }
}

struct SseFramer {
    buffer: Vec<u8>,
}

impl SseFramer {
    fn new() -> Self {
        Self { buffer: Vec::new() }
    }
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, StreamError> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();
        loop {
            let Some((position, delimiter)) = find_delimiter(&self.buffer) else {
                if self.buffer.len() > MAX_SSE_FRAME_BYTES {
                    return Err(StreamError::new(
                        StreamErrorCode::FrameLimitExceeded,
                        "SSE frame exceeds 256 KiB",
                    ));
                }
                break;
            };
            if position > MAX_SSE_FRAME_BYTES {
                return Err(StreamError::new(
                    StreamErrorCode::FrameLimitExceeded,
                    "SSE frame exceeds 256 KiB",
                ));
            }
            let frame = self.buffer.drain(..position).collect::<Vec<_>>();
            self.buffer.drain(..delimiter);
            frames.push(parse_frame(&frame)?);
        }
        Ok(frames)
    }
    fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

fn find_delimiter(bytes: &[u8]) -> Option<(usize, usize)> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4))
        .or_else(|| {
            bytes
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|position| (position, 2))
        })
}

fn parse_frame(bytes: &[u8]) -> Result<String, StreamError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        StreamError::new(
            StreamErrorCode::MalformedSse,
            "SSE frame is not valid UTF-8",
        )
    })?;
    let mut data = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => data.push(value),
            "event" | "id" | "retry" => {}
            _ => {
                return Err(StreamError::new(
                    StreamErrorCode::MalformedSse,
                    "unsupported SSE field",
                ))
            }
        }
    }
    if data.is_empty() {
        return Err(StreamError::new(
            StreamErrorCode::MalformedSse,
            "SSE frame has no data field",
        ));
    }
    Ok(data.join("\n"))
}

pub struct StreamingAdapter {
    response_id: String,
    model: String,
    created_at: i64,
    state: StreamState,
    sequence: u64,
    framer: SseFramer,
    text: String,
    item_id: String,
    output: Vec<ResponsesOutputItem>,
    usage: Option<super::nonstream::ResponsesUsage>,
    failures: Vec<ResponsesStreamEvent>,
    item_open: bool,
    tool_calls: BTreeMap<u32, ToolAccumulator>,
    tool_call_ids: BTreeSet<String>,
    reasoning: String,
    reasoning_limit: Option<usize>,
    response_limit: usize,
    terminal_status: ResponsesStatus,
    incomplete_reason: Option<String>,
}

struct ToolAccumulator {
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    output_index: usize,
}

impl StreamingAdapter {
    pub fn new(response_id: impl Into<String>, model: impl Into<String>, created_at: i64) -> Self {
        let response_id = response_id.into();
        Self {
            item_id: format!("msg-{response_id}-1"),
            response_id,
            model: model.into(),
            created_at,
            state: StreamState::New,
            sequence: 0,
            framer: SseFramer::new(),
            text: String::new(),
            output: Vec::new(),
            usage: None,
            failures: Vec::new(),
            item_open: false,
            tool_calls: BTreeMap::new(),
            tool_call_ids: BTreeSet::new(),
            reasoning: String::new(),
            reasoning_limit: None,
            response_limit: MAX_RESPONSE_BYTES,
            terminal_status: ResponsesStatus::Completed,
            incomplete_reason: None,
        }
    }
    pub fn state(&self) -> StreamState {
        self.state
    }
    pub fn failure_events(&self) -> &[ResponsesStreamEvent] {
        &self.failures
    }
    pub fn enable_reasoning_capture(&mut self, max_bytes: usize) {
        self.reasoning_limit = Some(max_bytes);
    }
    pub fn set_response_limit(&mut self, max_bytes: usize) {
        self.response_limit = max_bytes;
    }
    pub fn reasoning_content(&self) -> Option<&str> {
        (!self.reasoning.is_empty()).then_some(self.reasoning.as_str())
    }
    pub fn start(&mut self) -> Result<Vec<ResponsesStreamEvent>, StreamError> {
        if self.state != StreamState::New {
            return Err(StreamError::new(
                StreamErrorCode::AlreadyStarted,
                "stream already started",
            ));
        }
        self.state = StreamState::Open;
        let response = self.response(ResponsesStatus::InProgress);
        Ok(vec![self.event(|sequence| ResponsesStreamEvent::Created {
            sequence_number: sequence,
            response,
        })])
    }
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Result<Vec<ResponsesStreamEvent>, StreamError> {
        self.ensure_active()?;
        let frames = match self.framer.push(bytes) {
            Ok(frames) => frames,
            Err(error) => return self.fail(error),
        };
        let mut events = Vec::new();
        for frame in frames {
            if frame == "[DONE]" {
                events.extend(self.complete()?);
                continue;
            }
            let chunk: ChatChunk = match serde_json::from_str(&frame) {
                Ok(chunk) => chunk,
                Err(_) => {
                    return self.fail(StreamError::new(
                        StreamErrorCode::MalformedJson,
                        "SSE data is not a valid Chat Completions chunk",
                    ))
                }
            };
            if chunk.model != self.model {
                return self.fail(StreamError::new(
                    StreamErrorCode::ProtocolMismatch,
                    "upstream model changed during stream",
                ));
            }
            events.extend(self.accept_chunk(chunk)?);
        }
        Ok(events)
    }
    pub fn end_of_stream(&mut self) -> Result<(), StreamError> {
        if self.state == StreamState::Completed {
            return Ok(());
        }
        if matches!(self.state, StreamState::Failed | StreamState::Cancelled) {
            return Err(StreamError::new(
                StreamErrorCode::AlreadyTerminal,
                "stream already terminal",
            ));
        }
        if !self.framer.is_empty() || self.state != StreamState::Completed {
            return self.fail(StreamError::new(
                StreamErrorCode::MissingDone,
                "upstream closed before [DONE]",
            ));
        }
        Ok(())
    }
    pub fn cancel(&mut self) -> Result<ResponsesStreamEvent, StreamError> {
        self.ensure_active()?;
        self.state = StreamState::Cancelled;
        let response = self.response(ResponsesStatus::Cancelled);
        Ok(self.event(|sequence| ResponsesStreamEvent::Cancelled {
            sequence_number: sequence,
            response,
        }))
    }

    fn accept_chunk(&mut self, chunk: ChatChunk) -> Result<Vec<ResponsesStreamEvent>, StreamError> {
        if let Some(usage) = chunk.usage.as_ref() {
            if usage.prompt_tokens.saturating_add(usage.completion_tokens) != usage.total_tokens {
                return self.fail(StreamError::new(
                    StreamErrorCode::ProtocolMismatch,
                    "upstream usage totals are inconsistent",
                ));
            }
            self.usage = Some(map_usage(usage));
        }
        let mut events = Vec::new();
        for choice in chunk.choices {
            if choice.index != 0 {
                return self.fail(StreamError::new(
                    StreamErrorCode::ProtocolMismatch,
                    "only choice index zero is supported",
                ));
            }
            if let Some(reasoning) = choice.delta.reasoning_content {
                if !reasoning.is_empty() {
                    let Some(limit) = self.reasoning_limit else {
                        return self.fail(StreamError::new(
                            StreamErrorCode::ProtocolMismatch,
                            "reasoning_content is not enabled by the compatibility policy",
                        ));
                    };
                    if self.reasoning.len().saturating_add(reasoning.len()) > limit {
                        return self.fail(StreamError::new(
                            StreamErrorCode::ReasoningLimitExceeded,
                            "reasoning content exceeds the configured limit",
                        ));
                    }
                    self.reasoning.push_str(&reasoning);
                }
            }
            if let Some(content) = choice.delta.content {
                if !content.is_empty() {
                    if self.text.len().saturating_add(content.len()) > self.response_limit {
                        return self.fail(StreamError::new(
                            StreamErrorCode::ResponseLimitExceeded,
                            "streamed response exceeds the configured wire limit",
                        ));
                    }
                    events.extend(self.text_delta(content));
                }
            }
            for tool_delta in choice.delta.tool_calls {
                events.extend(self.accept_tool_delta(tool_delta)?);
            }
            if let Some(reason) = choice.finish_reason.as_deref() {
                if !matches!(reason, "stop" | "length" | "content_filter" | "tool_calls") {
                    return self.fail(StreamError::new(
                        StreamErrorCode::ProtocolMismatch,
                        "unknown stream finish reason",
                    ));
                }
                match reason {
                    "length" => {
                        self.terminal_status = ResponsesStatus::Incomplete;
                        self.incomplete_reason = Some("max_output_tokens".into());
                    }
                    "content_filter" => {
                        self.terminal_status = ResponsesStatus::Incomplete;
                        self.incomplete_reason = Some("content_filter".into());
                    }
                    _ => {}
                }
                if reason == "tool_calls" {
                    if self.item_open {
                        events.extend(self.finish_text_item());
                    }
                    events.extend(self.finish_tool_items()?);
                } else {
                    events.extend(self.finish_text_item());
                }
                self.state = StreamState::AwaitingDone;
            }
        }
        Ok(events)
    }

    fn accept_tool_delta(
        &mut self,
        delta: ChatToolCallDelta,
    ) -> Result<Vec<ResponsesStreamEvent>, StreamError> {
        let mut events = Vec::new();
        if !self.tool_calls.contains_key(&delta.index) {
            let call_id = delta
                .id
                .clone()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| {
                    StreamError::new(
                        StreamErrorCode::ProtocolMismatch,
                        "first tool delta requires an ID",
                    )
                })?;
            let name = delta
                .function
                .as_ref()
                .and_then(|function| function.name.clone())
                .filter(|name| !name.is_empty())
                .ok_or_else(|| {
                    StreamError::new(
                        StreamErrorCode::ProtocolMismatch,
                        "first tool delta requires a function name",
                    )
                })?;
            if !self.tool_call_ids.insert(call_id.clone()) {
                return self.fail(StreamError::new(
                    StreamErrorCode::DuplicateToolCall,
                    "duplicate streamed tool call ID",
                ));
            }
            let output_index = self.output.len() + self.tool_calls.len();
            let item_id = format!("fc-{}-{}", self.response_id, output_index + 1);
            let item = ResponsesOutputItem::FunctionCall {
                id: item_id.clone(),
                status: ResponsesStatus::InProgress,
                call_id: call_id.clone(),
                name: name.clone(),
                arguments: String::new(),
            };
            let response_id = self.response_id.clone();
            events.push(
                self.event(|sequence| ResponsesStreamEvent::OutputItemAdded {
                    sequence_number: sequence,
                    response_id,
                    output_index,
                    item,
                }),
            );
            self.tool_calls.insert(
                delta.index,
                ToolAccumulator {
                    item_id,
                    call_id,
                    name,
                    arguments: String::new(),
                    output_index,
                },
            );
        }
        let fragment = delta
            .function
            .and_then(|function| function.arguments)
            .unwrap_or_default();
        if !fragment.is_empty() {
            let accumulator = self.tool_calls.get_mut(&delta.index).unwrap();
            if accumulator.arguments.len().saturating_add(fragment.len()) > MAX_TOOL_ARGUMENT_BYTES
            {
                return self.fail(StreamError::new(
                    StreamErrorCode::ToolArgumentsLimitExceeded,
                    "streamed tool arguments exceed 1 MiB",
                ));
            }
            accumulator.arguments.push_str(&fragment);
            let response_id = self.response_id.clone();
            let item_id = accumulator.item_id.clone();
            let output_index = accumulator.output_index;
            events.push(self.event(
                |sequence| ResponsesStreamEvent::FunctionCallArgumentsDelta {
                    sequence_number: sequence,
                    response_id,
                    item_id,
                    output_index,
                    delta: fragment,
                },
            ));
        }
        Ok(events)
    }

    fn finish_tool_items(&mut self) -> Result<Vec<ResponsesStreamEvent>, StreamError> {
        if self.tool_calls.is_empty() {
            return self.fail(StreamError::new(
                StreamErrorCode::ProtocolMismatch,
                "tool_calls finish reason has no tool calls",
            ));
        }
        let calls = std::mem::take(&mut self.tool_calls);
        let mut events = Vec::new();
        for (_, call) in calls {
            if serde_json::from_str::<serde_json::Value>(&call.arguments).is_err() {
                return self.fail(StreamError::new(
                    StreamErrorCode::InvalidToolArguments,
                    "streamed tool arguments are not valid JSON",
                ));
            }
            let response_id = self.response_id.clone();
            let item_id = call.item_id.clone();
            let name = call.name.clone();
            let arguments = call.arguments.clone();
            events.push(
                self.event(|sequence| ResponsesStreamEvent::FunctionCallArgumentsDone {
                    sequence_number: sequence,
                    response_id,
                    item_id,
                    output_index: call.output_index,
                    name,
                    arguments,
                }),
            );
            let item = ResponsesOutputItem::FunctionCall {
                id: call.item_id,
                status: ResponsesStatus::Completed,
                call_id: call.call_id,
                name: call.name,
                arguments: call.arguments,
            };
            self.output.push(item.clone());
            let response_id = self.response_id.clone();
            events.push(self.event(|sequence| ResponsesStreamEvent::OutputItemDone {
                sequence_number: sequence,
                response_id,
                output_index: call.output_index,
                item,
            }));
        }
        Ok(events)
    }

    fn text_delta(&mut self, delta: String) -> Vec<ResponsesStreamEvent> {
        let mut events = Vec::new();
        if !self.item_open {
            self.item_open = true;
            self.state = StreamState::ItemOpen;
            let item = self.message_item(ResponsesStatus::InProgress, "");
            let response_id = self.response_id.clone();
            events.push(
                self.event(|sequence| ResponsesStreamEvent::OutputItemAdded {
                    sequence_number: sequence,
                    response_id,
                    output_index: 0,
                    item,
                }),
            );
            let part = output_text("");
            let response_id = self.response_id.clone();
            let item_id = self.item_id.clone();
            events.push(
                self.event(|sequence| ResponsesStreamEvent::ContentPartAdded {
                    sequence_number: sequence,
                    response_id,
                    item_id,
                    output_index: 0,
                    content_index: 0,
                    part,
                }),
            );
        }
        if !delta.is_empty() {
            self.text.push_str(&delta);
            let response_id = self.response_id.clone();
            let item_id = self.item_id.clone();
            events.push(
                self.event(|sequence| ResponsesStreamEvent::OutputTextDelta {
                    sequence_number: sequence,
                    response_id,
                    item_id,
                    output_index: 0,
                    content_index: 0,
                    delta,
                }),
            );
        }
        events
    }

    fn finish_text_item(&mut self) -> Vec<ResponsesStreamEvent> {
        let mut events = if self.item_open {
            Vec::new()
        } else {
            self.text_delta(String::new())
        };
        let text = self.text.clone();
        let response_id = self.response_id.clone();
        let item_id = self.item_id.clone();
        events.push(self.event(|sequence| ResponsesStreamEvent::OutputTextDone {
            sequence_number: sequence,
            response_id,
            item_id,
            output_index: 0,
            content_index: 0,
            text: text.clone(),
        }));
        let part = output_text(&text);
        let response_id = self.response_id.clone();
        let item_id = self.item_id.clone();
        events.push(
            self.event(|sequence| ResponsesStreamEvent::ContentPartDone {
                sequence_number: sequence,
                response_id,
                item_id,
                output_index: 0,
                content_index: 0,
                part,
            }),
        );
        let item = self.message_item(ResponsesStatus::Completed, &text);
        self.output.push(item.clone());
        let response_id = self.response_id.clone();
        events.push(self.event(|sequence| ResponsesStreamEvent::OutputItemDone {
            sequence_number: sequence,
            response_id,
            output_index: 0,
            item,
        }));
        self.item_open = false;
        events
    }

    fn complete(&mut self) -> Result<Vec<ResponsesStreamEvent>, StreamError> {
        self.ensure_active()?;
        if self.item_open || self.state == StreamState::Open {
            return self.fail(StreamError::new(
                StreamErrorCode::ProtocolMismatch,
                "[DONE] arrived before an upstream finish reason",
            ));
        }
        self.state = StreamState::Completed;
        let response = self.response(self.terminal_status.clone());
        if self.terminal_status == ResponsesStatus::Incomplete {
            Ok(vec![self.event(|sequence| {
                ResponsesStreamEvent::Incomplete {
                    sequence_number: sequence,
                    response,
                }
            })])
        } else {
            Ok(vec![self.event(|sequence| {
                ResponsesStreamEvent::Completed {
                    sequence_number: sequence,
                    response,
                }
            })])
        }
    }
    fn fail<T>(&mut self, error: StreamError) -> Result<T, StreamError> {
        if !matches!(
            self.state,
            StreamState::Failed | StreamState::Cancelled | StreamState::Completed
        ) {
            self.state = StreamState::Failed;
            let mut response = self.response(ResponsesStatus::Failed);
            response.error = Some(super::nonstream::ResponsesErrorBody {
                code: format!("{:?}", error.code),
                message: error.message.clone(),
            });
            let event = self.event(|sequence| ResponsesStreamEvent::Failed {
                sequence_number: sequence,
                response,
            });
            self.failures.push(event);
        }
        Err(error)
    }
    fn ensure_active(&self) -> Result<(), StreamError> {
        match self.state {
            StreamState::New => Err(StreamError::new(
                StreamErrorCode::NotStarted,
                "stream has not started",
            )),
            StreamState::Completed | StreamState::Failed | StreamState::Cancelled => Err(
                StreamError::new(StreamErrorCode::AlreadyTerminal, "stream already terminal"),
            ),
            _ => Ok(()),
        }
    }
    fn event(&mut self, make: impl FnOnce(u64) -> ResponsesStreamEvent) -> ResponsesStreamEvent {
        let sequence = self.sequence;
        self.sequence += 1;
        make(sequence)
    }
    fn message_item(&self, status: ResponsesStatus, text: &str) -> ResponsesOutputItem {
        ResponsesOutputItem::Message {
            id: self.item_id.clone(),
            status,
            role: "assistant".into(),
            content: if text.is_empty() {
                Vec::new()
            } else {
                vec![output_text(text)]
            },
        }
    }
    fn response(&self, status: ResponsesStatus) -> ResponsesResponse {
        ResponsesResponse {
            id: self.response_id.clone(),
            object: "response".into(),
            created_at: self.created_at,
            status,
            model: self.model.clone(),
            output: self.output.clone(),
            usage: self.usage.clone(),
            incomplete_details: self
                .incomplete_reason
                .as_ref()
                .map(|reason| IncompleteDetails {
                    reason: reason.clone(),
                }),
            error: None,
        }
    }
}

fn output_text(text: &str) -> ResponsesOutputContent {
    ResponsesOutputContent::OutputText {
        text: text.into(),
        annotations: Vec::new(),
    }
}
