# Recipe: Introduce Parameter Object

**Smell:** Long Parameter List / Data Clumps.

---

## Preconditions

- [ ] Baseline passes (`refactor.baseline`)
- [ ] At least 3 parameters travel together in multiple function signatures
- [ ] The group of parameters forms a coherent concept with a nameable identity

---

## Steps

1. **Identify** the cluster of parameters that always appear together.
2. **Name** the parameter object (it should reflect the domain concept: `SearchCriteria`, `OrderRequest`, `DateRange`).
3. **Create** the new class/struct/record with those fields and sensible defaults.
4. **Add validation** to the constructor if parameters have constraints.
5. **Create an overload** (or migrate in one pass): update the function to accept the new type. Keep the old signature temporarily if the function is public.
6. **Update call sites** one at a time. For each call site: create the object, pass it in.
7. **Remove old signature** once all call sites migrated.
8. **Run tests**: `refactor.run_tests` — after each call site migration.

---

## Tests to Add

- Test valid construction of the parameter object
- Test that invalid combinations are rejected at the object level
- Verify old callers still compile and behave correctly

---

## Commit Plan

```
Commit 1: refactor(<area>): introduce <ParameterObject> type
Commit 2: refactor(<area>): migrate <FunctionName> to accept <ParameterObject>
Commit 3: refactor(<area>): update call sites to use <ParameterObject>
Commit 4: refactor(<area>): remove old overload
```

---

## Pitfalls

- **Overly generic objects**: `Options`, `Config`, `Params` are not parameter objects — give the object a domain name.
- **Mutable state in value objects**: parameter objects should usually be immutable (all fields set at construction).
- **Chaining clusters**: if the new object immediately spawns another cluster, consider whether it should be a proper domain entity instead.
