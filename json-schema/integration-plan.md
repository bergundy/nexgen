# Integration Plan — Shared Codegen Base: Loaders, Symbols, Emitters

> **Revision 2026-06-30 (A): open generic symbols; no Plan; no factory.**
> The earlier "option b" (base `SymbolTable` pure + a private per-schema_type
> side table keyed by `SymbolId` + an emitter-construction factory) is
> **reversed**. Symbol kinds are now **open and frontend-defined** via a generic
> `SymbolTable<K>`; the base is compile-time-agnostic to `K`. `ApiPlan` is
> deleted — `WitLoader` lowers WIT directly into `SymbolTable<WitSymbolKind>`.
> There is **no factory and no private side table**: the emitter receives
> `&IR<K>` and works only from it. Services stay a base concept (`Service` /
> `Operation` data structs + `render_service`); a frontend kind wraps a base
> `Service`. Byte-identical WIT output is the hard requirement. Sections below
> that describe the old closed `SymbolKind` / side table / factory are
> superseded by this banner.

## Purpose

Define the refactor that extracts a **front-end-agnostic base layer** out
of the current WIT generator, so the JSON Schema generator (and any future
front-end) can build on it. Settled through design discussion; this doc is
the host-crate refactor only — the JSON Schema front-end itself is the
separate effort tracked by `PLAN.md` + `features/`.

## Vocabulary

- **schema_type** — an input format / front-end (`wit`, `json-schema`, …).
- **Loader** — per schema_type; validates input files and produces the IR.
- **Symbol** — the core IR primitive. A **service** and a **type** are both
  kinds of symbol. The base reasons over symbols uniformly.
- **IR** — what a loader produces: a base **`SymbolTable`** (pure, schema-
  agnostic) plus, kept *private to the schema_type*, the schema-specific
  definition data for its type symbols (correlated by `SymbolId`).
- **Emitter** — a `(lang, schema_type)` unit that renders symbols for one
  target language.

## Pipeline

```
 input files
     │
     ▼   Loader[schema_type]              validate inputs → IR
    IR = SymbolTable  (base: id, name, kind, refs — pure)
       + private side table  SymbolId → schema type data  (stays in the schema_type crate)
     │
     ▼   Emitter[(lang, schema_type)]     renders type symbols (reads its private data by id);
     │      base renders service symbols via reusable utilities, calling the emitter for names
     ▼   base: group by module → resolve imports (refs + module compare) → render blocks → write/format
 GeneratedFiles
```

## The Symbol abstraction

Service and type are unified as symbols, so placement, import resolution,
reachability, and dedup are **one algorithm** over `refs` + `module` — with
no schema-specific knowledge in the base. "Common to all symbols" splits
across two tiers:

**IR tier — loader-produced, language-agnostic, in the base `SymbolTable`:**

```rust
struct Symbol {
    id:   SymbolId,            // stable identity, unique in the table
    name: Name,                // canonical name (language mapping applied later)
    kind: SymbolKind,          // Service(ServiceDef) | Type | …
    refs: Vec<SymbolId>,       // symbols this one references (service I/O, type fields)
}

enum SymbolKind {
    Service(ServiceDef),       // base data: operations, wire names, docs
    Type,                      // schema-specific data lives in the schema_type's PRIVATE side table
}
```

`ServiceDef` is base data (services are frontend-independent). A `Type`
symbol carries **nothing** schema-specific in the base table; its definition
data sits in the schema_type's private side table, keyed by `id` — so the
base IR stays pure (no generics, no erased payloads).

**Emit tier — emitter-computed per language, uniform across kinds:**
`module` (placement), `type_ref` (how a referrer names it), `import_binding`
(how to import it cross-module), and the rendered `body`. A **foreign**
reference (protoc/ts-proto) is a `Type` symbol whose `module` is the foreign
module and whose `body` is empty.

## Loader

```rust
trait Loader {
    fn schema_type(&self) -> SchemaType;
    fn load(&self, inputs: &[PathBuf]) -> Result<IR>;
}

struct IR {
    symbols: SymbolTable,           // base, pure
    // + the schema_type's private side table (SymbolId → its type data),
    //   surfaced to its own emitters, never to the base
}
```

