//! Agent-facing KG tools: KGQuery, KGRelate, KGBlast, KGRisk, KGLint, KGIndex.

use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::event::Event;
use crate::tools::manager::{Tool, ToolDefinition, ToolResult};
use super::manager::KGManager;
use super::lint::{run_linters, LintStore};

// ---------------------------------------------------------------------------
// KGIndex — index a file or directory
// ---------------------------------------------------------------------------

pub struct KGIndexTool {
    pub kg: Arc<KGManager>,
}

#[async_trait]
impl Tool for KGIndexTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "KGIndex".into(),
            description: "Index a file or directory into the Knowledge Graph. Extracts functions, classes, imports, and cross-language FFI edges.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File or directory path to index." }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, args: Value, events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let path = match args["path"].as_str() {
            Some(p) => p.to_string(),
            None => return Ok(ToolResult::err("Missing 'path'")),
        };

        let kg = self.kg.clone();
        let path_clone = path.clone();

        if let Some(ch) = &events {
            let _ = ch.send(Event::kg(format!("Indexing {path}"))).await;
        }

        tokio::task::spawn_blocking(move || {
            let meta = std::fs::metadata(&path_clone);
            if let Ok(m) = meta {
                if m.is_dir() {
                    kg.index_dir(&path_clone);
                } else {
                    kg.index_file(&path_clone);
                }
            }
        }).await?;

        Ok(ToolResult::ok(format!("Indexed {path}. {}", self.kg.stats())))
    }
}

// ---------------------------------------------------------------------------
// KGQuery — find neighbours of a file/symbol
// ---------------------------------------------------------------------------

pub struct KGQueryTool {
    pub kg: Arc<KGManager>,
}

#[async_trait]
impl Tool for KGQueryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "KGQuery".into(),
            description: "Query the Knowledge Graph. Given a file path or symbol name, returns all related nodes with edge types (imports, calls, extends, implements, co_changes, ffi).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "File path or symbol key (e.g. 'src/auth.rs' or 'src/auth.rs::AuthService')." }
                },
                "required": ["key"]
            }),
        }
    }

    async fn execute(&self, args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let key = match args["key"].as_str() {
            Some(k) => k.to_string(),
            None => return Ok(ToolResult::err("Missing 'key'")),
        };

        let results = self.kg.query_neighbours(&key);
        if results.is_empty() {
            // Try search
            let search = self.kg.search(&key);
            if search.is_empty() {
                return Ok(ToolResult::ok(format!("No results for '{key}' in KG. Use KGIndex first.")));
            }
            let out = search.iter()
                .map(|r| format!("[{}] {}", r.kind, r.key))
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(ToolResult::ok(format!("Search results for '{key}':\n{out}")));
        }

        let out = results.iter()
            .map(|r| format!("  --[{}(w={:.2})]--> [{}] {}", r.edge, r.weight, r.target_kind, r.target))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult::ok(format!("{key}\n{out}")))
    }
}

// ---------------------------------------------------------------------------
// KGSearch — full-text search across all nodes
// ---------------------------------------------------------------------------

pub struct KGSearchTool {
    pub kg: Arc<KGManager>,
}

#[async_trait]
impl Tool for KGSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "KGSearch".into(),
            description: "Search the Knowledge Graph for any symbol, file, class, or function matching a query string.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search term (partial match, case-insensitive)." }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let query = match args["query"].as_str() {
            Some(q) => q.to_string(),
            None => return Ok(ToolResult::err("Missing 'query'")),
        };

        let results = self.kg.search(&query);
        if results.is_empty() {
            return Ok(ToolResult::ok(format!("No matches for '{query}'")));
        }
        let out = results.iter()
            .take(50)
            .map(|r| format!("[{}] {}", r.kind, r.key))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolResult::ok(format!("{} results for '{query}':\n{out}", results.len())))
    }
}

// ---------------------------------------------------------------------------
// KGBlast — blast radius / impact analysis
// ---------------------------------------------------------------------------

pub struct KGBlastTool {
    pub kg: Arc<KGManager>,
}

#[async_trait]
impl Tool for KGBlastTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "KGBlast".into(),
            description: "Compute the blast radius of a change: what files, classes, and functions would be affected if this symbol changes. Returns a depth-ordered impact list.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "File path or symbol key to analyze." }
                },
                "required": ["key"]
            }),
        }
    }

    async fn execute(&self, args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let key = match args["key"].as_str() {
            Some(k) => k.to_string(),
            None => return Ok(ToolResult::err("Missing 'key'")),
        };

        let blast = self.kg.blast_radius(&key);
        if blast.is_empty() {
            return Ok(ToolResult::ok(format!("No downstream impact found for '{key}'. Index the codebase first with KGIndex.")));
        }

        let out = blast.iter()
            .map(|n| format!("  depth={} [{}] {}", n.depth, n.kind, n.key))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult::ok(format!(
            "Blast radius for '{key}' ({} nodes affected):\n{out}",
            blast.len()
        )))
    }
}

// ---------------------------------------------------------------------------
// KGRisk — show highest risk functions (complexity × fan_in)
// ---------------------------------------------------------------------------

pub struct KGRiskTool {
    pub kg: Arc<KGManager>,
}

