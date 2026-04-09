use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::manager::{Tool, ToolDefinition, ToolResult};
use crate::event::Event;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: u32,
    pub title: String,
    pub status: TodoStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum TodoStatus {
    NotStarted,
    InProgress,
    Completed,
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStarted => write!(f, "not-started"),
            Self::InProgress => write!(f, "in-progress"),
            Self::Completed => write!(f, "completed"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TodoStore {
    items: Arc<Mutex<Vec<TodoItem>>>,
}

impl TodoStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&self) -> Vec<TodoItem> {
        self.items.lock().unwrap().clone()
    }

    pub fn write(&self, items: Vec<TodoItem>) {
        *self.items.lock().unwrap() = items;
    }
}

pub struct TodoReadTool {
    pub store: TodoStore,
}

#[async_trait]
impl Tool for TodoReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "TodoRead".into(),
            description: "Read the current todo list.".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        }
    }

    async fn execute(
        &self,
        _args: Value,
        _events: Option<mpsc::Sender<Event>>,
    ) -> Result<ToolResult> {
        let items = self.store.read();
        if items.is_empty() {
            return Ok(ToolResult::ok("No todos."));
        }
        let out: String = items
            .iter()
            .map(|i| format!("[{}] {} — {}", i.id, i.status, i.title))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolResult::ok(out))
    }
}

pub struct TodoWriteTool {
    pub store: TodoStore,
}

#[async_trait]
impl Tool for TodoWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "TodoWrite".into(),
            description: "Write/replace the full todo list.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "integer" },
                                "title": { "type": "string" },
                                "status": { "type": "string", "enum": ["not-started", "in-progress", "completed"] }
                            },
                            "required": ["id", "title", "status"]
                        }
                    }
                },
                "required": ["todos"]
            }),
        }
    }

    async fn execute(
        &self,
        args: Value,
        _events: Option<mpsc::Sender<Event>>,
    ) -> Result<ToolResult> {
        let raw = match args["todos"].as_array() {
            Some(a) => a.clone(),
            None => return Ok(ToolResult::err("Missing 'todos' array")),
        };

        let mut items = Vec::with_capacity(raw.len());
        let mut errors = Vec::new();
        for (i, v) in raw.iter().enumerate() {
            match serde_json::from_value::<TodoItem>(v.clone()) {
                Ok(item) => items.push(item),
                Err(e) => errors.push(format!("item {}: {e}", i + 1)),
            }
        }

        if !errors.is_empty() {
            return Ok(ToolResult::err(format!(
                "Failed to parse {} todo(s): {}",
                errors.len(),
                errors.join("; ")
            )));
        }

        let count = items.len();
        self.store.write(items);
        Ok(ToolResult::ok(format!("Saved {count} todos.")))
    }
}
