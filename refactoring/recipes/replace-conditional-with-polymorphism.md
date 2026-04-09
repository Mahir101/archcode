# Recipe: Replace Conditional with Polymorphism

**Smell:** Switch Statements / Conditional Complexity — type-tag conditionals that grow with each new variant.

---

## Preconditions

- [ ] Baseline passes (`refactor.baseline`)
- [ ] The conditional branches on a type discriminator (enum value, string tag, type field)
- [ ] The same conditional (or a close variant) appears in multiple places
- [ ] Adding a new type requires editing existing code (OCP violation)

---

## Steps

1. **Extract** each branch body into a method if they are inline (temporarily keep the switch).
2. **Define** an interface/trait with the common operation (e.g., `calculateDiscount()`, `render()`, `execute()`).
3. **Create** one concrete class per variant. Move the corresponding branch body into the concrete class.
4. **Create a factory** or use a registry to map the type tag to its concrete class.
5. **Replace** the original conditional with `factory.get(type).operation()`.
6. **Test** that each variant behaves identically.
7. **Delete** the switch/conditional.
8. **Confirm**: adding a new type now only requires a new class + registering in the factory.

---

## Tests to Add

- One test per variant verifying correct behavior
- A test that an unknown type produces a clear error (not a silent default)
- A factory/registry test verifying all variants are registered

---

## Commit Plan

```
Commit 1: refactor(<area>): introduce <OperationInterface> interface
Commit 2: refactor(<area>): implement <VariantA>, <VariantB> concrete classes
Commit 3: refactor(<area>): introduce <Type>Factory
Commit 4: refactor(<area>): replace switch on <typeField> with polymorphism
Commit 5: refactor(<area>): delete old conditional
```

---

## Pitfalls

- **Over-engineering 2-branch conditionals**: if the switch has 2 branches and will likely stay at 2, a simple conditional is clearer.
- **Factory leakage**: ensure the factory is the only place that knows about concretions — callers should never downcast.
- **Behavior difference on default/fallthrough**: if the switch had a `default` branch with non-trivial logic, map it to a `NullObject` or `UnknownVariant` implementation explicitly.
- **Language-specific caution**: in Rust, `match` exhaustiveness is a feature — prefer enums + `impl Trait for Enum` over a full class hierarchy if the variants are stable.
