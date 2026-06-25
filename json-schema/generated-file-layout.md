# Generated file layout (cross-cutting design note)

Not a JSON Schema keyword. Specifies how a set of input schema files
becomes generated source in each language: the package structure, file
names, where shared runtime lives, and how reference cycles and
name collisions are handled at the file level. Driven by **P14** (one
file per input; merge recursion, not files) and local-file-only
external refs; the reference semantics that feed it live in [[ref]].

## Single flat package per language

All output for one generator run lands in **one flat package** per
language. This is the decision that lets schema-independent boilerplate —
`ValidationError`/`Violation`, the spec-number helpers, the (de)serialize
scaffolding — be defined **once** instead of duplicated per file.

The flatten is **forced, not stylistic**: a Go package is exactly one
directory with no sub-packages, so a single Go package cannot mirror
nested input directories. To keep an **identical structure across all
four languages**, every language flattens. Input files from nested
directories collapse into the one package (see Module names below).

Because everything shares one namespace, **type names cannot collide
across files** — the loader validates this (see Collisions).

## Files per language

### Multi-input (≥2 input files in the closure)

| Language | Per-input file | Shared boilerplate | Recursive file* | Aggregator |
|---|---|---|---|---|
| **Python** | `<module>.py` | `definitions.py` | `_recursive.py` | `__init__.py` (re-exports all public types incl. `ValidationError`) |
| **TypeScript** | `<module>.ts` | `definitions.ts` | — | `index.ts` (re-exports all) |
| **Go** | `<module>.go` | `definitions.go` | — | — (capitalized = exported) |
| **Java** | one `<ClassName>.java` per exported class | each boilerplate class its own file (`ValidationException.java`, `Violation.java`, `SpecNumbers.java`, …) | — | — (`public` = exported) |

\* **`_recursive` is Python-only and is a single file in the package**
(`<package>/_recursive.py`), **never** per-input
(`<package>/<module>_recursive.py`). It holds every hoisted cross-file
SCC in the whole closure. See Recursion below.

### Single-input (exactly one input file — no cross-file refs possible)

| Language | Output |
|---|---|
| **Python** | one `__init__.py` — domain types + boilerplate inline; no per-input module, no `_recursive`, no re-export layer |
| **TypeScript** | one `index.ts` — same |
| **Go** | one `<package>.go` — types + boilerplate inline |
| **Java** | unchanged — one `.java` per public class + boilerplate classes (the language can't collapse to one file, and there is nothing to re-export) |

## The shared `definitions` file

Holds the schema-independent runtime, defined once per package:

- Error types — `ValidationError` + `Violation` (Python with the Pydantic
  aggregation machinery; Go `ValidationError`/`Violation` structs; TS
  `ValidationError` class; Java `ValidationException extends
  JsonMappingException` + `Violation`).
- Spec-number helpers — `parseSpecInteger` (Go), `SpecInt` /
  `_parse_spec_integer` (Python), `SpecNumbers.specLong` (Java), TS's
  safe-integer check.
- Shared (de)serialize scaffolding — the **P12** three-layer base, the
  Python optional-non-nullable `model_validator` helper. Java's
  collecting (de)serializer stays per-class, but the shared `Violation` /
  `ValidationException` / `SpecNumbers` classes live here.

## Module names (flattened path encoding)

The **input root** is the absolute path of the directory common to all
resolved input files (their longest common-ancestor directory). `$ref`
paths are resolved to absolute paths first — `..` segments are normalized,
not rejected ([[ref]]) — so a ref that walks upward simply raises the
common root. A per-input module is named from its absolute path **relative
to the input root**, minus extension, with the directory delimiter
replaced by `_` and **literal underscores preserved verbatim** — no
escaping. Because module names are relative to the common ancestor, they
never contain `..`:

```
input root = /abs/schemas   (common ancestor of all resolved inputs)

/abs/schemas/full_name.json       -> full_name
/abs/schemas/a/b/user.json        -> a_b_user
/abs/schemas/billing/invoice.json -> billing_invoice
```

Type names are directory-independent (basename-of-root or `$defs` name —
see [[ref]]), so they do not encode the path.

### Why no escaping

Identifiers are `[A-Za-z0-9_]`: two structural things (the directory
separator and a literal `_`) must encode into one non-alphanumeric
character. Injective + flat + underscores-preserved is impossible without
escaping. We choose **readable + collision-reject** over an escaping
scheme: keep names clean and reject the rare collision, consistent with
the **P15** "reject loudly, never mangle, offer an override" stance used
everywhere else. Module file names are largely internal organization
anyway — consumers import types from the aggregator
(`__init__.py`/`index.ts`) — so a rare reject costs little.

## Collisions

One unified namespace per package holds the **reserved generated names**
plus one entry per input module. Reserved names (compared after
per-language normalization):

- `definitions`
- `_recursive` (Python)
- `index` (TS) / `__init__` (Python)

Any collision in that namespace → **load reject** with a fix-it
(`x-output-module` override or rename):

- two inputs flattening to the same module (`full/name` vs `full_name`);
- an input flattening onto a reserved generated name (a root-level
  `definitions.json`);
- (type-name collisions are handled the same way — see [[ref]]).

Like [[properties]], collisions are evaluated **per emitted target
only** — normalization differs per language.

## Recursion: hoist types, not files

This is the file-level realization of **P14**. "Merge on cycle" does
**not** mean merge whole input files — it means hoist only the cyclic
types:

- Build the reference graph and its strongly-connected components
  ([[ref]]). An SCC spanning **≥2 input files** is a cross-file cycle.
- **Python**: the cross-file SCC moves wholesale into `_recursive.py`,
  where it becomes a within-module cycle (topological order + a string
  forward-ref back-edge + one `model_rebuild()`). Per-input modules and
  the aggregator import the finished classes from `_recursive.py`, which
  imports nothing back from them — so the cross-module import cycle is
  gone. A cycle **within** a single file stays in its module.
- **TypeScript**: no recursive file. Type references erase
  (`import type` is always cycle-safe) and validator-function imports are
  ESM live bindings resolved at call time, not module-init; generated
  const values are self-contained leaf literals, so there is no
  init-order hazard. Cyclic types stay in their per-input modules.
- **Go / Java**: a single package handles cycles natively (Go within one
  package; Java object references). No recursive file.

`model_rebuild()` is a **cycle** concern, not a same-module concern:
acyclic references emit in topological order with concrete annotations
and need no rebuild.

## Exports / visibility

*Public* = every top-level named type (file roots + all `$defs`,
including dead ones; anonymous types stay nested):

- **Python** — `__init__.py` re-exports all public types + `ValidationError`
  via `__all__`; hoisted types re-exported from `_recursive`.
- **TypeScript** — `index.ts`: `export … from './<module>'` plus
  `export { ValidationError } from './definitions'`.
- **Go** — no aggregator; capitalized identifiers are exported.
- **Java** — `public` class per file; boilerplate classes public too.

## See also

- [[ref]] — reference semantics, type-name derivation, recursion graph +
  satisfiability, bare-`$ref`-root alias.
- [[properties]] — the shared identifier/collision algorithm.
- [[nullability]] — optional/nullable wrapping (a source of cycle
  termination).
- [[PRINCIPLES.md]] — **P14** (one file per input; merge recursion, not
  files), local-file-only external refs, **P15** (one identifier namespace
  per scope).
