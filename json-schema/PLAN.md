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

- `PRINCIPLES.md`: P1–P18 with sub-principles. **P18 added:
  synthesized-identifier collisions reject at load time (no mangling),
  sharing one per-scope namespace with declared names — settles the
  const/default naming-collision TODOs.** **All four language
  sections complete** — Go (6), TypeScript (6), Python (6), Java (6).
  P16: uniform `±(2^53−1)` integer cap. **P17 added: serialize-side
  validation** — both directions share one `Validate(model)` over the
  decoded model, flanked by a deserialize-only parse adapter and a
  serialize-only encode adapter; no IR round-trip. **P11 amended**:
  `default` is materialized on *read* (set-ness tracked, omit-unset on
  serialize, no deep-equals), not stored on deserialize. Each language
  section gained a §6 serialize note. **Java §5 (error-aggregation
  primitive) RESOLVED (2026-06-23)** — a per-POJO class-level collecting
  `@JsonDeserialize`/`@JsonSerialize` (two-stage lenient-tree-then-
  validate, the Jackson analog of Go's shadow-layout `UnmarshalJSON`),
  throwing one `ValidationException extends JsonMappingException` with
  `List<Violation{path,reason}>`. Proven end-to-end through the *default*
  Temporal data converter (`/tmp/javaagg/`). Closes the last
  language-section gap **and** the serialize-direction dependency
  (§6 rides the same primitive).
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
  shape stability + clean declared/extra key separation. **OQ1 (Java
  closed-struct aggregation) RESOLVED** — the per-POJO collecting
  deserializer (Java §5) reads the tree first, so it flags each
  undeclared key as a `Violation` in the same single-shot pass. 1 open
  question — Go catch-all field naming/exposure.
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
- `features/const/spec.md`: complete — **supported (scalar)**. Emits the
  underlying primitive (P9.1, not a *closed* literal) via an open form
  per target, closed only at runtime — TS `'v' | (string & {})` +
  `readonly`, Go alias `type X = string` + typed value const, Python
  `Literal['v'] | str`, Java a generated **value class**
  (`@JsonCreator`/`@JsonValue`, `isUnrecognized()`) shared with the
  future [[enum]] feature (const = single-value specialization) — plus a
  runtime equality check. **No serialize-side special-casing (corrected):** const
  is a pure single-value `enum` assertion validated in both directions;
  presence is owned by `required`, and the value reaches the wire because
  it is *set in memory* — TS by type, Java by a `final` field initialized
  to the known constant (getter only, no setter, no builders), Python by
  a `before`-validator inject
  (which lands it in `model_fields_set`, so the generic
  `@model_serializer` emits it with **no** `const_fields` keep-set —
  verified `/tmp/const_fields_set_probe.py`), Go by the consumer via a
  named alias `type X = string` + typed value const (`UserEventKind` /
  `UserEventKindUser`). Dropping force-write also stops the serializer
  silently rewriting a wrong in-memory value (now a loud `Validate`
  failure). Mutually exclusive with `default` and `enum`. `const:null`
  rejected (degenerate, like standalone `type:null`); composite
  (object/array) const **temporarily unsupported** (deep-equals cost).
  1 open question — composite-const carve-out.
- `features/default/spec.md`: complete — **supported** with the amended
  P11 semantics: annotation (no validator, never fails validation);
  off-the-wire; set-ness tracked; omit-unset with **no deep-equals**;
  materialized on read. Strengthens the spec's "RECOMMENDED valid
  default" to a load-time **MUST** (P10.1); rejects `default` on a
  required member and `default`+`const`. Read-side surfacing is **native**
  in Python (Pydantic field default) and Java (getter), **advisory** in
  TS (`?? DEFAULT_X` constant), and a generated **`<Field>OrDefault()`
  accessor** in Go (Option C, proto3 `GetX()`-style — sealed).
  **Scalar-only in v1** (`string`/`number`/`integer`/`boolean`):
  object/array defaults rejected as "not yet supported," `default:null`
  rejected as degenerate — provisional, expected to relax. 1 open
  question — composite-default materialization.

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
  `/tmp/pyd_null_serialize_probe.py`). **Python encode adapter is not a
  call site we own** — the default Temporal `pydantic_data_converter`
  serializes via plain `pydantic_core.to_json` — so the omit/const/guard
  logic is baked into a generated `@model_serializer(mode='wrap')`, which
  `to_json` honors (verified `/tmp/temporal_pydantic_probe.py`).
