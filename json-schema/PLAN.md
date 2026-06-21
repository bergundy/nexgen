# Plan — JSON Schema → Cross-Language Code Generator

## Mission

Build a code generator that emits idiomatic, statically-typed model
code **and** runtime validators for Go, TypeScript, Python, and Java
from JSON Schema 2020-12 input. Output feeds the Temporal Nexus API
SDK ecosystem; emitted runtime depends only on each language's
Temporal/Nexus SDK plus minimal contrib libraries (Pydantic for
Python).

The generator implements a **strict subset** of JSON Schema:
ambiguous or non-lowerable features are rejected at generator time
with clear fix-it-style diagnostics rather than producing silently-
incorrect output.

## Foundations

- **`PRINCIPLES.md`** — authoritative source for design decisions.
  Cross-cutting principles are numbered (P1–P15) with sub-principles
  (e.g. P10.3); per-language principles live under language sections.
- **Target spec:** JSON Schema 2020-12 (per P6).
  - <https://json-schema.org/draft/2020-12/json-schema-validation>
  - <https://json-schema.org/draft/2020-12/json-schema-core>
- **Per-feature specs:** `features/<keyword>/spec.md`, following the
  template established by `features/type/spec.md`.

## Spec template (per feature)

Each `features/<keyword>/spec.md` follows this skeleton — worked
example at `features/type/spec.md`:

1. **Spec summary** — what the keyword does (3–5 bullets)
2. **Support decision** — support / partial / reject + rationale
   citing PRINCIPLES.md by P-number
3. **Type mapping** — emitted bare type for Go / TS / Python / Java
4. **Validator mapping** — runtime check per language + strategy
5. **Property-testing matrix** — accepted / rejected-at-load / runtime
   fixtures
6. **Interactions** — how this keyword changes meaning of others
7. **Ecosystem variance** — which input dialects accept/reject this
8. **Open questions** — unresolved design points
9. **See also** — wikilinks to related features

## Work completed

### Cross-cutting

- `PRINCIPLES.md`: P1–P16 with sub-principles. **All four language
  sections complete** — Go (5), TypeScript (5), Python (5), Java (4).
  P16 added: uniform `±(2^53−1)` integer cap. Java §4 (error
  aggregation primitive) is documented but its mechanism is still TBD.
- `features/nullability/spec.md`: cross-cutting design note (not a
  keyword). Covers optionality + nullability for all 4 languages with
  empirically-verified per-language enforcement strategies. Zero open
  questions.

### Features

- `features/type/spec.md`: complete. OQ1 (integer cap) **resolved** —
  ±(2^53−1). 1 open question remains — cross-language conformance suite.
- `features/properties/spec.md`: complete. 1 open question — identifier
  case-mapping policy.
- `features/additionalProperties/spec.md`: complete. Lands the
  open-by-default decision + typed-extras support. **All four languages**
  wrap the catch-all in a dedicated named member (even pure maps) for
  shape stability + clean declared/extra key separation. 1 open
  question — Java closed-struct aggregation.
- `features/required/spec.md`: complete. Zero open questions.
- `features/maxProperties/spec.md`, `features/minProperties/spec.md`:
  complete (runtime count assertions). Zero open questions.
- `features/dependentRequired/spec.md`: complete (runtime cross-field
  presence; `dependentSchemas` split off as P5-reject). Zero open Qs.
- `features/patternProperties/spec.md`: complete — **rejected per P5**
  (dynamic keys + regex-dialect + overlap ambiguity). 1 open question —
  possible future single-pattern typed-map carve-out.
- `features/propertyNames/spec.md`: complete — **partial** (map-shaped
  objects only; rejected alongside `properties`). 1 open question —
  static enforcement alongside `properties`.

### Key decisions taken

- **`type` is single-string only** — array form rejected; missing
  `type` rejected; `type: "null"` standalone rejected (only allowed
  inside the nullability pattern).
