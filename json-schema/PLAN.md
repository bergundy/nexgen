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
  Cross-cutting principles are numbered (P1–P16) with sub-principles
  (e.g. P7.1); per-language principles live under language sections.
- **Target spec:** JSON Schema 2020-12 (per P5).
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
   **including a `Serialize-side (P12)` subsection**: which checks the
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

- `PRINCIPLES.md`: P1–P16 with sub-principles. P12: serialize-side validation — both directions share one
  `Validate(model)` over the decoded model, flanked by a deserialize-only
  parse adapter and a serialize-only encode adapter; no IR round-trip.
  P15: synthesized-identifier collisions reject at load time (no
  mangling), sharing one per-scope namespace with declared names. (The
  uniform `±(2^53−1)` integer cap now lives in [[type]]; `default`
  off-the-wire / materialized-on-read lives in [[default]].) Java §5: a
  per-POJO class-level collecting `@JsonDeserialize`/`@JsonSerialize`
  (two-stage lenient-tree-then-validate, the Jackson analog of Go's
  shadow-layout `UnmarshalJSON`), throwing one `ValidationException
  extends JsonMappingException` with `List<Violation{path,reason}>`.
  All four language sections complete — Go (6), TypeScript (6),
  Python (6), Java (6).
- `features/nullability/spec.md`: cross-cutting design note (not a
  keyword). Covers optionality + nullability for all 4 languages with
  per-language enforcement strategies. Carries the per-field serialize
  omit-vs-emit-`null` table (P12). Python optional+nullable uses
  `model_fields_set`/`exclude_unset` — faithful round-trip, same tier as
  TS. Go/Java are conservative-omit. Zero open questions.

### Features

- `features/type/spec.md`: complete. Integer cap: ±(2^53−1). 1 open
  question — cross-language conformance suite.
- `features/properties/spec.md`: complete. Shared 4-stage case-mapping
  algorithm + `x-*-name` escape hatch + P15 per-scope collision pass.
  One implementation dependency: the Python serialize keep-set must map
  field name↔alias when a JSON name is case-mapped (PRINCIPLES Python §6).
- `features/additionalProperties/spec.md`: complete. Open-by-default;
  typed-extras supported in every position. All four languages wrap the
  catch-all in a dedicated named member (even pure maps) for shape
  stability and clean declared/extra key separation. Go exported
  `AdditionalProperties` map field; a declared member named
  `additionalProperties` is rejected at load time. Zero open questions.
- `features/required/spec.md`: complete. Zero open questions.
- `features/maxProperties/spec.md`, `features/minProperties/spec.md`:
  complete (runtime count assertions). Count is over **distinct wire
  member keys**, taken before default population (see [[default]]) — defaults never
  count; counted as one number, never summed across declared/extras
  buckets. Zero open questions.
- `features/dependentRequired/spec.md`: complete (runtime cross-field
  presence; `dependentSchemas` split off as P6-reject). Zero open Qs.
- `features/patternProperties/spec.md`: complete — **temporarily
  unsupported** (rejected at load time in v1, deferred not categorically
  excluded: a single-pattern typed-map form is plausibly lowerable).
  1 open question — single-pattern carve-out.
- `features/propertyNames/spec.md`: complete — **partial** (map-shaped
  objects only; rejected alongside `properties`). 1 open question —
  static enforcement alongside `properties`.
- `features/const/spec.md`: complete — **supported (scalar)**. Emits the
  underlying primitive (P13.1) via an open form per target, closed only at
  runtime: TS `'v' | (string & {})` + `readonly`, Go alias
  `type X = string` + typed value const, Python `Literal['v'] | str`,
  Java a generated value class (`@JsonCreator`/`@JsonValue`,
  `isUnrecognized()`) shared with [[enum]] (const = single-value
  specialization). `const` is a pure assertion validated in both
  directions; presence is owned by `required`; the value reaches the wire
  because it is set in memory. Mutually exclusive with `default` and
  `enum`. `const:null` rejected; composite (object/array) const
  temporarily unsupported. 1 open question — composite-const carve-out.
