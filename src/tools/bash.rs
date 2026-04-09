use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::manager::{Tool, ToolDefinition, ToolResult};
use crate::event::Event;

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "Bash".into(),
            description: "Execute a shell command and return stdout/stderr. Use with caution."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to run." },
                    "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default 30)." }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(
        &self,
        args: Value,
        events: Option<mpsc::Sender<Event>>,
    ) -> Result<ToolResult> {
        let command = match args["command"].as_str() {
            Some(c) => c.to_string(),
            None => return Ok(ToolResult::err("Missing 'command'")),
        };

        let timeout_secs = args["timeout_secs"].as_u64().unwrap_or(30);

        if let Some(ch) = &events {
            let _ = ch
                .send(Event::tool("Bash", format!("$ {}", truncate(&command, 80))))
                .await;
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new("bash")
                .arg("-c")
                .arg(&command)
                .output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Command timed out after {timeout_secs}s"))?;

        let output = output.map_err(|e| anyhow::anyhow!("Failed to spawn process: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut result = String::new();
        if !stdout.is_empty() {
            result.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("STDERR:\n");
            result.push_str(&stderr);
        }

        if result.is_empty() {
            result = format!("Exit code: {}", output.status.code().unwrap_or(-1));
        }

        let is_error = !output.status.success();
        Ok(ToolResult {
            content: result,
            is_error,
        })
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