- **`type: "object"` requires explicit shape** — bare
  `{type:"object"}` rejected; must add `properties`,
  `additionalProperties: true`, or `additionalProperties: false`.
- **Typed structs are open by default** (per spec + P9) — extras
  preserved into a catch-all; closed behavior requires explicit
  `additionalProperties: false`. **Landed** in `properties/spec.md` +
  `additionalProperties/spec.md`. Typed `additionalProperties` is
  **supported in every position** — including alongside `properties` —
  via a **named catch-all field** (`AdditionalProperties` in Go,
  `additionalProperties` in Java), which sidesteps the TS index-signature
  conformance problem. Go/Java emit the wrapper struct **even for pure
  maps** so the shape stays stable when `properties` are added later.
  **All four languages wrap** the catch-all in a dedicated named member,
  even for pure maps: Go `AdditionalProperties`, Java
  `additionalProperties`, TS `additionalProperties: Record<string,T>`,
  Python `BaseModel` + `model_extra`. Two reasons: shape stability (a Py
  `dict` alias → `BaseModel` breaks `m["k"]`, verified
  `/tmp/pyd_map_shape.py`) and a clean split between case-mapped declared
  keys and verbatim extra keys (helps the not-yet-specced key
  canonicalization). A declared member named `additionalProperties`
  collides → reject (Go/Java/TS; Python exempt via `model_extra`).
- **Integer cap = ±(2^53−1)** (`Number.MAX_SAFE_INTEGER`), uniform
  across all four languages (P16). Empirically verified that TS
  `Number.isSafeInteger` enforces it soundly with no third-party parser
  (`/tmp/ts_cap_probe.mjs`). Resolves type/spec.md OQ1.
- **Nullability via `oneOf: [{T}, {null}]`** — narrow exemption to
  P10's discriminator-less `oneOf` rejection, formalized as P10.3.
- **Required + nullable forbidden** (P10.2) — users collapse to
  optional if they want the 2-state space.
- **Optional-non-nullable strictly rejects explicit `null`** —
  per-language enforcement strategies in `nullability/spec.md`.
- **Integer parsing honors spec** — accept `1.0`/`1e2` as integers;
  reject `1.5`. Per-language runtime helpers (`parseSpecInteger`,
  `_parse_spec_integer`, `SpecLongDeserializer`).
- **Java reference types carry JSpecify nullness annotations**
  (PRINCIPLES Java §2). Emitted packages are `@NullMarked`; optional
  reference fields are `@Nullable`, required ones non-null by default.
  Restores for reference types the in-memory nullness signal that
  `long`-vs-`Long` gives scalars (P1); complementary to the non-null
  validator (P7), not a replacement. Tracks required-vs-optional
  (in-memory nullness), not the wire nullable/non-nullable distinction.
  CLASS retention → no runtime dependency (P4 intact); consumers without
  JSpecify on the classpath still compile. JSpecify chosen over JSR-305
  (abandoned + JPMS split-package).
- **Java baseline = Java 8; POJOs, not records** (PRINCIPLES Java §1).
  Records require Java 16+ and would impose a stricter floor than the
  Temporal Java SDK itself (Java 8+) — emitted code must never be more
  restrictive than the SDK it plugs into (P3/P4). All Java mapping tables
  now say POJO/class, not record.
- **Go-specific:** `int64`/`float64` numeric primitives; optional via
  `*T`; `new(expr)` for pointer-from-literal (Go 1.26+ preferred,
  not required); custom `UnmarshalJSON` on every struct with
  `*json.RawMessage` shadow + `errors.Join` aggregation.

### Methodology established

- **Empirical verification.** Pydantic v2, Jackson 2.18, and
  `JSON.parse` + `Number.isInteger` all have non-trivial behaviors
  that diverge from what their docs imply. Verify with throwaway
  probes before committing the spec text.