#[async_trait]
impl Tool for KGRiskTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "KGRisk".into(),
            description: "List the highest-risk functions in the codebase ranked by risk_score = complexity × (fan_in + 1). High-risk functions are most likely to cause bugs when changed.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "top": { "type": "integer", "description": "Number of top results to return (default 20)." }
                }
            }),
        }
    }

    async fn execute(&self, args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let top = args["top"].as_u64().unwrap_or(20) as usize;
        let scores = self.kg.risk_scores();

        if scores.is_empty() {
            return Ok(ToolResult::ok("No function data in KG. Run KGIndex first."));
        }

        let out = scores.iter()
            .take(top)
            .map(|r| format!(
                "  score={:.1} complexity={} fan_in={} → {}",
                r.score, r.complexity, r.fan_in, r.name
            ))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult::ok(format!("Top {top} highest-risk functions:\n{out}")))
    }
}

// ---------------------------------------------------------------------------
// KGRelate — manually record a relation
// ---------------------------------------------------------------------------

pub struct KGRelateTool {
    pub kg: Arc<KGManager>,
}

#[async_trait]
impl Tool for KGRelateTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "KGRelate".into(),
            description: "Record a custom relation between two symbols or files in the Knowledge Graph.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Source file or symbol." },
                    "to":   { "type": "string", "description": "Target file or symbol." },
                    "kind": { "type": "string", "description": "Relation kind: imports|calls|extends|implements|related|co_changes." }
                },
                "required": ["from", "to", "kind"]
            }),
        }
    }

    async fn execute(&self, args: Value, _events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let from = match args["from"].as_str() { Some(s) => s.to_string(), None => return Ok(ToolResult::err("Missing 'from'")) };
        let to = match args["to"].as_str() { Some(s) => s.to_string(), None => return Ok(ToolResult::err("Missing 'to'")) };
        let kind = args["kind"].as_str().unwrap_or("related");

        use super::graph::{EdgeKind, KGNode, FileDef, Language};

        let edge_kind = match kind {
            "imports" => EdgeKind::Imports,
            "calls" => EdgeKind::Calls,
            "extends" => EdgeKind::Extends,
            "implements" => EdgeKind::Implements,
            "co_changes" => EdgeKind::CoChanges,
            _ => EdgeKind::Related,
        };

        let from_ni = self.kg.get_or_create(&from, KGNode::File(FileDef {
            path: from.clone(), language: Language::Unknown("manual".into()),
            size_bytes: 0, line_count: 0, churn: 0, mtime: 0,
        }));
        let to_ni = self.kg.get_or_create(&to, KGNode::File(FileDef {
            path: to.clone(), language: Language::Unknown("manual".into()),
            size_bytes: 0, line_count: 0, churn: 0, mtime: 0,
        }));

        use super::graph::KGEdge;
        self.kg.add_edge_once(from_ni, to_ni, KGEdge::new(edge_kind));

        Ok(ToolResult::ok(format!("Recorded: {from} --[{kind}]--> {to}")))
    }
}

// ---------------------------------------------------------------------------
// KGLint — run linters and annotate KG
// ---------------------------------------------------------------------------

pub struct KGLintTool {
    pub kg: Arc<KGManager>,
    pub lint_store: Arc<std::sync::Mutex<LintStore>>,
}

#[async_trait]
impl Tool for KGLintTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "KGLint".into(),
            description: "Run language-specific linters (clippy, golangci-lint, mypy, eslint, checkstyle, dotnet, clang-tidy) on the codebase and return annotated diagnostics grouped by file.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "cwd": { "type": "string", "description": "Directory to run linters in (default: current dir)." },
                    "file": { "type": "string", "description": "Optional: only show results for this file." }
                }
            }),
        }
    }

    async fn execute(&self, args: Value, events: Option<mpsc::Sender<Event>>) -> Result<ToolResult> {
        let cwd = args["cwd"].as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| std::env::current_dir()
                .unwrap_or_default().to_string_lossy().to_string());

        let filter_file = args["file"].as_str().map(|s| s.to_string());

        if let Some(ch) = &events {
            let _ = ch.send(Event::kg(format!("Running linters in {cwd}"))).await;
        }

        use super::graph::Language;
        let all_langs = vec![
            Language::Rust, Language::Go, Language::Python,
            Language::TypeScript, Language::Java, Language::CSharp, Language::Cpp,
        ];

        let cwd_clone = cwd.clone();
        let diags = tokio::task::spawn_blocking(move || {
            run_linters(&cwd_clone, &all_langs)
        }).await?;

        let mut store = self.lint_store.lock().unwrap();
        store.ingest(diags);

        let summary = store.summary();

        if let Some(ref file) = filter_file {
            let file_diags = store.for_file(file);
            if file_diags.is_empty() {
                return Ok(ToolResult::ok(format!("No lint issues for {file}. {summary}")));
            }
            let out = file_diags.iter()
                .map(|d| format!("  {}:{} [{}] {} ({})", d.line, d.col, d.severity, d.message, d.code))
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(ToolResult::ok(format!("{file}:\n{out}\n\n{summary}")));
        }

        // Return all grouped by file
        let all_files: Vec<String> = store.diagnostics.keys().cloned().collect();
        let mut out_parts = vec![summary];
        for file in all_files.iter().take(20) {
            let diags = store.for_file(file);
            out_parts.push(format!("\n{file}:"));
            for d in diags.iter().take(5) {
                out_parts.push(format!("  {}:{} [{}] {}", d.line, d.col, d.severity, d.message));
            }
            if diags.len() > 5 {
                out_parts.push(format!("  ... and {} more", diags.len() - 5));
            }
        }

        Ok(ToolResult::ok(out_parts.join("\n")))
    }
}