- **`default` materialized on read, not stored (P11 amended).** Track
  set-ness; serialize omits unset fields with **no deep-equals** against
  the default; surface the default on read. **Why omit rather than
  populate-on-deserialize:** preserving absent-vs-set (P12) protects
  forward-compat (P9), live default evolution, and proxy/intermediary
  fidelity — exactly proto3's omit-on-wire model (which re-added
  `optional` after losing presence). Explicit-set pins.
  *Serialize/omit mechanisms:* Go `,omitempty`+pointer, Pydantic a
  generated `@model_serializer` over `model_fields_set` (the default
  Temporal converter owns `to_json`, so omission is baked into the model,
  not a `model_dump` call we make), TS `undefined`, Java
  `@JsonInclude(NON_NULL)`. *Read-side materialize mechanisms:* Java
  getter / Pydantic field default (native), Go generated
  **`<Field>OrDefault()` accessor** (proto3 `GetX()`-style, Option C),
  TS `?? DEFAULT_X` + emitted constant.
- **`const` is a pure assertion — no serialize-side special-casing
  (corrected from an earlier auto-emit design).** const is treated as a
  single-value `enum`: validate `== value` in both directions; the
  generator does **not** force-write. Presence is owned by `required`
  (a required+const is always present because it is required, not because
  const says so), and the fixed value reaches the wire because it is *set
  in memory*. The earlier "auto-populate + always-emit" rule was solving
  a presence problem `required` already owns, and force-write had the
  downside of **silently rewriting** a wrong in-memory value instead of
  failing loudly. **Landed in [[const]]:** emits the underlying primitive
  not a *closed* literal (P9.1; TS adds the `'v' | (string & {})` hint +
  `readonly`, Go a named alias `type X = string` + typed value const,
  Python the open union `Literal['v'] | str`, Java a generated value
  class shared with [[enum]]);
  Go sets it via that value const (zero value fails `Validate` loudly);
  Java uses a `final` field initialized to the known constant + getter,
  no setter, no builders; Python injects it in a `model_validator(mode='before')`
  (which marks it set → `model_fields_set` → emitted by the generic
  `@model_serializer`, **no** `const_fields` keep-set —
  `/tmp/const_fields_set_probe.py`); mutually exclusive with
  `default`/`enum`; `const:null` and composite consts rejected/deferred.
- **Synthesized-identifier collisions reject at load time, never mangle
  (P18).** The generator emits names that aren't in the schema — [[const]]
  type aliases + value consts, the future [[enum]] value class/members,
  the Go `<Field>OrDefault()` accessor and TS `DEFAULT_<FIELD>` / const
  `<FIELD>_CONST` ([[default]]). These share **one per-scope namespace**
  with declared types/members and with each other (package/module scope;
  the Go accessor sits in the struct method-set, where a field/method
  clash is a **hard compile error** — verified `/tmp/collide_probe`). A
  single collision pass (after case-mapping) **rejects loudly** on any
  coincidence; auto-mangling (`EventKind2`) is rejected as unstable under
  schema evolution (P9) and exactly the silently-wrong output the mission
  forbids (P10). The escape hatch is the existing [[properties]]
  `x-*-name` override on the declaring member — re-mapping it moves every
  synthesized name with it. Settles the two const/default naming-collision
  TODOs. Landed in PRINCIPLES P18 + [[properties]] collision policy
  (widened from single-object to per-scope) + [[const]]/[[default]] specs.
