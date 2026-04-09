use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub trigger: String,
    pub prompt: String,
    pub source: String,
}

pub struct SkillManager {
    skills: Vec<Skill>,
    lookup: HashMap<String, usize>,
}

impl SkillManager {
    pub fn new() -> Self {
        Self { skills: vec![], lookup: HashMap::new() }
    }

    /// Load skills from standard directories: .bitcode/, .claude/, .agents/
    /// in both home dir and current dir.
    pub fn load_default() -> Self {
        let mut mgr = Self::new();
        let dirs = skill_dirs();
        for dir in dirs {
            if dir.exists() {
                let _ = mgr.load_from_dir(&dir);
            }
        }
        mgr
    }

    pub fn load_from_dir(&mut self, dir: &Path) -> Result<()> {
        for entry in walkdir::WalkDir::new(dir)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(s) = parse_skill_file(path) {
                    let idx = self.skills.len();
                    self.lookup.insert(s.name.clone(), idx);
                    self.skills.push(s);
                }
            }
        }
        Ok(())
    }

    pub fn list(&self) -> &[Skill] {
        &self.skills
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.name == name)
    }
}

fn skill_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = vec![];
    let subdirs = [".bitcode/skills", ".claude/skills", ".agents/skills"];

    // Home dir via HOME env var
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        for sub in &subdirs {
            dirs.push(home.join(sub));
        }
    }

    // Current dir
    if let Ok(cwd) = std::env::current_dir() {
        for sub in &subdirs {
            dirs.push(cwd.join(sub));
        }
    }

    dirs
}

/// Parse a YAML-frontmatter + markdown skill file.
fn parse_skill_file(path: &Path) -> Result<Skill> {
    let content = std::fs::read_to_string(path)?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut description = String::new();
    let mut trigger = String::new();
    let mut prompt = content.clone();

    // Parse YAML frontmatter if present
    if content.starts_with("---") {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() >= 3 {
            let frontmatter = parts[1];
            prompt = parts[2].trim().to_string();

            for line in frontmatter.lines() {
                if let Some(val) = line.strip_prefix("description:") {
                    description = val.trim().trim_matches('"').to_string();
                } else if let Some(val) = line.strip_prefix("trigger:") {
                    trigger = val.trim().trim_matches('"').to_string();
                }
            }
        }
    }

    Ok(Skill {
        name,
        description,
        trigger,
        prompt,
        source: path.to_string_lossy().to_string(),
    })
}
