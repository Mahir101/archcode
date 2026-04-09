use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

/// Events emitted during streaming LLM responses.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StreamEvent {
    /// A chunk of text content from the assistant.
    TextDelta(String),
    /// A tool call is starting (id + name known).
    ToolCallStart { id: String, name: String },
    /// Additional arguments chunk for a tool call being streamed.
    ToolCallDelta { index: usize, arguments: String },
    /// The LLM has finished generating.
    Done,
}

pub type StreamSender = mpsc::UnboundedSender<StreamEvent>;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Backend {
    OpenAI,
    Anthropic,
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub model: String,
    pub api_key: String,
    pub base_url: String,
    pub backend: Backend,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ContentType {
    Text,
    ToolCall,
    ToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String, // JSON string
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub tool_call_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
    pub tool_call: Option<ToolCall>,
    pub tool_result: Option<ToolCallResult>,
}

impl ContentBlock {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            content_type: "text".into(),
            text: Some(s.into()),
            tool_call: None,
            tool_result: None,
        }
    }

    pub fn tool_call(tc: ToolCall) -> Self {
        Self {
            content_type: "tool_call".into(),
            text: None,
            tool_call: Some(tc),
            tool_result: None,
        }
    }

    pub fn tool_result(tr: ToolCallResult) -> Self {
        Self {
            content_type: "tool_result".into(),
            text: None,
            tool_call: None,
            tool_result: Some(tr),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: vec![ContentBlock::text(text)],
            tool_call_id: None,
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::text(text)],
            tool_call_id: None,
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::text(text)],
            tool_call_id: None,
        }
    }

    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| {
                b.text
                    .as_deref()
                    .or_else(|| b.tool_result.as_ref().map(|tr| tr.content.as_str()))
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn tool_calls(&self) -> Vec<&ToolCall> {
        self.content
            .iter()
            .filter_map(|b| b.tool_call.as_ref())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct CompletionParams {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub message: Message,
    pub finish_reason: FinishReason,
    pub usage: TokenUsage,
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait LlmProvider {
    async fn complete(&self, params: CompletionParams) -> Result<CompletionResponse>;
    fn model(&self) -> &str;

    /// Streaming completion — sends deltas through `tx` while accumulating the full response.
    /// Does NOT send `StreamEvent::Done` — the caller is responsible for that.
    /// Default implementation falls back to non-streaming `complete()`.
    async fn stream_complete(
        &self,
        params: CompletionParams,
        tx: StreamSender,
    ) -> Result<CompletionResponse> {
        let resp = self.complete(params).await?;
        let text = resp.message.text();
        if !text.is_empty() {
            let _ = tx.send(StreamEvent::TextDelta(text));
        }
        Ok(resp)
    }
}