- `features/default/spec.md`: complete — **supported** with off-the-wire /
  materialized-on-read semantics: annotation (no validator, never fails validation);
  off-the-wire; set-ness tracked; omit-unset with no deep-equals;
  materialized on read. Strengthens the spec's "RECOMMENDED valid
  default" to a load-time MUST (P7.1); rejects `default` on a required
  member and `default`+`const`. Read-side surfacing is native in Python
  (Pydantic field default) and Java (getter), advisory in TS
  (`?? DEFAULT_X` constant), and a generated `<Field>OrDefault()`
  accessor in Go (proto3 `GetX()`-style). Scalar-only in v1
  (`string`/`number`/`integer`/`boolean`); object/array defaults and
  `default:null` rejected, expected to relax. 1 open question —
  composite-default materialization.

- `features/maximum/spec.md` (+ `minimum`, `exclusiveMaximum`,
  `exclusiveMinimum`): complete — **supported** (runtime comparison
  assertions). `maximum` is the canonical numeric-bound spec; the other
  three reference its machinery. Comparison is a shared-`Validate`
  predicate (P12), enforced both directions. **Integer-field bounds MUST
  be integer-valued** (`5.0` ok, `5.5` reject) — Pydantic can't build a
  fractional `le`/`ge` on an `int` field, and an integer bound keeps the
  integer-vs-float comparison lossless (the `±(2^53−1)` cap is exactly
  representable as a double, so `(double)cap == cap`, verified). Combined
  bounds reject when the interval is empty (incl. the integer "no integer
  in range" case). A **same-axis bound pair on one node is rejected as
  redundant** (`maximum`+`exclusiveMaximum`, `minimum`+`exclusiveMinimum`
  — one always dominates; P7.1), with an allOf-tightening caveat noted in
  the Applicators section. The **exclusive pair reject the draft-4 /
  OpenAPI-3.0 boolean form** (`exclusiveMaximum: true`) with a rewrite
  fix-it — the largest source-dialect difference in the family. Zero open
  questions.
- `features/multipleOf/spec.md`: complete — **partial**: positive
  **integer** divisor only; **fractional divisor temporarily unsupported**
  (reject at load, deferred not excluded). Empirically justified (P1):
  integer modulo and IEEE `fmod` agree value-for-value across all four
  languages, but Pydantic's native float `multiple_of` is *tolerant* and
  diverges for fractional divisors (accepts `0.3` against `multipleOf:0.1`
  where `fmod` rejects) — irreconcilable without a shared decimal lib (P4).
  Integer field → integer modulo; number field → `fmod` (Python uses an
  explicit `fmod` AfterValidator for numbers, native `multiple_of` for
  ints). Combined with a range: reject when no multiple lies in the
  interval. 1 open question — fractional-divisor decimal-scaling carve-out.
- `features/services/spec.md`: complete — **supported**. Nexus extension
  (not a JSON Schema keyword, like [[nullability]]): a top-level
  `services` map → per-language service bindings (Go `struct` of
  `OperationReference`s, TS `nexus.service`, Python `@nexusrpc.service`
  class, Java `@Service` interface). Two-name model (identifier key +
  optional `fqn` wire name); operation default wire name = PascalCase
  canonical; the generator always emits the resolved wire name explicitly
  (the four SDKs default differently) for P1. `input`/`output` is
  **object-only** — a `$ref` to an object `$defs` or an inline object
  promoted to a synthesized `<Op>Input`/`Output` type (P15 namespace);
  non-object I/O rejects; omitted → void (`nexus.NoValue`/`void`/`None`/
  `void`). **Document gating now lives in [[input-files]]:** `services` is
  recognized only in a Nexus document (root `nexusrpc: "1.0.0"`); the
  envelope, dialect, and stray-`services` rules moved there. TS service
  const is `camelCase` (the WIT generator emits
  PascalCase and must be fixed). Services emit into the declaring module;
  Java is the per-file exception. Shape-compatible with the existing WIT
  emitters for reuse by the future separate crate. 2 open questions.

### Cross-cutting (continued)

- `input-files.md`: cross-cutting design note (not a keyword) — the
  document-level concerns of an input file. Two file modes selected by the
  root `nexusrpc` property: **Nexus document** (root is an envelope, not a
  type; enables [[services]]; a schema-shaped root keyword rejects) vs
  **pure JSON Schema** (root is a type). Root rules: `nexusrpc` v1 =
  exactly `"1.0.0"` (else reject, P13 forward-compat); `$schema` optional,
  2020-12 only / assumed-when-absent / reject-other (P5); `$id` rejected
  (owned by [[ref]]); stray-`services` guard (P7.1). References [[ref]] /
  [[generated-file-layout]] for the input-set/closure and module
  computation rather than duplicating them. Houses the dialect/`$id`
  decisions the standalone `$schema`/`$id` keyword specs should defer to.
  2 open questions (keyword-spec deferral; widening the `nexusrpc` range).

### Key decisions taken

- **Serialize-side validation is first-class (P12).** Validation runs in
  both directions over one shared `Validate(model)` (constraint
  predicates over the decoded model), with mirror-image adapters: a
  deserialize-only parse adapter (spec-number parse, explicit-`null`
  reject, wire-absence→required, type-token classification) and a
  serialize-only encode adapter (omit-vs-emit-`null`, default omission,
  const auto-emit). No IR round-trip — sharing is at the predicate layer.
  Serialize fails before emitting a byte; Python re-validates to catch
  `model_construct`/mutation bypasses. The Python encode adapter is not a
  call site we own — the default Temporal `pydantic_data_converter`
  serializes via plain `pydantic_core.to_json` — so the omit/const/guard
  logic is baked into a generated `@model_serializer(mode='wrap')`, which
  `to_json` honors.
- **`default` materialized on read, not stored (see [[default]]).** Track set-ness;
  serialize omits unset fields with no deep-equals; surface the default
  on read. Preserving absent-vs-set (P9) protects forward-compat (P13),
  live default evolution, and proxy/intermediary fidelity — exactly
  proto3's omit-on-wire model. Serialize/omit mechanisms: Go
  `,omitempty`+pointer, Pydantic a generated `@model_serializer` over
  `model_fields_set`, TS `undefined`, Java `@JsonInclude(NON_NULL)`.
  Read-side materialize mechanisms: Java getter / Pydantic field default
  (native), Go `<Field>OrDefault()` accessor (proto3 `GetX()`-style),
  TS `?? DEFAULT_X` + emitted constant.
- **`const` is a pure assertion — no serialize-side special-casing.**
  `const` is treated as a single-value `enum`: validate `== value` in
  both directions; the generator does not force-write. Presence is owned
  by `required`; the fixed value reaches the wire because it is set in
  memory. Emits the underlying primitive via an open form (P13.1): TS
  `'v' | (string & {})` + `readonly`, Go named alias `type X = string`
  + typed value const, Python `Literal['v'] | str`, Java a generated
  value class shared with [[enum]]. Go sets it via that value const (zero
  value fails `Validate` loudly); Java uses a `final` field initialized
  to the known constant + getter, no setter, no builders; Python injects
  it in a `model_validator(mode='before')` (which marks it set →
  `model_fields_set` → emitted by the generic `@model_serializer`).
  Mutually exclusive with `default`/`enum`; `const:null` and composite
  consts rejected/deferred.
- **Synthesized-identifier collisions reject at load time, never mangle
  (P15).** Synthesized names — [[const]] type aliases + value consts,
  future [[enum]] value class/members, the Go `<Field>OrDefault()`
  accessor and TS `DEFAULT_<FIELD>` ([[default]]) — share one per-scope
  namespace with declared types/members and with each other
  (package/module scope; the Go accessor sits in the struct method-set,
  where a field/method clash is a hard compile error). A single collision
  pass (after case-mapping) rejects loudly on any coincidence.
  Auto-mangling is rejected as unstable under schema evolution (P13). The
  escape hatch is the [[properties]] `x-*-name` override on the
  declaring member.
- **Optional+nullable round-trip is capability-tiered:** faithful in
  TS and Python (`undefined` / `model_fields_set` via the generated
  `@model_serializer`), conservative-omit in Go/Java (`*T` nil / `null`
  collapse; faithful would need a presence wrapper — rejected for v1 as
  P2 overhead). Per-field omit-vs-emit-`null` is a static decision from
  the optional/nullable/required declaration; the full table lives in
  [[nullability]].
- **`type` is single-string only** — array form rejected; missing
  `type` rejected; `type: "null"` standalone rejected (only allowed
  inside the nullability pattern).
- **`type: "object"` requires explicit shape** — bare `{type:"object"}`
  rejected; must add `properties`, `additionalProperties: true`, or
  `additionalProperties: false`.
- **Typed structs are open by default** (per spec + P13) — extras
  preserved into a catch-all; closed behavior requires explicit
  `additionalProperties: false`. Typed `additionalProperties` is
  supported in every position — including alongside `properties` — via a
  named catch-all field (`AdditionalProperties` in Go,
  `additionalProperties` in Java), which sidesteps the TS index-signature
  conformance problem. All four languages wrap the catch-all in a
  dedicated named member, even for pure maps: Go `AdditionalProperties`,
  Java `additionalProperties`, TS `additionalProperties: Record<string,T>`,
  Python `BaseModel` + `model_extra`. A declared member named
  `additionalProperties` collides → reject (Go/Java/TS; Python exempt via
  `model_extra`).
- **Integer cap = ±(2^53−1)** (`Number.MAX_SAFE_INTEGER`), uniform
  across all four languages (see [[type]]). TS `Number.isSafeInteger` enforces it
  soundly with no third-party parser.
- **Nullability via `oneOf: [{T}, {null}]`** — narrow exemption to
  P7's discriminator-less `oneOf` rejection (see [[nullability]]; a
  general `oneOf` convention is deferred to a future oneOf spec).