- **Optional+nullable round-trip is capability-tiered:** faithful in
  TS *and Python* (`undefined` / `model_fields_set` via the generated
  `@model_serializer`),
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
  `_parse_spec_integer`, Java node helper `SpecNumbers.specLong`).
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
- **Java error aggregation is a per-POJO collecting (de)serializer
  (Java §4–§6, RESOLVED 2026-06-23).** Each emitted POJO carries
  class-level `@JsonDeserialize(using=<Pojo>.Deserializer.class)` +
  `@JsonSerialize(using=<Pojo>.Serializer.class)` — the Jackson analog of
  Go's shadow-layout `UnmarshalJSON`. **The two (de)serializers are
  emitted as `public static final` nested classes on the model
  (`User.Deserializer` / `User.Serializer`), not top-level
  `UserDeserializer` types** — each model owns its pair, so the names
  never collide across models, and they sit with the type they serve
  (same nesting idiom as P18's const/enum value classes; verified
  `/tmp/javaagg/`). The deserializer does a **two-stage
  lenient-tree-then-validate** bind (`readValueAsTree()` defeats
  Jackson's fail-fast `MismatchedInputException`, then every field runs
  through shared spec-strict + constraint helpers, collecting
  `Violation{path,reason}`) and throws one `ValidationException extends
  JsonMappingException`. **The §4 spec-strict parse is a node helper
  (`SpecNumbers.specLong(JsonNode,…)`) called by the collecting
  deserializer — *not* a per-field `@JsonDeserialize` (fail-fast, can't
  aggregate), and there is no `…StrictDeserializer` sibling: the
  explicit-`null` decision is a per-field branch over `node.isNull()`,
  the same three-way Go makes (Option A, chosen over driving a retained
  `JsonDeserializer` through a sub-parser — `/tmp/javaagg/SpecCmp.java`).**
  Crucially this works through the
  **default** Temporal data converter (which owns a stock
  `new ObjectMapper()` we can't configure — so a mapper-level
  `DeserializationProblemHandler` is out): the hook is baked into the
  POJO via annotations, the converter honors it, and the aggregated
  `ValidationException` surfaces as the **cause** of `DataConverterException`
  (handler walks the chain → `getViolations()` → one BAD_REQUEST). Exact
  parallel of the Python `to_json`/`@model_serializer` finding. Serialize
  side (§6) rides the same primitive. Empirically proven end-to-end in
  `/tmp/javaagg/`. Also closes additionalProperties OQ1 (closed-struct
  extra-key aggregation falls out of the tree stage).

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
    `ACCEPT_FLOAT_AS_INT=false`, a custom token-based `SpecLongDeserializer`
    (**superseded** — the strict parse is now a node helper
    `SpecNumbers.specLong`, see `/tmp/javaagg/SpecCmp.java`)
  - `/tmp/javaagg/` — **Java error-aggregation primitive** (Java §5/§6)
    proven end-to-end through `DefaultDataConverter.STANDARD_INSTANCE`
    (temporal-sdk 1.30.1): a per-POJO class-level `@JsonDeserialize`
    collecting deserializer — emitted as a **nested `User.Deserializer`**
    (and `User.Serializer`), referenced via
    `@JsonDeserialize(using = User.Deserializer.class)`, which Jackson
    resolves through the default converter; two models in the probe
    (`User`, `OpenUser`) each own their pair with no collision —
    (two-stage `readValueAsTree()` then
    per-field shared validators, one `ValidationException extends
    JsonMappingException` with `List<Violation{path,reason}>`) aggregates
    3 independent errors in one shot; `1.0` accepted / `1.5` rejected;
    the exception survives as the **cause** of `DataConverterException`
    (recoverable via the cause chain). `@JsonSerialize` mirror does
    validate-then-write (§6) and omits an optional `null`. Confirms the
    mechanism rides the **default** converter's stock ObjectMapper with
    no mapper config — the Jackson parallel to the Python `to_json`/
    `@model_serializer` finding. Also proves the **open-struct extras**
    path: the collecting (de)serializer routes undeclared tree keys into a
    `Map<String,Object>` catch-all and round-trips them faithfully (nested
    arrays/objects included) — **without** `@JsonAnySetter`/`@JsonAnyGetter`
    (a class-level custom (de)serializer bypasses those), so [[additionalProperties]]
    Java now rides the same primitive.
  - `/tmp/javaagg/SpecCmp.java` — settles the spec-strict integer primitive
    shape (type/spec.md OQ): a **node helper** over `JsonNode` (Option A,
    chosen) vs a retained `JsonDeserializer<Long>` driven over
    `node.traverse()` (Option B). Both make **identical** accept/reject
    decisions (`1`/`1.0`/`1e2` ok; `1.5`/`>2^53`/`"1"`/`true` rejected);
    Option A wins — no per-field throw/catch, zero sub-parser allocation,
    full `{path,reason}` control, and it matches Go/Python helpers. Landed
    in PRINCIPLES Java §4 + [[type]] + [[nullability]] Java.
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
    deserialize-only; default omit-unset (const adds nothing); round-trip
    byte-identical (no default echo) — proves the P17 decomposition
  - `/tmp/oe/` — Go `omitempty` quadrant: nil omits / ptr-to-`""` emits;
    no-`omitempty` `*T` nil → `null`; type-alias `MarshalJSON` honors
    tags without recursion (the declarative omit-vs-`null` encode layer)
  - `/tmp/pyd_serialize_probe.py` — Pydantic `exclude_unset` omits while
    the attr still reads the default (no deep-equals); explicit-set pins;
    const+`exclude_unset` would wrongly drop a *defaulted* discriminator;
    `model_construct` bypass caught by re-validation. (The "const must
    force-emit" conclusion is **superseded** — see
    `/tmp/const_fields_set_probe.py`: a `before`-inject marks the field
    set, so it emits with no force-keep and no Pydantic default.)
  - `/tmp/const_fields_set_probe.py` — proves the **validate-only** const
    design: a key injected by a `model_validator(mode='before')` lands in
    `model_fields_set`, so const emits under the generic
    `@model_serializer` (keep-set = `model_fields_set`, **no**
    `const_fields`) and under plain `to_json` (the default Temporal path);
    a wrong value is rejected; a genuinely-absent optional stays omitted.
  - `/tmp/const_open_enum_probe.py` — the Python **open-enum hint** for
    const: field typed `Union[Literal['user'], str]`. Pydantic preserves
    the union, auto-fill+emit work, a wrong value is rejected *by our
    validator*; crucially, without the validator Pydantic accepts an
    arbitrary `'user_v2'` through the `str` arm — proving the type is open
    (P9.1) and the runtime check is what closes it (the Python parallel to
    TS `string & {}` / the Go alias).
  - `/tmp/pyd_null_serialize_probe.py` — per-field omit-vs-emit-`null`:
    required+nullable emits `null`; `model_fields_set` distinguishes
    wire-`null` from wire-absent (Python optional+nullable is faithful);
    optional-non-nullable explicit `null` rejected in strict mode
  - `/tmp/collide_probe/` — Go **field/method name collision** is a hard
    compile error (`field and method with the same name NicknameOrDefault`),
    proving the `<Field>OrDefault` accessor can't silently shadow a
    declared member — grounds the P18 reject-at-load decision.
  - `/tmp/javacollide/` — Java value-class (const/[[enum]]) collisions:
    (A) two static members with the same name → compile error (the
    enum-specific member-vs-member surface; const can't hit it with one
    member); (B) UPPER_SNAKE member `VALUE` coexists with lowerCamel
    scaffolding (`value`/`getValue`) — no collision; (C) a same-name
    field+method is legal in Java (unlike Go). Conclusion: the value
    class's class-body pass need only police member-vs-member, = the
    [[properties]] case-mapping collision policy applied per value class.
  - `/tmp/nestprobe/` — **nesting** synthesized const/enum types to shrink
    the collision surface: Java `public static final class Kind` nested in
    `UserEvent` round-trips via Jackson **and coexists with an independent
    top-level `UserEventKind`** (the win); Python `Kind: ClassVar =
    Union[…]` resolves via `model_rebuild()` and round-trips; Go nested
    `type` decl is a **syntax error** (so Go stays flat package-level +
    P18 backstop). Grounds the "nest where supported, Go flat" decision.
  - `/tmp/temporal_pydantic_probe.py` — **compatibility with the default
    Temporal `pydantic_data_converter`** (SDK 1.29, Pydantic 2.13). The
    converter owns serialization via plain `pydantic_core.to_json`
    (`exclude_unset=False`, no validation), so `model_dump(exclude_unset=
    True)` is never ours to call. Proves `to_json` **honors a generated
    `@model_serializer(mode='wrap')`**: keep-set `model_fields_set`
    (the `∪ const_fields` union was later **dropped** — const rides the
    normal keep-set via a `before`-inject, see
    `/tmp/const_fields_set_probe.py`) reproduces omit-unset, explicit-set
    pinning (no deep-equals), faithful optional+nullable, and nested
    recursion; an in-serializer `validate_python` catches the
    `model_construct`/mutation bypass without recursion; deserialize via
    `TypeAdapter.validate_json` runs our validators. The
    `ToJsonOptions(exclude_unset=True)` converter knob is the wrong lever
    (non-default, global, and drops const discriminators).
