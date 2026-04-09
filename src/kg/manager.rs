//! KGManager: the central graph store.
//! Handles indexing, querying, blast radius, impact, and session tracking.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;

use super::graph::{EdgeKind, FfiKind, FileDef, KGEdge, KGNode, Language};
use super::parser::parse_file;

// ---------------------------------------------------------------------------
// KGManager
// ---------------------------------------------------------------------------

pub struct KGManager {
    pub(crate) graph: Arc<Mutex<DiGraph<KGNode, KGEdge>>>,
    /// path or symbol name → NodeIndex
    pub(crate) index: Arc<Mutex<HashMap<String, NodeIndex>>>,
    session_accesses: Arc<Mutex<Vec<String>>>,
}

impl KGManager {
    pub fn new() -> Self {
        Self {
            graph: Arc::new(Mutex::new(DiGraph::new())),
            index: Arc::new(Mutex::new(HashMap::new())),
            session_accesses: Arc::new(Mutex::new(vec![])),
        }
    }

    // -----------------------------------------------------------------------
    // Node management
    // -----------------------------------------------------------------------

    pub(crate) fn get_or_create(&self, key: &str, node: KGNode) -> NodeIndex {
        let mut graph = self.graph.lock().unwrap();
        let mut index = self.index.lock().unwrap();
        if let Some(&ni) = index.get(key) {
            return ni;
        }
        let ni = graph.add_node(node);
        index.insert(key.to_string(), ni);
        ni
    }

    fn update_node(&self, key: &str, node: KGNode) -> NodeIndex {
        let mut graph = self.graph.lock().unwrap();
        let mut index = self.index.lock().unwrap();
        if let Some(&ni) = index.get(key) {
            graph[ni] = node;
            return ni;
        }
        let ni = graph.add_node(node);
        index.insert(key.to_string(), ni);
        ni
    }

    pub(crate) fn add_edge_once(&self, from: NodeIndex, to: NodeIndex, edge: KGEdge) {
        let mut graph = self.graph.lock().unwrap();
        // Avoid exact duplicate (same kind)
        let exists = graph
            .edges_connecting(from, to)
            .any(|e| e.weight().kind_str() == edge.kind_str());
        if !exists {
            graph.add_edge(from, to, edge);
        }
    }

    // -----------------------------------------------------------------------
    // Index a single file — parses symbols and wires up graph edges
    // -----------------------------------------------------------------------

