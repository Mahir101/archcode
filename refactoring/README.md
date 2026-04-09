# RapCode SOLID Refactoring Kit

A built-in, behavior-preserving refactoring toolkit integrated directly into RapCode.
Powered by [Refactoring.Guru](https://refactoring.guru) patterns, SOLID principles, and static analysis via Semgrep + language-native linters.

---

## Quick Start

```bash
# Activate refactoring mode (injects playbook into system prompt)
archcode --refactor

# Single-shot refactoring session
archcode --refactor -p "The OrderService class does too much—fix it"

# Tools are always available even without --refactor flag
archcode -p "Run refactor.baseline on this project"
```

---

## What It Does

| Layer | What it provides |
|---|---|
| **Playbook** | SOLID rules, safety contract, stop conditions (`PLAYBOOK_RULES.md`) |
| **Workflow** | Step-by-step refactoring loop with commit discipline (`PLAYBOOK_WORKFLOW.md`) |
| **Smell catalogue** | 12 code smells from Refactoring.Guru, each with a recipe reference |
| **Recipes** | Step-by-step playbooks for Extract Function, Extract Class, Introduce Parameter Object, Move Method, Replace Conditional with Polymorphism, Dependency Inversion |
| **Tool runner** | 6 agent-callable tools for running tests, lint, format, semgrep, git diff, and full baseline |
| **Semgrep rules** | Built-in SOLID smell rules for Python, TypeScript, JavaScript (`solid-smells.yml`) |
| **Auto-detection** | Detects stack (Rust/Node/Python/Java/Maven/Gradle/.NET) and selects the right commands |
| **Override config** | `.archcode/refactor.json` for per-project command customization |

---

## How Auto-Detection Works

RapCode inspects the project root for these files (in priority order):

| File detected | Tests | Lint | Format |
|---|---|---|---|
| `Cargo.toml` | `cargo test` | `cargo clippy -- -D warnings` | `cargo fmt` |
| `package.json` | `npm test` | `npm run lint` / eslint fallback | `npx prettier --write .` |
| `pyproject.toml` / `requirements.txt` | `pytest -q` | `ruff check .` | `ruff format .` |
| `pom.xml` | `mvn test -q` | `mvn checkstyle:check` | `mvn fmt:format` |
| `build.gradle` | `./gradlew test` | `./gradlew checkstyleMain` | — |
| `*.sln` / `*.csproj` | `dotnet test` | `dotnet build` | `dotnet format` |

If a command cannot be detected, the tool returns `skipped: true` with instructions.

---

## Overriding Commands

Create `.archcode/refactor.json` in your project root:

```json
{
  "run_tests": "make test",
  "run_lint": "make lint",
  "run_format": "make fmt",
  "run_semgrep": "semgrep --config p/owasp-top-ten ."
}
```

**Precedence: user override > auto-detect > skipped**

---

## Available Agent Tools

All tools return structured JSON:

```json
{
  "ok": true,
  "exitCode": 0,
  "command": "cargo test",
  "stdout": "...",
  "stderr": "...",
  "skipped": false,
  "reason": ""
}
```

| Tool | Description |
|---|---|
| `refactor.baseline` | Run all checks (tests + lint + format + semgrep). **Start every session here.** |
| `refactor.run_tests` | Run test suite only |
| `refactor.run_lint` | Run linter only |
| `refactor.run_format` | Run code formatter |
| `refactor.run_semgrep` | Scan for SOLID smells using `solid-smells.yml` |
| `refactor.git_diff` | Show current diff (staged or unstaged) |

---

## Semgrep Integration

Built-in rules live in `refactoring/semgrep-rules/solid-smells.yml`. They detect:

- **SRP violations**: Long functions (30+ lines), God Objects
- **OCP violations**: Type-tag conditionals with 3+ branches
- **DIP violations**: `new ConcreteClass()` inside constructors
- **ISP violations**: Functions with 5+ parameters
- **Feature Envy**: Deep access chains (5+ levels)
- **Primitive Obsession**: Magic string comparisons

Install semgrep: `pip install semgrep`

If semgrep is not installed, `refactor.run_semgrep` returns `skipped: true` with installation instructions.

---

## Sample Refactoring Session

```
User: "The PaymentProcessor class is 400 lines and handles validation, payment, and receipts."

Agent:
  → refactor.baseline
    tests: ✓ (38 pass) | lint: ✓ | format: ✓ | semgrep: 2 warnings

  → Identifies smell: Large Class / SRP violation in PaymentProcessor
  → Opens recipe: refactoring/recipes/extract-class.md

  → Adds characterization tests for PaymentProcessor.process()
  → refactor.run_tests → 40 pass ✅

  → Extracts PaymentValidator (validation logic) — 2 new files, 1 modified
  → refactor.run_tests → 40 pass ✅
  → refactor.run_lint  → 0 errors ✅
  → refactor.git_diff  → shows 3 files changed, +89 -47

  → Commits: "refactor(payment): Extract PaymentValidator (SRP)"

  → "Should I continue with extracting ReceiptService?"
```

---

## Directory Structure

```
refactoring/
├── README.md                          ← this file
├── PLAYBOOK_RULES.md                  ← safety contract + SOLID heuristics
├── PLAYBOOK_WORKFLOW.md               ← step-by-step refactoring loop
├── SMELLS.md                          ← 12 code smells with recipes
├── semgrep-rules/
│   └── solid-smells.yml               ← built-in semgrep rules
└── recipes/
    ├── extract-function.md
    ├── extract-class.md
    ├── introduce-parameter-object.md
    ├── move-method.md
    ├── replace-conditional-with-polymorphism.md
    └── dependency-inversion.md

src/refactor/
├── mod.rs                             ← module + REFACTOR_SYSTEM_SNIPPET
├── detector.rs                        ← stack detection, config loading, command runner
└── tools.rs                           ← 6 Tool implementations registered in RapCode
```

---

## Non-Goals

- This is a **guided toolkit**, not an automatic full-project refactorer.
- It will not rewrite your entire codebase in one go.
- It does not guarantee correctness — always review the diff before committing.
