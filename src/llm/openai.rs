use anyhow::{Context, Result};
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::{json, Value};
use uuid::Uuid;

use super::provider::{
    CompletionParams, CompletionResponse, ContentBlock, FinishReason, LlmProvider, Message,
    ProviderConfig, Role, StreamEvent, StreamSender, TokenUsage, ToolCall,
};

pub struct OpenAIProvider {
    cfg: ProviderConfig,
    client: reqwest::Client,
}

impl OpenAIProvider {
    pub fn new(cfg: ProviderConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }

    fn base_url(&self) -> &str {
        if self.cfg.base_url.is_empty() {
            "https://api.openai.com/v1"
        } else {
            &self.cfg.base_url
        }
    }

    fn messages_to_json(messages: &[Message]) -> Vec<Value> {
        messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };
                let mut obj = json!({ "role": role, "content": m.text() });
                if let Some(id) = &m.tool_call_id {
                    obj["tool_call_id"] = json!(id);
                }
                // Attach tool_calls array for assistant messages with tool calls
                let tcs = m.tool_calls();
                if !tcs.is_empty() {
                    obj["tool_calls"] = json!(tcs
                        .iter()
                        .map(|tc| json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments,
                            }
                        }))
                        .collect::<Vec<_>>());
                }
                obj
            })
            .collect()
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn complete(&self, params: CompletionParams) -> Result<CompletionResponse> {
        let url = format!("{}/chat/completions", self.base_url());

        let tools_json: Vec<Value> = params
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();

        let mut body = json!({
            "model": params.model,
            "messages": Self::messages_to_json(&params.messages),
        });

        if !tools_json.is_empty() {
            body["tools"] = json!(tools_json);
            body["tool_choice"] = json!("auto");
        }
        if let Some(t) = params.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(mt) = params.max_tokens {
            body["max_tokens"] = json!(mt);
        }

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&body)
            .send()
            .await
            .context("OpenAI request failed")?;

        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            anyhow::bail!("OpenAI API error {status}: {text}");
        }

        let json: Value = serde_json::from_str(&text)?;
        let choice = &json["choices"][0];
        let msg = &choice["message"];

        let mut finish_reason = match choice["finish_reason"].as_str().unwrap_or("") {
            "stop" => FinishReason::Stop,
            "tool_calls" => FinishReason::ToolCalls,
            "length" => FinishReason::Length,
            _ => FinishReason::Unknown,
        };

        let mut content_blocks = vec![];

        // Extract content — fall back to reasoning field for thinking models (qwen3, etc.)
        let content_text = msg["content"].as_str().unwrap_or("");
        if !content_text.is_empty() {
            content_blocks.push(ContentBlock::text(content_text));
        } else if let Some(reasoning) = msg["reasoning"].as_str() {
            if !reasoning.is_empty() {
                content_blocks.push(ContentBlock::text(reasoning));
            }
        }

        if let Some(tcs) = msg["tool_calls"].as_array() {
            for tc in tcs {
                let id = tc["id"].as_str().unwrap_or("").to_string();
                let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let arguments = tc["function"]["arguments"]
                    .as_str()
                    .unwrap_or("{}")
                    .to_string();
                content_blocks.push(ContentBlock::tool_call(ToolCall {
                    id,
                    name,
                    arguments,
                }));
            }
        }

        // Fallback: local models (Ollama) embed tool calls in text as markdown JSON
        // instead of returning a structured tool_calls array.
        let has_tool_calls = content_blocks.iter().any(|b| b.content_type == "tool_call");
        if !has_tool_calls {
            if let Some(text_block) = content_blocks.first().cloned() {
                if text_block.content_type == "text" {
                    if let Some(text) = &text_block.text {
                        let extracted = extract_text_tool_calls(text);
                        if !extracted.is_empty() {
                            // Replace text content with parsed tool calls
                            content_blocks =
                                extracted.into_iter().map(ContentBlock::tool_call).collect();
                            finish_reason = FinishReason::ToolCalls;
                        }
                    }
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
            input_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0),
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

    async fn stream_complete(
        &self,
        params: CompletionParams,
        tx: StreamSender,
    ) -> Result<CompletionResponse> {
        let url = format!("{}/chat/completions", self.base_url());

        let tools_json: Vec<Value> = params
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();

        let mut body = json!({
            "model": params.model,
            "messages": Self::messages_to_json(&params.messages),
            "stream": true,
            "stream_options": { "include_usage": true },
        });

        if !tools_json.is_empty() {
            body["tools"] = json!(tools_json);
            body["tool_choice"] = json!("auto");
        }
        if let Some(t) = params.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(mt) = params.max_tokens {
            body["max_tokens"] = json!(mt);
        }

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&body)
            .send()
            .await
            .context("OpenAI streaming request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await?;
            anyhow::bail!("OpenAI API error {status}: {text}");
        }

        // Check if the server actually returned a streaming response.
        // Some backends (Ollama, local proxies) may ignore "stream":true
        // and return a plain JSON response instead.
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // If the response is NOT event-stream, parse it as a regular JSON response
        if !content_type.contains("text/event-stream") && !content_type.contains("text/plain") {
            let text = resp.text().await?;
            let json: Value = serde_json::from_str(&text)?;
            let choice = &json["choices"][0];
            let msg = &choice["message"];

            let mut finish = match choice["finish_reason"].as_str().unwrap_or("") {
                "stop" => FinishReason::Stop,
                "tool_calls" => FinishReason::ToolCalls,
                "length" => FinishReason::Length,
                _ => FinishReason::Unknown,
            };

            let mut cbs = vec![];
            let content_text = msg["content"].as_str().unwrap_or("");
            if !content_text.is_empty() {
                let _ = tx.send(StreamEvent::TextDelta(content_text.to_string()));
                cbs.push(ContentBlock::text(content_text));
            } else if let Some(reasoning) = msg["reasoning"].as_str() {
                if !reasoning.is_empty() {
                    let _ = tx.send(StreamEvent::TextDelta(reasoning.to_string()));
                    cbs.push(ContentBlock::text(reasoning));
                }
            }
            if let Some(tcs) = msg["tool_calls"].as_array() {
                for tc in tcs {
                    let id = tc["id"].as_str().unwrap_or("").to_string();
                    let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                    let arguments = tc["function"]["arguments"]
                        .as_str()
                        .unwrap_or("{}")
                        .to_string();
                    cbs.push(ContentBlock::tool_call(ToolCall { id, name, arguments }));
                }
            }

            // Ollama fallback: tool calls embedded in text
            let has_tc = cbs.iter().any(|b| b.content_type == "tool_call");
            if !has_tc {
                if let Some(tb) = cbs.first().cloned() {
                    if tb.content_type == "text" {
                        if let Some(t) = &tb.text {
                            let extracted = extract_text_tool_calls(t);
                            if !extracted.is_empty() {
                                cbs = extracted.into_iter().map(ContentBlock::tool_call).collect();
                                finish = FinishReason::ToolCalls;
                            }
                        }
                    }
                }
            }

            let u = TokenUsage {
                input_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            };

            return Ok(CompletionResponse {
                message: Message {
                    role: Role::Assistant,
                    content: cbs,
                    tool_call_id: None,
                },
                finish_reason: finish,
                usage: u,
            });
        }

        // Parse SSE stream
        let mut stream = resp.bytes_stream().eventsource();

        let mut accumulated_text = String::new();
        let mut finish_reason = FinishReason::Unknown;
        let mut usage = TokenUsage::default();
        // Buffer text that might be a JSON tool call (Ollama streams tool calls as content)
        let mut text_buffer = String::new();
        let mut buffering_json = true; // Start buffering until we know it's not JSON

        // Tool call accumulators: index -> (id, name, arguments)
        let mut tool_calls_acc: std::collections::HashMap<usize, (String, String, String)> =
            std::collections::HashMap::new();

        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(_) => continue,
            };

            if event.data == "[DONE]" {
                break;
            }

            let chunk: Value = match serde_json::from_str(&event.data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Parse finish_reason
            if let Some(fr) = chunk["choices"][0]["finish_reason"].as_str() {
                finish_reason = match fr {
                    "stop" => FinishReason::Stop,
                    "tool_calls" => FinishReason::ToolCalls,
                    "length" => FinishReason::Length,
                    _ => FinishReason::Unknown,
                };
            }

            // Parse tool call deltas first to detect tool_call mode
            let mut has_tool_calls_in_chunk = false;
            if let Some(tcs) = chunk["choices"][0]["delta"]["tool_calls"].as_array() {
                has_tool_calls_in_chunk = !tcs.is_empty();
                for tc in tcs {
                    let index = tc["index"].as_u64().unwrap_or(0) as usize;
                    let entry = tool_calls_acc
                        .entry(index)
                        .or_insert_with(|| (String::new(), String::new(), String::new()));

                    if let Some(id) = tc["id"].as_str() {
                        entry.0 = id.to_string();
                    }
                    if let Some(name) = tc["function"]["name"].as_str() {
                        entry.1.push_str(name);
                        let _ = tx.send(StreamEvent::ToolCallStart {
                            id: entry.0.clone(),
                            name: entry.1.clone(),
                        });
                    }
                    if let Some(args) = tc["function"]["arguments"].as_str() {
                        entry.2.push_str(args);
                        let _ = tx.send(StreamEvent::ToolCallDelta {
                            index,
                            arguments: args.to_string(),
                        });
                    }
                }
            }

            // Parse text deltas only when not accumulating tool calls
            // (Ollama sometimes echoes tool call JSON in the content field)
            if !has_tool_calls_in_chunk && tool_calls_acc.is_empty() {
                if let Some(text) = chunk["choices"][0]["delta"]["content"].as_str() {
                    if !text.is_empty() {
                        accumulated_text.push_str(text);
                        if buffering_json {
                            text_buffer.push_str(text);
                            let trimmed = text_buffer.trim_start();
                            // Keep buffering if it looks like JSON or markdown-wrapped JSON
                            let looks_like_json = trimmed.starts_with('{')
                                || trimmed.starts_with('[')
                                || trimmed.starts_with("```");
                            if !trimmed.is_empty() && !looks_like_json {
                                // Not JSON — flush buffer and switch to streaming mode
                                let _ = tx.send(StreamEvent::TextDelta(text_buffer.clone()));
                                text_buffer.clear();
                                buffering_json = false;
                            }
                        } else {
                            let _ = tx.send(StreamEvent::TextDelta(text.to_string()));
                        }
                    }
                }
                // Parse reasoning delta (qwen3 and other reasoning models)
                if let Some(reasoning) = chunk["choices"][0]["delta"]["reasoning"].as_str() {
                    if !reasoning.is_empty() && accumulated_text.is_empty() {
                        // Reasoning is never a tool call — stream immediately
                        buffering_json = false;
                        let _ = tx.send(StreamEvent::TextDelta(reasoning.to_string()));
                    }
                }
            }

            // Parse usage (some providers include it in the final chunk)
            if let Some(u) = chunk.get("usage") {
                usage.input_tokens = u["prompt_tokens"].as_u64().unwrap_or(usage.input_tokens);
                usage.output_tokens =
                    u["completion_tokens"].as_u64().unwrap_or(usage.output_tokens);
            }
        }

        // Build content blocks
        let mut content_blocks = vec![];

        // If we buffered text that turned out NOT to be a tool call, flush it now
        if !text_buffer.is_empty() {
            // Check if it parses as a tool call before deciding to flush
            let maybe_tool_calls = extract_text_tool_calls(&text_buffer);
            if maybe_tool_calls.is_empty() {
                // Not a tool call — send as text
                let _ = tx.send(StreamEvent::TextDelta(text_buffer));
            }
            // If it IS a tool call, don't send TextDelta; the fallback below handles it
        }

        if !accumulated_text.is_empty() {
            content_blocks.push(ContentBlock::text(&accumulated_text));
        }

        // Add tool calls from accumulator
        let mut sorted_indices: Vec<usize> = tool_calls_acc.keys().copied().collect();
        sorted_indices.sort();
        for idx in sorted_indices {
            if let Some((id, name, arguments)) = tool_calls_acc.remove(&idx) {
                content_blocks.push(ContentBlock::tool_call(ToolCall {
                    id,
                    name,
                    arguments,
                }));
            }
        }

        // Fallback: local models (Ollama) embed tool calls in text
        let has_tool_calls = content_blocks.iter().any(|b| b.content_type == "tool_call");
        if !has_tool_calls {
            if let Some(text_block) = content_blocks.first().cloned() {
                if text_block.content_type == "text" {
                    if let Some(text) = &text_block.text {
                        let extracted = extract_text_tool_calls(text);
                        if !extracted.is_empty() {
                            content_blocks =
                                extracted.into_iter().map(ContentBlock::tool_call).collect();
                            finish_reason = FinishReason::ToolCalls;
                        }
                    }
                }
            }
        }

        let message = Message {
            role: Role::Assistant,
            content: content_blocks,
            tool_call_id: None,
        };

        Ok(CompletionResponse {
            message,
            finish_reason,
            usage,
        })
    }
}

