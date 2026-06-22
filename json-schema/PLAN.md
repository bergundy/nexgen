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
4. **Validator mapping** — runtime check per language + strategy,
   **including a `Serialize-side (P17)` subsection**: which checks the
   shared `Validate` re-runs on emit vs. which are parse-adapter-only
   (deserialize) or encode-adapter-only (serialize: omit/emit-`null`,
   default omission, const auto-emit)
5. **Property-testing matrix** — accepted / rejected-at-load / runtime
   fixtures
6. **Interactions** — how this keyword changes meaning of others
7. **Ecosystem variance** — which input dialects accept/reject this
8. **Open questions** — unresolved design points
9. **See also** — wikilinks to related features

## Work completed

### Cross-cutting

- `PRINCIPLES.md`: P1–P17 with sub-principles. **All four language
  sections complete** — Go (6), TypeScript (6), Python (6), Java (6).
  P16: uniform `±(2^53−1)` integer cap. **P17 added: serialize-side
  validation** — both directions share one `Validate(model)` over the
  decoded model, flanked by a deserialize-only parse adapter and a
  serialize-only encode adapter; no IR round-trip. **P11 amended**:
  `default` is materialized on *read* (set-ness tracked, omit-unset on
  serialize, no deep-equals), not stored on deserialize. Each language
  section gained a §6 serialize note. Java §5 (error aggregation
  primitive) is still TBD and **now gates the serialize direction too**.
- `features/nullability/spec.md`: cross-cutting design note (not a
  keyword). Covers optionality + nullability for all 4 languages with
  empirically-verified per-language enforcement strategies. **Now carries
  the per-field serialize omit-vs-emit-`null` table (P17)** and the
  **Python faithful-round-trip upgrade** for optional+nullable (via
  `model_fields_set`/`exclude_unset`, same tier as TS) — closing the old
  "wrapper type" open question. Go/Java stay conservative-omit. Zero open
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
  complete (runtime count assertions). Count is over **distinct wire
  member keys**, taken before default population (P11) — defaults never
  count; counted as one number, never summed across declared/extras
  buckets (avoids case-mapping mis-routing + Pydantic bucket overlap).
  Interactions with `default` and `const` documented (object-level
  `const` → static satisfiability check, deferred to the const spec).
  Zero open questions.
- `features/dependentRequired/spec.md`: complete (runtime cross-field
  presence; `dependentSchemas` split off as P5-reject). Zero open Qs.
- `features/patternProperties/spec.md`: complete — **temporarily
  unsupported** (rejected at load time in v1, but *deferred* not
  categorically excluded: the general form has dynamic-keys +
  regex-dialect + overlap ambiguity, yet a single-pattern typed-map form
  is plausibly lowerable). 1 open question — that single-pattern carve-out
  is the path to "partial."
- `features/propertyNames/spec.md`: complete — **partial** (map-shaped
  objects only; rejected alongside `properties`). 1 open question —
  static enforcement alongside `properties`.

### Key decisions taken

- **Serialize-side validation is first-class (P17).** Validation runs in
  *both* directions over one shared `Validate(model)` (constraint
  predicates over the decoded model), with mirror-image adapters: a
  deserialize-only parse adapter (spec-number parse, explicit-`null`
  reject, wire-absence→required, type-token classification) and a
  serialize-only encode adapter (omit-vs-emit-`null`, default omission,
  const auto-emit). **No IR round-trip** — sharing is at the predicate
  layer, not by re-serializing to a generic value tree. Serialize fails
  before emitting a byte; Python only re-validates to catch
  `model_construct`/mutation bypasses. Empirically proven in Go +
  Python (`/tmp/serialize_probe/`, `/tmp/oe/`, `/tmp/pyd_serialize_probe.py`,
  `/tmp/pyd_null_serialize_probe.py`).
