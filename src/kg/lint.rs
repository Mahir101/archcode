//! Lint CLI integration — runs language-specific linters and stores results
//! as annotations on KG nodes.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use super::graph::Language;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintDiagnostic {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub tool: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
            Self::Info => write!(f, "info"),
            Self::Hint => write!(f, "hint"),
        }
    }
}

/// Run all available linters for a detected language set and return diagnostics.
pub fn run_linters(cwd: &str, languages: &[Language]) -> Vec<LintDiagnostic> {
    let mut all = vec![];
    for lang in languages {
        match lang {
            Language::Rust => all.extend(run_clippy(cwd)),
            Language::Go => all.extend(run_golangci(cwd)),
            Language::Python => all.extend(run_mypy(cwd)),
            Language::TypeScript | Language::JavaScript => all.extend(run_eslint(cwd)),
            Language::Java => all.extend(run_checkstyle(cwd)),
            Language::CSharp => all.extend(run_dotnet_analyze(cwd)),
            Language::Cpp | Language::C => all.extend(run_clang_tidy(cwd)),
            _ => {}
        }
    }
    all
}

// ---------------------------------------------------------------------------
// Rust: cargo clippy
// ---------------------------------------------------------------------------

fn run_clippy(cwd: &str) -> Vec<LintDiagnostic> {
    let out = std::process::Command::new("cargo")
        .args(["clippy", "--message-format=json", "--quiet"])
        .current_dir(cwd)
        .output();

    let out = match out {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    let mut diags = vec![];
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v["reason"] != "compiler-message" { continue; }
        let msg = &v["message"];
        let severity = match msg["level"].as_str().unwrap_or("") {
            "error" => Severity::Error,
            "warning" => Severity::Warning,
            _ => continue,
        };
        let text = msg["message"].as_str().unwrap_or("").to_string();
        let code = msg["code"]["code"].as_str().unwrap_or("").to_string();
        if let Some(span) = msg["spans"].as_array().and_then(|s| s.first()) {
            let file = span["file_name"].as_str().unwrap_or("").to_string();
            let line = span["line_start"].as_u64().unwrap_or(0) as usize;
            let col = span["column_start"].as_u64().unwrap_or(0) as usize;
            diags.push(LintDiagnostic { file, line, col, severity, code, message: text, tool: "clippy".into() });
        }
    }
    diags
}

// ---------------------------------------------------------------------------
// Go: golangci-lint
// ---------------------------------------------------------------------------

