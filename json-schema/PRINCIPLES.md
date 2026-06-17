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
10. **Strict schema validation**. The *schema* is held to a strict shape: ambiguous constructs are rejected at generator time with clear errors (no `oneOf` discriminator → reject; `additionalProperties: {}` → reject; required+nullable → reject).
    1. **Reject ambiguity loudly at generator time.** Better to error than to guess. Unsupported features get explicit errors, not silent passthrough.
    2. **Distinguish optional from nullable.** Two different concerns: "key may be absent" vs "value may be null." Required + nullable is a schema bug.
    3. **Nullability is the only `oneOf` shape accepted without a discriminator.** The recognized pattern is `oneOf: [{type: "T"}, {type: "null"}]` — exactly 2 branches, exactly one being `{type: "null"}`, order-insensitive. Any other discriminator-less `oneOf` is rejected per the main P10 rule. See [[nullability]] for details.
11. **Default off-the-wire, populated on-the-way-in.** `default` values are never sent back on serialize (server-defined); they are populated when missing on deserialize.
12. **Distinguish absent from zero value**. For example in Go, prefer `string` for representing optional strings.
13. **One file per input by default; merge on cycle.** Cross-file ref cycles auto-merge into a single output. No circular-import gymnastics.
14. **External refs are local-file-only.** YAML and JSON files relative to the input. HTTP refs rejected for reproducibility.
15. **CLI and in-process API converge.** The CLI is a thin parser over API.

## TypeScript

TODO

## Python

TODO

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

## Java

TODO

