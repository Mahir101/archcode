use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::event::Event;

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn ok(content: impl Into<String>) -> Self {
        Self { content: content.into(), is_error: false }
    }
    pub fn err(content: impl Into<String>) -> Self {
        Self { content: content.into(), is_error: true }
    }
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(
        &self,
        args: Value,
        events: Option<mpsc::Sender<Event>>,
    ) -> Result<ToolResult>;
}

pub struct ToolManager {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolManager {
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    pub fn register(&mut self, tool: impl Tool + 'static) {
        let def = tool.definition();
        self.tools.insert(def.name.clone(), Arc::new(tool));
    }

    /// Register a pre-boxed tool (e.g., returned from a factory function).
    pub fn register_boxed(&mut self, tool: Box<dyn Tool>) {
        let def = tool.definition();
        self.tools.insert(def.name.clone(), Arc::from(tool));
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    pub async fn execute(
        &self,
        name: &str,
        args: Value,
        events: Option<mpsc::Sender<Event>>,
    ) -> ToolResult {
        match self.tools.get(name) {
            Some(tool) => match tool.execute(args, events).await {
                Ok(r) => r,
                Err(e) => ToolResult::err(format!("Tool error: {e}")),
            },
            None => ToolResult::err(format!("Unknown tool: {name}")),
        }
    }
}

impl Default for ToolManager {
    fn default() -> Self {
        Self::new()
    }
}
