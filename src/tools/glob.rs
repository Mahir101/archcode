use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::event::Event;
use super::manager::{Tool, ToolDefinition, ToolResult};

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "Glob".into(),
            description: "Find files matching a glob pattern. Returns a newline-separated list of matching paths.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern e.g. **/*.rs" },
                    "cwd": { "type": "string", "description": "Base directory for the search (optional, defaults to process cwd)." }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let pattern = match args["pattern"].as_str() {
            Some(p) => p.to_string(),
            None => return Ok(ToolResult::err("Missing 'pattern'")),
        };

        let base = args["cwd"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string());

        let full_pattern = if pattern.starts_with('/') {
            pattern.clone()
        } else {
            format!("{base}/{pattern}")
        };

        let paths: Vec<String> = glob::glob(&full_pattern)
            .map_err(|e| anyhow::anyhow!("Invalid glob: {e}"))?
            .filter_map(|e| e.ok())
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        if paths.is_empty() {
            return Ok(ToolResult::ok("No files matched."));
        }

        Ok(ToolResult::ok(paths.join("\n")))
    }
}
