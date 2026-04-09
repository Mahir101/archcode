//! Per-language regex-based symbol extractor.
//! Extracts functions, classes, imports, types — no compiler required.

use regex::Regex;
use std::path::Path;

use super::graph::{ClassDef, ClassKind, FunctionDef, Language, TypeDef};

// ---------------------------------------------------------------------------
// Output of parsing one file
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct ParsedSymbols {
    pub imports: Vec<String>, // module paths imported by this file
    pub functions: Vec<FunctionDef>,
    pub classes: Vec<ClassDef>,
    pub types: Vec<TypeDef>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn parse_file(path: &str, source: &str) -> ParsedSymbols {
    let lang = Language::from_ext(
        Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(""),
    );

    match lang {
        Language::Rust => parse_rust(source),
        Language::Go => parse_go(source),
        Language::Python => parse_python(source),
        Language::TypeScript | Language::JavaScript => parse_ts_js(source),
        Language::Java => parse_java(source),
        Language::CSharp => parse_csharp(source),
        Language::Cpp | Language::C => parse_cpp(source),
        _ => ParsedSymbols::default(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn cyclomatic(body: &str) -> usize {
    // Count decision points: if, else if, for, while, match arm (=>), case, &&, ||, ?
    let mut n = 1usize;
    for kw in [
        "if ", "else if", " for ", " while ", " => ", "case ", " && ", " || ", "?;",
    ] {
        n += body.matches(kw).count();
    }
    n
}

fn line_of(source: &str, byte_offset: usize) -> usize {
    source[..byte_offset].matches('\n').count() + 1
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

fn parse_rust(src: &str) -> ParsedSymbols {
    let mut out = ParsedSymbols::default();

    // use statements
    let use_re = Regex::new(r"(?m)^use\s+([\w::{}, \n]+);").unwrap();
    for cap in use_re.captures_iter(src) {
        out.imports
            .push(cap[1].split_whitespace().collect::<String>());
    }

    // fn definitions
    let fn_re = Regex::new(
        r"(?m)^(\s*)(pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*(<[^>]*>)?\s*(\([^)]*\))",
    )
    .unwrap();
    for cap in fn_re.captures_iter(src) {
        let is_pub = cap.get(2).is_some();
        let is_async = cap[0].contains("async");
        let name = cap[3].to_string();
        let sig = cap[0].trim().to_string();
        let line = line_of(src, cap.get(0).unwrap().start());
        let body_start = src[cap.get(0).unwrap().end()..].find('{').unwrap_or(0);
        let body = &src[cap.get(0).unwrap().end()..];
        let complexity = cyclomatic(&body[..body_start.min(body.len())]);
        out.functions.push(FunctionDef {
            name,
            signature: sig,
            line_start: line,
            line_end: line,
            is_public: is_pub,
            is_async,
            complexity,
            fan_in: 0,
            fan_out: 0,
        });
    }

    // struct / enum / trait
    let type_re =
        Regex::new(r"(?m)^(pub(?:\s*\([^)]*\))?\s+)?(struct|enum|trait|type)\s+(\w+)").unwrap();
    for cap in type_re.captures_iter(src) {
        let is_pub = cap.get(1).is_some();
        let kind_str = &cap[2];
        let name = cap[3].to_string();
        let line = line_of(src, cap.get(0).unwrap().start());
        let kind = match kind_str {
            "struct" => ClassKind::Struct,
            "enum" => ClassKind::Enum,
            "trait" => ClassKind::Trait,
            _ => ClassKind::Class,
        };
        out.classes.push(ClassDef {
            name,
            kind,
            superclass: None,
            interfaces: vec![],
            line_start: line,
            line_end: line,
            is_public: is_pub,
        });
    }

    // impl X for Y
    let impl_re = Regex::new(r"(?m)^impl(?:<[^>]*>)?\s+(\w+)\s+for\s+(\w+)").unwrap();
    for cap in impl_re.captures_iter(src) {
        let trait_name = cap[1].to_string();
        let struct_name = cap[2].to_string();
        // Update interfaces on already-found class if possible
        if let Some(c) = out.classes.iter_mut().find(|c| c.name == struct_name) {
            c.interfaces.push(trait_name);
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

fn parse_go(src: &str) -> ParsedSymbols {
    let mut out = ParsedSymbols::default();

    let import_re = Regex::new(r#"(?m)"([\w./]+)""#).unwrap();
    for cap in import_re.captures_iter(src) {
        out.imports.push(cap[1].to_string());
    }

    let fn_re = Regex::new(r"(?m)^func\s+(?:\(\w+\s+\*?\w+\)\s+)?(\w+)\s*\(([^)]*)\)").unwrap();
    for cap in fn_re.captures_iter(src) {
        let name = cap[1].to_string();
        let sig = cap[0].to_string();
        let line = line_of(src, cap.get(0).unwrap().start());
        let is_pub = name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false);
        let complexity = cyclomatic(&src[cap.get(0).unwrap().end()..]);
        out.functions.push(FunctionDef {
            name,
            signature: sig,
            line_start: line,
            line_end: line,
            is_public: is_pub,
            is_async: false,
            complexity,
            fan_in: 0,
            fan_out: 0,
        });
    }

    let type_re =
        Regex::new(r"(?m)^type\s+(\w+)\s+(struct|interface|func|string|int|\w+)").unwrap();
    for cap in type_re.captures_iter(src) {
        let name = cap[1].to_string();
        let kind = match &cap[2] {
            s if s == "struct" => ClassKind::Struct,
            s if s == "interface" => ClassKind::Interface,
            _ => ClassKind::Class,
        };
        let line = line_of(src, cap.get(0).unwrap().start());
        out.classes.push(ClassDef {
            name,
            kind,
            superclass: None,
            interfaces: vec![],
            line_start: line,
            line_end: line,
            is_public: true,
        });
    }

    out
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

fn parse_python(src: &str) -> ParsedSymbols {
    let mut out = ParsedSymbols::default();

    let import_re = Regex::new(r"(?m)^(?:import|from)\s+([\w.]+)").unwrap();
    for cap in import_re.captures_iter(src) {
        out.imports.push(cap[1].to_string());
    }

    let fn_re = Regex::new(r"(?m)^(\s*)(async\s+)?def\s+(\w+)\s*\(([^)]*)\)").unwrap();
    for cap in fn_re.captures_iter(src) {
        let indent = cap[1].len();
        let is_async = cap.get(2).map_or(false, |m| !m.as_str().is_empty());
        let name = cap[3].to_string();
        let sig = cap[0].trim().to_string();
        let line = line_of(src, cap.get(0).unwrap().start());
        let is_pub = !name.starts_with('_');
        let complexity = cyclomatic(&src[cap.get(0).unwrap().end()..]);
        out.functions.push(FunctionDef {
            name,
            signature: sig,
            line_start: line,
            line_end: line,
            is_public: is_pub,
            is_async,
            complexity,
            fan_in: 0,
            fan_out: 0,
        });
    }

    let class_re = Regex::new(r"(?m)^class\s+(\w+)\s*(?:\(([^)]*)\))?:").unwrap();
    for cap in class_re.captures_iter(src) {
        let name = cap[1].to_string();
        let bases: Vec<String> = cap
            .get(2)
            .map(|m| {
                m.as_str()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let superclass = bases.first().cloned();
        let interfaces = if bases.len() > 1 {
            bases[1..].to_vec()
        } else {
            vec![]
        };
        let line = line_of(src, cap.get(0).unwrap().start());
        out.classes.push(ClassDef {
            name,
            kind: ClassKind::Class,
            superclass,
            interfaces,
            line_start: line,
            line_end: line,
            is_public: true,
        });
    }

    out
}

// ---------------------------------------------------------------------------
// TypeScript / JavaScript
// ---------------------------------------------------------------------------

fn parse_ts_js(src: &str) -> ParsedSymbols {
    let mut out = ParsedSymbols::default();

    let import_re =
        Regex::new(r#"(?m)(?:import|require)\s*(?:\(['" ]|from\s+['"])([^'")\s]+)"#).unwrap();
    for cap in import_re.captures_iter(src) {
        out.imports.push(cap[1].to_string());
    }

    // function declarations, arrow functions assigned to const/let/var
    let fn_re = Regex::new(
        r"(?m)(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\(|(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s+)?\(",
    ).unwrap();
    for cap in fn_re.captures_iter(src) {
        let name = cap
            .get(1)
            .or_else(|| cap.get(2))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let is_async = cap[0].contains("async");
        let is_pub = cap[0].contains("export");
        let line = line_of(src, cap.get(0).unwrap().start());
        let complexity = cyclomatic(&src[cap.get(0).unwrap().end()..]);
        out.functions.push(FunctionDef {
            name,
            signature: cap[0].trim().to_string(),
            line_start: line,
            line_end: line,
            is_public: is_pub,
            is_async,
            complexity,
            fan_in: 0,
            fan_out: 0,
        });
    }

    let class_re = Regex::new(
        r"(?m)(?:export\s+)?(?:abstract\s+)?class\s+(\w+)(?:\s+extends\s+(\w+))?(?:\s+implements\s+([\w,\s]+))?\s*\{",
    ).unwrap();
    for cap in class_re.captures_iter(src) {
        let name = cap[1].to_string();
        let superclass = cap.get(2).map(|m| m.as_str().to_string());
        let interfaces: Vec<String> = cap
            .get(3)
            .map(|m| {
                m.as_str()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let line = line_of(src, cap.get(0).unwrap().start());
        let is_pub = cap[0].contains("export");
        out.classes.push(ClassDef {
            name,
            kind: ClassKind::Class,
            superclass,
            interfaces,
            line_start: line,
            line_end: line,
            is_public: is_pub,
        });
    }

    // interface
    let iface_re =
        Regex::new(r"(?m)(?:export\s+)?interface\s+(\w+)(?:\s+extends\s+([\w,\s]+))?").unwrap();
    for cap in iface_re.captures_iter(src) {
        let name = cap[1].to_string();
        let interfaces: Vec<String> = cap
            .get(2)
            .map(|m| {
                m.as_str()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let line = line_of(src, cap.get(0).unwrap().start());
        out.classes.push(ClassDef {
            name,
            kind: ClassKind::Interface,
            superclass: None,
            interfaces,
            line_start: line,
            line_end: line,
            is_public: cap[0].contains("export"),
        });
    }

    out
}

// ---------------------------------------------------------------------------
// Java
// ---------------------------------------------------------------------------

fn parse_java(src: &str) -> ParsedSymbols {
    let mut out = ParsedSymbols::default();

    let import_re = Regex::new(r"(?m)^import\s+([\w.]+);").unwrap();
    for cap in import_re.captures_iter(src) {
        out.imports.push(cap[1].to_string());
    }

    let class_re = Regex::new(
        r"(?m)(?:public\s+)?(?:abstract\s+)?(?:class|interface|enum|record)\s+(\w+)(?:<[^>]*>)?(?:\s+extends\s+([\w<>, ]+))?(?:\s+implements\s+([\w<>, ]+))?",
    ).unwrap();
    for cap in class_re.captures_iter(src) {
        let name = cap[1].to_string();
        let superclass = cap.get(2).map(|m| m.as_str().trim().to_string());
        let interfaces: Vec<String> = cap
            .get(3)
            .map(|m| {
                m.as_str()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let line = line_of(src, cap.get(0).unwrap().start());
        out.classes.push(ClassDef {
            name,
            kind: ClassKind::Class,
            superclass,
            interfaces,
            line_start: line,
            line_end: line,
            is_public: cap[0].contains("public"),
        });
    }

    let fn_re = Regex::new(
        r"(?m)(?:public|private|protected|static|\s)+\s+\w+\s+(\w+)\s*\([^)]*\)\s*(?:throws[\w,\s]+)?\s*\{",
    ).unwrap();
    for cap in fn_re.captures_iter(src) {
        let name = cap[1].to_string();
        if name == "if" || name == "for" || name == "while" || name == "switch" {
            continue;
        }
        let line = line_of(src, cap.get(0).unwrap().start());
        let is_pub = cap[0].contains("public");
        let complexity = cyclomatic(&src[cap.get(0).unwrap().end()..]);
        out.functions.push(FunctionDef {
            name,
            signature: cap[0].trim().to_string(),
            line_start: line,
            line_end: line,
            is_public: is_pub,
            is_async: false,
            complexity,
            fan_in: 0,
            fan_out: 0,
        });
    }

    out
}

// ---------------------------------------------------------------------------
// C#
// ---------------------------------------------------------------------------

fn parse_csharp(src: &str) -> ParsedSymbols {
    let mut out = ParsedSymbols::default();

    let using_re = Regex::new(r"(?m)^using\s+([\w.]+);").unwrap();
    for cap in using_re.captures_iter(src) {
        out.imports.push(cap[1].to_string());
    }

    let class_re = Regex::new(
        r"(?m)(?:public|private|internal|protected|abstract|sealed|\s)+\s+(?:class|interface|struct|enum|record)\s+(\w+)(?:\s*:\s*([\w<>, ]+))?",
    ).unwrap();
    for cap in class_re.captures_iter(src) {
        let name = cap[1].to_string();
        let bases: Vec<String> = cap
            .get(2)
            .map(|m| {
                m.as_str()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let superclass = bases.first().cloned();
        let interfaces = if bases.len() > 1 {
            bases[1..].to_vec()
        } else {
            vec![]
        };
        let line = line_of(src, cap.get(0).unwrap().start());
        out.classes.push(ClassDef {
            name,
            kind: ClassKind::Class,
            superclass,
            interfaces,
            line_start: line,
            line_end: line,
            is_public: cap[0].contains("public"),
        });
    }

    let fn_re = Regex::new(
        r"(?m)(?:public|private|protected|internal|static|virtual|override|async|\s)+\s+\w+\s+(\w+)\s*\([^)]*\)\s*(?:where[^{]*)?\s*\{",
    ).unwrap();
    for cap in fn_re.captures_iter(src) {
        let name = cap[1].to_string();
        if matches!(name.as_str(), "if" | "while" | "for" | "foreach" | "switch") {
            continue;
        }
        let is_async = cap[0].contains("async");
        let line = line_of(src, cap.get(0).unwrap().start());
        let complexity = cyclomatic(&src[cap.get(0).unwrap().end()..]);
        out.functions.push(FunctionDef {
            name,
            signature: cap[0].trim().to_string(),
            line_start: line,
            line_end: line,
            is_public: cap[0].contains("public"),
            is_async,
            complexity,
            fan_in: 0,
            fan_out: 0,
        });
    }

    out
}

// ---------------------------------------------------------------------------
// C / C++
// ---------------------------------------------------------------------------

fn parse_cpp(src: &str) -> ParsedSymbols {
    let mut out = ParsedSymbols::default();

    let include_re = Regex::new(r#"(?m)#include\s+[<"]([\w./]+)[>"]"#).unwrap();
    for cap in include_re.captures_iter(src) {
        out.imports.push(cap[1].to_string());
    }

    let class_re = Regex::new(
        r"(?m)(?:class|struct)\s+(\w+)(?:\s*:\s*(?:public|private|protected)?\s*([\w:, ]+))?\s*\{",
    )
    .unwrap();
    for cap in class_re.captures_iter(src) {
        let name = cap[1].to_string();
        let superclass = cap.get(2).map(|m| m.as_str().trim().to_string());
        let line = line_of(src, cap.get(0).unwrap().start());
        out.classes.push(ClassDef {
            name,
            kind: ClassKind::Class,
            superclass,
            interfaces: vec![],
            line_start: line,
            line_end: line,
            is_public: true,
        });
    }

    // Simple function pattern: return_type name(params) {
    let fn_re =
        Regex::new(r"(?m)^(?:[\w:*&<> ]+\s+)+(\w+)\s*\([^;{]*\)\s*(?:const\s*)?\{").unwrap();
    for cap in fn_re.captures_iter(src) {
        let name = cap[1].to_string();
        if matches!(name.as_str(), "if" | "while" | "for" | "switch" | "else") {
            continue;
        }
        let line = line_of(src, cap.get(0).unwrap().start());
        let complexity = cyclomatic(&src[cap.get(0).unwrap().end()..]);
        out.functions.push(FunctionDef {
            name,
            signature: cap[0].trim().to_string(),
            line_start: line,
            line_end: line,
            is_public: true,
            is_async: false,
            complexity,
            fan_in: 0,
            fan_out: 0,
        });
    }

    out
}