- **Required + nullable supported** (P8) — presence and
  null-acceptance are independent axes; all four states are legal,
  including required+nullable ("must be present, may be `null`").
  Required+nullable is decidable, enforceable (presence-check on,
  null-rejection off), and round-trips losslessly in all four languages.
  The only residual absent-vs-`null` collapse is optional+nullable in
  Java/Go/Python (conservative-omit); TS round-trips all states
  faithfully.
- **Optional-non-nullable strictly rejects explicit `null`** —
  per-language enforcement strategies in `nullability/spec.md`.
- **Integer parsing honors spec** — accept `1.0`/`1e2` as integers;
  reject `1.5`. Per-language runtime helpers (`parseSpecInteger`,
  `_parse_spec_integer`, Java node helper `SpecNumbers.specLong`).
- **Java reference types carry JSpecify nullness annotations**
  (PRINCIPLES Java §3). Emitted packages are `@NullMarked`; optional
  reference fields are `@Nullable`, required ones non-null by default.
  CLASS retention → no runtime dependency (P4 intact). JSpecify chosen
  over JSR-305 (abandoned + JPMS split-package).
- **Java baseline = Java 8; POJOs, not records** (PRINCIPLES Java §1).
  Records require Java 16+ and would impose a stricter floor than the
  Temporal Java SDK (Java 8+) — emitted code must never be more
  restrictive than the SDK it plugs into (P3/P4).
