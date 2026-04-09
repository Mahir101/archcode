use async_trait::async_trait;
use regex::Regex;

use super::manager::{Decision, EvalContext, GuardRule};

// ---------------------------------------------------------------------------
// Dangerous command patterns
// ---------------------------------------------------------------------------

const DANGEROUS_PATTERNS: &[&str] = &[
    r"rm\s+-rf\s+/",
    r"mkfs",
    r"dd\s+if=",
    r">\s*/dev/sd",
    r"chmod\s+-R\s+777\s+/",
    r"curl\s+.*\|\s*bash",
    r"wget\s+.*\|\s*sh",
    r":(){ :|:& };:",   // fork bomb
    r"sudo\s+rm\s+-rf",
];

pub struct DangerousCommandRule;

#[async_trait]
impl GuardRule for DangerousCommandRule {
    async fn evaluate(&self, ctx: &EvalContext) -> Option<Decision> {
        if ctx.tool_name != "Bash" {
            return None;
        }
        for pat in DANGEROUS_PATTERNS {
            if let Ok(re) = Regex::new(pat) {
                if re.is_match(&ctx.input) {
                    return Some(Decision::deny(format!(
                        "Dangerous command pattern detected: {pat}"
                    )));
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Working directory confinement
// ---------------------------------------------------------------------------

pub struct WorkingDirRule;

#[async_trait]
impl GuardRule for WorkingDirRule {
    async fn evaluate(&self, ctx: &EvalContext) -> Option<Decision> {
        if !matches!(ctx.tool_name.as_str(), "Write" | "Edit" | "Bash") {
            return None;
        }
        if ctx.working_dir.is_empty() {
            return None;
        }
        // If input references an absolute path outside cwd, ask
        let input = &ctx.input;
        if input.contains("/etc/passwd")
            || input.contains("/etc/shadow")
            || input.contains("~/.ssh")
            || input.contains("~/.aws")
        {
            return Some(Decision::deny(
                "Attempt to access sensitive system path detected",
            ));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Sensitive file rule
// ---------------------------------------------------------------------------

const SENSITIVE_FILES: &[&str] = &[
    ".env",
    ".envrc",
    "id_rsa",
    "id_ed25519",
    ".aws/credentials",
    ".npmrc",
    ".pypirc",
];

pub struct SensitiveFileRule;

#[async_trait]
impl GuardRule for SensitiveFileRule {
    async fn evaluate(&self, ctx: &EvalContext) -> Option<Decision> {
        if !matches!(ctx.tool_name.as_str(), "Read" | "Write" | "Edit") {
            return None;
        }
        for sf in SENSITIVE_FILES {
            if ctx.input.contains(sf) {
                return Some(Decision::ask(format!(
                    "Accessing potentially sensitive file: {sf}"
                )));
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Default policy (last resort — allow)
// ---------------------------------------------------------------------------

pub struct DefaultPolicyRule;

#[async_trait]
impl GuardRule for DefaultPolicyRule {
    async fn evaluate(&self, _ctx: &EvalContext) -> Option<Decision> {
        Some(Decision::allow("Default policy: allow"))
    }
}
