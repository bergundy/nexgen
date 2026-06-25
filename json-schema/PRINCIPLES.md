# Core Design Principles

## Cross-cutting

1. **Hand-written feel over generated feel.** Output should read like something a human wrote idiomatically for that language.
2. **Prefer ergonomics over performance**. Prefer to pay for normalization and conversion over giving a subpar experience for a given language.
3. **Works with default Temporal payload converter setup (contrib libraries okay if necessary (Pydantic))**.
4. **Minimal external runtime dependencies on generated code.** All languages require `nexus-rpc` SDKs for representing service contracts in code or Temporal SDKs for typed client generation.
5. **Strict JSON schema subset.** Generator rejects JSON schema features that cannot be coherently represented in all supported languages. (e.g., `anyOf`, `allOf`).
6. **Use 2020-12 as the base spec version**. The strict subset is based on the latest draft.
7. **Validation is enforced, not advisory.** Constraints (`minLength`, `pattern`, `minimum`, …), `const`, and discriminator strings are checked at the (de)serializer boundary. Schemas are not just documentation.
8. **Aggregate validation errors.** Surface every violation in one shot using the language-native aggregation primitive: TS `AggregateError` of `ValidationError { path, reason }`; Python uses Pydantic's native `pydantic.ValidationError` which already aggregates via `.errors()` (each entry has `loc` + `msg` + `type`); Go `errors.Join` of `ValidationError { Path, Reason }`; Java a per-POJO class-level collecting `JsonDeserializer`/`JsonSerializer` that throws one `ValidationException` (a `JsonMappingException`) carrying `List<Violation { path, reason }>` (Java §5). Structured payloads; never stringly-typed messages. Error set as the cause of a Nexus RPC HandlerError with BAD_REQUEST error type.
9. **Forward compatibility over strict types**. Accept and preserve unknown enum values and unknown fields as best as possible.
    1. **Forward-compatible `const`.** Field type emitted as the underlying primitive (`string`, not `'v1'`); value validated at runtime. Bumping a const value never breaks the type signature. `const` is a pure assertion — equivalent to a single-value `enum`, no serialize-side special-casing (presence is owned by `required`). All four targets emit an **open form, closed only at runtime**, not a closed literal: TS `'v1' | (string & {})`, Go `type X = string` + typed value consts, Python `Literal['v1'] | str`, Java a generated value class (string-valued; `@JsonCreator`/`@JsonValue`, `isUnrecognized()`). `const` and `enum` share this representation — a const is the single-value specialization; they differ only in the validator (const rejects an unrecognized value, enum preserves it per P9). See [[const]].
10. **Strict schema validation**. The *schema* is held to a strict shape: ambiguous constructs are rejected at generator time with clear errors (no `oneOf` discriminator → reject; `additionalProperties: {}` → reject; bare `{type:"object"}` → reject).
    1. **Reject ambiguity loudly at generator time.** Better to error than to guess. Unsupported features get explicit errors, not silent passthrough.
    2. **Distinguish optional from nullable.** Two orthogonal concerns: "key may be absent" (optional, owned by [[required]]) vs "value may be null" (nullable, owned by the [[nullability]] `oneOf` pattern). Because they are orthogonal, **all four combinations are legal** — including *required + nullable* ("must be present, value may be `null`"), which is a well-defined, unambiguous, enforceable contract (presence-check on, null-rejection off). We do **not** reject it: it round-trips losslessly in every language (presence is guaranteed, so in-memory `null`/`nil`/`None` maps unambiguously to wire `null`). The only residual wire-vs-memory collapse is *optional + nullable* in Java/Go/Python, where absent and `null` share one in-memory value; see [[nullability]] round-trip note.
    3. **Nullability is the only `oneOf` shape accepted without a discriminator.** The recognized pattern is `oneOf: [{type: "T"}, {type: "null"}]` — exactly 2 branches, exactly one being `{type: "null"}`, order-insensitive. Any other discriminator-less `oneOf` is rejected per the main P10 rule. See [[nullability]] for details.
    4. **Literal values are validated against their sibling constraints at load time** (cross-cutting; *called out, not yet fully specced*). A `const`, `default`, or `enum` value baked into the schema must itself satisfy every other assertion on the same node — not just `type` (already enforced as static satisfiability) but the *constraint* keywords (`pattern`, `minLength`/`maxLength`, `minimum`/`maximum`/`exclusiveMinimum`/`exclusiveMaximum`, `multipleOf`, …). A value that can never satisfy its field is a schema bug, rejected loudly. The full mechanism — reusing each constraint's own validator over the literal at generator time — is **deferred to land with those constraint features** (none are specced yet); today only `type`-compatibility and shape checks are defined. Tracked in [[PLAN.md]]; surfaced in [[const]] and [[default]].