The loader is where **input validation** lives: JSON Schema strict-subset
checks (reject unsupported keywords at load, fix-it diagnostics —
`PLAN.md`/`PRINCIPLES.md`) for the JSON loader; WIT parse + proto descriptor
resolution for the WIT loader.

## Emitter + `EmittedFile`

No `Ir` generic — the emitter renders against the shared `SymbolTable` and
its own private data, and **produces the files** (it owns layout). Each
emitter *is* a language; no `lang` parameter.

```rust
trait Emitter {
    fn language(&self) -> Language;
    fn schema_type(&self) -> SchemaType;
    fn emit(&self, symbols: &SymbolTable) -> Vec<EmittedFile>;
    fn resolver(&self) -> &dyn NameResolver;   // type_ref / module_of / import_binding by SymbolId
}

struct EmittedFile {
    path:   PathBuf,
    module: Module,
    refs:   Vec<SymbolId>,      // symbols the file uses; base resolves → cross-module Imports
    runtime_imports: Vec<Import>, // non-symbol imports (nexus-rpc, dataclasses, …)
    body:   String,             // body WITHOUT the import block
}
```

- **Type symbols** are rendered per `(lang, schema_type)` by the emitter,
  reading the schema-specific data the loader produced for that `id`.
- **Service symbols** are rendered by the **base service/operation rendering
  utility** (per language), which the emitter calls — using the same
  `resolver()` to name operation I/O types. Service rendering is written once
  per language and reused by every schema_type.
- **The emitter only declares import needs** (`refs` + `runtime_imports`); the
  base resolves them (same-module dropped) and renders the block via the
  **base per-language `render_imports` utility**, symmetric with service
  rendering. `render_imports` is *not* an emitter method. One `NameResolver`
  per emitter serves both import resolution and service rendering.

## Assembly + import rendering (base-owned)

Two reusable base per-language functions, used by every emitter:

```rust
fn render_service(lang: Language, svc: &ServiceDef, names: &dyn NameResolver) -> String;
fn render_imports(lang: Language, imports: &[Import]) -> String;
```

Final assembly, per file: resolve `file.refs` through the emitter's
`resolver()` to cross-module `Import`s (same-module dropped), union with
`file.runtime_imports`, dedup, then stitch `render_imports(lang, &imports)` +
`file.body` → write/format. Because `refs` are explicit on every symbol,
**reachability, placement (`module`), import resolution, and dedup are
base-owned** — `module` comparison drives both placement (which file) and
imports (cross-module ⇒ import; same module ⇒ none); foreign references and
same-module exclusion fall out uniformly.

## Registry

The registry keys are **inferred**, not passed: a `Loader` already reports
`schema_type()`, and an emitter (factory) reports `language()` +
`schema_type()`, so registration takes only the value.

```rust
register_loader(WitLoader::new());          // key = loader.schema_type()
register_emitter(WitPythonEmitter::new());  // key = (emitter.language(), emitter.schema_type())
```

The library/CLI entry resolves `(lang, schema_type)` → loader + emitter →
`load(inputs)` → construct the emitter over the loaded private data →
`assemble(&ir.symbols, &emitter)` → write/format. No `Ir` type escapes and
nothing needs erasing, because the base table is already schema-agnostic.

## What is shared vs. per-schema_type

