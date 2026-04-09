use std::path::{Path, PathBuf};

const INSTRUCTION_FILES: &[&str] = &["CLAUDE.md", "AGENTS.md", "ARCHCODE.md"];

pub struct DiscoveredFiles {
    pub project_files: Vec<PathBuf>,
}

pub fn discover_instruction_files(cwd: &Path) -> DiscoveredFiles {
    let mut project_files = vec![];

    // Walk up from cwd
    let mut dir = cwd.to_path_buf();
    loop {
        for name in INSTRUCTION_FILES {
            let candidate = dir.join(name);
            if candidate.exists() {
                project_files.push(candidate);
            }
        }
        if !dir.pop() {
            break;
        }
    }

    DiscoveredFiles { project_files }
}