11. **Default off-the-wire, materialized on read.** `default` values are
    never emitted on serialize **and never stored into the field on
    deserialize**. The generator tracks field *set-ness*: serialize omits
    any unset field — with **no value comparison** (never a deep-equals
    against the default) — and the default is surfaced lazily *on read*:
    via the language-native default (Pydantic field default, Java getter),
    a generated `<Field>OrDefault()` accessor (Go, modeled on proto3's
    `GetX()`), or the `?? DEFAULT_X` idiom + emitted constant (TS). The
    bare field always retains set-ness; the default is never written back
    into it. Explicitly setting a
    field, even to a value equal to the default, marks it set and pins it
    on the wire. This supersedes the earlier "populate the field on
    deserialize" rule, which required a deep-equals to strip on the way
    out and destroyed the absent-vs-set distinction (P12). Mechanisms: Go
    `,omitempty`+pointer; Pydantic a generated `@model_serializer` over
    `model_fields_set` (Python §6 — the default Temporal converter owns
    `to_json`, so omission is baked into the model, not a `model_dump`
    call we make); TS `undefined`; Java `@JsonInclude(NON_NULL)`.
    Empirically verified (`json-schema/research/serialize_probe/`,
    `json-schema/research/temporal_pydantic_probe.py`, `json-schema/research/pyd_serialize_probe.py`).
    See [[default]].
