use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::event::Event;
use super::manager::{Tool, ToolDefinition, ToolResult};

pub struct EditTool;

#[async_trait]
impl Tool for EditTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "Edit".into(),
            description: "Replace an exact string in a file with a new string. The old_string must match exactly once.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to edit." },
                    "old_string": { "type": "string", "description": "Exact text to find and replace." },
                    "new_string": { "type": "string", "description": "Replacement text." }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        }
    }

    async fn execute(&self, args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let path = match args["path"].as_str() {
            Some(p) => p.to_string(),
            None => return Ok(ToolResult::err("Missing 'path'")),
        };
        let old = match args["old_string"].as_str() {
            Some(s) => s.to_string(),
            None => return Ok(ToolResult::err("Missing 'old_string'")),
        };
        let new = match args["new_string"].as_str() {
            Some(s) => s.to_string(),
            None => return Ok(ToolResult::err("Missing 'new_string'")),
        };

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return Ok(ToolResult::err(format!("Cannot read {path}: {e}"))),
        };

        let count = content.matches(&old as &str).count();
        if count == 0 {
            return Ok(ToolResult::err("old_string not found in file"));
        }
        if count > 1 {
            return Ok(ToolResult::err(format!(
                "old_string matches {count} locations — make it more specific"
            )));
        }

        let updated = content.replacen(&old as &str, &new, 1);
        tokio::fs::write(&path, &updated).await?;

        Ok(ToolResult::ok(format!("Edited {path}")))
    }
}
