# Recipe: Dependency Inversion

**Smell:** Dependency Violation (DIP) — business logic depends on concretions, making testing impossible without real infrastructure.

---

## Preconditions

- [ ] Baseline passes (`refactor.baseline`)
- [ ] The class instantiates its own dependencies internally (`new X()`, direct static calls)
- [ ] Tests require real infrastructure (DB, HTTP, filesystem) due to tight coupling

---

## Steps

1. **Identify** the concrete dependency being instantiated.
2. **Extract an interface/trait** that captures only the methods the class actually uses (ISP: keep it narrow).
3. **Update the class**:
   - Remove the internal instantiation
   - Accept the interface via **constructor injection** (preferred over setter/method injection)
   - Store it as a field of the interface type
4. **Update instantiation sites** (factory, `main`, DI container): pass the concrete implementation there.
5. **Create a fake/stub** for the interface in tests. Inject the fake in unit tests.
6. **Run tests**: unit tests should now run without real infrastructure.
7. **Remove any remaining direct references** to the concrete class in business logic.

---

## Tests to Add

- Unit tests with a fake/stub implementation of the extracted interface
- Verify happy path and error path through the fake
- One integration test (test container or real infra) to ensure the concrete implementation also works

---

## Commit Plan

```
Commit 1: refactor(<area>): extract <InterfaceName> from <ConcreteClass>
Commit 2: refactor(<area>): inject <InterfaceName> via constructor in <ConsumerClass>
Commit 3: test: add <FakeImplementation> and unit tests for <ConsumerClass>
Commit 4: refactor(<area>): wire <ConcreteClass> at composition root
```

---

## Pitfalls

- **Interface bloat**: only include methods actually called by the consumer. If the interface grows to 10+ methods, split it (ISP).
- **Too many abstractions**: leaf-level stable deps (math, string utils) don't need interfaces. Focus on IO boundaries.
- **Constructor explosion**: if a class needs 5+ injected dependencies, it likely violates SRP itself — extract a class first.
- **Service Locator anti-pattern**: do not use a global registry to resolve dependencies inside business logic — that's not DI, it's a hidden dependency.
