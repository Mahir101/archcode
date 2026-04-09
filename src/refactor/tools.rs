use std::path::PathBuf;
use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::event::Event;
use crate::tools::{ToolDefinition, ToolResult};
use crate::tools::manager::Tool;
use super::detector::{RefactorConfig, RefactorResult, StackDetector, resolve_command, run_command};

// ---------------------------------------------------------------------------
// Shared context
// ---------------------------------------------------------------------------

/// Shared refactoring context injected into every tool.
#[derive(Clone)]
pub struct RefactorContext {
    pub root: PathBuf,
    pub config: Arc<RefactorConfig>,
    pub detector: Arc<StackDetector>,
    /// Timeout for each command in seconds (default 600 = 10 min).
    pub timeout_secs: u64,
}

impl RefactorContext {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let config = RefactorConfig::load(&root);
        let detector = StackDetector::new(root.clone());
        Self {
            config: Arc::new(config),
            detector: Arc::new(detector),
            root,
            timeout_secs: 600,
        }
    }
}

fn to_tool_result(r: &RefactorResult) -> ToolResult {
    let json = serde_json::to_string_pretty(r).unwrap_or_else(|_| format!("{r:?}"));
    if r.skipped {
        ToolResult::err(json)
    } else if r.ok {
        ToolResult::ok(json)
    } else {
        ToolResult::err(json)
    }
}

// ---------------------------------------------------------------------------
// refactor.run_tests
// ---------------------------------------------------------------------------

pub struct RunTestsTool {
    pub ctx: RefactorContext,
}

#[async_trait]
impl Tool for RunTestsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "refactor.run_tests".into(),
            description: "Run the project test suite. Returns structured JSON with pass/fail status. \
                          Uses .archcode/refactor.json override, then auto-detects from project files.".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(&self, _args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let cmd = match resolve_command(
            self.ctx.config.run_tests.as_deref(),
            self.ctx.detector.detect_tests(),
            "run_tests",
        ) {
            Ok(c) => c,
            Err(reason) => return Ok(to_tool_result(&RefactorResult::skipped(reason))),
        };
        let result = run_command(&cmd, &self.ctx.root, self.ctx.timeout_secs).await;
        Ok(to_tool_result(&result))
    }
}

// ---------------------------------------------------------------------------
// refactor.run_lint
// ---------------------------------------------------------------------------

pub struct RunLintTool {
    pub ctx: RefactorContext,
}

#[async_trait]
impl Tool for RunLintTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "refactor.run_lint".into(),
            description: "Run the project linter. Returns structured JSON with lint results. \
                          Uses .archcode/refactor.json override, then auto-detects.".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(&self, _args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let cmd = match resolve_command(
            self.ctx.config.run_lint.as_deref(),
            self.ctx.detector.detect_lint(),
            "run_lint",
        ) {
            Ok(c) => c,
            Err(reason) => return Ok(to_tool_result(&RefactorResult::skipped(reason))),
        };
        let result = run_command(&cmd, &self.ctx.root, self.ctx.timeout_secs).await;
        Ok(to_tool_result(&result))
    }
}

// ---------------------------------------------------------------------------
// refactor.run_format
// ---------------------------------------------------------------------------

pub struct RunFormatTool {
    pub ctx: RefactorContext,
}

#[async_trait]
impl Tool for RunFormatTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "refactor.run_format".into(),
            description: "Run the project code formatter. Returns structured JSON. \
                          Uses .archcode/refactor.json override, then auto-detects.".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(&self, _args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let cmd = match resolve_command(
            self.ctx.config.run_format.as_deref(),
            self.ctx.detector.detect_format(),
            "run_format",
        ) {
            Ok(c) => c,
            Err(reason) => return Ok(to_tool_result(&RefactorResult::skipped(reason))),
        };
        let result = run_command(&cmd, &self.ctx.root, self.ctx.timeout_secs).await;
        Ok(to_tool_result(&result))
    }
}

// ---------------------------------------------------------------------------
// refactor.run_semgrep
// ---------------------------------------------------------------------------

pub struct RunSemgrepTool {
    pub ctx: RefactorContext,
}

#[async_trait]
impl Tool for RunSemgrepTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "refactor.run_semgrep".into(),
            description: "Run Semgrep with the built-in SOLID smell rules. Returns structured JSON. \
                          Skips gracefully if semgrep is not installed. \
                          Uses .archcode/refactor.json override to customize rules.".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(&self, _args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let cmd = match resolve_command(
            self.ctx.config.run_semgrep.as_deref(),
            self.ctx.detector.detect_semgrep(),
            "run_semgrep",
        ) {
            Ok(c) => c,
            Err(_) => {
                return Ok(to_tool_result(&RefactorResult::skipped(
                    "semgrep is not installed. Install it with: pip install semgrep\n\
                     Then re-run to scan SOLID smells via refactoring/semgrep-rules/solid-smells.yml"
                        .to_string(),
                )));
            }
        };
        let result = run_command(&cmd, &self.ctx.root, self.ctx.timeout_secs).await;
        Ok(to_tool_result(&result))
    }
}

