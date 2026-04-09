use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::manager::{Tool, ToolDefinition, ToolResult};
use crate::event::Event;

/// Persistent shell state shared across BashTool invocations.
#[derive(Clone)]
pub struct ShellState {
    /// Current working directory (persists across commands).
    pub cwd: Arc<Mutex<String>>,
    /// Environment variable overrides set by previous commands.
    pub env_vars: Arc<Mutex<HashMap<String, String>>>,
}

impl ShellState {
    pub fn new(initial_cwd: &str) -> Self {
        Self {
            cwd: Arc::new(Mutex::new(initial_cwd.to_string())),
            env_vars: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

pub struct BashTool {
    pub state: ShellState,
}

#[async_trait]
impl Tool for BashTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "Bash".into(),
            description: "Execute a shell command in a persistent shell session. \
                Working directory and exported environment variables persist across calls."
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

        let cwd = self.state.cwd.lock().unwrap().clone();
        let env_snapshot: HashMap<String, String> =
            self.state.env_vars.lock().unwrap().clone();

        if let Some(ch) = &events {
            let _ = ch
                .send(Event::tool("Bash", format!("[{}] $ {}", truncate(&cwd, 30), truncate(&command, 60))))
                .await;
        }

        // Build the wrapped command: run user command, then print cwd + env markers
        let marker = "__ARCHCODE_STATE__";
        let wrapped = format!(
            "{command}\n__exit=$?\necho \"\"\necho \"{marker}\"\npwd\nenv -0\necho \"{marker}_END\"\nexit $__exit",
        );

        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c").arg(&wrapped).current_dir(&cwd);

        // Inject persisted env vars
        for (k, v) in &env_snapshot {
            cmd.env(k, v);
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            cmd.output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Command timed out after {timeout_secs}s"))?;

        let output = output.map_err(|e| anyhow::anyhow!("Failed to spawn process: {e}"))?;

        let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Parse state from stdout
        let (user_stdout, new_cwd, new_env) = parse_shell_state(&raw_stdout, marker);

        // Update persistent state
        if let Some(dir) = new_cwd {
            *self.state.cwd.lock().unwrap() = dir;
        }
        if let Some(env) = new_env {
            let mut store = self.state.env_vars.lock().unwrap();
            for (k, v) in env {
                // Only persist user-set vars, skip internal shell vars
                if !k.starts_with("__") && k != "PWD" && k != "OLDPWD" && k != "SHLVL" && k != "_" {
                    store.insert(k, v);
                }
            }
        }

        let mut result = String::new();
        if !user_stdout.is_empty() {
            result.push_str(&user_stdout);
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

/// Parse the shell state marker block from stdout.
/// Returns (user_stdout, Option<new_cwd>, Option<env_vars>).
fn parse_shell_state(
    stdout: &str,
    marker: &str,
) -> (String, Option<String>, Option<HashMap<String, String>>) {
    let end_marker = format!("{marker}_END");
    if let Some(marker_pos) = stdout.find(marker) {
        // Check this isn't the END marker
        let before_marker = stdout[..marker_pos].trim_end().to_string();
        let after_marker = &stdout[marker_pos + marker.len()..];

        if let Some(end_pos) = after_marker.find(&end_marker) {
            let state_block = after_marker[..end_pos].trim();
            let mut lines_iter = state_block.lines();

            // First line is pwd output
            let new_cwd = lines_iter.next().map(|s| s.trim().to_string());

            // Rest is null-separated env (env -0 output)
            let env_str: String = lines_iter.collect::<Vec<_>>().join("\n");
            let new_env: HashMap<String, String> = env_str
                .split('\0')
                .filter_map(|entry| {
                    let mut parts = entry.splitn(2, '=');
                    let key = parts.next()?.trim().to_string();
                    let val = parts.next()?.to_string();
                    if key.is_empty() {
                        None
                    } else {
                        Some((key, val))
                    }
                })
                .collect();

            let parsed_env = if env_str.is_empty() { None } else { Some(new_env) };
            return (before_marker, new_cwd, parsed_env);
        }
    }
    // No marker found — return raw stdout
    (stdout.to_string(), None, None)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let end = s.floor_char_boundary(max);
        format!("{}...", &s[..end])
    }
}
