use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The output shape returned by every refactor tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactorResult {
    pub ok: bool,
    pub exit_code: i32,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub skipped: bool,
    pub reason: String,
}

impl RefactorResult {
    pub fn ok(
        command: impl Into<String>,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
    ) -> Self {
        Self {
            ok: true,
            exit_code: 0,
            command: command.into(),
            stdout: stdout.into(),
            stderr: stderr.into(),
            skipped: false,
            reason: String::new(),
        }
    }

    pub fn fail(
        command: impl Into<String>,
        exit_code: i32,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
    ) -> Self {
        Self {
            ok: false,
            exit_code,
            command: command.into(),
            stdout: stdout.into(),
            stderr: stderr.into(),
            skipped: false,
            reason: String::new(),
        }
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            ok: false,
            exit_code: -1,
            command: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            skipped: true,
            reason: reason.into(),
        }
    }
}

/// User override config loaded from `.archcode/refactor.json`
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RefactorConfig {
    pub run_tests: Option<String>,
    pub run_lint: Option<String>,
    pub run_format: Option<String>,
    pub run_semgrep: Option<String>,
}

impl RefactorConfig {
    /// Load from `.archcode/refactor.json` relative to `root`.
    pub fn load(root: &Path) -> Self {
        let path = root.join(".archcode").join("refactor.json");
        if let Ok(content) = std::fs::read_to_string(&path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }
}

/// Detects which stack is in use and returns default commands.
#[derive(Debug, Clone)]
pub struct StackDetector {
    pub root: PathBuf,
}

impl StackDetector {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn has(&self, file: &str) -> bool {
        self.root.join(file).exists()
    }

    fn has_glob(&self, pattern: &str) -> bool {
        glob::glob(&self.root.join(pattern).to_string_lossy())
            .ok()
            .and_then(|mut it| it.next())
            .is_some()
    }

    /// Detect the `run_tests` command for this stack.
    pub fn detect_tests(&self) -> Option<String> {
        if self.has("Cargo.toml") {
            Some("cargo test".into())
        } else if self.has("package.json") {
            // Read scripts from package.json
            if let Ok(raw) = std::fs::read_to_string(self.root.join("package.json")) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if v["scripts"]["test"].is_string() {
                        return Some("npm test".into());
                    }
                }
            }
            Some("npm test".into())
        } else if self.has("pyproject.toml") || self.has("requirements.txt") || self.has("setup.py")
        {
            Some("pytest -q".into())
        } else if self.has("pom.xml") {
            Some("mvn test -q".into())
        } else if self.has("build.gradle") || self.has("build.gradle.kts") {
            Some("./gradlew test".into())
        } else if self.has_glob("*.sln") || self.has_glob("*.csproj") {
            Some("dotnet test".into())
        } else {
            None
        }
    }

    /// Detect the `run_lint` command for this stack.
    pub fn detect_lint(&self) -> Option<String> {
        if self.has("Cargo.toml") {
            Some("cargo clippy -- -D warnings".into())
        } else if self.has("package.json") {
            if let Ok(raw) = std::fs::read_to_string(self.root.join("package.json")) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if v["scripts"]["lint"].is_string() {
                        return Some("npm run lint".into());
                    }
                }
            }
            Some("npx eslint . --ext .ts,.js".into())
        } else if self.has("pyproject.toml") || self.has("requirements.txt") {
            Some("ruff check .".into())
        } else if self.has("pom.xml") {
            Some("mvn checkstyle:check".into())
        } else if self.has("build.gradle") || self.has("build.gradle.kts") {
            Some("./gradlew checkstyleMain".into())
        } else if self.has_glob("*.sln") || self.has_glob("*.csproj") {
            Some("dotnet build".into())
        } else {
            None
        }
    }

    /// Detect the `run_format` command for this stack.
    pub fn detect_format(&self) -> Option<String> {
        if self.has("Cargo.toml") {
            Some("cargo fmt".into())
        } else if self.has("package.json") {
            if let Ok(raw) = std::fs::read_to_string(self.root.join("package.json")) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if v["scripts"]["format"].is_string() {
                        return Some("npm run format".into());
                    }
                }
            }
            Some("npx prettier --write .".into())
        } else if self.has("pyproject.toml") || self.has("requirements.txt") {
            Some("ruff format .".into())
        } else if self.has("pom.xml") {
            Some("mvn fmt:format".into())
        } else if self.has_glob("*.sln") || self.has_glob("*.csproj") {
            Some("dotnet format".into())
        } else {
            None
        }
    }

    /// Detect whether semgrep binary is available and return its command.
    pub fn detect_semgrep(&self) -> Option<String> {
        // Check if semgrep is on PATH
        if std::process::Command::new("semgrep")
            .arg("--version")
            .output()
            .is_ok()
        {
            Some(format!(
                "semgrep --config {}/refactoring/semgrep-rules/solid-smells.yml .",
                self.root.display()
            ))
        } else {
            None
        }
    }
}

