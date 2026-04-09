# RapCode Refactoring Playbook — Workflow

This document defines the **exact loop** the agent follows for every refactoring session.

---

## Pre-flight Checklist

Before touching any code, the agent must:

1. **Run `refactor.baseline`** — collects tests + lint + format + semgrep results.
2. Record the baseline in the session context (pass/fail counts, lint counts).
3. Identify the smell to address (one smell per session).
4. Confirm with the user if any step fails at baseline (do not proceed with red tests).

---

## The Refactoring Loop

```
┌─────────────────────────────────────────────────────────────────┐
│  STEP 1  │  Run baseline (refactor.baseline)                     │
│           │  → All green? Proceed. Any red? Stop and report.     │
├─────────────────────────────────────────────────────────────────┤
│  STEP 2  │  Pick ONE smell from SMELLS.md                        │
│           │  → State the smell name and the target file/class.   │
├─────────────────────────────────────────────────────────────────┤
│  STEP 3  │  Add or adjust tests                                  │
│           │  → Write characterization tests if area is untested. │
│           │  → Run tests: refactor.run_tests → must be green.    │
├─────────────────────────────────────────────────────────────────┤
│  STEP 4  │  Apply the refactoring (small, atomic)                │
│           │  → Follow the recipe in refactoring/recipes/         │
│           │  → Keep diff < 15 files.                             │
├─────────────────────────────────────────────────────────────────┤
│  STEP 5  │  Re-run all checks                                    │
│           │  → refactor.run_tests   (must pass)                  │
│           │  → refactor.run_lint    (must not increase)          │
│           │  → refactor.run_format  (apply if safe)              │
│           │  → refactor.run_semgrep (must not add new errors)    │
├─────────────────────────────────────────────────────────────────┤
│  STEP 6  │  Show diff: refactor.git_diff                         │
│           │  → Summarise the change for the user.                │
├─────────────────────────────────────────────────────────────────┤
│  STEP 7  │  Commit with structured message                       │
│           │  → refactor(<area>): <pattern>                       │
│           │  → Example: refactor(auth): Extract PasswordHasher   │
├─────────────────────────────────────────────────────────────────┤
│  STEP 8  │  Ask: continue with next smell? Or stop here?         │
└─────────────────────────────────────────────────────────────────┘
```

---

## Stack-Specific Defaults

### Node / TypeScript
```
run_tests:  npm test  (or: npx jest, npx vitest)
run_lint:   npm run lint  (or: npx eslint . --ext .ts,.js)
run_format: npx prettier --write .
run_semgrep: semgrep --config refactoring/semgrep-rules/solid-smells.yml .
```

### Python
```
run_tests:  pytest -q
run_lint:   ruff check .
run_format: ruff format .
run_semgrep: semgrep --config refactoring/semgrep-rules/solid-smells.yml .
```

### Java (Maven)
```
run_tests:  mvn test -q
run_lint:   mvn checkstyle:check
run_format: mvn fmt:format  (if fmt-maven-plugin present)
run_semgrep: semgrep --config refactoring/semgrep-rules/solid-smells.yml .
```

### Java (Gradle)
```
run_tests:  ./gradlew test
run_lint:   ./gradlew checkstyleMain
run_format: ./gradlew spotlessApply  (if spotless present)
```

### C# / .NET
```
run_tests:  dotnet test
run_lint:   dotnet build  (captures Roslyn warnings)
run_format: dotnet format
```

### Rust
```
run_tests:  cargo test
run_lint:   cargo clippy -- -D warnings
run_format: cargo fmt
run_semgrep: semgrep --config refactoring/semgrep-rules/solid-smells.yml .
```

---

## Override via Config

Create `.rapcode/refactor.json` in the project root to override any command:

```json
{
  "run_tests": "make test",
  "run_lint": "make lint",
  "run_format": "make fmt",
  "run_semgrep": "semgrep --config p/owasp-top-ten ."
}
```

Precedence order (highest → lowest):
1. `.rapcode/refactor.json` — user override
2. Auto-detected from project files (`package.json`, `pyproject.toml`, `pom.xml`, …)
3. Generic fallback commands
4. `skipped` — tool not available

---

## Commit Message Format

```
refactor(<area>): <what changed> — <why/pattern>

Body: (optional)
- Moved X from ClassA to ClassB (SRP)
- Introduced IDiscountStrategy interface (OCP)
- Removed circular dependency via DI (DIP)

Refs: SMELLS.md#LongMethod
```

---

## Example Session Transcript

```
User: "The OrderProcessor class does too much—shipping, payment, and email."

Agent:
  1. Runs refactor.baseline → tests: 42 pass, lint: 0 errors
  2. Identifies smell: Large Class / SRP violation in OrderProcessor
  3. Opens recipe: refactoring/recipes/extract-class.md
  4. Adds characterization tests for OrderProcessor.process()
  5. Extracts PaymentService from payment logic → 3 new files, 1 modified
  6. Runs refactor.run_tests → 44 pass ✅
  7. Runs refactor.run_lint → 0 errors ✅
  8. Shows git diff summary
  9. Commits: "refactor(order): Extract PaymentService (SRP)"
  10. Asks: proceed to extract ShippingService?
```