- **Decisions cite principles by P-number.** Every Support decision
  in a feature spec must reference the P-number(s) it's grounded in.
- **Surprises worth noting for future Python/Java/TS work:**
  - Pydantic's `_FIELD: ClassVar[T]` is required — bare `_FIELD`
    becomes a private model attr.
  - Pydantic's `model_validator(mode='before')` that raises
    short-circuits Pydantic's own field validation; use `mode='wrap'`
    if you want P8 aggregation across both error sources.
  - A key **injected** into the input dict by a
    `model_validator(mode='before')` lands in `model_fields_set` — Pydantic
    treats it as provided. This is what lets `const` auto-fill *and* emit
    through the generic omit-unset serializer with no special keep-set
    (`/tmp/const_fields_set_probe.py`).
  - Jackson's default `Long` deserializer **silently truncates**
    `1.5` to `1`. Custom deserializer is mandatory, not optional.
  - Jackson is **fail-fast**: the first field's `MismatchedInputException`
    aborts the whole bind, so per-field `@JsonDeserialize` annotations
    **cannot** aggregate (P8). Aggregation needs either a mapper-level
    `DeserializationProblemHandler` (`mapper.addHandler`, Jackson 2) or a
    **class-level collecting deserializer that reads the whole object into
    a tree first**, then validates field-by-field. The handler is out on
    two counts: we can't reach the default converter's mapper to add it
    (and no annotation binds it to a type), **and** it wouldn't suffice if
    we could — it sees only Jackson's structural binding events and needs a
    fabricated fallback to continue, so 4 of 6 P8 cases fire no hook
    (`1.5`→`1` silent, cap is a valid `long`, missing-required is a
    non-event), it's mapper-global, and deserialize-only. Verified
    `/tmp/javaagg/HandlerProbe.java`. The tree route is the only one that
    works through the default converter (Java §5).
  - **Jackson 3.1's built-in problem collection**
    (`CollectingProblemHandler` / `readValueCollectingProblems()` →
    `DeferredBindingException`, shipped 2026-02-23) does **not** rescue us:
    it's reader-configured + per-call (so it never fires under the default
    converter — it's a `DeserializationProblemHandler`, the unavailable
    lever), it floors at Jackson 3.1 (SDK default is Jackson 2.x; P3/P4),
    and it collects only *structural* problems — not P7 constraints (misses
    `1.5`→`1`, the cap, `minLength`/`const`/…) — and is deserialize-only.
    Recorded as the rejected alternative in PRINCIPLES Java §5.
  - A `JsonMappingException` (or subclass) thrown from inside a custom
    `JsonDeserializer` **propagates verbatim** — Jackson does not re-wrap
    it — and the Temporal `DefaultDataConverter` surfaces it as the
    **cause** of a `DataConverterException`. So a custom
    `ValidationException extends JsonMappingException` reaches the Nexus
    handler intact via `getCause()`, carrying its `List<Violation>`.
    Verified `/tmp/javaagg/`.
  - The default Temporal Java converter
    (`JacksonJsonPayloadConverter.newDefaultObjectMapper()`) is a stock
    `new ObjectMapper()` + JavaTimeModule + Jdk8Module + field-visibility
    ANY. We do **not** own it — so, exactly like the Python `to_json`
    case, any (de)serialize behavior must be **baked into the POJO** via
    class-level `@JsonDeserialize`/`@JsonSerialize`, never expressed as
    mapper configuration we apply.
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
  - The default Temporal `pydantic_data_converter` **owns the serialize
    call** (plain `pydantic_core.to_json`, `exclude_unset=False`, no
    validation) — so any serialize behavior we want (omit-unset, const
    force-emit, the `model_construct`/mutation guard) must be **baked
    into the model** via `@model_serializer(mode='wrap')`, which `to_json`
    honors, NOT expressed as a `model_dump(...)` call we make. This is
    strictly more robust: the contract travels with the model under any
    caller. Caveat: the keep-set filter matches Python field names; JSON
    aliases (future case-mapping) will need a name↔alias map. Verified
    `/tmp/temporal_pydantic_probe.py`.