/// Resolve command with priority: override > auto-detect > skipped.
pub fn resolve_command(
    override_cmd: Option<&str>,
    detected: Option<String>,
    tool_name: &str,
) -> Result<String, String> {
    if let Some(cmd) = override_cmd {
        return Ok(cmd.to_string());
    }
    if let Some(cmd) = detected {
        return Ok(cmd);
    }
    Err(format!(
        "Could not auto-detect `{tool_name}` command. \
         Create `.archcode/refactor.json` with a `\"{tool_name}\"` key to override, \
         or install the appropriate toolchain for your stack."
    ))
}

/// Run a shell command in `cwd` with a timeout (default 10 min).
pub async fn run_command(command: &str, cwd: &Path, timeout_secs: u64) -> RefactorResult {
    use tokio::process::Command;

    // Split into shell invocation for cross-platform safety
    let output = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
            .output(),
    )
    .await;

    match output {
        Err(_) => RefactorResult::fail(
            command,
            -1,
            "",
            format!("Command timed out after {timeout_secs}s"),
        ),
        Ok(Err(e)) => RefactorResult::fail(command, -1, "", format!("Failed to spawn: {e}")),
        Ok(Ok(out)) => {
            let exit_code = out.status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if out.status.success() {
                RefactorResult::ok(command, stdout, stderr)
            } else {
                RefactorResult::fail(command, exit_code, stdout, stderr)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn detect_rust_stack() {
        let dir = tmp();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let det = StackDetector::new(dir.path());
        assert_eq!(det.detect_tests().unwrap(), "cargo test");
        assert_eq!(det.detect_lint().unwrap(), "cargo clippy -- -D warnings");
        assert_eq!(det.detect_format().unwrap(), "cargo fmt");
    }

    #[test]
    fn detect_python_stack() {
        let dir = tmp();
        fs::write(dir.path().join("requirements.txt"), "requests").unwrap();
        let det = StackDetector::new(dir.path());
        assert_eq!(det.detect_tests().unwrap(), "pytest -q");
        assert_eq!(det.detect_lint().unwrap(), "ruff check .");
        assert_eq!(det.detect_format().unwrap(), "ruff format .");
    }

    #[test]
    fn detect_node_stack() {
        let dir = tmp();
        fs::write(
            dir.path().join("package.json"),
            r#"{"scripts":{"test":"jest","lint":"eslint .","format":"prettier --write ."}}"#,
        )
        .unwrap();
        let det = StackDetector::new(dir.path());
        assert_eq!(det.detect_tests().unwrap(), "npm test");
    }

    #[test]
    fn detect_dotnet_stack() {
        let dir = tmp();
        fs::write(dir.path().join("MyApp.csproj"), "<Project />").unwrap();
        let det = StackDetector::new(dir.path());
        assert_eq!(det.detect_tests().unwrap(), "dotnet test");
        assert_eq!(det.detect_lint().unwrap(), "dotnet build");
        assert_eq!(det.detect_format().unwrap(), "dotnet format");
    }

    #[test]
    fn resolve_command_override_wins() {
        let result = resolve_command(Some("make test"), Some("cargo test".into()), "run_tests");
        assert_eq!(result.unwrap(), "make test");
    }

    #[test]
    fn resolve_command_detected_fallback() {
        let result = resolve_command(None, Some("cargo test".into()), "run_tests");
        assert_eq!(result.unwrap(), "cargo test");
    }

    #[test]
    fn resolve_command_skipped_when_none() {
        let result = resolve_command(None, None, "run_tests");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("run_tests"));
    }

    #[test]
    fn config_load_missing_returns_default() {
        let dir = tmp();
        let cfg = RefactorConfig::load(dir.path());
        assert!(cfg.run_tests.is_none());
    }

    #[test]
    fn config_load_override() {
        let dir = tmp();
        fs::create_dir_all(dir.path().join(".archcode")).unwrap();
        fs::write(
            dir.path().join(".archcode").join("refactor.json"),
            r#"{"run_tests":"make test"}"#,
        )
        .unwrap();
        let cfg = RefactorConfig::load(dir.path());
        assert_eq!(cfg.run_tests.unwrap(), "make test");
    }
}
