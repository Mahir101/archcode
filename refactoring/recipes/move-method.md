# Recipe: Move Method

**Smells:** Feature Envy / Shotgun Surgery / Inappropriate Intimacy.

---

## Preconditions

- [ ] Baseline passes (`refactor.baseline`)
- [ ] The method clearly "belongs" to another class (it uses more data from that class than its own)
- [ ] Moving the method will not create a circular dependency

---

## Steps

1. **Identify** the target class — the one the method is most envious of.
2. **Check** if all the data the method needs is accessible from the target class.
3. **Create** a copy of the method in the target class with the same signature. Adjust internal references (replace parameter access with `self`/`this`).
4. **Add a delegation** in the original class: call `this.targetObject.movedMethod(...)`. Run tests.
5. **Update callers** to call the method on the target class directly.
6. **Remove the delegation** from the original class (or keep it as a deprecated alias if it's a public API).
7. **Run tests** after each step.

---

## Tests to Add

- Unit tests for the moved method on the new class
- Test that old call-sites behave identically through the delegation
- Test that direct callers (after migration) still work

---

## Commit Plan

```
Commit 1: test: characterization tests for <MethodName> on <SourceClass>
Commit 2: refactor(<area>): copy <MethodName> into <TargetClass>
Commit 3: refactor(<area>): delegate <SourceClass>.<MethodName> to <TargetClass>
Commit 4: refactor(<area>): migrate callers to <TargetClass>.<MethodName>
Commit 5: refactor(<area>): remove delegation from <SourceClass>
```

---

## Pitfalls

- **Hidden side effects**: if the method mutates the source class's state, moving it requires passing the source as a parameter — consider whether that reveals a deeper design issue.
- **Visibility**: make the moved method the right visibility level (don't accidentally over-expose).
- **Method chain breakage**: if callers do `obj.method()` and you move `method` to a nested object, they now need `obj.nested.method()`. This is a breaking change — keep the delegation.