- **`default` materialized on read, not stored (P11 amended).** Track
  set-ness; serialize omits unset fields with **no deep-equals** against
  the default; surface the default on read (accessor / native default).
  Explicit-set pins. Mechanisms: Go `,omitempty`+pointer, Pydantic
  `exclude_unset`, TS `undefined`, Java `@JsonInclude(NON_NULL)`.
- **`const` auto-emits on serialize, never omit-unset.** `const` is a
  contract assertion (often a discriminator that *must* be on the wire),
  not a population directive — so it is auto-populated and always
  emitted (optional+const is the only emit-if-set case). Lumping it into
  omit-unset would drop a defaulted discriminator (proven in
  `/tmp/pyd_serialize_probe.py`). Enforcement detail deferred to the
  `const` spec.
- **Optional+nullable round-trip is capability-tiered:** faithful in
  TS *and Python* (`undefined` / `model_fields_set`+`exclude_unset`),
  conservative-omit in Go/Java (`*T` nil / `null` collapse; faithful
  would need a presence wrapper — rejected for v1 as P2 overhead).
  Per-field omit-vs-emit-`null` is a static decision from the
  optional/nullable/required declaration; the full table lives in
  [[nullability]]. Proven `/tmp/pyd_null_serialize_probe.py`.
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
- **Required + nullable supported** (P10.2 reread as orthogonality) —
  presence and null-acceptance are independent axes, so all four
  states are legal, including required+nullable ("must be present, may
  be `null`"). The earlier prohibition rested on a flawed
  "operationally indistinguishable" rationale; required+nullable is
  decidable, enforceable (presence-check on, null-rejection off), and
  round-trips losslessly in all four languages. The only residual
  absent-vs-`null` collapse is *optional+nullable* in Java/Go/Python,
  which serializes to a single canonical form (absent/omitted — the
  conservative default); TS round-trips all states faithfully. Landed in
  [[nullability]], [[required]], [[properties]] (self-ref termination
  now also via `null`), PRINCIPLES P10/P10.2.
- **Optional-non-nullable strictly rejects explicit `null`** —
  per-language enforcement strategies in `nullability/spec.md`.
- **Integer parsing honors spec** — accept `1.0`/`1e2` as integers;
  reject `1.5`. Per-language runtime helpers (`parseSpecInteger`,
  `_parse_spec_integer`, `SpecLongDeserializer`).
- **Java reference types carry JSpecify nullness annotations**
  (PRINCIPLES Java §3). Emitted packages are `@NullMarked`; optional
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
  - `/tmp/pyd_minprops_probe.py` — property counting for
    min/maxProperties: `len(model_fields_set)` is the exact wire-key
    count (includes extras, excludes default-filled fields). Naive
    `len(model_dump())` over-counts via defaults; `model_fields_set` +
    `__pydantic_extra__` over-counts because extras appear in *both*.
    Count wire keys as one number, never sum buckets.
  - `/tmp/ts_flatten.ts` — TS index-signature conformance: a *typed*
    `[k:string]:T` alongside `id:number` is TS2411 (illegal); this is why
    typed extras use a named `additionalProperties: Record<string,T>`
    member instead of an inline index signature
  - `/tmp/serialize_probe/` — Go shared `Validate()` called by BOTH
    `MarshalJSON` and `UnmarshalJSON`; parse-layer (1.5 reject) stays
    deserialize-only; default omit-unset + const auto-emit; round-trip
    byte-identical (no default echo) — proves the P17 decomposition
  - `/tmp/oe/` — Go `omitempty` quadrant: nil omits / ptr-to-`""` emits;
    no-`omitempty` `*T` nil → `null`; type-alias `MarshalJSON` honors
    tags without recursion (the declarative omit-vs-`null` encode layer)
  - `/tmp/pyd_serialize_probe.py` — Pydantic `exclude_unset` omits while
    the attr still reads the default (no deep-equals); explicit-set pins;
    const+`exclude_unset` would wrongly drop a discriminator (→ const
    must force-emit); `model_construct` bypass caught by re-validation
  - `/tmp/pyd_null_serialize_probe.py` — per-field omit-vs-emit-`null`:
    required+nullable emits `null`; `model_fields_set` distinguishes
    wire-`null` from wire-absent (Python optional+nullable is faithful);
    optional-non-nullable explicit `null` rejected in strict mode
- **Decisions cite principles by P-number.** Every Support decision
  in a feature spec must reference the P-number(s) it's grounded in.
- **Surprises worth noting for future Python/Java/TS work:**
  - Pydantic's `_FIELD: ClassVar[T]` is required — bare `_FIELD`
    becomes a private model attr.
  - Pydantic's `model_validator(mode='before')` that raises
    short-circuits Pydantic's own field validation; use `mode='wrap'`
    if you want P8 aggregation across both error sources.
  - Jackson's default `Long` deserializer **silently truncates**
    `1.5` to `1`. Custom deserializer is mandatory, not optional.
  - Pydantic's `model_fields_set` **includes extras and excludes
    default-filled fields** — so it is the exact wire-key count for
    min/maxProperties. Summing `model_fields_set` + `__pydantic_extra__`
    double-counts extras (they live in both). Count wire keys once.
  - Pydantic `model_dump(exclude_unset=True)` omits unset fields **while
    the attribute still reads the default** — gives P11 omit-unset and
    faithful optional+nullable in one flag, no deep-equals. And
    `model_fields_set` marks a field set when the wire carried explicit
    `null`, so it distinguishes wire-`null` from wire-absent (Python is
    in the faithful round-trip tier with TS).
  - `JSON.stringify` **silently coerces `NaN`/`±Infinity` to `null`** —
    the TS serializer must reject non-finite numbers before stringifying
    (the one numeric check the TS type system doesn't already give).

## Remaining work

### High priority

All three former high-priority items are **resolved** (2026-06-16):

1. ~~Resolve `type/spec.md` OQ1 (large integer cap).~~ **DONE** —
   ±(2^53−1) uniform cap (P16). TS enforces via `Number.isSafeInteger`,
   no third-party parser needed (empirically verified).
2. ~~Fill PRINCIPLES.md → TypeScript, Python, Java sections.~~ **DONE** —
   all three sections written (Java §5 aggregation mechanism still TBD,
   tracked as an open question).
3. ~~Decide + land closed-vs-open default for typed structs.~~ **DONE** —
   open-by-default; landed in `properties/spec.md` +
   `additionalProperties/spec.md`.

Next-highest leverage (newly surfaced TBDs that gate clusters of specs):
- **Java error-aggregation primitive** (PRINCIPLES Java §5). Gates the
  closed-struct and multi-field-error story across every Java spec.
- **`$ref`** — still the single highest-priority structural keyword
  (drives file-per-input + merge-on-cycle, P13–P14).

### Feature specs to write (≈50)

Roughly in priority order — start with keywords that gate other
decisions:

**Object structure:**
- ✅ `properties`, ✅ `additionalProperties` (open/closed landed)
- ✅ `required`, ✅ `minProperties`, ✅ `maxProperties`,
  ✅ `dependentRequired`, ✅ `patternProperties` (temporarily unsupported),
  ✅ `propertyNames` (partial)
- Remaining: `unevaluatedProperties` (expect P5-reject),
  `dependentSchemas` (expect P5-reject)

**Any-type assertions:**
- `enum`, `const` — `const` must specify the **serialize-side rule
  (P17): auto-populate + always emit** the fixed value (it is a contract
  assertion / discriminator, never omit-unset; optional+const is the
  only emit-if-set case).

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
  `contentSchema` — lower priority; mostly pure metadata. **Exception:
  `default` is not pure metadata** — its spec must encode the amended
  P11 (set-ness tracking, omit-unset on serialize, materialize-on-read,
  no deep-equals).

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
- Java §5 — **error-aggregation primitive mechanism still TBD** (the
  one remaining language-section gap). **Now gates the serialize
  direction too (P17 / Java §6):** the same single-shot aggregation
  problem applies mirror-image when validating before write.

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
