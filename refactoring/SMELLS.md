# Code Smells Catalogue

Based on Refactoring.Guru patterns. Each smell maps to a recipe with symptoms, safe first step, done criteria, and risks.

---

## 1. Long Method

**Symptoms**
- Method > 30 lines (heuristic varies; 20 for critical paths)
- Deeply nested conditionals (indent level > 3)
- Hard to name the method with a single verb phrase
- Requires a comment to explain each "paragraph"

**Safe First Step**
Extract the first "paragraph" that can stand alone into a private method with a descriptive name.

**Recipe:** `refactoring/recipes/extract-function.md`

**Done When**
- Each method has a single, clear intent
- Method body reads like a high-level story
- No method exceeds 25 lines (excluding signature/braces)

**Risks**
- Extracting too many tiny methods (over-engineering)
- Losing performance-critical inlining (profile first)

---

## 2. Large Class (God Object)

**Symptoms**
- Class > 200–300 lines
- Mixes concerns: IO + business logic + formatting
- Has fields that are only used in some methods
- Name is vague: `Manager`, `Handler`, `Helper`, `Util`, `Service`

**Safe First Step**
Identify a cluster of fields + methods that belong together. Extract Class.

**Recipe:** `refactoring/recipes/extract-class.md`

**Done When**
- Each class has a single reason to change (SRP)
- Class name is specific and domain-meaningful
- No class mixes IO and pure computation

**Risks**
- Splitting prematurely before understanding the domain
- Creating too many small classes with anemic behaviour

---

## 3. Long Parameter List

**Symptoms**
- Function/method has > 4 parameters
- Multiple boolean flags as parameters (`doX: true, doY: false`)
- Multiple consecutive parameters of the same type (easy to swap)

**Safe First Step**
Introduce a Parameter Object (data class/record) grouping related params.

**Recipe:** `refactoring/recipes/introduce-parameter-object.md`

**Done When**
- No public method has > 4 parameters
- Boolean flags replaced by explicit method names or enums

**Risks**
- Parameter object becomes a bloated data blob
- Calling code becomes more verbose temporarily

---

## 4. Divergent Change

**Symptoms**
- A single class must be changed whenever feature A changes *and* also whenever feature B changes
- Changes for different reasons always converge in the same class

**Safe First Step**
Split the class along the axis of change.

**Recipe:** `refactoring/recipes/extract-class.md`

**Done When**
- Each class has exactly one axis of change

**Risks**
- Identifying the axes incorrectly (domain knowledge required)

---

## 5. Shotgun Surgery

**Symptoms**
- Changing one concept requires editing many small classes spread across the codebase
- One logical change → 10+ files touched

**Safe First Step**
Move Method / Move Field to consolidate related behaviour in one place.

**Recipe:** `refactoring/recipes/move-method.md`

**Done When**
- A single conceptual change touches ≤ 3–4 files

**Risks**
- Creating a new God Object by over-consolidation

---

## 6. Feature Envy

**Symptoms**
- Method uses fields/methods of another class more than its own
- Method calls 5+ getter chains on another object

**Safe First Step**
Move Method to the class its body "envies".

**Recipe:** `refactoring/recipes/move-method.md`

**Done When**
- Methods live close to the data they operate on

**Risks**
- Moving breaks if the method has dual responsibility

---

## 7. Switch Statements / Conditional Complexity

**Symptoms**
- Long `if/else if` or `switch/match` on type tags
- Same conditional duplicated in multiple places
- Adding a new type means editing the switch

**Safe First Step**
Replace Conditional with Polymorphism — extract an interface, implement a class per case.

**Recipe:** `refactoring/recipes/replace-conditional-with-polymorphism.md`

**Done When**
- New behavior added by adding a class, not changing a switch
- No type-tag conditionals in business logic

**Risks**
- Over-engineering simple 2-branch conditionals (leave those as-is)

---

## 8. Primitive Obsession

**Symptoms**
- Plain strings/ints used for domain concepts (email, money, percent, status)
- Validation logic scattered near every use of the primitive

**Safe First Step**
Extract Value Object (e.g., `Email`, `Money`, `Percentage`).

**Done When**
- Domain concepts have their own types with validation in the constructor

**Risks**
- Too many micro-types for trivial values

---

## 9. Data Clumps

**Symptoms**
- Same 3–4 variables appear together everywhere (passed as a group, stored as a group)
- Removing one from the group makes the others meaningless

**Safe First Step**
Extract a class / struct / record for the group.

**Recipe:** `refactoring/recipes/introduce-parameter-object.md`

**Risks**
- Grouping concepts that actually belong to different domains

---

## 10. Inappropriate Intimacy

**Symptoms**
- Class A directly accesses private internals of class B
- Two classes are tightly coupled, changes in one always break the other

**Safe First Step**
Move Method / Extract Interface to hide internals.

**Done When**
- Classes communicate only via stable interfaces

**Risks**
- Over-abstraction when two classes intentionally co-evolve

---

## 11. Middle Man

**Symptoms**
- Class delegates almost all its methods to another class
- Class adds no value; it's just a pass-through proxy

**Safe First Step**
Remove Middle Man — callers talk to the real class directly, or inline the delegation.

**Done When**
- Every class does meaningful work

**Risks**
- The middle man may hide future extension points

---

## 12. Dependency Violation (DIP Smell)

**Symptoms**
- Business logic instantiates infrastructure (`new DatabaseConnection()`, `new HttpClient()`)
- Tests require actual DB/network connections
- Impossible to mock/stub a dependency

**Safe First Step**
Extract Interface for the dependency, inject via constructor.

**Recipe:** `refactoring/recipes/dependency-inversion.md`

**Done When**
- Business logic depends only on interfaces/traits
- Tests can inject fakes without modifying production code

**Risks**
- Introducing too many interfaces for stable leaf dependencies