## Remaining work

### High priority

All three former high-priority items are **resolved** (2026-06-16):

1. ~~Resolve `type/spec.md` OQ1 (large integer cap).~~ **DONE** —
   ±(2^53−1) uniform cap (P16). TS enforces via `Number.isSafeInteger`,
   no third-party parser needed (empirically verified).
2. ~~Fill PRINCIPLES.md → TypeScript, Python, Java sections.~~ **DONE** —
   all three sections written; the last gap (Java §5 aggregation
   mechanism) is now **RESOLVED (2026-06-23)** — see Java §4–§6.
3. ~~Decide + land closed-vs-open default for typed structs.~~ **DONE** —
   open-by-default; landed in `properties/spec.md` +
   `additionalProperties/spec.md`.

Next-highest leverage (newly surfaced TBDs that gate clusters of specs):
- ~~**Java error-aggregation primitive** (PRINCIPLES Java §5).~~
  **RESOLVED (2026-06-23)** — per-POJO collecting `@JsonDeserialize`/
  `@JsonSerialize`, two-stage lenient-tree-then-validate, proven through
  the default Temporal data converter (`/tmp/javaagg/`). Unblocked the
  closed-struct + multi-field-error story across every Java spec.
- **`$ref`** — now the single highest-priority structural keyword
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
- `enum` — remaining. **Representation decided ahead of time** (with
  const): per-language open form — TS `… | (string & {})`, Go alias,
  Python `Literal[…] | str`, **Java a generated value class** (static
  known constants + private ctor + `@JsonCreator fromString` that
  gracefully captures unknowns + `@JsonValue` + `isUnrecognized()`).
  const shares this and is its single-value specialization; enum
  **preserves** unknown values (P9) where const rejects them.
  **Collision handling pre-settled (P18):** enum is the first feature to
  synthesize *multiple* members into one Java value class / Go alias /
  Python union, so it is the first to exercise the **member-vs-member**
  surface (two values case-mapping to the same identifier → load reject;
  Java verified `/tmp/javacollide` Case A). The value-class **name** is
  package-scoped like const's; the class-body pass is the [[properties]]
  collision policy applied per value class. Scaffolding members don't
  constrain member names (Cases B/C). No new naming policy needed.