fn run_golangci(cwd: &str) -> Vec<LintDiagnostic> {
    let out = std::process::Command::new("golangci-lint")
        .args(["run", "--out-format=json"])
        .current_dir(cwd)
        .output();

    let out = match out {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    let mut diags = vec![];
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else { return diags };
    for issue in v["Issues"].as_array().unwrap_or(&vec![]) {
        let file = issue["Pos"]["Filename"].as_str().unwrap_or("").to_string();
        let line = issue["Pos"]["Line"].as_u64().unwrap_or(0) as usize;
        let col = issue["Pos"]["Column"].as_u64().unwrap_or(0) as usize;
        let text = issue["Text"].as_str().unwrap_or("").to_string();
        let code = issue["FromLinter"].as_str().unwrap_or("").to_string();
        diags.push(LintDiagnostic {
            file, line, col,
            severity: Severity::Warning,
            code,
            message: text,
            tool: "golangci-lint".into(),
        });
    }
    diags
}

// ---------------------------------------------------------------------------
// Python: mypy (JSON output via --output=json)
// ---------------------------------------------------------------------------

fn run_mypy(cwd: &str) -> Vec<LintDiagnostic> {
    let out = std::process::Command::new("mypy")
        .args([".", "--output=json", "--no-error-summary"])
        .current_dir(cwd)
        .output();

    let out = match out {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    let mut diags = vec![];
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let file = v["file"].as_str().unwrap_or("").to_string();
        let line_num = v["line"].as_u64().unwrap_or(0) as usize;
        let col = v["column"].as_u64().unwrap_or(0) as usize;
        let text = v["message"].as_str().unwrap_or("").to_string();
        let code = v["code"].as_str().unwrap_or("mypy").to_string();
        let severity = match v["severity"].as_str().unwrap_or("") {
            "error" => Severity::Error,
            _ => Severity::Warning,
        };
        diags.push(LintDiagnostic { file, line: line_num, col, severity, code, message: text, tool: "mypy".into() });
    }
    diags
}

// ---------------------------------------------------------------------------
// TypeScript/JavaScript: eslint
// ---------------------------------------------------------------------------

fn run_eslint(cwd: &str) -> Vec<LintDiagnostic> {
    let out = std::process::Command::new("npx")
        .args(["eslint", "--format=json", "."])
        .current_dir(cwd)
        .output();

    let out = match out {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    let mut diags = vec![];
    let Ok(files) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else { return diags };
    for file_result in files.as_array().unwrap_or(&vec![]) {
        let file = file_result["filePath"].as_str().unwrap_or("").to_string();
        for msg in file_result["messages"].as_array().unwrap_or(&vec![]) {
            let line = msg["line"].as_u64().unwrap_or(0) as usize;
            let col = msg["column"].as_u64().unwrap_or(0) as usize;
            let text = msg["message"].as_str().unwrap_or("").to_string();
            let code = msg["ruleId"].as_str().unwrap_or("").to_string();
            let severity = match msg["severity"].as_u64().unwrap_or(1) {
                2 => Severity::Error,
                _ => Severity::Warning,
            };
            diags.push(LintDiagnostic { file: file.clone(), line, col, severity, code, message: text, tool: "eslint".into() });
        }
    }
    diags
}

// ---------------------------------------------------------------------------
// Java: checkstyle (simple text parse)
// ---------------------------------------------------------------------------

fn run_checkstyle(cwd: &str) -> Vec<LintDiagnostic> {
    // Runs if checkstyle jar exists; otherwise skipped gracefully
    let out = std::process::Command::new("checkstyle")
        .args(["-f", "xml", "-r", "."])
        .current_dir(cwd)
        .output();

    let out = match out {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    // Very simple text-based extraction from XML
    let text = String::from_utf8_lossy(&out.stdout);
    let mut diags = vec![];
    let file_re = regex::Regex::new(r#"<file name="([^"]+)""#).unwrap();
    let err_re = regex::Regex::new(r#"line="(\d+)"[^>]*col="(\d+)"[^>]*severity="(\w+)"[^>]*message="([^"]+)""#).unwrap();
    let mut current_file = String::new();
    for line in text.lines() {
        if let Some(cap) = file_re.captures(line) { current_file = cap[1].to_string(); }
        if let Some(cap) = err_re.captures(line) {
            diags.push(LintDiagnostic {
                file: current_file.clone(),
                line: cap[1].parse().unwrap_or(0),
                col: cap[2].parse().unwrap_or(0),
                severity: if &cap[3] == "error" { Severity::Error } else { Severity::Warning },
                code: "checkstyle".into(),
                message: cap[4].to_string(),
                tool: "checkstyle".into(),
            });
        }
    }
    diags
}

// ---------------------------------------------------------------------------
// C#: dotnet analyze (Roslyn analyzers)
// ---------------------------------------------------------------------------

fn run_dotnet_analyze(cwd: &str) -> Vec<LintDiagnostic> {
    let out = std::process::Command::new("dotnet")
        .args(["build", "--no-restore", "-v", "q"])
        .current_dir(cwd)
        .output();
    let out = match out { Ok(o) => o, Err(_) => return vec![] };
    let text = String::from_utf8_lossy(&out.stderr);
    let re = regex::Regex::new(r"([^:\s]+\.cs)\((\d+),(\d+)\):\s*(error|warning)\s+(CS\w+):\s*(.+)").unwrap();
    let mut diags = vec![];
    for cap in re.captures_iter(&text) {
        diags.push(LintDiagnostic {
            file: cap[1].to_string(),
            line: cap[2].parse().unwrap_or(0),
            col: cap[3].parse().unwrap_or(0),
            severity: if &cap[4] == "error" { Severity::Error } else { Severity::Warning },
            code: cap[5].to_string(),
            message: cap[6].to_string(),
            tool: "dotnet".into(),
        });
    }
    diags
}

// ---------------------------------------------------------------------------
// C/C++: clang-tidy
// ---------------------------------------------------------------------------

fn run_clang_tidy(cwd: &str) -> Vec<LintDiagnostic> {
    let out = std::process::Command::new("clang-tidy")
        .args(["--quiet", "*.cpp", "*.c", "*.h"])
        .current_dir(cwd)
        .output();
    let out = match out { Ok(o) => o, Err(_) => return vec![] };
    let text = String::from_utf8_lossy(&out.stdout);
    let re = regex::Regex::new(r"([^:\s]+)\:(\d+)\:(\d+):\s*(error|warning|note):\s*(.+?)\s*\[([^\]]+)\]").unwrap();
    let mut diags = vec![];
    for cap in re.captures_iter(&text) {
        if &cap[4] == "note" { continue; }
        diags.push(LintDiagnostic {
            file: cap[1].to_string(),
            line: cap[2].parse().unwrap_or(0),
            col: cap[3].parse().unwrap_or(0),
            severity: if &cap[4] == "error" { Severity::Error } else { Severity::Warning },
            code: cap[6].to_string(),
            message: cap[5].to_string(),
            tool: "clang-tidy".into(),
        });
    }
    diags
}

// ---------------------------------------------------------------------------
// Annotation store: file path → diagnostics list
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct LintStore {
    pub diagnostics: HashMap<String, Vec<LintDiagnostic>>,
}

impl LintStore {
    pub fn new() -> Self { Self::default() }

    pub fn ingest(&mut self, diags: Vec<LintDiagnostic>) {
        for d in diags {
            self.diagnostics.entry(d.file.clone()).or_default().push(d);
        }
    }

    pub fn for_file(&self, path: &str) -> &[LintDiagnostic] {
        self.diagnostics.get(path).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn summary(&self) -> String {
        let total: usize = self.diagnostics.values().map(|v| v.len()).sum();
        let errors: usize = self.diagnostics.values()
            .flat_map(|v| v.iter())
            .filter(|d| d.severity == Severity::Error)
            .count();
        format!("Lint: {total} diagnostics ({errors} errors) across {} files", self.diagnostics.len())
    }
}
