# Recipe: Extract Class

**Smell:** Large Class / God Object / SRP violation.

---

## Preconditions

- [ ] Baseline passes (`refactor.baseline`)
- [ ] You can name the new class with a single, clear responsibility
- [ ] The fields to extract form a coherent cluster (used together, not mixed with unrelated fields)

---

## Steps

1. **Decide** what responsibilities to extract and name the new class.
2. **Create** the new class with only its fields and no methods yet.
3. **Move fields**: move each field belonging to the new responsibility. Update references in the original class (delegate to `this.newClass.field`).
4. **Move methods**: for each method that primarily operates on the extracted fields, move it to the new class. Keep a forwarding method in the original class temporarily.
5. **Update callers**: once confident, remove forwarding methods and update callers to use the new class directly (or keep the original class as a facade if it simplifies the API).
6. **Run tests**: `refactor.run_tests` — must be green after every move.
7. **Run lint**: `refactor.run_lint` — track any new warnings.

---

## Tests to Add

- Unit tests for the new class in isolation
- Integration tests verifying the original class still works end-to-end

---

## Commit Plan

```
Commit 1: test: add characterization tests for <LargeClass>
Commit 2: refactor(<area>): introduce <NewClass> skeleton
Commit 3: refactor(<area>): move fields X, Y to <NewClass>
Commit 4: refactor(<area>): move method Z to <NewClass>
Commit 5: refactor(<area>): remove forwarding methods from <LargeClass>
```

---

## Pitfalls

- **Circular dependency**: if NewClass references OriginalClass and vice versa, you've created a cycle. Introduce an interface or merge back.
- **Premature extraction**: don't extract until you can confidently name the new class without using words from the original class name.
- **Breaking public API**: if the extracted class was a public field or a returned type, update all callers before removing it.