// ---------------------------------------------------------------------------
// refactor.git_diff
// ---------------------------------------------------------------------------

pub struct GitDiffTool {
    pub ctx: RefactorContext,
}

#[async_trait]
impl Tool for GitDiffTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "refactor.git_diff".into(),
            description: "Show the current git diff (staged + unstaged changes). \
                          Use after refactoring to review what changed before committing.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "staged": {
                        "type": "boolean",
                        "description": "If true, show only staged (cached) changes. Default: false (shows all)."
                    }
                },
                "required": []
            }),
        }
    }

    async fn execute(&self, args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let staged = args["staged"].as_bool().unwrap_or(false);
        let cmd = if staged {
            "git diff --cached --stat && git diff --cached".to_string()
        } else {
            "git diff --stat && git diff".to_string()
        };
        let result = run_command(&cmd, &self.ctx.root, 30).await;
        Ok(to_tool_result(&result))
    }
}

// ---------------------------------------------------------------------------
// refactor.baseline
// ---------------------------------------------------------------------------

/// Runs tests + lint + format + semgrep in sequence and returns a combined report.
pub struct BaselineTool {
    pub ctx: RefactorContext,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BaselineReport {
    tests: RefactorResult,
    lint: RefactorResult,
    format: RefactorResult,
    semgrep: RefactorResult,
    overall_ok: bool,
    summary: String,
}

#[async_trait]
impl Tool for BaselineTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "refactor.baseline".into(),
            description: "Run ALL quality checks (tests, lint, format, semgrep) and return a combined \
                          baseline report. Call this BEFORE and AFTER any refactoring session.".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(&self, _args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let tests = run_or_skip(
            self.ctx.config.run_tests.as_deref(),
            self.ctx.detector.detect_tests(),
            "run_tests",
            &self.ctx.root,
            self.ctx.timeout_secs,
        ).await;

        let lint = run_or_skip(
            self.ctx.config.run_lint.as_deref(),
            self.ctx.detector.detect_lint(),
            "run_lint",
            &self.ctx.root,
            self.ctx.timeout_secs,
        ).await;

        let format = run_or_skip(
            self.ctx.config.run_format.as_deref(),
            self.ctx.detector.detect_format(),
            "run_format",
            &self.ctx.root,
            self.ctx.timeout_secs,
        ).await;

        let semgrep = run_or_skip(
            self.ctx.config.run_semgrep.as_deref(),
            self.ctx.detector.detect_semgrep(),
            "run_semgrep",
            &self.ctx.root,
            self.ctx.timeout_secs,
        ).await;

        let overall_ok = (tests.ok || tests.skipped)
            && (lint.ok || lint.skipped)
            && (format.ok || format.skipped)
            && (semgrep.ok || semgrep.skipped);

        let mut parts = vec![];
        parts.push(fmt_status("tests", &tests));
        parts.push(fmt_status("lint", &lint));
        parts.push(fmt_status("format", &format));
        parts.push(fmt_status("semgrep", &semgrep));

        let report = BaselineReport {
            tests,
            lint,
            format,
            semgrep,
            overall_ok,
            summary: parts.join(" | "),
        };

        let json = serde_json::to_string_pretty(&report).unwrap_or_default();
        if overall_ok {
            Ok(ToolResult::ok(json))
        } else {
            Ok(ToolResult::err(json))
        }
    }
}

async fn run_or_skip(
    override_cmd: Option<&str>,
    detected: Option<String>,
    tool_name: &str,
    root: &std::path::Path,
    timeout_secs: u64,
) -> RefactorResult {
    match resolve_command(override_cmd, detected, tool_name) {
        Ok(cmd) => run_command(&cmd, root, timeout_secs).await,
        Err(reason) => RefactorResult::skipped(reason),
    }
}

fn fmt_status(name: &str, r: &RefactorResult) -> String {
    if r.skipped {
        format!("{name}: skipped")
    } else if r.ok {
        format!("{name}: ✓")
    } else {
        format!("{name}: ✗ (exit {})", r.exit_code)
    }
}

// ---------------------------------------------------------------------------
// Public constructor helpers
// ---------------------------------------------------------------------------

/// Build all refactor tools from a shared context. Returns a Vec of boxed Tool impls.
pub fn build_refactor_tools(ctx: RefactorContext) -> Vec<Box<dyn crate::tools::manager::Tool>> {
    vec![
        Box::new(RunTestsTool { ctx: ctx.clone() }),
        Box::new(RunLintTool { ctx: ctx.clone() }),
        Box::new(RunFormatTool { ctx: ctx.clone() }),
        Box::new(RunSemgrepTool { ctx: ctx.clone() }),
        Box::new(GitDiffTool { ctx: ctx.clone() }),
        Box::new(BaselineTool { ctx }),
    ]
}
