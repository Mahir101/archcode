use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::manager::{Tool, ToolDefinition, ToolResult};
use crate::event::Event;

pub struct ReadTool;

#[async_trait]
impl Tool for ReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "Read".into(),
            description: "Read a file from the filesystem. Returns the file content as text."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute or relative file path to read." },
                    "start_line": { "type": "integer", "description": "1-based start line (optional)." },
                    "end_line": { "type": "integer", "description": "1-based end line inclusive (optional)." }
                },
                "required": ["path"]
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

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return Ok(ToolResult::err(format!("Cannot read {path}: {e}"))),
        };

        let start = args["start_line"].as_u64().map(|n| n as usize).unwrap_or(1);
        let end = args["end_line"].as_u64().map(|n| n as usize);

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let from = start.saturating_sub(1).min(total);
        let to = end.map(|e| e.min(total)).unwrap_or(total);

        let slice: String = lines[from..to]
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{}: {}", from + i + 1, l))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult::ok(slice))
    }
}
