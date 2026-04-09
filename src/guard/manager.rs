use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::event::Event;

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone)]
pub struct Decision {
    pub verdict: Verdict,
    pub reason: String,
}

impl Decision {
    pub fn allow(reason: impl Into<String>) -> Self {
        Self {
            verdict: Verdict::Allow,
            reason: reason.into(),
        }
    }
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            verdict: Verdict::Deny,
            reason: reason.into(),
        }
    }
    pub fn ask(reason: impl Into<String>) -> Self {
        Self {
            verdict: Verdict::Ask,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EvalContext {
    pub tool_name: String,
    pub input: String,
    pub working_dir: String,
    pub events_ch: Option<mpsc::Sender<Event>>,
}

#[async_trait]
pub trait GuardRule: Send + Sync {
    /// Return Some(Decision) to short-circuit; None to pass to next rule.
    async fn evaluate(&self, ctx: &EvalContext) -> Option<Decision>;
}

#[async_trait]
pub trait LlmValidator: Send + Sync {
    async fn validate(&self, ctx: &EvalContext) -> Result<Decision>;
}

pub struct GuardManager {
    rules: Vec<Arc<dyn GuardRule>>,
    llm_validator: Option<Arc<dyn LlmValidator>>,
}

impl GuardManager {
    pub fn new() -> Self {
        Self {
            rules: vec![],
            llm_validator: None,
        }
    }

    pub fn add_rule(&mut self, rule: impl GuardRule + 'static) {
        self.rules.push(Arc::new(rule));
    }

    pub fn set_llm_validator(&mut self, v: impl LlmValidator + 'static) {
        self.llm_validator = Some(Arc::new(v));
    }

    /// Evaluate all rules in order. If none decide, fall through to LLM validator.
    pub async fn evaluate(&self, ctx: &EvalContext) -> Decision {
        for rule in &self.rules {
            if let Some(d) = rule.evaluate(ctx).await {
                return d;
            }
        }
        // Fall through to LLM validator
        if let Some(v) = &self.llm_validator {
            match v.validate(ctx).await {
                Ok(d) => return d,
                Err(e) => {
                    return Decision::ask(format!("Guard LLM error: {e}"));
                }
            }
        }
        Decision::allow("No rules matched")
    }
}

impl Default for GuardManager {
    fn default() -> Self {
        Self::new()
    }
}