- ✅ `const` — landed (scalar). Pure assertion (single-value `enum`),
  validated both directions, **no serialize-side special-casing** —
  presence owned by `required`, value set in memory (not force-written).
  Emits the underlying primitive via an open form (P9.1; TS
  `'v' | (string & {})` + `readonly`, Go named alias `type X = string` +
  typed value const, Python `Literal['v'] | str`, Java a value class
  shared with `enum`); mutually exclusive with `default`/`enum`;
  `const:null` + composite consts rejected/deferred.

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
- ✅ `default` — landed. Encodes the amended P11 (annotation, no
  validator; set-ness tracking; omit-unset with no deep-equals;
  materialize-on-read). Read-side native in Python/Java, advisory in TS,
  and a generated `<Field>OrDefault()` accessor in Go (Option C, sealed).
- `title`, `description`, `examples`, `deprecated`,
  `readOnly`, `writeOnly`, `contentEncoding`, `contentMediaType`,
  `contentSchema` — lower priority; mostly pure metadata.

## Open question inventory

Snapshot as of this checkpoint.

### `features/type/spec.md`
1. ~~Large integers — cross-language cap.~~ **Resolved: ±(2^53−1).**
2. **Cross-language conformance suite** for integer runtime helpers.

### `features/properties/spec.md`
1. ~~**Identifier case-mapping policy.**~~ **RESOLVED** — one shared
   4-stage JSON-name → idiomatic-identifier algorithm + `x-*-name`
   escape hatch + collision policy (spec "Resolved questions"). The
   collision pass **widened under P18** from a single object's members to
   the full per-scope namespace, so it also catches synthesized
   const/default/enum names.
   **Remaining dependency:** the Python serialize `@model_serializer`
   keep-set (PRINCIPLES Python §6) filters Python field names against
   serialized keys; an `x-py-name`/case-mapped JSON alias means the
   keep-set must map name↔alias.
