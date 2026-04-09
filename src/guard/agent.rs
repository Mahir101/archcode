use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

use super::manager::{Decision, EvalContext, LlmValidator, Verdict};
use crate::llm::{CompletionParams, FinishReason, LlmProvider, Message};

pub struct GuardAgent {
    provider: Arc<dyn LlmProvider + Send + Sync>,
    model: String,
    max_turns: usize,
}

impl GuardAgent {
    pub fn new(
        provider: Arc<dyn LlmProvider + Send + Sync>,
        model: String,
        max_turns: usize,
    ) -> Self {
        let max_turns = if max_turns == 0 { 5 } else { max_turns };
        Self {
            provider,
            model,
            max_turns,
        }
    }
}

#[async_trait]
impl LlmValidator for GuardAgent {
    async fn validate(&self, ctx: &EvalContext) -> Result<Decision> {
        let system = format!(
            "You are a security guard agent for an AI coding assistant called archcode.\n\
             Your job is to evaluate tool calls for safety and security.\n\
             Working directory: {}\n\n\
             Respond with exactly one of:\n\
             ALLOW — if the action is safe\n\
             DENY: <reason> — if the action is dangerous\n\
             ASK: <reason> — if you need human confirmation\n\
             Be concise and decisive.",
            ctx.working_dir
        );

        let user = format!(
            "Evaluate this tool call:\nTool: {}\nInput: {}",
            ctx.tool_name,
            truncate(&ctx.input, 2000)
        );

        let mut messages = vec![Message::system(&system), Message::user(&user)];

        for _ in 0..self.max_turns {
            let resp = self
                .provider
                .complete(CompletionParams {
                    model: self.model.clone(),
                    messages: messages.clone(),
                    tools: vec![],
                    max_tokens: Some(256),
                    temperature: Some(0.0),
                })
                .await?;

            messages.push(resp.message.clone());

            match resp.finish_reason {
                FinishReason::Stop => {
                    let text = resp.message.text();
                    let text = text.trim();
                    return Ok(parse_verdict(text));
                }
                _ => break,
            }
        }

        Ok(Decision::ask("Guard agent could not reach a decision"))
    }
}

fn parse_verdict(text: &str) -> Decision {
    let upper = text.to_uppercase();
    if upper.starts_with("ALLOW") {
        Decision {
            verdict: Verdict::Allow,
            reason: text.to_string(),
        }
    } else if upper.starts_with("DENY") {
        let reason = text
            .splitn(2, ':')
            .nth(1)
            .unwrap_or(text)
            .trim()
            .to_string();
        Decision {
            verdict: Verdict::Deny,
            reason,
        }
    } else if upper.starts_with("ASK") {
        let reason = text
            .splitn(2, ':')
            .nth(1)
            .unwrap_or(text)
            .trim()
            .to_string();
        Decision {
            verdict: Verdict::Ask,
            reason,
        }
    } else {
        Decision {
            verdict: Verdict::Ask,
            reason: format!("Unclear guard response: {text}"),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
