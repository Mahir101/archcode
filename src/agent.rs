use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::cost::CostTracker;
use crate::event::Event;
use crate::guard::{EvalContext, GuardManager, Verdict};
use crate::llm::{
    CompletionParams, CompletionResponse, ContentBlock, FinishReason, LlmProvider, Message,
    StreamSender, ToolCall, ToolCallResult, ToolDef,
};
use crate::reminder::{ConversationState, ReminderManager};
use crate::tools::ToolManager;

const MAX_TURNS: usize = 200;

pub struct Agent {
    provider: Arc<dyn LlmProvider + Send + Sync>,
    model: String,
    tool_manager: Arc<ToolManager>,
    guard_manager: Arc<GuardManager>,
    reminder_manager: ReminderManager,
    system_prompt: String,
    messages: Vec<Message>,
    events_tx: mpsc::Sender<Event>,
    working_dir: String,
    cost_tracker: CostTracker,
    interactive: bool,
    stream_tx: Option<StreamSender>,
}

impl Agent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Arc<dyn LlmProvider + Send + Sync>,
        model: String,
        tool_manager: Arc<ToolManager>,
        guard_manager: Arc<GuardManager>,
        reminder_manager: ReminderManager,
        system_prompt: String,
        events_tx: mpsc::Sender<Event>,
        working_dir: String,
        cost_tracker: CostTracker,
        interactive: bool,
    ) -> Self {
        Self {
            provider,
            model,
            tool_manager,
            guard_manager,
            reminder_manager,
            system_prompt,
            messages: vec![],
            events_tx,
            working_dir,
            cost_tracker,
            interactive,
            stream_tx: None,
        }
    }

    /// Set a stream sender for live streaming of LLM text output.
    pub fn set_stream_sender(&mut self, tx: StreamSender) {
        self.stream_tx = Some(tx);
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn messages_mut(&mut self) -> &mut Vec<Message> {
        &mut self.messages
    }

    pub async fn run(&mut self, user_input: &str) -> Result<String> {
        if self.messages.is_empty() {
            self.messages.push(Message::system(&self.system_prompt));
        }

        let turn = self.messages.len() / 2;
        let state = ConversationState {
            turn,
            message_count: self.messages.len(),
        };

        // Inject reminders if any
        let mut user_msg = user_input.to_string();
        if let Some(reminder) = self.reminder_manager.inject(&state) {
            user_msg = format!("{user_msg}\n\n{reminder}");
        }

        self.messages.push(Message::user(user_msg));

        let tool_defs: Vec<ToolDef> = self
            .tool_manager
            .definitions()
            .into_iter()
            .map(|d| ToolDef {
                name: d.name,
                description: d.description,
                parameters: d.parameters,
            })
            .collect();

        let mut continuation_count = 0;
        const MAX_CONTINUATIONS: usize = 3;

        for _turn in 0..MAX_TURNS {
            let completion_params = CompletionParams {
                model: self.model.clone(),
                messages: self.messages.clone(),
                tools: tool_defs.clone(),
                max_tokens: None,
                temperature: None,
            };

            let resp: CompletionResponse = if let Some(stx) = &self.stream_tx {
                self.provider
                    .stream_complete(completion_params, stx.clone())
                    .await?
            } else {
                self.provider.complete(completion_params).await?
            };

            // Record token usage
            self.cost_tracker
                .record(resp.usage.input_tokens, resp.usage.output_tokens);

            self.messages.push(resp.message.clone());

            match resp.finish_reason {
                FinishReason::Stop => {
                    return Ok(resp.message.text());
                }
                FinishReason::Length => {
                    // MaxTokens hit — auto-continue up to MAX_CONTINUATIONS times
                    if continuation_count < MAX_CONTINUATIONS {
                        continuation_count += 1;
                        let _ = self
                            .events_tx
                            .send(Event::text(format!(
                                "Response truncated (max_tokens). Auto-continuing ({continuation_count}/{MAX_CONTINUATIONS})..."
                            )))
                            .await;
                        self.messages
                            .push(Message::user("Please continue from where you left off."));
                        continue;
                    } else {
                        return Ok(resp.message.text());
                    }
                }
                FinishReason::ToolCalls => {
                    continuation_count = 0; // Reset on tool calls
                    for tc in resp.message.tool_calls() {
                        let tc: ToolCall = tc.clone();
                        let args: serde_json::Value =
                            serde_json::from_str(&tc.arguments).unwrap_or(serde_json::json!({}));

                        // Guard evaluation
                        let eval_ctx = EvalContext {
                            tool_name: tc.name.clone(),
                            input: tc.arguments.clone(),
                            working_dir: self.working_dir.clone(),
                            events_ch: Some(self.events_tx.clone()),
                        };

                        let decision = self.guard_manager.evaluate(&eval_ctx).await;
                        match decision.verdict {
                            Verdict::Deny => {
                                let _ = self
                                    .events_tx
                                    .send(Event::guard(
                                        &tc.name,
                                        format!("DENIED: {}", decision.reason),
                                        true,
                                    ))
                                    .await;
                                self.messages.push(Message {
                                    role: crate::llm::Role::Tool,
                                    content: vec![crate::llm::ContentBlock::text(format!(
                                        "Tool call denied by guard: {}",
                                        decision.reason
                                    ))],
                                    tool_call_id: Some(tc.id.clone()),
                                });
                                continue;
                            }
                            Verdict::Ask => {
                                if self.interactive {
                                    // Interactive mode: prompt user for permission
                                    let _ = self
                                        .events_tx
                                        .send(Event::guard(
                                            &tc.name,
                                            format!("Permission required: {}", decision.reason),
                                            false,
                                        ))
                                        .await;
                                    eprint!(
                                        "\x1b[33m[Guard]\x1b[0m Allow '{}' {}? [y/N] ",
                                        tc.name, decision.reason
                                    );
                                    let mut input = String::new();
                                    if std::io::stdin().read_line(&mut input).is_ok() {
                                        let answer = input.trim().to_lowercase();
                                        if answer != "y" && answer != "yes" {
                                            self.messages.push(Message {
                                                role: crate::llm::Role::Tool,
                                                content: vec![crate::llm::ContentBlock::text(
                                                    "Tool call denied by user.".to_string(),
                                                )],
                                                tool_call_id: Some(tc.id.clone()),
                                            });
                                            continue;
                                        }
                                    }
                                }
                                // Non-interactive: auto-allow
                            }
                            Verdict::Allow => {}
                        }

                        // Execute tool
                        let _ = self
                            .events_tx
                            .send(Event::tool(&tc.name, format!("Calling {}", tc.name)))
                            .await;

                        let result = self
                            .tool_manager
                            .execute(&tc.name, args, Some(self.events_tx.clone()))
                            .await;

                        // Use is_error to signal tool failure in the event
                        if result.is_error {
                            let _ = self
                                .events_tx
                                .send(Event::tool(
                                    &tc.name,
                                    format!("Tool error: {}", result.content),
                                ))
                                .await;
                        }

                        self.messages.push(Message {
                            role: crate::llm::Role::Tool,
                            content: vec![ContentBlock::tool_result(ToolCallResult {
                                tool_call_id: tc.id.clone(),
                                content: result.content.clone(),
                            })],
                            tool_call_id: Some(tc.id.clone()),
                        });
                    }
                }
                _ => break,
            }
        }

        // If we exhausted the turn budget, push a final assistant message
        let fallback = "Reached maximum turns without a final response.".to_string();
        self.messages.push(Message::assistant(&fallback));
        Ok(self.messages.last().map(|m| m.text()).unwrap_or_default())
    }
}