12. **Distinguish absent from zero value**. For example in Go, prefer `string` for representing optional strings.
13. **One file per input; merge recursion, not files.** Each input schema file maps to one generated module in a single flat output package per language (Go's one-directory-per-package rule forces the flatten; all languages flatten for an identical structure). Cross-file reference *cycles* do **not** merge whole files — only the cycle's strongly-connected types hoist into one shared module (Python `_recursive.py`; Go/Java/TS handle cycles natively). No circular-import gymnastics. See [[ref]], [[generated-file-layout]].
14. **External refs are local-file-only.** YAML and JSON files relative to the input. HTTP refs rejected for reproducibility.
15. **CLI and in-process API converge.** The CLI is a thin parser over API.
16. **Uniform integer cap of `±(2^53−1)`.** Every language's `integer`
    runtime helper accepts magnitudes up to `Number.MAX_SAFE_INTEGER`
    (`9007199254740991`) and rejects past it, so all four languages
    agree on the accepted set. This is the only cap TypeScript can
    enforce with a plain `Number.isSafeInteger` check (no third-party
    JSON parser, keeping P4 intact); Go/Java hold ±2^63 natively but
    validate down to the cap; Python's unbounded ints are capped by the
    helper. See [[type]] (resolved question 1) and gates every numeric
    keyword ([[minimum]], [[maximum]], [[multipleOf]], …).
17. **Serialize-side validation; one shared validator, mirror-image
    adapters.** Validation is enforced in *both* directions — P7's
    "(de)serializer boundary" is literal. Every (de)serializer
    decomposes into three layers, and crucially **no intermediate
    representation is round-tripped** to achieve sharing:
    1. **Parse adapter (deserialize-only).** Wire IR → decoded value.
       Owns the checks that only exist on the wire: spec-number parsing
       (`1.0` accepted, `1.5` rejected), explicit-`null` rejection,
       wire-absence → required-presence, type-token classification,
       unknown-key preservation. These cannot live in the shared layer
       because the decoded value no longer carries the wire information
       they inspect.
    2. **Shared `Validate(model)` over the decoded model.** Every
       constraint predicate — the `±(2^53−1)` cap, numeric ranges,
       string `minLength`/`pattern`, `const`/`enum` value checks,
       property counts, nested recursion. Pure functions over
       already-decoded language values, so they are *identical in both
       directions* and called by both. The single source of truth.
    3. **Encode adapter (serialize-only).** Decoded value → wire. Owns
       omit-vs-emit-`null` (per-field, driven by the
       optional/nullable/required declaration — see [[nullability]]) and
       `default` omission (P11). `const` adds **nothing** here — it is a
       pure assertion in the shared `Validate`; the fixed value is set in
       memory and emitted by the normal path (see [[const]]).
    Serialize runs the shared `Validate` **before emitting a byte** and
    fails with the same aggregated error primitive as deserialize (P8).
    In statically-typed languages (Go/TS/Java) in-memory construction is
    unchecked, so serialize-side validation has real teeth; in Python
    strict construction already validates the happy path, so serialize
    only re-validates to catch the `model_construct`/mutation bypasses.
    In Python the encode adapter is **not a call site we own** — the
    default Temporal `pydantic_data_converter` performs serialization via
    plain `pydantic_core.to_json` (**P3**) — so the omit/const/guard
    logic is baked into a generated `@model_serializer(mode='wrap')`,
    which `to_json` honors (Python §6). Empirically verified in Go and
    Python, including against the live default converter
    (`json-schema/research/serialize_probe/`, `json-schema/research/temporal_pydantic_probe.py`,
    `json-schema/research/pyd_serialize_probe.py`, `json-schema/research/pyd_null_serialize_probe.py`).
18. **One identifier namespace per scope; synthesized-name collisions
    reject at load time — never silently mangled.** Beyond the declared
    properties and types, the generator *synthesizes* identifiers:
    [[const]]'s type aliases and value constants, the future [[enum]]'s
    value class and its members, the Go `<Field>OrDefault()` accessor
    (P11), and the TS `DEFAULT_<FIELD>` / `<FIELD>_CONST` constants. These
    do **not** live in a private namespace — each enters the same
    per-scope identifier set as the declared names and as each other:
    package/module scope for package-level types/consts/aliases, the
    struct method-set for the Go accessor (which Go **forbids** from
    coinciding with a field — a hard compile error, verified
    `json-schema/research/collide_probe`), the value-class body for enum/const members.
    The generator runs **one collision pass** over that union (after the
    identifier case-mapping is applied) and any two would-be identifiers
    that coincide are a **load-time reject** with a fix-it diagnostic. We
    **never auto-mangle** (numeric suffixes, etc.): a synthesized
    `NicknameOrDefault2` would be unstable under schema evolution — adding
    a field could renumber it, a P9 break — and is exactly the
    "silently-incorrect output" the mission rejects (P10/P10.1). The
    **escape hatch** — the per-language `x-go-name` / `x-ts-name` /
    `x-py-name` / `x-java-name` override resolved in the [[properties]]
    identifier case-mapping policy — re-maps the *declaring* property, and
    every name synthesized from it moves with it. That policy's collision
    pass, scoped to a single object's members today, **widens under P18**
    to the full per-scope set (declared types/members **plus** synthesized
    aliases/consts/accessors). One algorithm and one override set govern
    declared and synthesized names alike. Surfaced in [[const]],
    [[default]], and (future) [[enum]].

## TypeScript

1. **Hand-emitted validators, no runtime schema library (P4).** The
   generated runtime ships only plain `typeof`/`Array.isArray`/
   `Number.isSafeInteger` checks — no `zod`/`ajv`/`lossless-json`
   dependency. The `±(2^53−1)` cap (P16) is what makes this possible.

2. **`integer` and `number` both map to `number`.** Integer-ness is a
   validator concern (`Number.isSafeInteger(v)`), not a type-level one.

3. **Optionality via the `?` field modifier; `undefined` is the absence
   channel.** Optional fields are `x?: T`; absent reads as `undefined`.
   `null` is never used to mean "absent" — it is reserved for the
   nullability convention, giving a natural three-state
   (`undefined` / `null` / `T`) that the validator enforces at read
   time. See [[nullability]].

4. **Objects emit `interface`s, not classes.** Models are structural
   types; (de)serialization and validation are free functions over
   plain objects, not methods. Keeps output tree-shakeable and
   hand-written-feeling (P1).

5. **Aggregate via `AggregateError` of `ValidationError { path, reason }`
   (P8).** Collect every violation into an array, throw one
   `AggregateError` at the end. Structured `path`/`reason`, never
   stringly-typed.

6. **`serialize(x)` free function: validate-then-stringify (P17).** A
   free `serializeX(x: X): string` runs the same hand-emitted validators
   the deserializer uses (collecting into `AggregateError`, P8) then
   `JSON.stringify`s. Free function, not a method — consistent with
   interfaces-not-classes (§4). Optional fields omit on `undefined`; the
   natural three-state (`undefined` / `null` / `T`) lets TS round-trip
   optional+nullable **faithfully** — omit `undefined`, emit `null`
   (see [[nullability]]).

## Python

1. **Pydantic v2 in strict mode, globally.** Every generated model is a
   `pydantic.BaseModel` configured strict. Strict mode is the only mode
   we use; it rejects lax coercions (`"1"`→`1`, `1`→`True`) that would
   otherwise violate P7/P10.

2. **`Annotated` types for spec-compliant primitives.** Where strict
   mode alone doesn't match the spec, wrap the primitive in an
   `Annotated[...]` alias from the generated runtime — e.g.
   `SpecInt = Annotated[int, BeforeValidator(_parse_spec_integer)]`
   accepts `1.0`/`1e2`, rejects `1.5`/`True`, and enforces the P16 cap.
   User-facing field type stays the plain primitive (`int`).

3. **Optionality via `Optional[T] = None`.** Optional fields default to
   `None`; absence and explicit `null` both surface as `None` (the
   language can't distinguish them post-validation — acceptable per
   P10.2). See [[nullability]].

4. **Aggregate via Pydantic's native `pydantic.ValidationError` (P8).**
   It already collects every field error (`.errors()` → `loc`/`msg`/
   `type`). For violations Pydantic can't see on its own (e.g.
   optional-non-nullable explicit `null`), use a
   `model_validator(mode='wrap')` that runs the inner handler, catches
   its `ValidationError`, and merges pre-errors + field errors into one
   `ValidationError` — preserving single-shot aggregation. `mode='wrap'`
   not `mode='before'`: a raising `before` validator short-circuits
   field validation and breaks aggregation.

