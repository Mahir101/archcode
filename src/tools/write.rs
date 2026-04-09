use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::manager::{Tool, ToolDefinition, ToolResult};
use crate::event::Event;

pub struct WriteTool;

#[async_trait]
impl Tool for WriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "Write".into(),
            description: "Write content to a file. Creates parent directories if needed.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute or relative file path." },
                    "content": { "type": "string", "description": "Content to write to the file." }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(
        &self,
        args: Value,
        _events: Option<mpsc::Sender<Event>>,
    ) -> Result<ToolResult> {
        let path = match args["path"].as_str() {
            Some(p) => p.to_string(),
            None => return Ok(ToolResult::err("Missing 'path' argument")),
        };
        let content = match args["content"].as_str() {
            Some(c) => c.to_string(),
            None => return Ok(ToolResult::err("Missing 'content' argument")),
        };

        if let Some(parent) = std::path::Path::new(&path).parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        tokio::fs::write(&path, &content)
            .await
            .map_err(|e| anyhow::anyhow!("Write failed: {e}"))?;

        Ok(ToolResult::ok(format!(
            "Written {} bytes to {path}",
            content.len()
        )))
    }
}