- **Probes at `/tmp/`** (re-runnable any time, candidate to promote
  into a conformance suite — see `type/spec.md` OQ2):
  - `/tmp/pyd_int_test.py` — baseline Pydantic int under strict vs
    lax × Python-mode vs JSON-mode
  - `/tmp/pyd_int_test2.py` — `Annotated[int, BeforeValidator(...)]`
    + strict combo
  - `/tmp/pyd_nullopt_probe.py` — `model_validator(mode='wrap')`
    aggregated error handling
  - `/tmp/jacktest/` — Jackson Maven project: default Long behavior,
    `ACCEPT_FLOAT_AS_INT=false`, custom `SpecLongDeserializer`
  - `/tmp/ts_int_probe.mjs` — JSON.parse + Number.isInteger,
    including the silent >2^53 precision loss
  - `/tmp/ts_cap_probe.mjs` — proves `Number.isSafeInteger` is a
    complete+sound ±(2^53−1) cap check (zero leaks swept above 2^53)
  - `/tmp/pyd_extra_probe.py` — Pydantic `extra='allow'` preserves +
    round-trips unknowns; `extra='forbid'` aggregates per-key
    (run via `uv run --with pydantic python3 …`)
  - `/tmp/pyd_typed_extra.py` — typed extras: `extra='allow'` + post-init
    per-extra `T` validation aggregates bad keys, round-trips good ones
  - `/tmp/gorawprobe/` — Go `any` vs `json.RawMessage` round-trip: `any`
    loses int precision (`>2^53`), reformats numbers, reorders keys;
    `RawMessage` is byte-faithful (justifies the untyped-extras choice)
  - `/tmp/pyd_map_shape.py` — Python pure-map shape instability: a
    `dict[str,T]` alias that becomes a `BaseModel` breaks `m["k"]`
    (justifies always emitting pure maps as Pydantic models)
  - `/tmp/ts_flatten.ts` — TS index-signature conformance: a *typed*
    `[k:string]:T` alongside `id:number` is TS2411 (illegal); this is why
    typed extras use a named `additionalProperties: Record<string,T>`
    member instead of an inline index signature
- **Decisions cite principles by P-number.** Every Support decision
  in a feature spec must reference the P-number(s) it's grounded in.
- **Three surprises worth noting for future Python/Java work:**
  - Pydantic's `_FIELD: ClassVar[T]` is required — bare `_FIELD`
    becomes a private model attr.
  - Pydantic's `model_validator(mode='before')` that raises
    short-circuits Pydantic's own field validation; use `mode='wrap'`
    if you want P8 aggregation across both error sources.
  - Jackson's default `Long` deserializer **silently truncates**
    `1.5` to `1`. Custom deserializer is mandatory, not optional.

## Remaining work

### High priority

All three former high-priority items are **resolved** (2026-06-16):

1. ~~Resolve `type/spec.md` OQ1 (large integer cap).~~ **DONE** —
   ±(2^53−1) uniform cap (P16). TS enforces via `Number.isSafeInteger`,
   no third-party parser needed (empirically verified).
2. ~~Fill PRINCIPLES.md → TypeScript, Python, Java sections.~~ **DONE** —
   all three sections written (Java §4 aggregation mechanism still TBD,
   tracked as an open question).
3. ~~Decide + land closed-vs-open default for typed structs.~~ **DONE** —
   open-by-default; landed in `properties/spec.md` +
   `additionalProperties/spec.md`.

Next-highest leverage (newly surfaced TBDs that gate clusters of specs):
- **Java error-aggregation primitive** (PRINCIPLES Java §4). Gates the
  closed-struct and multi-field-error story across every Java spec.
- **`$ref`** — still the single highest-priority structural keyword
  (drives file-per-input + merge-on-cycle, P13–P14).

### Feature specs to write (≈50)

Roughly in priority order — start with keywords that gate other
decisions:

**Object structure:**
- ✅ `properties`, ✅ `additionalProperties` (open/closed landed)
- ✅ `required`, ✅ `minProperties`, ✅ `maxProperties`,
  ✅ `dependentRequired`, ✅ `patternProperties` (reject),
  ✅ `propertyNames` (partial)