    pub fn index_file(&self, path: &str) {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let lang = Language::from_ext(ext);
        let size = content.len() as u64;
        let line_count = content.lines().count();
        let mtime = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let file_node = KGNode::File(FileDef {
            path: path.to_string(),
            language: lang,
            size_bytes: size,
            line_count,
            churn: 0,
            mtime,
        });
        let file_ni = self.update_node(path, file_node);

        // Parse symbols
        let symbols = parse_file(path, &content);

        // Import edges: File --[imports]--> File (or module)
        for import in &symbols.imports {
            let target_ni = self.get_or_create(
                import,
                KGNode::File(FileDef {
                    path: import.clone(),
                    language: Language::Unknown("module".into()),
                    size_bytes: 0,
                    line_count: 0,
                    churn: 0,
                    mtime: 0,
                }),
            );
            self.add_edge_once(file_ni, target_ni, KGEdge::new(EdgeKind::Imports));
        }

        // Function nodes: File --[contains]--> Function
        for func in &symbols.functions {
            let key = format!("{path}::{}", func.name);
            let fn_ni = self.update_node(&key, KGNode::Function(func.clone()));
            self.add_edge_once(file_ni, fn_ni, KGEdge::new(EdgeKind::Contains));
        }

        // Class nodes: File --[contains]--> Class  +  Class --[extends/implements]--> ...
        for class in &symbols.classes {
            let key = format!("{path}::{}", class.name);
            let cls_ni = self.update_node(&key, KGNode::Class(class.clone()));
            self.add_edge_once(file_ni, cls_ni, KGEdge::new(EdgeKind::Contains));

            if let Some(ref sup) = class.superclass {
                let sup_ni = self.get_or_create(sup, KGNode::Class(super::graph::ClassDef {
                    name: sup.clone(),
                    kind: super::graph::ClassKind::Class,
                    superclass: None,
                    interfaces: vec![],
                    line_start: 0,
                    line_end: 0,
                    is_public: true,
                }));
                self.add_edge_once(cls_ni, sup_ni, KGEdge::new(EdgeKind::Extends));
            }

            for iface in &class.interfaces {
                let iface_ni = self.get_or_create(iface, KGNode::Class(super::graph::ClassDef {
                    name: iface.clone(),
                    kind: super::graph::ClassKind::Interface,
                    superclass: None,
                    interfaces: vec![],
                    line_start: 0,
                    line_end: 0,
                    is_public: true,
                }));
                self.add_edge_once(cls_ni, iface_ni, KGEdge::new(EdgeKind::Implements));
            }
        }

        // FFI edge detection
        for (from_key, to_key, ffi_kind) in detect_ffi(path, &content) {
            let from_ni = self.get_or_create(&from_key, KGNode::File(FileDef {
                path: from_key.clone(),
                language: Language::Unknown("ffi".into()),
                size_bytes: 0, line_count: 0, churn: 0, mtime: 0,
            }));
            let to_ni = self.get_or_create(&to_key, KGNode::File(FileDef {
                path: to_key.clone(),
                language: Language::Unknown("ffi".into()),
                size_bytes: 0, line_count: 0, churn: 0, mtime: 0,
            }));
            self.add_edge_once(from_ni, to_ni, KGEdge::new(EdgeKind::FfiBridge(ffi_kind)));
        }

        // Record session access
        self.session_accesses.lock().unwrap().push(path.to_string());
    }

