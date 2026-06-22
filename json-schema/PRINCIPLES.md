# Core Design Principles

## Cross-cutting

1. **Hand-written feel over generated feel.** Output should read like something a human wrote idiomatically for that language.
2. **Prefer ergonomics over performance**. Prefer to pay for normalization and conversion over giving a subpar experience for a given language.
3. **Works with default Temporal payload converter setup (contrib libraries okay if necessary (Pydantic))**.
4. **Minimal external runtime dependencies on generated code.** All languages require `nexus-rpc` SDKs for representing service contracts in code or Temporal SDKs for typed client generation.
5. **Strict JSON schema subset.** Generator rejects JSON schema features that cannot be coherently represented in all supported languages. (e.g., `anyOf`, `allOf`).
6. **Use 2020-12 as the base spec version**. The strict subset is based on the latest draft.
7. **Validation is enforced, not advisory.** Constraints (`minLength`, `pattern`, `minimum`, …), `const`, and discriminator strings are checked at the (de)serializer boundary. Schemas are not just documentation.
8. **Aggregate validation errors.** Surface every violation in one shot using the language-native aggregation primitive: TS `AggregateError` of `ValidationError { path, reason }`; Python uses Pydantic's native `pydantic.ValidationError` which already aggregates via `.errors()` (each entry has `loc` + `msg` + `type`); Go `errors.Join` of `ValidationError { Path, Reason }`. Structured payloads; never stringly-typed messages. Error set as the cause of a Nexus RPC HandlerError with BAD_REQUEST error type.
9. **Forward compatibility over strict types**. Accept and preserve unknown enum values and unknown fields as best as possible.
    1. **Forward-compatible `const`.** Field type emitted as the underlying primitive (`string`, not `'v1'`); value validated at runtime. Bumping a const value never breaks the type signature.
10. **Strict schema validation**. The *schema* is held to a strict shape: ambiguous constructs are rejected at generator time with clear errors (no `oneOf` discriminator → reject; `additionalProperties: {}` → reject; bare `{type:"object"}` → reject).
    1. **Reject ambiguity loudly at generator time.** Better to error than to guess. Unsupported features get explicit errors, not silent passthrough.
    2. **Distinguish optional from nullable.** Two orthogonal concerns: "key may be absent" (optional, owned by [[required]]) vs "value may be null" (nullable, owned by the [[nullability]] `oneOf` pattern). Because they are orthogonal, **all four combinations are legal** — including *required + nullable* ("must be present, value may be `null`"), which is a well-defined, unambiguous, enforceable contract (presence-check on, null-rejection off). We do **not** reject it: it round-trips losslessly in every language (presence is guaranteed, so in-memory `null`/`nil`/`None` maps unambiguously to wire `null`). The only residual wire-vs-memory collapse is *optional + nullable* in Java/Go/Python, where absent and `null` share one in-memory value; see [[nullability]] round-trip note.
    3. **Nullability is the only `oneOf` shape accepted without a discriminator.** The recognized pattern is `oneOf: [{type: "T"}, {type: "null"}]` — exactly 2 branches, exactly one being `{type: "null"}`, order-insensitive. Any other discriminator-less `oneOf` is rejected per the main P10 rule. See [[nullability]] for details.
11. **Default off-the-wire, materialized on read.** `default` values are
    never emitted on serialize **and never stored into the field on
    deserialize**. The generator tracks field *set-ness*: serialize omits
    any unset field — with **no value comparison** (never a deep-equals
    against the default) — and the default is surfaced lazily *on read*
    via an accessor or the language-native default. Explicitly setting a
    field, even to a value equal to the default, marks it set and pins it
    on the wire. Mechanisms: Go `,omitempty` on the optional pointer;
    Pydantic `model_dump(exclude_unset=True)`; TS `undefined`; Java
    `@JsonInclude(NON_NULL)`. This supersedes the earlier "populate the
    field on deserialize" rule, which required a deep-equals to strip on
    the way out and destroyed the absent-vs-set distinction (P12).
    Empirically verified (`/tmp/serialize_probe/`,
    `/tmp/pyd_serialize_probe.py`). See [[default]] (when written).
12. **Distinguish absent from zero value**. For example in Go, prefer `string` for representing optional strings.
13. **One file per input by default; merge on cycle.** Cross-file ref cycles auto-merge into a single output. No circular-import gymnastics.
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
       optional/nullable/required declaration — see [[nullability]]),
       `default` omission (P11), and `const` auto-emit.
    Serialize runs the shared `Validate` **before emitting a byte** and
    fails with the same aggregated error primitive as deserialize (P8).
    In statically-typed languages (Go/TS/Java) in-memory construction is
    unchecked, so serialize-side validation has real teeth; in Python
    strict construction already validates the happy path, so serialize
    only re-validates to catch the `model_construct`/mutation bypasses.
    Empirically verified in Go and Python (`/tmp/serialize_probe/`,
    `/tmp/pyd_serialize_probe.py`, `/tmp/pyd_null_serialize_probe.py`).

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

6. **Serialize via `model_dump(exclude_unset=True)`; re-validate to
   catch bypasses (P17).** Strict construction already validates the
   happy path, so serialize-side validation only re-runs the model
   validators (`model_validate(m.model_dump())`, or `validate_assignment`
   for mutation) to defend against the `model_construct` /
   post-construction-mutation escape hatches. `exclude_unset` implements
   the entire per-field omit-vs-emit-`null` table in one flag: unset
   optionals omit; an explicitly-set `None` (including required+nullable)
   emits `null`. Because `model_fields_set` distinguishes a wire `null`
   from a wire-absent key, Python round-trips optional+nullable
   **faithfully** — the same tier as TS, not the Go/Java collapse.
   Empirically verified (`/tmp/pyd_serialize_probe.py`,
   `/tmp/pyd_null_serialize_probe.py`). See [[nullability]].

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

4. **Paired strict / non-strict custom deserializers per primitive.**
   Each spec-sensitive primitive ships two deserializers in the
   runtime: a base one (e.g. `SpecLongDeserializer`) used for
   nullable fields, and a `…StrictDeserializer` that overrides
   `getNullValue` to reject an explicit `null` token (used for
   optional-non-nullable and required fields). The generator picks the
   `@JsonDeserialize` annotation per field from its nullability
   declaration. Custom deserializers are mandatory because Jackson's
   defaults silently truncate (`1.5`→`1`) — a P10 violation. They also
   enforce the P16 cap.

5. **Error aggregation primitive — TBD.** Jackson fails fast on the
   first `MismatchedInputException` by default; achieving P8 single-shot
   aggregation needs a chosen mechanism (collecting deserializer, or a
   validation pass over a lenient first-stage bind). Open question;
   tracked here and in [[nullability]] until resolved. **Now gates the
   serialize direction too** (§6) — the same single-shot aggregation
   problem applies mirror-image when validating before write.

6. **Serialize: validate-then-write; per-field `@JsonInclude` (P17).**
   A validation pass runs the shared constraint checks before Jackson
   writes the POJO. Per-field omit-vs-`null` mirrors Go via
   `@JsonInclude`: optional fields `@JsonInclude(NON_NULL)` (`null` →
   omitted); required+nullable forces inclusion
   (`@JsonInclude(ALWAYS)`, `null` → `null`); required-non-nullable is
   always emitted. Optional+nullable collapses (absent and `null` share
   one in-memory `null`) → **conservative omit**, the same tier as Go
   (not the faithful TS/Python round-trip). Serialize-side error
   aggregation depends on the still-open primitive in §5.

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
   No hand-built map needed. Verified `/tmp/oe/`,
   `/tmp/serialize_probe/`.

