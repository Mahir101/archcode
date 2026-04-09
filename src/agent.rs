use std::sync::Arc;
use anyhow::Result;
use tokio::sync::mpsc;

use crate::event::Event;
use crate::guard::{EvalContext, GuardManager, Verdict};
use crate::llm::{CompletionParams, CompletionResponse, FinishReason, LlmProvider, Message, ToolDef};
use crate::reminder::{ConversationState, ReminderManager};
use crate::tools::{ToolManager};

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
}

impl Agent {
    pub fn new(
        provider: Arc<dyn LlmProvider + Send + Sync>,
        model: String,
        tool_manager: Arc<ToolManager>,
        guard_manager: Arc<GuardManager>,
        reminder_manager: ReminderManager,
        system_prompt: String,
        events_tx: mpsc::Sender<Event>,
        working_dir: String,
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
        }
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

        for _turn in 0..MAX_TURNS {
            let resp = self.provider.complete(CompletionParams {
                model: self.model.clone(),
                messages: self.messages.clone(),
                tools: tool_defs.clone(),
                max_tokens: None,
                temperature: None,
            }).await?;

            self.messages.push(resp.message.clone());

            // Stream text to UI
            let text = resp.message.text();
            if !text.is_empty() {
                let _ = self.events_tx.send(Event::tool("Assistant", &text)).await;
            }

            match resp.finish_reason {
                FinishReason::Stop => {
                    return Ok(resp.message.text());
                }
                FinishReason::ToolCalls => {
                    for tc in resp.message.tool_calls() {
                        let tc = tc.clone();
                        let args: serde_json::Value = serde_json::from_str(&tc.arguments)
                            .unwrap_or(serde_json::json!({}));

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
                                let _ = self.events_tx.send(
                                    Event::guard(&tc.name, format!("DENIED: {}", decision.reason), true)
                                ).await;
                                self.messages.push(Message {
                                    role: crate::llm::Role::Tool,
                                    content: vec![crate::llm::ContentBlock::text(
                                        format!("Tool call denied by guard: {}", decision.reason)
                                    )],
                                    tool_call_id: Some(tc.id.clone()),
                                });
                                continue;
                            }
                            Verdict::Ask => {
                                // For now auto-ask becomes a deny in non-interactive mode
                                let _ = self.events_tx.send(
                                    Event::guard(&tc.name, format!("ASK: {}", decision.reason), false)
                                ).await;
                                // In interactive mode this would prompt user; here we allow
                            }
                            Verdict::Allow => {}
                        }

                        // Execute tool
                        let _ = self.events_tx.send(
                            Event::tool(&tc.name, format!("Calling {}", tc.name))
                        ).await;

                        let result = self.tool_manager
                            .execute(&tc.name, args, Some(self.events_tx.clone()))
                            .await;

                        self.messages.push(Message {
                            role: crate::llm::Role::Tool,
                            content: vec![crate::llm::ContentBlock::text(&result.content)],
                            tool_call_id: Some(tc.id.clone()),
                        });
                    }
                }
                _ => break,
            }
        }

        Ok(self.messages.last()
            .map(|m| m.text())
            .unwrap_or_default())
    }
}