    /// Index all files in a directory tree.
    pub fn index_dir(&self, dir: &str) {
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path().to_string_lossy().to_string();
            let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
            if INDEXABLE_EXTS.contains(&ext) {
                self.index_file(&path);
            }
        }
        // After indexing all files, compute fan-in scores
        self.compute_fan_in();
    }

    // -----------------------------------------------------------------------
    // Git co-change coupling (Layer 3)
    // -----------------------------------------------------------------------

    /// Parse `git log` output and add co-change edges between files that
    /// frequently change together. Runs `git log --name-only --pretty=format:`.
    pub fn add_git_cochange_edges(&self, repo_dir: &str) {
        let output = std::process::Command::new("git")
            .args(["log", "--name-only", "--pretty=format:COMMIT"])
            .current_dir(repo_dir)
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return,
        };

        let text = String::from_utf8_lossy(&output.stdout);
        let mut commits: Vec<Vec<String>> = vec![];
        let mut current: Vec<String> = vec![];

        for line in text.lines() {
            if line == "COMMIT" {
                if !current.is_empty() {
                    commits.push(std::mem::take(&mut current));
                }
            } else if !line.trim().is_empty() {
                current.push(line.trim().to_string());
            }
        }
        if !current.is_empty() {
            commits.push(current);
        }

        // Count co-occurrences
        let mut pair_count: HashMap<(String, String), usize> = HashMap::new();
        let mut file_commit_count: HashMap<String, usize> = HashMap::new();

        for commit_files in &commits {
            for f in commit_files {
                *file_commit_count.entry(f.clone()).or_default() += 1;
            }
            for i in 0..commit_files.len() {
                for j in (i + 1)..commit_files.len() {
                    let a = commit_files[i].clone();
                    let b = commit_files[j].clone();
                    let key = if a < b { (a, b) } else { (b, a) };
                    *pair_count.entry(key).or_default() += 1;
                }
            }
        }

        // Add edges where coupling >= 0.3
        for ((a, b), count) in &pair_count {
            let max_commits = file_commit_count.get(a)
                .copied()
                .unwrap_or(0)
                .max(file_commit_count.get(b).copied().unwrap_or(0));
            if max_commits == 0 { continue; }
            let weight = *count as f32 / max_commits as f32;
            if weight >= 0.3 {
                let a_ni = self.get_or_create(a, KGNode::File(FileDef {
                    path: a.clone(),
                    language: Language::Unknown("git".into()),
                    size_bytes: 0, line_count: 0, churn: 0, mtime: 0,
                }));
                let b_ni = self.get_or_create(b, KGNode::File(FileDef {
                    path: b.clone(),
                    language: Language::Unknown("git".into()),
                    size_bytes: 0, line_count: 0, churn: 0, mtime: 0,
                }));
                self.add_edge_once(a_ni, b_ni, KGEdge::with_weight(EdgeKind::CoChanges, weight));
            }
        }

        // Update churn counts on file nodes
        for (path, count) in &file_commit_count {
            let idx = self.index.lock().unwrap().get(path.as_str()).copied();
            if let Some(ni) = idx {
                let mut graph = self.graph.lock().unwrap();
                if let KGNode::File(ref mut f) = graph[ni] {
                    f.churn = *count;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Fan-in computation (Layer 5)
    // -----------------------------------------------------------------------

    fn compute_fan_in(&self) {
        let graph = self.graph.lock().unwrap();
        // Build map: NodeIndex → in-degree for Calls edges
        let mut fan_in: HashMap<NodeIndex, usize> = HashMap::new();
        for edge in graph.edge_indices() {
            if let Some((_, target)) = graph.edge_endpoints(edge) {
                if graph[edge].kind == EdgeKind::Calls {
                    *fan_in.entry(target).or_default() += 1;
                }
            }
        }
        drop(graph);

        let mut graph = self.graph.lock().unwrap();
        for (ni, count) in fan_in {
            if let KGNode::Function(ref mut f) = graph[ni] {
                f.fan_in = count;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Blast radius / impact analysis (Layer 4)
    // -----------------------------------------------------------------------

    /// BFS forward from a file or symbol: return all nodes reachable via
    /// Calls, Imports, Extends, Implements, Contains — i.e., what would break.
    pub fn blast_radius(&self, key: &str) -> Vec<BlastNode> {
        let graph = self.graph.lock().unwrap();
        let index = self.index.lock().unwrap();

        let Some(&start_ni) = index.get(key) else {
            return vec![];
        };

        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();
        queue.push_back((start_ni, 0));
        visited.insert(start_ni);

        let mut results = vec![];

        while let Some((ni, depth)) = queue.pop_front() {
            if depth == 0 { continue; } // skip start node itself
            let node = &graph[ni];
            results.push(BlastNode {
                key: node.label(),
                kind: node.kind_str().to_string(),
                depth,
            });
            if depth >= 5 { continue; } // max depth
            for edge in graph.edges(ni) {
                let target = edge.target();
                if !visited.contains(&target) {
                    visited.insert(target);
                    queue.push_back((target, depth + 1));
                }
            }
        }

        results.sort_by_key(|n| n.depth);
        results
    }

    // -----------------------------------------------------------------------
    // Querying  (Layer 6)
    // -----------------------------------------------------------------------

    pub fn query_neighbours(&self, key: &str) -> Vec<QueryResult> {
        let graph = self.graph.lock().unwrap();
        let index = self.index.lock().unwrap();
        let Some(&ni) = index.get(key) else {
            return vec![];
        };

        graph.edges(ni)
            .map(|e| QueryResult {
                target: graph[e.target()].label(),
                target_kind: graph[e.target()].kind_str().to_string(),
                edge: e.weight().kind_str(),
                weight: e.weight().weight,
            })
            .collect()
    }

    /// Find all nodes matching a name substring.
    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let graph = self.graph.lock().unwrap();
        let q = query.to_lowercase();
        graph
            .node_indices()
            .filter_map(|ni| {
                let node = &graph[ni];
                if node.label().to_lowercase().contains(&q) {
                    Some(SearchResult {
                        key: node.label(),
                        kind: node.kind_str().to_string(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // Risk score (Layer 5)
    // -----------------------------------------------------------------------

    pub fn risk_scores(&self) -> Vec<RiskScore> {
        let graph = self.graph.lock().unwrap();
        let mut scores = vec![];
        for ni in graph.node_indices() {
            if let KGNode::Function(ref f) = graph[ni] {
                let score = f.complexity as f32 * (f.fan_in as f32 + 1.0);
                scores.push(RiskScore {
                    name: f.name.clone(),
                    score,
                    complexity: f.complexity,
                    fan_in: f.fan_in,
                });
            }
        }
        scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }

    // -----------------------------------------------------------------------
    // Session summary
    // -----------------------------------------------------------------------

    pub fn session_summary(&self) -> String {
        let accesses = self.session_accesses.lock().unwrap();
        if accesses.is_empty() {
            return "No file activity in current session.".into();
        }
        let unique: HashSet<&String> = accesses.iter().collect();
        format!(
            "Files accessed this session ({}):\n{}",
            unique.len(),
            unique.iter().map(|p| format!("  - {p}")).collect::<Vec<_>>().join("\n")
        )
    }

    pub fn stats(&self) -> String {
        let graph = self.graph.lock().unwrap();
        format!(
            "KG: {} nodes, {} edges",
            graph.node_count(),
            graph.edge_count()
        )
    }
}

impl Default for KGManager {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------
// Output types
// -----------------------------------------------------------------------

#[derive(Debug)]
pub struct BlastNode {
    pub key: String,
    pub kind: String,
    pub depth: usize,
}

#[derive(Debug)]
pub struct QueryResult {
    pub target: String,
    pub target_kind: String,
    pub edge: String,
    pub weight: f32,
}

#[derive(Debug)]
pub struct SearchResult {
    pub key: String,
    pub kind: String,
}

#[derive(Debug)]
pub struct RiskScore {
    pub name: String,
    pub score: f32,
    pub complexity: usize,
    pub fan_in: usize,
}

// -----------------------------------------------------------------------
// FFI detection (Layer 2)
// -----------------------------------------------------------------------

fn detect_ffi(path: &str, src: &str) -> Vec<(String, String, FfiKind)> {
    let mut edges = vec![];

    // PyO3: Rust crate that exposes to Python
    if path.ends_with(".rs") && (src.contains("pyo3") || src.contains("#[pymodule]") || src.contains("#[pyfunction]")) {
        edges.push((path.to_string(), "python:pyo3_binding".to_string(), FfiKind::PyO3));
    }

    // CGo: Go file with import "C"
    if path.ends_with(".go") && src.contains("import \"C\"") {
        edges.push((path.to_string(), "c:cgo_binding".to_string(), FfiKind::CGo));
    }

    // JNI: Java/Kotlin native method
    if (path.ends_with(".java") || path.ends_with(".kt")) && src.contains("native ") {
        edges.push((path.to_string(), "cpp:jni_binding".to_string(), FfiKind::Jni));
    }

    // NAPI: Node.js native addon
    if (path.ends_with(".cc") || path.ends_with(".cpp")) && src.contains("napi") {
        edges.push((path.to_string(), "js:napi_binding".to_string(), FfiKind::Napi));
    }

    // WASM target
    if path.ends_with(".rs") && (src.contains("wasm_bindgen") || src.contains("#[wasm_bindgen]")) {
        edges.push((path.to_string(), "js:wasm_binding".to_string(), FfiKind::Wasm));
    }

    // subprocess calls
    for subprocess_marker in ["subprocess.run", "subprocess.call", "exec.Command", "child_process", "ProcessBuilder"] {
        if src.contains(subprocess_marker) {
            edges.push((path.to_string(), "subprocess:shell".to_string(), FfiKind::ChildProcess));
            break;
        }
    }

    edges
}

const INDEXABLE_EXTS: &[&str] = &[
    "rs", "go", "py", "ts", "tsx", "js", "jsx", "mjs",
    "java", "cs", "cpp", "cxx", "cc", "c", "h", "hpp",
];
