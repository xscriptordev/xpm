# Dependency Resolution Engine

Overview of the native SAT-based dependency resolver implemented in
`xpm-core::resolver`.

---

## Architecture

The resolver lives under `crates/xpm-core/src/resolver/` and is organized into
four modules:

| Module | File | Purpose |
|--------|------|---------|
| `version` | `version.rs` | ALPM-compatible version parsing and comparison |
| `dependency` | `dependency.rs` | Dependency constraint parsing and matching |
| `types` | `types.rs` | Package pool and interning bridge to resolvo |
| `provider` | `provider.rs` | `Interner` + `DependencyProvider` implementation |

An additional `tests.rs` module contains integration tests that exercise the
full solver pipeline.

---

## External Dependencies

| Crate | Version | Role |
|-------|---------|------|
| **resolvo** | 0.10 | CDCL SAT solver — handles clause learning, unit propagation, and backtracking |
| **itertools** | 0.14 | Combinatorial helpers (workspace utility) |

The resolver delegates all SAT mechanics (CNF encoding, watched literals, CDCL)
to resolvo. xpm only provides the **data model** and the
**DependencyProvider** trait implementation.

---

## Module Details

### `version` — Version Parsing and Comparison

Implements the ALPM `vercmp` algorithm in pure Rust.

- **`Version`** struct with fields `epoch`, `pkgver`, `pkgrel`.
- Parses the format `[epoch:]pkgver[-pkgrel]`.
- Segment-by-segment comparison: numeric segments compared as integers,
  alphabetic segments compared lexicographically. Numeric beats alphabetic
  when the two types differ.
- Implements `Ord`, `Display`, `Hash`, `Eq`, `Clone`.

### `dependency` — Dependency Constraints

Parses ALPM dependency strings and evaluates them against candidate versions.

- **`Operator`** enum: `Ge (>=)`, `Le (<=)`, `Gt (>)`, `Lt (<)`, `Eq (=)`.
- **`DepConstraint`** struct: `name`, optional `op`, optional `version`.
- `DepConstraint::parse("glibc>=2.38")` → `{ name: "glibc", op: Ge, version: "2.38" }`.
- `DepConstraint::matches(&Version)` returns `true` when the candidate
  satisfies the constraint (unconstrained always matches).

### `types` — Package Pool

The interning layer that maps xpm's package model to resolvo's opaque ID
system.

- **`PackageCandidate`** — a single installable package version with
  `name`, `version`, `depends`, `conflicts`, `provides`, `optdepends`.
- **`PackagePool`** — central arena that manages:
  - `NameId` ↔ package name strings
  - `SolvableId` ↔ `PackageCandidate` entries
  - `VersionSetId` ↔ version constraints (with a `negated` flag for
    conflict sets)
  - `VersionSetUnionId` ↔ grouped version sets
  - `StringId` ↔ arbitrary interned strings

The pool provides `intern_version_set` (normal dependencies) and
`intern_conflict_version_set` (conflicts — the match is inverted so that
resolvo's "forbid non-matching" semantics correctly forbid matching
candidates).

### `provider` — DependencyProvider

**`XpmProvider`** wraps a `PackagePool` and implements two resolvo traits:

1. **`Interner`** — display methods for solvables, names, version sets, and
   unions; maps ID lookups to the pool.
2. **`DependencyProvider`** — feeds package data into the solver:
   - `filter_candidates` — tests each candidate against a version set
     constraint (respects the `negated` flag for conflicts).
   - `get_candidates` — returns all solvable IDs for a given package name.
   - `sort_candidates` — highest version first (preferred).
   - `get_dependencies` — maps `depends` to resolvo *requirements* and
     `conflicts` to resolvo *constrains*.

---

## Solving Flow

```text
PackageCandidate[]          DepConstraint.parse()
        │                          │
        ▼                          ▼
   PackagePool  ──────────►  XpmProvider
        │                          │
        │   intern names,          │ implements Interner
        │   solvables,             │ + DependencyProvider
        │   version sets           │
        ▼                          ▼
                    Solver::new(provider)
                           │
                           ▼
                  solver.solve(problem)
                           │
                           ▼
              Result<Vec<SolvableId>, Error>
```

1. Build a `PackagePool` and add all known candidates.
2. Intern version sets for every dependency and conflict.
3. Wrap the pool in an `XpmProvider`.
4. Construct a `Problem` with the root requirements (packages the user
   wants to install).
5. Call `solver.solve(problem)` — resolvo returns the minimal set of
   `SolvableId`s that satisfies all constraints, or an error if
   unsolvable.

---

## Test Coverage

56 tests total (14 version + 16 dependency + 4 types + 9 integration +
13 config/repo from other modules).

### Integration Tests (`tests.rs`)

| Test | Scenario |
|------|----------|
| `solve_single_package_no_deps` | Install one package with no dependencies |
| `solve_picks_highest_version` | Three versions available — solver picks newest |
| `solve_with_dependency` | Versioned dependency `glibc>=2.38` |
| `solve_dependency_chain` | Transitive chain A → B → C |
| `solve_multiple_root_packages` | Two independent root requests |
| `solve_unsatisfiable_dependency` | Required version not available — expect error |
| `solve_with_conflict` | ALPM `conflicts` forbids co-installation |
| `solve_version_constrained_dependency` | Picks highest from filtered candidate set |
| `solve_shared_dependency` | Diamond dependency deduplication |
