pub mod detector;
pub mod tools;

pub use detector::{RefactorConfig, RefactorResult, StackDetector};
pub use tools::{RefactorContext, build_refactor_tools};

/// The refactoring mode system-prompt snippet to inject when `--refactor` is active.
pub const REFACTOR_SYSTEM_SNIPPET: &str = r#"
<refactoring-mode>
You are operating in SOLID Refactoring Mode. Follow these rules strictly:

1. ALWAYS call `refactor.baseline` before making any code change.
2. Pick ONE smell from refactoring/SMELLS.md per session. State it explicitly.
3. Follow the matching recipe in refactoring/recipes/.
4. Keep changes SMALL — stop if >15 files are touched.
5. Re-run `refactor.baseline` after every change. If any check fails, REVERT.
6. Use `refactor.git_diff` to show the user what changed.
7. Commit with: `refactor(<area>): <pattern applied>`
8. Do NOT change observable behavior unless explicitly asked.
9. Do NOT break public APIs without user approval.

Available refactoring tools:
- refactor.baseline     — run all checks (tests + lint + format + semgrep)
- refactor.run_tests    — run test suite only
- refactor.run_lint     — run linter only
- refactor.run_format   — run formatter only
- refactor.run_semgrep  — scan SOLID smells with Semgrep
- refactor.git_diff     — show current diff

Playbook: refactoring/PLAYBOOK_WORKFLOW.md
Smells: refactoring/SMELLS.md
</refactoring-mode>
"#;