5. **`ClassVar[frozenset]` for compile-time field-set constants.** When
   a model-level validator needs a fixed set of field names (e.g. the
   optional-non-nullable set), declare it as
   `_NAME: ClassVar[frozenset] = frozenset({...})`. The `ClassVar`
   annotation is **required** — a bare `_NAME` becomes a private model
   attribute Pydantic won't expose to the validator.

6. **Serialize via a generated `@model_serializer(mode='wrap')` (P17).**
   The default Temporal `pydantic_data_converter` **owns** the serialize
   call — it does a plain `pydantic_core.to_json(value)`
   (`exclude_unset=False`, no validation; verified
   `json-schema/research/temporal_pydantic_probe.py`, SDK 1.29 / Pydantic 2.13). So we
   **cannot** depend on calling `model_dump(exclude_unset=True)`
   ourselves (P3 — work with the default converter) and instead bake the
   behavior into the model, where `to_json` honors it. Every generated
   model carries a `@model_serializer(mode='wrap')` whose keep-set is
   exactly `model_fields_set`: unset optionals omit (P11) and an
   explicitly-set `None` (incl. required+nullable) emits `null`. `const`
   needs **no** keep-set entry: its `model_validator(mode='before')`
   injects the fixed value when absent, which lands it *in*
   `model_fields_set` (verified `json-schema/research/const_fields_set_probe.py`), so the
   discriminator emits via the normal path with no special-casing — see
   [[const]]. Because `model_fields_set` distinguishes a wire `null`
   from a wire-absent key, Python round-trips optional+nullable
   **faithfully** — the same tier as TS, not the Go/Java collapse. The
   same serializer re-validates the current field values
   (`type(self).__pydantic_validator__.validate_python({...})` — returns
   a throwaway instance, no serialize recursion) to catch the
   `model_construct`/mutation bypasses strict construction can't see;
   `validate_assignment` covers in-place mutation. On the read side the
   converter's `TypeAdapter.validate_json` runs every model validator, so
   deserialize-side validation needs no extra hook. Empirically verified
   against the live default converter (`json-schema/research/temporal_pydantic_probe.py`,
   `json-schema/research/pyd_serialize_probe.py`, `json-schema/research/pyd_null_serialize_probe.py`).
   **Caveat:** the keep-set filters Python field names against serialized
   keys; once JSON-name aliases land (the case-mapping question in
   [[properties]]) the filter must map name↔alias. See [[nullability]].

