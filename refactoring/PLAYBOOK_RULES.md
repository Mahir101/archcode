# RapCode Refactoring Playbook — Rules

## Safety Contract

These rules are **non-negotiable**. The agent must follow them in every refactoring session.

### 1. Behavior-Preserving by Default

- **No behavior changes** without explicit user approval.
- A refactoring is considered safe only when existing tests pass before and after.
- If there are no tests for a risky area, write **characterization tests first**.
- Public APIs must not be altered unless the user explicitly approves the breaking change.

### 2. One Smell, One Change-Set

- Pick exactly **one code smell** per change-set (branch, PR, or commit sequence).
- Do not bundle multiple independent refactorings into one commit.
- Exception: cosmetic cleanups (rename, reformat) may accompany a structural refactor.

### 3. Scope Limits

- **Stop if >15 files** are touched in a single refactoring. Split the work.
- **Stop if a public interface breaks** unless explicitly approved by the user.
- **Stop and ask** before removing any public symbol (function, class, constant).

### 4. Test Coverage Gate

- Run the test suite before starting (`refactor.baseline`).
- Run again after each logical change.
- If tests fail after your change: **revert immediately**, do not push forward.

### 5. Commit Discipline

Commit messages must follow the pattern:

```
refactor(<area>): <applied-pattern>

Examples:
  refactor(auth): Extract password hashing into PasswordHasher class
  refactor(order): Replace conditional with polymorphism (DiscountStrategy)
  refactor(api): Introduce parameter object (SearchCriteria)
```

---

## SOLID Principles — Practical Heuristics

### S — Single Responsibility Principle (SRP)

**Heuristic:** A class/module should have only one reason to change.

- Smell: Class > 200 lines *and* mixes IO + business logic + formatting.
- Test: "What does this class do?" — if over 1 sentence, it violates SRP.
- Safe first step: Extract one responsibility into a new class (Extract Class).

### O — Open/Closed Principle (OCP)

**Heuristic:** Open for extension, closed for modification.

- Smell: Every new feature requires editing the same `if/switch` block.
- Test: Adding a new type requires changing existing code → violation.
- Safe first step: Introduce a Strategy or Command interface; migrate one case.

### L — Liskov Substitution Principle (LSP)

**Heuristic:** Subclass must be substitutable for its base class without breaking callers.

- Smell: Override throws `NotImplementedException`, or narrows preconditions.
- Test: Replace parent with child in a test — does it fail? → violation.
- Safe first step: Flatten hierarchy or replace inheritance with composition.

### I — Interface Segregation Principle (ISP)

**Heuristic:** Don't force clients to depend on interfaces they don't use.

- Smell: Interface with 10+ methods where callers implement 2-3.
- Test: Count how many interface methods each implementor uses.
- Safe first step: Split fat interface into role-specific interfaces.

### D — Dependency Inversion Principle (DIP)

**Heuristic:** Depend on abstractions, not concretions.

- Smell: `new ConcreteDatabase()` inside a business-logic class.
- Test: Can you substitute a fake/mock without changing the class? If not → violation.
- Safe first step: Extract interface, inject via constructor (Constructor Injection).

---

## Stop Conditions

Stop the refactoring session and report to the user if any of the following are true:

| Condition | Action |
|---|---|
| Tests fail after change | Revert, report failures |
| >15 files in diff | Split the work, pause |
| Public API would break | Ask user for approval |
| Semgrep reports new errors | Revert, investigate |
| Lint violations increase | Revert, report |
| Circular dependencies introduced | Revert |
| Ambiguous ownership of behaviour | Ask user |

## When to Revert

- Immediately when tests go red.
- When a refactoring grows unexpectedly (scope creep).
- When confidence is < 80% that the transformation is equivalent.
- Use: `git stash` or `git checkout -- .` — never delete; always restore.