- **Go-specific:** `int64`/`float64` numeric primitives; optional via
  `*T`; `new(expr)` for pointer-from-literal (Go 1.26+ preferred, not
  required); custom `UnmarshalJSON` on every struct with `*json.RawMessage`
  shadow, collecting `Violation`s into a single aggregating
  `ValidationError` (a struct over `[]Violation` implementing `error`, not
  `errors.Join`).
- **Java error aggregation is a per-POJO collecting (de)serializer
  (Java §4–§6).** Each emitted POJO carries class-level
  `@JsonDeserialize(using=<Pojo>.Deserializer.class)` +
  `@JsonSerialize(using=<Pojo>.Serializer.class)`. The (de)serializers
  are emitted as `public static final` nested classes on the model
  (`User.Deserializer` / `User.Serializer`) — each model owns its pair,
  names never collide across models (same nesting idiom as P15's
  const/enum value classes). The deserializer does a two-stage
  lenient-tree-then-validate bind (`readValueAsTree()` defeats Jackson's
  fail-fast `MismatchedInputException`, then every field runs through
  shared spec-strict + constraint helpers, collecting
  `Violation{path,reason}`) and throws one `ValidationException extends
  JsonMappingException`. The spec-strict integer parse is a node helper
  (`SpecNumbers.specLong(JsonNode,…)`) called by the collecting
  deserializer; the explicit-`null` decision is a per-field branch over
  `node.isNull()`. This works through the default Temporal data converter
  (which owns a stock `new ObjectMapper()` we can't configure): the hook
  is baked into the POJO via annotations, and the aggregated
  `ValidationException` surfaces as the cause of `DataConverterException`
  (handler walks the chain → `getViolations()` → one BAD_REQUEST).
  Serialize side (§6) rides the same primitive. Closed-struct extra-key
  aggregation falls out of the tree stage, closing the additionalProperties
  Java question.
- **NOTE — aggregated error does not yet surface to the caller.** The
  current Temporal SDKs offer no hook to aggregate per-field validation
  errors and map them onto the `BAD_REQUEST` `HandlerError` returned to
  the Nexus caller (the path P11 assumes — handler walks the cause chain,
  pulls `getViolations()`, emits one `HandlerError`). All the generated
  (de)serializers still *collect* every violation into the per-language
  aggregated primitive (the Go/TS `ValidationError` over `[]Violation` /
  `pydantic.ValidationError` / `ValidationException`), but for now that
  aggregate stays internal: it
  fails the (de)serialize boundary without being projected onto the
  wire-level error the caller receives. Surfacing it requires an SDK
  change; tracked in `TODO.md`.

### Methodology established

- **Empirical verification.** Pydantic v2, Jackson 2.18, and
  `JSON.parse` + `Number.isInteger` all have non-trivial behaviors
  that diverge from what their docs imply. Verify with throwaway
  probes before committing the spec text.
- **Probes at `json-schema/research/`** — re-runnable at any time, candidate
  to promote into a conformance suite (see `type/spec.md` OQ2).
- **Decisions cite principles by P-number.** Every Support decision
  in a feature spec must reference the P-number(s) it's grounded in.
- **Key empirical findings for future work:**
  - Pydantic's `_FIELD: ClassVar[T]` is required — bare `_FIELD`
    becomes a private model attr.
  - Pydantic's `model_validator(mode='before')` that raises
    short-circuits Pydantic's own field validation; use `mode='wrap'`
    for P11 aggregation across both error sources.
  - A key injected into the input dict by a `model_validator(mode='before')`
    lands in `model_fields_set` — Pydantic treats it as provided. This
    is what lets `const` auto-fill and emit through the generic omit-unset
    serializer with no special keep-set.
  - Jackson's default `Long` deserializer silently truncates `1.5` to `1`.
    Custom deserializer is mandatory.
  - Jackson is fail-fast: the first field's `MismatchedInputException`
    aborts the whole bind, so per-field `@JsonDeserialize` cannot aggregate
    (P11). The class-level collecting deserializer (tree-first) is the only
    approach that works through the default converter (Java §5). A
    mapper-level `DeserializationProblemHandler` is out: we can't reach
    the default converter's mapper, and it misses 4 of 6 P11 cases anyway.
    Jackson 3.1's built-in `CollectingProblemHandler` is also out: it
    floors at Jackson 3.1 (SDK default is 2.x; P3/P4), is
    reader-configured + per-call so it never fires under the default
    converter, and collects only structural problems — not P10 constraints.
  - A `JsonMappingException` subclass thrown from a custom
    `JsonDeserializer` propagates verbatim through the Temporal
    `DefaultDataConverter` as the cause of `DataConverterException`,
    carrying its `List<Violation>` intact via `getCause()`.
  - The default Temporal Java converter owns a stock `new ObjectMapper()`
    we cannot configure — so all (de)serialize behavior must be baked into
    the POJO via class-level `@JsonDeserialize`/`@JsonSerialize`, exactly
    as Python's `to_json` case requires `@model_serializer(mode='wrap')`.
  - Pydantic's `model_fields_set` includes extras and excludes
    default-filled fields — the exact wire-key count for min/maxProperties.
    Summing `model_fields_set` + `__pydantic_extra__` double-counts extras.
  - `JSON.stringify` silently coerces `NaN`/`±Infinity` to `null` — the
    TS serializer must reject non-finite numbers before stringifying.

## Remaining work

### High priority

All former high-priority blockers are resolved. Completed:
- ±(2^53−1) integer cap (see [[type]]); all four PRINCIPLES.md language sections;
  open-by-default typed structs; Java error-aggregation primitive; `$ref`
  spec (`features/ref/spec.md` + `generated-file-layout.md`).

### Feature specs to write (≈50)

Roughly in priority order — start with keywords that gate other
decisions:

**Object structure:**
- ✅ `properties`, ✅ `additionalProperties` (open/closed landed)
- ✅ `required`, ✅ `minProperties`, ✅ `maxProperties`,
  ✅ `dependentRequired`, ✅ `patternProperties` (temporarily unsupported),
  ✅ `propertyNames` (partial)
- Remaining: `unevaluatedProperties` (expect P6-reject),
  `dependentSchemas` (expect P6-reject)

**Any-type assertions:**
- `enum` — remaining. Representation pre-decided (with const):
  per-language open form — TS `… | (string & {})`, Go alias,
  Python `Literal[…] | str`, Java a generated value class (static known
  constants + private ctor + `@JsonCreator fromString` + `@JsonValue` +
  `isUnrecognized()`). `enum` preserves unknown values (P13) where `const`
  rejects them. Collision handling pre-settled (P15): two enum values
  mapping to the same identifier → load reject; the value-class name is
  package-scoped like const's; the class-body collision pass is the
  [[properties]] policy applied per value class.
- ✅ `const` — landed (scalar). Pure assertion, validated both
  directions; presence owned by `required`. Open-form emit per target
  (P13.1). Mutually exclusive with `default`/`enum`; composite consts
  deferred.

**Numeric assertions** (gated by integer-cap decision):
- ✅ `maximum`, ✅ `minimum`, ✅ `exclusiveMaximum`,
  ✅ `exclusiveMinimum`, ✅ `multipleOf` — landed. See "Features" below.

**Array structure:**
- `items`, `prefixItems`, `contains`, `unevaluatedItems`,
  `minItems`, `maxItems`, `uniqueItems`, `maxContains`, `minContains`

**String assertions:**
- `maxLength`, `minLength`, `pattern`

**Applicators (mostly P6-rejected, each needs a spec'd rejection):**
- `allOf`, `anyOf`, `not`, `if-then-else` — reject per P6; document
  rationale and rewrite hints.
  - **allOf constraint-tightening note (revisit when allOf is
    considered):** the numeric specs reject a same-axis bound pair on a
    *single* node — `maximum`+`exclusiveMaximum`, `minimum`+
    `exclusiveMinimum` — as redundant (one always dominates; see
    [[maximum]]). allOf semantics is the opposite — *intersect / tighten*
    across subschemas — so two same-axis bounds arriving from *different*
    allOf branches (e.g. `allOf:[{maximum:10},{exclusiveMaximum:8}]`) are
    legitimate and must be **merged to the tighter**, not rejected. Any
    future allOf lowering / merge-flatten step must apply the tighten rule
    there instead of the single-node redundancy reject. Same care for
    combining a bound with a tighter bound of the same kind (two
    `maximum`s → keep the smaller).
- `oneOf` — partial: nullability pattern accepted (see [[nullability]]);
  formalizing the nullability-only `oneOf` rule and the
  discriminator-bearing form is the next open question (P7 implies
  support for the discriminator form, but no convention spec'd yet).

**Core / structural:**
- ✅ `$ref`, ✅ `$defs` — landed (`features/ref/spec.md` +
  `generated-file-layout.md`). Named-targets-only, local-file-only, no
  siblings, no `$id`; single flat package per language; cyclic types hoist
  (not merge; P14); unsatisfiable-cycle reject.
- Remaining: `$schema`, `$id` (the document-level rules are already
  settled — `$schema` 2020-12-only/assumed in [[input-files]]; `$id`
  reject in [[ref]] + restated in [[input-files]] — so these keyword specs
  should defer to those rather than restate),
  `$anchor`, `$dynamicRef`, `$dynamicAnchor`, `$vocabulary`, `$comment`.

**Metadata / annotations:**
- `format` — high priority, codegen-relevant (e.g. `date-time` →
  `time.Time` in Go).
- ✅ `default` — landed. Off-the-wire semantics (annotation, set-ness tracking,
  omit-unset, materialize-on-read). Native in Python/Java, advisory in
  TS, `<Field>OrDefault()` accessor in Go.
- `title`, `description`, `examples`, `deprecated`,
  `readOnly`, `writeOnly`, `contentEncoding`, `contentMediaType`,
  `contentSchema` — lower priority; mostly pure metadata.

## Open question inventory

### `features/type/spec.md`
1. **Cross-language conformance suite** for integer runtime helpers.

### `features/properties/spec.md`
1. **Python serialize keep-set name↔alias mapping** — the
   `@model_serializer` keep-set (PRINCIPLES Python §6) filters Python
   field names against serialized keys; an `x-py-name`/case-mapped JSON
   alias means the keep-set must map name↔alias.

### `features/patternProperties/spec.md`
1. Possible future single-pattern typed-map carve-out (deferred).

### `features/propertyNames/spec.md`
1. Static enforcement of `propertyNames` alongside `properties`
   (currently rejected; deferred).

### `features/const/spec.md`
1. Composite (object/array) const — temporarily unsupported; would need
   a deep structural-equality check. Deferred.
2. Validating the const value against constraint keywords (`pattern`,
   `minLength`, `minimum`, `multipleOf`, …) at load time — deferred to
   land with those constraint features.

### `input-files.md`
1. **Standalone `$schema` / `$id` keyword specs defer here** — the
   dialect rule (2020-12 only / assumed-when-absent) and the `$id` reject
   are settled in this doc + [[ref]]; those keyword specs should point
   here rather than restate.
2. **Widening the `nexusrpc` version range** — v1 pins `"1.0.0"`; the
   reject path is forward-compatible but the acceptance policy for
   `>1.0.0` is deferred (P13.2).

### `features/multipleOf/spec.md`
1. **Fractional-divisor carve-out** — `multipleOf: 0.1`/`2.5` is rejected
   (deferred). A future decimal-scaling lowering could support
   fixed-precision fractional divisors if all four targets agree; revisit
   on demand.

### `features/maximum/spec.md` (+ minimum / exclusive pair)
- Zero open questions. (Flooring a fractional bound on an integer field —
  rather than rejecting — was considered and rejected: Pydantic can't
  represent it and silent flooring violates "reject ambiguity loudly".)

### `features/services/spec.md`
1. **Explicit-vs-default wire name (Python/Java)** — generator always
   emits `name=` for P1 clarity; could omit when `fqn` equals the SDK
   default. Deferred.
2. **Async/handler-shape metadata** — generated binding is a typed
   contract only; sync/async, headers, links are implementation-time and
   not surfaced. Flagged for future I/O cardinality/streaming concepts.

### `features/default/spec.md`
1. Composite (object/array) defaults — deferred, expected to relax. v1
   is scalar-only; lifting needs a spec for materializing a literal
   object/array default into a constructed language value on read and
   folding it into the omit-unset machinery. Tracks with [[const]]'s
   composite-const carve-out (same materialization problem).
2. Validating the default value against constraint keywords at load time
   — deferred to land with those constraint features.

### `features/ref/spec.md`
1. **Sibling annotation passthrough** — currently all `$ref` siblings
   are rejected, including pure annotations (`description`/`title`/
   `deprecated`). A future relaxation could allow annotation-only
   siblings. Deferred.
2. **Pointer into a non-`$defs` subschema** — currently rejected (must
   extract to `$defs`); could relax via anonymous-name-synthesis.
   Deferred pending demand.

### Cross-cutting
1. **Literal-value-against-constraint validation at load time** (partially
   closed). A `const`, `default`, or `enum` value must satisfy every
   sibling assertion on the same node. `type`-compatibility and now the
   **numeric constraints** (`minimum`/`maximum`/`exclusive*`/`multipleOf`)
   are enforced at load — the numeric bound/multipleOf specs each require
   the supplied literal to satisfy them. **Still deferred:** the string
   assertions (`pattern`, `minLength`/`maxLength`), to land with those
   specs.

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
