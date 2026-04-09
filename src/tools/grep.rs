use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::manager::{Tool, ToolDefinition, ToolResult};
use crate::event::Event;

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "Grep".into(),
            description: "Search file contents using ripgrep (rg). Returns matching lines with \
                file paths and line numbers. Supports regex patterns, file type filters, and \
                context lines. Use this for fast text search across the codebase."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Search pattern (regex by default)."
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory or file to search in (defaults to cwd)."
                    },
                    "include": {
                        "type": "string",
                        "description": "Glob pattern for files to include, e.g. '*.rs' or '*.py'."
                    },
                    "fixed_strings": {
                        "type": "boolean",
                        "description": "Treat pattern as a literal string, not a regex."
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "description": "Force case-sensitive search (default: smart-case)."
                    },
                    "context_lines": {
                        "type": "integer",
                        "description": "Number of context lines before and after each match (default: 0)."
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of matching lines to return (default: 100)."
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(
        &self,
        args: Value,
        events: Option<mpsc::Sender<Event>>,
    ) -> Result<ToolResult> {
        let pattern = match args["pattern"].as_str() {
            Some(p) => p.to_string(),
            None => return Ok(ToolResult::err("Missing 'pattern'")),
        };

        let path = args["path"].as_str().unwrap_or(".").to_string();
        let include = args["include"].as_str();
        let fixed = args["fixed_strings"].as_bool().unwrap_or(false);
        let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(false);
        let context = args["context_lines"].as_u64().unwrap_or(0);
        let max_results = args["max_results"].as_u64().unwrap_or(100);

        if let Some(ch) = &events {
            let _ = ch
                .send(Event::tool(
                    "Grep",
                    format!("Searching for '{}' in {}", truncate(&pattern, 40), &path),
                ))
                .await;
        }

        // Try ripgrep first, fall back to grep
        let (cmd_name, use_rg) = if which_exists("rg") {
            ("rg", true)
        } else {
            ("grep", false)
        };

        let mut cmd = tokio::process::Command::new(cmd_name);

        if use_rg {
            // ripgrep args
            cmd.arg("--line-number")
                .arg("--no-heading")
                .arg("--color=never")
                .arg(format!("--max-count={max_results}"));

            if fixed {
                cmd.arg("--fixed-strings");
            }
            if case_sensitive {
                cmd.arg("--case-sensitive");
            }
            if context > 0 {
                cmd.arg(format!("--context={context}"));
            }
            if let Some(glob) = include {
                cmd.arg("--glob").arg(glob);
            }

            cmd.arg(&pattern).arg(&path);
        } else {
            // GNU grep fallback
            cmd.arg("-rn").arg("--color=never");

            if fixed {
                cmd.arg("-F");
            }
            if !case_sensitive {
                cmd.arg("-i");
            }
            if context > 0 {
                cmd.arg(format!("-C{context}"));
            }
            if let Some(glob) = include {
                cmd.arg("--include").arg(glob);
            }

            cmd.arg(&pattern).arg(&path);
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            cmd.output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Grep timed out after 15s"))?;

        let output = output.map_err(|e| anyhow::anyhow!("Failed to run {cmd_name}: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.code() == Some(1) && stdout.is_empty() {
            return Ok(ToolResult::ok("No matches found."));
        }

        if !output.status.success() && output.status.code() != Some(1) {
            return Ok(ToolResult::err(format!(
                "{cmd_name} failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            )));
        }

        // Truncate output if very large
        let result = stdout.to_string();
        let lines: Vec<&str> = result.lines().collect();
        if lines.len() > max_results as usize {
            let truncated: String = lines[..max_results as usize].join("\n");
            Ok(ToolResult::ok(format!(
                "{truncated}\n\n... ({} total matches, showing first {max_results})",
                lines.len()
            )))
        } else {
            Ok(ToolResult::ok(format!(
                "{result}\n({} matches)",
                lines.len()
            )))
        }
    }
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let end = s.floor_char_boundary(max);
        format!("{}...", &s[..end])
    }
}