/// Parse tool calls embedded as JSON in assistant text (fallback for local models).
/// Handles patterns:
///   - ` ```json\n{"name":"...","arguments":{...}}\n``` `
///   - `{"name":"...","arguments":{...}}` (bare JSON)
///   - Multiple tool calls in sequence
fn extract_text_tool_calls(text: &str) -> Vec<ToolCall> {
    let mut calls = vec![];

    // Pattern 1: ```json ... ``` code blocks
    let mut search = text;
    while let Some(start) = search.find("```") {
        let after_fence = &search[start + 3..];
        // skip optional language tag (json, tool_call, etc.)
        let after_lang = after_fence
            .trim_start_matches(|c: char| c.is_alphabetic() || c == '_')
            .trim_start_matches('\n');
        if let Some(end) = after_lang.find("```") {
            let block = after_lang[..end].trim();
            if let Some(tc) = parse_tool_call_json(block) {
                calls.push(tc);
            }
            search = &after_lang[end + 3..];
        } else {
            break;
        }
    }

    if !calls.is_empty() {
        return calls;
    }

    // Pattern 2: scan the full text for all bare JSON objects (handles trailing
    // natural-language text that follows the JSON, and multiple consecutive calls).
    let mut rest = text;
    while let Some(json_str) = find_json_object(rest) {
        if let Some(tc) = parse_tool_call_json(json_str) {
            calls.push(tc);
        }
        // Advance past this object so we can find the next one
        let skip = rest.find(json_str).unwrap_or(0) + json_str.len();
        rest = &rest[skip..];
    }

    calls
}

/// Find the first complete JSON object `{...}` in `text` using brace counting,
/// correctly ignoring braces inside strings and escape sequences.
fn find_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let slice = &text[start..];
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, c) in slice.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if in_string {
            match c {
                '\\' => escape_next = true,
                '"' => in_string = false,
                _ => {}
            }
        } else {
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&slice[..=i]);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn parse_tool_call_json(s: &str) -> Option<ToolCall> {
    let v: Value = serde_json::from_str(s).ok()?;
    let name = v["name"].as_str().or_else(|| v["function"].as_str())?;
    let arguments = v
        .get("arguments")
        .or_else(|| v.get("parameters"))
        .or_else(|| v.get("args"))
        .map(|a| a.to_string())
        .unwrap_or_else(|| "{}".to_string());
    Some(ToolCall {
        id: format!(
            "local_{}",
            Uuid::new_v4().to_string().split('-').next().unwrap_or("x")
        ),
        name: name.to_string(),
        arguments,
    })
}