## Java

1. **Java 8 baseline; POJOs, not records.** The generator targets
   Java 8 to match the Temporal Java SDK's own minimum — emitted code
   must never impose a stricter floor than the SDK it plugs into
   (**P3**/**P4**). Records require Java 16+ (the `java.lang.Record`
   base class does not exist before then), which would exclude the large
   Java 8/11 install base, so models are emitted as **POJOs** (private
   final fields, constructor, getters, `equals`/`hashCode`/`toString`).
   The extra boilerplate is the cost of reach; it stays hidden behind a
   hand-written-feeling API (**P1**).

2. **Jackson typed binding into POJOs.** Jackson handles structural
   (de)serialization. No reflection-based schema library beyond Jackson
   itself (P4).

3. **Primitive for required, boxed for optional.** Required scalar
   fields use primitives (`long`/`double`/`boolean`); optional ones box
   (`Long`/`Double`/`Boolean`) so absence is representable as `null`.
   Reference types (`String`, `List<T>`, object types) carry a non-null
   validator instead, since the type system can't express the
   constraint. See [[nullability]].

   **JSpecify nullness annotations restore the scalar signal for
   reference types.** Emitted Java packages are `@NullMarked`
   (non-null by default); optional reference fields are annotated
   `@Nullable`. This makes a reference field carry the same in-memory
   nullness information that `long`-vs-`Long` already gives scalars,
   closing a consistency gap (**P1**). The annotation tracks
   **in-memory post-construction nullness** — i.e. required (never
   null) vs optional (null when the key is absent) — **not** the
   wire-level nullable/non-nullable distinction; the optional-non-
   nullable "reject explicit `null`" rule stays a validator concern
   (Java §4, [[nullability]]). It is complementary to, not a
   replacement for, the runtime validator (**P7**): the validator
   enforces at the boundary, the annotation propagates that guarantee
   into the consumer's static null-analysis. JSpecify (`org.jspecify`)
   is chosen over JSR-305 (`javax.annotation.Nonnull`, abandoned +
   JPMS split-package) and is the cross-tool consensus (Google,
   JetBrains, Spring, Micronaut, Kotlin, NullAway, Checker Framework).
   Its annotations are `@Retention(CLASS)` / `@Target(TYPE_USE)`:
   present in bytecode, never loaded at runtime, so **no runtime
   dependency** (**P4** intact); a consumer compiling without JSpecify
   on the classpath still compiles cleanly (javac ignores the missing
   CLASS-retained type) and only forfeits the null-analysis benefit.

4. **Spec-strict per-primitive parse logic as a node helper, invoked
   from the collecting deserializer (§5), not as a per-field
   `@JsonDeserialize`.** Each spec-sensitive primitive ships a
   **strict-parse helper** in the runtime that operates over a `JsonNode`
   — e.g. `SpecNumbers.specLong(JsonNode node, String path,
   List<Violation> errs)`: it accepts `1.0`/`1e2`, rejects `1.5`,
   enforces the P16 cap, and on a bad value **pushes a `Violation` and
   returns `null` rather than throwing**. Custom logic is mandatory
   because Jackson's defaults silently truncate (`1.5`→`1`) — a P10
   violation. **It is *not* wired as a per-field `@JsonDeserialize`,
   because per-field binders are fail-fast** — the first field's
   `MismatchedInputException` aborts the whole bind and defeats P8
   aggregation. The per-POJO collecting deserializer (§5) calls the
   helper over each field's tree node and collects its violations. This
   is the exact Go parallel: the spec-strict parse (`parseSpecInteger`)
   is a helper called from the shadow-layout decoder (Go §5), not the
   default binding path. **There is no `…StrictDeserializer` sibling and
   no `getNullValue` override:** the explicit-`null` decision (reject for
   optional-non-nullable/required, accept for nullable) is a per-field
   branch in the collecting deserializer over the node's `isNull()`,
   exactly the three-way Go makes in `UnmarshalJSON` (see
   [[nullability]] Java). The node-helper-vs-retained-`JsonDeserializer`
   choice was settled empirically (`json-schema/research/javaagg/SpecCmp.java`): both
   make identical accept/reject decisions, but the node helper wins —
   no per-field throw/catch, zero sub-parser allocation, and full
   `{path, reason}` control.

5. **Error aggregation primitive — one class-level collecting
   (de)serializer per POJO (RESOLVED).** Each emitted POJO carries
   class-level `@JsonDeserialize(using = <Pojo>.Deserializer.class)` (and
   the mirror `@JsonSerialize(using = <Pojo>.Serializer.class)`, §6). The
   two (de)serializers are emitted as **`public static final` nested
   classes on the model** (`User.Deserializer` / `User.Serializer`), not
   top-level `UserDeserializer` types: every model has its own pair, so
   the names never collide across models (no need to involve them in the
   P18 per-scope collision pass) and they sit visibly with the type they
   serve (P1). This is the same nest-where-the-language-allows idiom P18
   uses for synthesized const/enum value classes (`json-schema/research/nestprobe/`);
   Jackson resolves `@JsonDeserialize(using = User.Deserializer.class)`
   referencing a nested class on the enclosing type with no issue
   (verified `json-schema/research/javaagg/`). The deserializer is the Jackson analog
   of Go's shadow-layout `UnmarshalJSON` (Go §5): a **two-stage
   lenient-then-validate** bind. Stage 1 reads the whole object into a
   `JsonNode` tree (`p.readValueAsTree()`), which *cannot* throw
   Jackson's fail-fast `MismatchedInputException` on field #1. Stage 2
   walks every declared field through the shared spec-strict parse +
   constraint helpers (P7; the §4 node helpers invoked
   here), pushing a `Violation { path, reason }` per problem into one
   list, and throws a single `ValidationException` carrying them all.
   `ValidationException extends JsonMappingException` so it propagates
   out of the deserializer **verbatim** (Jackson does not re-wrap it) and
   the Temporal converter surfaces it as the **cause** of a
   `DataConverterException`; the Nexus handler walks the cause chain,
   pulls `getViolations()`, and emits one BAD_REQUEST `HandlerError`
   (P8). The tree stage also gives closed structs their extra-key check
   (`additionalProperties:false` → a violation per undeclared key) and
   open structs natural unknown-key tolerance (P9).

   **Compatible with the *default* Temporal Java data converter — the
   binding constraint.** The mechanism is baked into the POJO via the
   class-level annotation + runtime-shipped (de)serializer classes, which
   the converter's stock `new ObjectMapper()` (JavaTimeModule + Jdk8Module
   + field-visibility ANY; `JacksonJsonPayloadConverter.newDefaultObjectMapper()`)
   honors. We do **not** own or configure that mapper — so a mapper-level
   `DeserializationProblemHandler` (`mapper.addHandler(...)`, the other
   common Jackson recover-and-continue lever, available in Jackson 2) is
   unavailable: it mutates a mapper instance we can't reach, and no
   annotation binds it to a type so it can't travel with the POJO. And it
   **wouldn't even suffice if we owned the mapper** — it intercepts only a
   fixed set of Jackson-recoverable *binding* events and must return a
   fabricated fallback to continue, so it never sees our spec/constraint
   violations: verified (`json-schema/research/javaagg/HandlerProbe.java`) that 4 of 6 P8
   cases fire **no** hook at all — `1.5` is silently truncated to `1`, the
   `±(2^53−1)` cap is a valid `long`, and missing-required is a non-event
   — while `"abc"`→`long` recovers by writing a fabricated `0`. It is also
   mapper-global and deserialize-only. So the per-POJO collecting
   deserializer is the only path that aggregates through the default
   converter. This exactly parallels the Python finding that the
   converter owns `to_json`, so behavior must live in the model
   (`@model_serializer`), not a call we make. Empirically proven
   end-to-end through `DefaultDataConverter.STANDARD_INSTANCE` in
   `json-schema/research/javaagg/`: three independent deserialize errors aggregated in one
   shot; `1.0` accepted / `1.5` rejected as integer; type mismatches
   aggregated; the `ValidationException` recovered from the
   `DataConverterException` cause chain with all violations intact.

   **Considered alternative — Jackson 3.1's built-in problem collection
   (`CollectingProblemHandler` / `ObjectReader.problemCollectingReader()`
   / `readValueCollectingProblems()` → `DeferredBindingException`),
   rejected.** Shipped in Jackson 3.1.0 (2026-02-23). Rejected for three
   independent reasons. (1) **Doesn't engage under the default converter:**
   it activates only when *the reader is configured* **and** the read is
   invoked via `readValueCollectingProblems()`; the default
   `JacksonJsonPayloadConverter` calls plain `mapper.readValue(...)` on a
   mapper we don't own. `CollectingProblemHandler` *is* a
   `DeserializationProblemHandler` — the same lever already noted
   unavailable above, just newer packaging. (2) **Version floor (P3/P4):**
   Jackson 3.1 vs the SDK's Jackson 2.x default — requiring it would
   impose a stricter floor than the SDK ships. (3) **Solves only half:**
   it collects Jackson's *structural/binding* problems (mismatched
   property names, missing type ids), **not** our constraint validation
   (P7) — notably it would miss `1.5`→`1` (Jackson's default `Long`
   silently truncates, raising no problem), the P16 cap,
   `minLength`/`pattern`/`const`/`minProperties`/`dependentRequired`, etc.
   — and it is deserialize-only (no P17/§6 serialize analog). We would
   still need the collecting (de)serializer, plus a second error model to
   merge. No net gain.

6. **Serialize: validate-then-write; per-field `@JsonInclude` (P17).**
   Class-level `@JsonSerialize(using = <Pojo>.Serializer.class)` (the
   nested `Serializer` from §5) runs the shared
   constraint predicates **first**, collecting into the same
   `ValidationException`, and throws before writing a byte; otherwise it
   writes fields honoring per-field omit-vs-`null` (the `@JsonInclude`
   semantics, applied in code since the serializer is custom): optional
   `NON_NULL` (`null` → omitted); required+nullable forces inclusion
   (`null` → `null`); required-non-nullable always emitted. Optional+
   nullable collapses (absent and `null` share one in-memory `null`) →
   **conservative omit**, the same tier as Go (not the faithful TS/Python
   round-trip). Serialize-side error aggregation is **resolved with §5**
   (same `ValidationException` primitive) — proven in `json-schema/research/javaagg/`: an
   invalid in-memory model fails loudly with every violation; a valid one
   omits an optional `null`.

## Go

1. **Numeric primitives.** `integer` → `int64`, `number` → `float64`.
   No per-schema width tuning in v1.

2. **Optionality via pointer.** Optional fields use pointer types
   (`*int64`, `*string`, `*Foo`). Exception: optional arrays use the
   bare `[]T` since Go distinguishes `nil` slice from empty slice.

3. **Distinguish absent from zero via pointer (P12).** Required-field
   validators read the shadow pointer; `nil` means absent. This is
   how Go gets an "absent" state for value types.

4. **`new(expr)` for pointer-from-literal — ergonomic, not required.**
   When targeting Go 1.26+, generator-emitted call sites use
   `new(expr)` for constructing optional fields
   (`User{Nickname: new(42)}`). Older toolchains are supported via
   the equivalent verbose form (`tmp := 42; u.Nickname = &tmp`);
   the user-facing API is identical. Go 1.26+ is preferred but not
   a hard floor.

5. **Custom `UnmarshalJSON` on every emitted struct.** Per-struct
   `UnmarshalJSON` decodes into a shadow layout of `*json.Number` /
   `*T` pointers, dispatches each field through runtime validation
   helpers, and aggregates `ValidationError{Path, Reason}` with
   `errors.Join` (per P8). This is also where spec-strict parsing
   lives — e.g., `parseSpecInteger(json.Number) (int64, error)`
   accepts `1.0` per JSON Schema where the stdlib's `int64`
   unmarshal would reject it.

6. **`MarshalJSON` mirrors `UnmarshalJSON`: validate-then-delegate
   (P17).** Each struct's `MarshalJSON` runs the shared `Validate()`
   (the constraint-predicate layer the decoder also calls), then
   delegates to the stdlib encoder via a local `type alias` — this
   avoids infinite recursion while still honoring the struct tags, so
   the per-field omit-vs-`null` decision is **declarative**:
   - optional fields: `*T` with `,omitempty` → `nil` omitted;
   - required+nullable: `*T` **without** `omitempty` → `nil` emits `null`;
   - required-non-nullable: bare value type → always emitted.
   `omitempty` on a pointer omits only `nil`, so a pointer-to-zero-value
   still serializes — pointer presence is the set-ness signal (P11/P12).
   No hand-built map needed. Verified `json-schema/research/oe/`,
   `json-schema/research/serialize_probe/`.

7. **`<Field>OrDefault()` accessor for default-bearing fields (P11).**
   Go has no language-native default and the structs are otherwise
   plain-field, so a field carrying a schema `default` gets a single
   generated read accessor:
   ```go
   func (u User) NicknameOrDefault() string {
       if u.Nickname != nil { return *u.Nickname }
       return "anon" // schema default
   }
   ```
   The bare field stays `*T` (set-ness intact, omit-on-serialize stays
   faithful — P11/P12); the accessor is the *materialize-on-read* path,
   modeled on proto3's `GetX()`. Emitted **only** for default-bearing
   fields, so the plain-struct feel (P1) holds everywhere else. Named
   `<Field>OrDefault` (not `Get<Field>`) to read as "value or its
   default" and to avoid implying a getter on every field. See [[default]].