2. ~~Synthesized *type-name* derivation rule (anonymous const/enum).~~
   **RESOLVED (2026-06-22):** reuse the `$defs` name when the const/enum
   is a named definition; when anonymous, **nest the synthesized type in
   its enclosing model where the language allows** — Java
   `UserEvent.Kind` (the main beneficiary; only target that can't inline),
   Python a `ClassVar` alias, TS inline (no named type), **Go falls back
   to flat package-level `UserEventKind`** (no nested types) with the P18
   collision pass as backstop. Deliberately trades uniform cross-language
   shape for collision-minimization + idiom. All four verified
   (`/tmp/nestprobe/`: Java nested value class coexists with a top-level
   `UserEventKind`; Python `ClassVar` alias round-trips; Go nested-type
   decl is a syntax error). Landed in [[properties]] Resolved Q2 +
   [[const]] naming section. Surfaced by `UserEvent.kind` ⨯
   top-level `UserEventKind`.

### `features/additionalProperties/spec.md`
1. ~~**Java closed-struct aggregation** (depends on Java agg primitive).~~
   **RESOLVED (2026-06-23)** — the per-POJO collecting deserializer
   (Java §5) reads the tree first, so an `additionalProperties:false`
   struct emits one `Violation` per undeclared key in the same
   single-shot pass; no separate mechanism needed.
2. **Go catch-all field naming/exposure** for open structs.

### `features/patternProperties/spec.md`
1. Possible future single-pattern typed-map carve-out (deferred).

### `features/propertyNames/spec.md`
1. Static enforcement of `propertyNames` alongside `properties`
   (currently rejected; deferred).

### `features/const/spec.md`
1. Composite (object/array) const — temporarily unsupported; would need
   a deep structural-equality check + structural auto-emit. Deferred.
2. Validating the const value against *constraint* keywords (`pattern`,
   `minLength`, `minimum`, `multipleOf`, …) at load time — see the
   cross-cutting entry below (P10.4).
3. ~~Synthesized type-alias / value-const name collisions.~~ **RESOLVED
   (P18)** — share one per-scope namespace with declared + sibling
   synthesized names; collision → load reject, no mangling; escape hatch
   is the [[properties]] `x-*-name` override.

### `features/default/spec.md`
1. **Composite (object/array) defaults — deferred, expected to relax.**
   v1 is scalar-only (object/array defaults rejected, `default:null`
   rejected as degenerate). Lifting needs a spec for materializing a
   literal object/array default into a constructed language value on read
   + folding it into the omit-unset machinery. Tracks with [[const]]'s
   composite-const carve-out (same materialization problem).

   **Go read-side surfacing RESOLVED (Option C):** a generated
  `<Field>OrDefault()` accessor (proto3 `GetX()`-style) materializes the
  default on read while the bare `*T` field keeps set-ness and
  omit-on-serialize stays faithful (P11/P12). Chose the accessor over the
  advisory-constant route (A) because it gives Go the same frictionless
  read as populate-on-deserialize *without* losing the absent-vs-set bit;
  rejected populate-on-deserialize (B) for breaking P12 / live defaults /
  proxy fidelity. Landed in PRINCIPLES P11 + Go §7 and the default spec.
2. Validating the default value against *constraint* keywords at load
   time — see the cross-cutting entry below (P10.4).
3. ~~`<Field>OrDefault` / `DEFAULT_<FIELD>` name collisions.~~ **RESOLVED
   (P18)** — Go accessor vs declared member is a hard compile error
   (`/tmp/collide_probe`); both names join the per-scope collision pass
   → load reject, no mangling; escape hatch is the [[properties]]
   `x-*-name` override. Python/Java synthesize no new name.

### Cross-cutting
1. **Literal-value-against-constraint validation at load time (P10.4) —
   called out, not yet fully specced.** A `const`, `default`, or `enum`
   value baked into a schema must satisfy every sibling assertion on the
   same node. `type`-compatibility (static satisfiability) is enforced
   today; validating against the *constraint* keywords — `pattern`,
   `minLength`/`maxLength`, `minimum`/`maximum`/`exclusive*`,
   `multipleOf`, … — reuses each keyword's own validator over the literal
   at generator time and is **deferred to land with those constraint
   features** (none specced yet). Surfaced in [[const]], [[default]], and
   (future) [[enum]]; recorded as PRINCIPLES P10.4.

### `features/nullability/spec.md`
- None.

### `PRINCIPLES.md`
- ~~Java §5 — error-aggregation primitive mechanism TBD.~~ **RESOLVED
  (2026-06-23)** — per-POJO class-level collecting `@JsonDeserialize`/
  `@JsonSerialize`, two-stage lenient-tree-then-validate, one
  `ValidationException extends JsonMappingException` with
  `List<Violation{path,reason}>`. Works through the default Temporal data
  converter (mechanism baked into the POJO, not the mapper — parallel to
  the Python `to_json` finding); the exception surfaces as the cause of
  `DataConverterException`. Serialize direction (§6) rides the same
  primitive. Proven `/tmp/javaagg/`. **No open language-section gaps
  remain.**

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
