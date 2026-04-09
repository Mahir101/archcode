use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

use super::provider::{
    CompletionParams, CompletionResponse, ContentBlock, FinishReason, LlmProvider, Message,
    ProviderConfig, Role, TokenUsage, ToolCall,
};

pub struct AnthropicProvider {
    cfg: ProviderConfig,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(cfg: ProviderConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }

    fn base_url(&self) -> &str {
        if self.cfg.base_url.is_empty() {
            "https://api.anthropic.com/v1"
        } else {
            &self.cfg.base_url
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, params: CompletionParams) -> Result<CompletionResponse> {
        let url = format!("{}/messages", self.base_url());

        // Split system message from the rest
        let system_text: String = params
            .messages
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.text())
            .collect::<Vec<_>>()
            .join("\n");

        let messages_json: Vec<Value> = params
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                let role = match m.role {
                    Role::User | Role::Tool => "user",
                    _ => "assistant",
                };
                let content = if !m.tool_calls().is_empty() {
                    // Assistant turn with tool use
                    let mut blocks: Vec<Value> = vec![];
                    if !m.text().is_empty() {
                        blocks.push(json!({ "type": "text", "text": m.text() }));
                    }
                    for tc in m.tool_calls() {
                        let input: Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": input,
                        }));
                    }
                    json!(blocks)
                } else if m.role == Role::Tool {
                    // Tool result — Anthropic wraps in tool_result block
                    let id = m.tool_call_id.clone().unwrap_or_default();
                    json!([{
                        "type": "tool_result",
                        "tool_use_id": id,
                        "content": m.text(),
                    }])
                } else {
                    json!(m.text())
                };
                json!({ "role": role, "content": content })
            })
            .collect();

        let tools_json: Vec<Value> = params
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect();

        let mut body = json!({
            "model": params.model,
            "max_tokens": params.max_tokens.unwrap_or(8192),
            "messages": messages_json,
        });

        if !system_text.is_empty() {
            body["system"] = json!(system_text);
        }
        if !tools_json.is_empty() {
            body["tools"] = json!(tools_json);
        }
        if let Some(t) = params.temperature {
            body["temperature"] = json!(t);
        }

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.cfg.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("Anthropic request failed")?;

        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            anyhow::bail!("Anthropic API error {status}: {text}");
        }

        let json: Value = serde_json::from_str(&text)?;
        let finish_reason = match json["stop_reason"].as_str().unwrap_or("") {
            "end_turn" => FinishReason::Stop,
            "tool_use" => FinishReason::ToolCalls,
            "max_tokens" => FinishReason::Length,
            _ => FinishReason::Unknown,
        };

        let mut content_blocks = vec![];
        if let Some(blocks) = json["content"].as_array() {
            for block in blocks {
                match block["type"].as_str().unwrap_or("") {
                    "text" => {
                        let t = block["text"].as_str().unwrap_or("").to_string();
                        if !t.is_empty() {
                            content_blocks.push(ContentBlock::text(t));
                        }
                    }
                    "tool_use" => {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        let arguments = block["input"].to_string();
                        content_blocks.push(ContentBlock::tool_call(ToolCall {
                            id,
                            name,
                            arguments,
                        }));
                    }
                    _ => {}
                }
            }
        }

        let message = Message {
            role: Role::Assistant,
            content: content_blocks,
            tool_call_id: None,
        };

        // Parse token usage
        let usage = TokenUsage {
            input_tokens: json["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: json["usage"]["output_tokens"].as_u64().unwrap_or(0),
        };

        Ok(CompletionResponse {
            message,
            finish_reason,
            usage,
        })
    }

    fn model(&self) -> &str {
        &self.cfg.model
    }
}
