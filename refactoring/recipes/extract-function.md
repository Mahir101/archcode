# Recipe: Extract Function

**Smell:** Long Method — method does too much in one place.

---

## Preconditions

- [ ] Baseline passes (`refactor.baseline`)
- [ ] The method has at least one identifiable "paragraph" (a block of code with a unified sub-purpose)
- [ ] No side effects preventing extraction (e.g., `goto`, non-local `return` in some languages)

---

## Steps

1. **Identify** the block of code to extract (a logical paragraph, usually 5–15 lines).
2. **Name** the extracted method before writing it — the name should describe "what it does", not "how".
3. **Identify parameters**: any locally-scoped variables the block reads from the outer scope become parameters.
4. **Identify return value**: if the block assigns a single variable used afterward, that's the return value.
5. **Create** the new method with the identified signature. Copy the block body.
6. **Replace** the original block with a call to the new method.
7. **Run tests**: `refactor.run_tests` — must be green.
8. **Repeat** for the next paragraph if needed (one commit per extraction is fine).

---

## Tests to Add

- A focused unit test for the extracted method in isolation
- Verify output with at least 2 representative inputs + 1 edge case

---

## Commit Plan

```
Commit 1: test: add characterization tests for <OriginalMethod>
Commit 2: refactor(<area>): extract <NewMethodName> from <OriginalMethod>
Commit 3: refactor(<area>): extract <AnotherMethod> (if more extractions)
```

---

## Pitfalls

- **Don't extract for its own sake**: a 5-line method that is only called once and is perfectly readable in context does not need extraction.
- **Avoid extracting partial conditionals**: extract complete branch bodies, not half of an `if`.
- **Watch out for mutable state**: if the block mutates multiple variables, extraction is harder — consider "Split Temporary Variable" first.