- Remaining: `unevaluatedProperties` (expect P5-reject),
  `dependentSchemas` (expect P5-reject)

**Any-type assertions:**
- `enum`, `const`

**Numeric assertions** (gated by integer-cap decision):
- `multipleOf`, `maximum`, `exclusiveMaximum`, `minimum`,
  `exclusiveMinimum`

**Array structure:**
- `items`, `prefixItems`, `contains`, `unevaluatedItems`,
  `minItems`, `maxItems`, `uniqueItems`, `maxContains`, `minContains`

**String assertions:**
- `maxLength`, `minLength`, `pattern`

**Applicators (mostly P5-rejected, each needs a spec'd rejection):**
- `allOf`, `anyOf`, `not`, `if-then-else` — reject per P5; document
  rationale and rewrite hints.
- `oneOf` — partial: nullability pattern accepted (P10.3);
  discriminator-bearing form is the next open question (P10 implies
  support for the discriminator form, but no convention spec'd yet).

**Core / structural:**
- `$schema`, `$id`, `$ref`, `$defs`, `$anchor`, `$dynamicRef`,
  `$dynamicAnchor`, `$vocabulary`, `$comment`. `$ref` is highest
  priority (drives file-per-input + merge-on-cycle per P13–P14).

**Metadata / annotations:**
- `format` — high priority, codegen-relevant (e.g. `date-time` →
  `time.Time` in Go).
- `title`, `description`, `default`, `examples`, `deprecated`,
  `readOnly`, `writeOnly`, `contentEncoding`, `contentMediaType`,
  `contentSchema` — lower priority; mostly pure metadata.

## Open question inventory

Snapshot as of this checkpoint.

### `features/type/spec.md`
1. ~~Large integers — cross-language cap.~~ **Resolved: ±(2^53−1).**
2. **Cross-language conformance suite** for integer runtime helpers.

### `features/properties/spec.md`
1. **Identifier case-mapping policy** — one shared JSON-name →
   idiomatic-identifier algorithm + collision/escape-hatch rules.

### `features/additionalProperties/spec.md`
1. **Java closed-struct aggregation** (depends on Java agg primitive).
2. **Go catch-all field naming/exposure** for open structs.

### `features/patternProperties/spec.md`
1. Possible future single-pattern typed-map carve-out (deferred).

### `features/propertyNames/spec.md`
1. Static enforcement of `propertyNames` alongside `properties`
   (currently rejected; deferred).

### `features/nullability/spec.md`
- None.

### `PRINCIPLES.md`
- Java §4 — **error-aggregation primitive mechanism still TBD** (the
  one remaining language-section gap).

## How to pick up the work in a new session

1. Read `PRINCIPLES.md` and this `PLAN.md`.
2. Read `features/type/spec.md` as the worked-example template
   and `features/nullability/spec.md` for cross-cutting conventions.
3. Pick a feature from the priority list above.
4. Use `WebFetch` to grab the JSON Schema 2020-12 spec text for that
   keyword (links at top). **Don't trust the doc-fetcher's summary**
   — it has truncated/misreported tables on several keywords. Quote
   verbatim from the spec proper.
5. Draft the spec.md against the template.
6. For any non-trivial language behavior (Pydantic, Jackson, JS
   parsing), write a quick probe in `/tmp/` and verify empirically
   before committing prose.
7. Cite PRINCIPLES.md P-numbers in every Support decision.
8. If a decision needs human input, surface it explicitly — don't
   guess and don't quietly defer.
9. Update this `PLAN.md` open-question inventory and "work completed"
   sections after the spec lands.

## Files of record

- `json-schema/PRINCIPLES.md` — decisions
- `json-schema/PLAN.md` — this file (state + next steps)
- `json-schema/features/<keyword>/spec.md` — per-feature design
