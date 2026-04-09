//! Core graph types: nodes, edges, and the KG graph itself.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Node kinds
// ---------------------------------------------------------------------------

/// A function or method in any language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub signature: String,
    pub line_start: usize,
    pub line_end: usize,
    pub is_public: bool,
    pub is_async: bool,
    /// Cyclomatic complexity (branches + 1)
    pub complexity: usize,
    /// How many other functions call this (filled by linker pass)
    pub fan_in: usize,
    /// How many functions this calls
    pub fan_out: usize,
}

/// A class, struct, or impl block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassDef {
    pub name: String,
    pub kind: ClassKind,
    pub superclass: Option<String>,
    pub interfaces: Vec<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub is_public: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClassKind {
    Class,
    Struct,
    Trait,
    Interface,
    Enum,
    Module,
}

/// A type alias or named type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDef {
    pub name: String,
    pub kind: String, // "alias", "enum", "union", "newtype"
    pub line: usize,
}

/// A file in the codebase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDef {
    pub path: String,
    pub language: Language,
    pub size_bytes: u64,
    pub line_count: usize,
    /// Git churn: number of commits that touched this file
    pub churn: usize,
    /// mtime as unix timestamp (for cache invalidation)
    pub mtime: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Language {
    Rust,
    Go,
    Python,
    TypeScript,
    JavaScript,
    Java,
    CSharp,
    Cpp,
    C,
    Unknown(String),
}

impl Language {
    pub fn from_ext(ext: &str) -> Self {
        match ext {
            "rs" => Self::Rust,
            "go" => Self::Go,
            "py" => Self::Python,
            "ts" | "tsx" => Self::TypeScript,
            "js" | "jsx" | "mjs" => Self::JavaScript,
            "java" => Self::Java,
            "cs" => Self::CSharp,
            "cpp" | "cxx" | "cc" => Self::Cpp,
            "c" | "h" | "hpp" => Self::C,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Java => "java",
            Self::CSharp => "csharp",
            Self::Cpp => "cpp",
            Self::C => "c",
            Self::Unknown(s) => s.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// Graph node: union of all node kinds
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KGNode {
    File(FileDef),
    Function(FunctionDef),
    Class(ClassDef),
    Type(TypeDef),
}

impl KGNode {
    pub fn label(&self) -> String {
        match self {
            Self::File(f) => f.path.clone(),
            Self::Function(f) => f.name.clone(),
            Self::Class(c) => c.name.clone(),
            Self::Type(t) => t.name.clone(),
        }
    }

    pub fn kind_str(&self) -> &str {
        match self {
            Self::File(_) => "file",
            Self::Function(_) => "function",
            Self::Class(_) => "class",
            Self::Type(_) => "type",
        }
    }
}

// ---------------------------------------------------------------------------
// Edge kinds
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KGEdge {
    pub kind: EdgeKind,
    /// For co-change edges: fraction of commits where both changed (0.0–1.0)
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EdgeKind {
    /// File imports another file/module
    Imports,
    /// File contains a symbol
    Contains,
    /// Class extends another class
    Extends,
    /// Class/struct implements interface/trait
    Implements,
    /// Function calls another function
    Calls,
    /// Function uses a type
    UsesType,
    /// Files frequently change together in git commits
    CoChanges,
    /// Cross-language FFI boundary
    FfiBridge(FfiKind),
    /// Generic "related" fallback
    Related,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FfiKind {
    PyO3,         // Rust ↔ Python
    CGo,          // Go ↔ C
    Jni,          // Java ↔ C++
    Wasm,         // Any → Wasm module
    Napi,         // Node.js ↔ Rust/C++
    ChildProcess, // any language calls another via subprocess
}

impl KGEdge {
    pub fn new(kind: EdgeKind) -> Self {
        Self { kind, weight: 1.0 }
    }
    pub fn with_weight(kind: EdgeKind, weight: f32) -> Self {
        Self { kind, weight }
    }
    pub fn kind_str(&self) -> String {
        match &self.kind {
            EdgeKind::Imports => "imports".into(),
            EdgeKind::Contains => "contains".into(),
            EdgeKind::Extends => "extends".into(),
            EdgeKind::Implements => "implements".into(),
            EdgeKind::Calls => "calls".into(),
            EdgeKind::UsesType => "uses_type".into(),
            EdgeKind::CoChanges => format!("co_changes({:.2})", self.weight),
            EdgeKind::FfiBridge(k) => format!("ffi:{k:?}").to_lowercase(),
            EdgeKind::Related => "related".into(),
        }
    }
}