| Concern | Where it lives |
|---|---|
| `Symbol` / `SymbolTable` / `ServiceDef`; `Loader` / `Emitter` traits; registry | **Base** |
| Service/operation rendering + import rendering utilities (per language) | **Base** |
| Assembly: placement, reachability, import resolution + dedup; output plumbing; `Language`/`SchemaType` | **Base** |
| `EmittedFile` layout (which files, what's in each) | **Each emitter** (rendered imports stitched in via the base utility) |
| Input validation (JSON strict-subset / WIT parse) → IR | **Each loader** |
| Type symbols' schema-specific data (private side table) + per-language type rendering | **Each schema_type** |
| Proto conversion, WIT type-system lowering, resources | **WIT crate** |
| Foreign-generated type references (protoc, ts-proto) | **Each schema_type** (a `Type` symbol, foreign `module`, empty body) |
| Validators, nullability, `const`/`default`/`enum`, constraints | **JSON crate** |
| Client generation, other high-level abstractions | **WIT crate (for now)** — not in the base; may move to a higher layer later |

## Required refactor of the current WIT generator (limited, behavior-preserving)

Each phase keeps the existing example outputs byte-identical; the snapshot /
example test suite is the guardrail.

### Phase 1 — Stand up the base crate skeleton
Move the front-end-agnostic pieces into `nex-gen-codegen`: `Language`,
`SchemaType`, `GeneratedFiles`/layout, file writing, formatter invocation.
Define `Symbol` / `SymbolTable` / `SymbolKind` / `ServiceDef`, `Module` /
`Import` / `ImportBinding`, the `Loader` and `Emitter` traits, the `assemble`
function (placement + import resolution), and the registry.

### Phase 2 — Extract service/operation rendering utilities
Pull Nexus service-binding rendering out of `python::generate` /
`typescript::generate` into base utility functions (structural logic + per-
language formatting) that name I/O types via the emitter. Type rendering,
proto conversion, and type-system lowering stay in the WIT crate.

### Phase 3 — WIT loader + per-language emitters
Wrap WIT parse + proto resolution as `WitLoader`, producing the base
`SymbolTable` (services + types as symbols, with `refs`) plus its private
type-data side table (`ApiPlan` stays as that data — proto/WIT-shaped, now
correctly). Wrap the per-language rendering + proto conversion as
`WitPythonEmitter` / `WitTypeScriptEmitter` implementing `Emitter`
(`render_type` reads the side table; service rendering via the base
utilities). Register the loader + the `(lang, wit)` emitters.

### Phase 4 — Rewire the WIT CLI
`(lang, wit)` → registry → `load` → construct emitter → `assemble` →
write/format. WIT-only commands (`add-rpc`, `debug-wit-dir`,
`build-examples`) stay in the WIT crate untouched.

## Explicitly out of scope (separate efforts)

- The **JSON Schema front-end** — its loader/validation, private type data,
  per-language emitters, validators, nullability, `const`/`default`/`enum`,
  constraints, `format`, file layout. (`PLAN.md` + `features/`.)
- **Client generation** and other high-level abstractions — they stay in the
  WIT crate for now (not in the base); may move to a higher layer later.

## Settled decisions

- **The base is a real crate** (`nex-gen-codegen`) in a workspace.
- **Symbol is the core IR primitive; service and type are symbol kinds.** The
  base `SymbolTable` is pure/schema-agnostic; schema-specific type data is
  **private to the schema_type, correlated by `SymbolId`** (option b) — so
  there is **no `Ir` generic and no erased payload** in the base.
- **Two-tier common contract:** IR tier (`id`, `name`, `kind`, `refs`) from
  the loader; emit tier (`module`, `type_ref`, `import_binding`, `body`) from
  the emitter.
- **Pipeline is loader → IR → emitter**, with emitters registered per
  `(lang, schema_type)` and loaders per `schema_type`.
- **Service/operation rendering *and* import rendering are reusable base
  per-language utilities** (`render_service`, `render_imports`); only type
  rendering is per-schema_type. `render_imports` is *not* an emitter method.
- **Rendered imports are part of `EmittedFile`.** The emitter owns file layout;
  the base renders the import block (per language) and stitches it in.
- **Assembly, reachability, and import resolution are base-owned** (driven by
  `refs` + `module`); the emitter owns type-body rendering and layout.

## Open items

Resolved by the import-resolution prototype (landed in `nex-gen-codegen`):

1. ✅ **Import resolution flow** — the emitter declares `refs` +
   `runtime_imports` and exposes a `NameResolver` via `resolver()`; the base
   resolves (same-module dropped, deduped) and renders via `render_imports`.
   One resolver serves both import resolution and service rendering.
2. ✅ **`Module` / `Import` / `ImportBinding` shapes** — `Module(String)`
   placement key (first-party or foreign); `ImportBinding::{Module, Namespace,
   Named}`; `Import { module, name, binding, type_only }`. Covers first-party
   same/cross-module named imports, proto namespace-head imports, and runtime
   imports. `render_imports` implemented for Python + TypeScript.

Still open:

3. **Emitter construction over private data** — the registry entry for a
   `(lang, schema_type)` pair should build the emitter from the loader's
   private side table (a factory over the IR's private data). The skeleton
   currently stores a constructed emitter; settle the factory signature when
   the WIT loader/emitters land (Phase 3).
