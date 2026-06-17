# `type`

Source: JSON Schema 2020-12, Validation vocabulary, §6.1.1.

Constrains an instance to one of seven named JSON-Schema type families. The
single most fundamental validation keyword — every other validation keyword
is defined relative to an instance type, so `type` gates whether the rest of
a schema's assertions are meaningful for a given instance.

## Spec summary

- Value MUST be either a string or an array of unique strings.
- Each string MUST be one of: `"null"`, `"boolean"`, `"object"`, `"array"`,
  `"number"`, `"string"`, `"integer"`.
- `"integer"` is **not** a JSON primitive type — it matches any JSON number
  whose fractional part is zero (so `1`, `1.0`, and `1e2` all satisfy
  `type: integer`).
- Array form validates if the instance matches **any** listed type (OR).
- Absence of `type` means "any type" — equivalent to listing all seven.

## Support decision

**Support:** partial — single-string form only.

We accept `type: "<primitive>"` for all seven primitive type names. We
**reject** schemas where `type` is an array, and **reject** schemas with no
`type` keyword.

Rationale (citing [[PRINCIPLES.md]]):
- **P5 (strict subset)**: Multi-type unions don't lower coherently across
  Go/Java; we keep the language ceiling at OpenAPI 3.0's level.
- **P10 / P10.1 (strict schema, reject loudly)**: Array `type` is
  structurally ambiguous (is `["T","null"]` an optional T, a nullable T, or
  a sum type?). Reject at load time with a fix-it message.
- **P10.2 (optional ≠ nullable)**: The `["T","null"]` idiom collapses two
  different concerns; model nullability through a dedicated convention
  instead (TBD — see [[nullability]]).
- Absent `type` makes field shape undecidable, violating **P10**.

Loader behavior:
- Array `type` → reject with diagnostic naming the schema location and
  pointing at the nullability convention.
- Missing `type` → reject with diagnostic; require explicit type on every
  schema.
- Unknown type name (`"int"`, `"date"`, etc.) → reject.
- `type: "object"` with no `properties`, `patternProperties`, or
  `additionalProperties` → reject (P10.1). Per spec this is "any object",
  but the typed-codegen contract requires explicit intent. Diagnostic
  names the three resolutions: add `properties: {...}` (typed struct),
  add `additionalProperties: true` (open opaque map), or add
  `additionalProperties: false` (closed empty object).
- `type: "null"` standalone (not inside the [[nullability]] pattern) →
  reject. A field that is *always* `null` carries no information and
  is almost always a schema bug. The only legitimate appearance of
  `{"type":"null"}` is as one branch of the recognized nullability
  `oneOf` (see [[nullability]]).

## Type mapping

Emitted field type when `type` appears in a field-producing position.
Optional/nullable wrapping is owned by [[required]] and [[nullability]] —
this table is the bare type only.

Required form below. Optional fields wrap per [[nullability]] (Java
boxes to `Long`/`Double`/`Boolean`; Go uses `*T`; TS uses `?` on the
field; Python uses `Optional[T]`).

| `type` token | Go | TypeScript | Python | Java |
|---|---|---|---|---|
| `"string"`  | `string`            | `string`             | `str`             | `String` |
| `"integer"` | `int64`             | `number`             | `int`             | `long` |
| `"number"`  | `float64`           | `number`             | `float`           | `double` |
| `"boolean"` | `bool`              | `boolean`            | `bool`            | `boolean` |
| `"object"`  | struct from [[properties]] | interface from [[properties]] (**not classes**) | Pydantic model | POJO class (Java 8; **not records** — see PRINCIPLES Java §1) |
| `"array"`   | `[]T` (T from [[items]])   | `T[]`                | `list[T]`         | `List<T>` |
| `"null"`    | only inside [[nullability]] pattern | only inside [[nullability]] pattern | only inside [[nullability]] pattern | only inside [[nullability]] pattern |

Notes:
- **TS**: `integer` and `number` collapse to `number`; integer-ness moves
  to the validator.
- **Java**: `long`/`double`/`boolean` for required fields; `Long`/
  `Double`/`Boolean` for optional fields (see [[nullability]]). The
  primitive-vs-boxed split is what the JVM gives us for free; reference
  types like `String`/`List<T>` use a non-null validator instead.
- **Python**: `bool <: int`; Pydantic strict mode (the only mode we use)
  keeps them distinct.

## Validator mapping

Per **P7** validation is enforced at the (de)serializer boundary. Per **P8**
errors aggregate into the language-native primitive.

| `type` token | Go | TypeScript | Python | Java |
|---|---|---|---|---|
| `"string"`  | typed `Unmarshal` into `string` | `typeof v === 'string'` | Pydantic `str` strict | Jackson typed binding |
| `"integer"` | shadow `*json.Number` → runtime `parseSpecInteger` → `int64` (accepts `1.0`, rejects `1.5`, caps ±(2^53−1)) | `typeof v === 'number' && Number.isSafeInteger(v)` (accepts `1.0` natively; caps ±(2^53−1)) | `SpecInt = Annotated[int, BeforeValidator(_parse_spec_integer)]` in runtime | `@JsonDeserialize(using = SpecLongDeserializer.class)` from runtime |
| `"number"`  | `float64` unmarshal | `typeof v === 'number'` | Pydantic `float` strict | `Double` binding |
| `"boolean"` | `bool` unmarshal | `typeof v === 'boolean'` | Pydantic `bool` strict (rejects `1`/`0`) | `Boolean` binding |
| `"object"`  | typed struct unmarshal | `typeof v === 'object' && v !== null && !Array.isArray(v)` | Pydantic model | typed class binding |
| `"array"`   | typed slice unmarshal | `Array.isArray(v)` | Pydantic `list` | typed `List` binding |
| `"null"`    | `raw == nil` / `bytes.Equal(raw, []byte("null"))` | `v === null` | `v is None` | `v == null` |

Strategy per language:
- **Go**: Every generated struct gets a custom `UnmarshalJSON`. It decodes
  into a shadow struct of `*json.Number` / `*T` pointers (absence
  observable per P12), dispatches per field, builds
  `ValidationError{Path, Reason}` and aggregates with `errors.Join`.
  Integer fields go through a runtime helper that also enforces the
  cross-language integer cap (`±(2^53−1)`, see Open question 1 —
  **resolved**):
  ```go
  // IntegerCap = 1<<53 - 1 = 9007199254740991 (== JS Number.MAX_SAFE_INTEGER)
  func parseSpecInteger(n json.Number) (int64, error) {
      f, err := n.Float64()
      if err != nil { return 0, err }
      if f != math.Trunc(f) { return 0, ErrFractional }         // "1.5" → reject
      if f < -IntegerCap || f > IntegerCap { return 0, ErrRange } // > ±(2^53-1) → reject
      i, err := n.Int64()
      if err != nil { return 0, err }                            // belt-and-suspenders
      return i, nil                                              // "1", "1.0", "1e2"
  }
  ```
  User-facing field stays plain `int64`. The Go primitive holds ±2^63,
  but the validator rejects anything past the ±(2^53−1) cap so all four
  languages agree on the accepted range.
- **TypeScript**: Hand-emit `typeof`/`Array.isArray` checks per field; no
  runtime schema library (P4). `Number.isInteger(v)` is spec-compliant
  for type-classification (`1.0 === 1` in JS, so
  `Number.isInteger(1.0) === true`) — verified empirically across all
  10 type-classification fixtures. Push `ValidationError { path, reason }`
  into an array, throw `AggregateError` at the end.
  **Precision — resolved by the ±(2^53−1) cap (Open question 1):**
  `JSON.parse` silently rounds integers past 2^53 to the nearest
  double, but with the cap fixed at `Number.MAX_SAFE_INTEGER`
  (`2^53−1`), a plain post-parse `Number.isSafeInteger(v)` is a
  **complete and sound** check — no text pre-scan, no `lossless-json`,
  no P4 tension. Empirically verified (`/tmp/ts_cap_probe.mjs`): every
  integer literal `>` `MAX_SAFE_INTEGER` rounds to a double that fails
  `Number.isSafeInteger` (e.g. `9007199254740993` → `9007199254740992`,
  which is `> MAX_SAFE_INTEGER` → rejected); swept `[2^53, 2^53+10^5)`
  with zero leaks. Integer fields therefore emit
  `typeof v === 'number' && Number.isSafeInteger(v)`.
- **Python**: Pydantic v2 models in strict mode. `pydantic.ValidationError`
  already aggregates via `.errors()`. Integer fields are typed as
  `SpecInt = Annotated[int, BeforeValidator(_parse_spec_integer)]` from
  the generated runtime; the helper explicitly rejects `bool` (closes
  the `bool <: int` trap), accepts `int`, accepts `float` with zero
  fractional part. User-facing field type remains `int`.
  Rationale (empirically verified, Pydantic 2.13): strict mode alone
  rejects `1.0` and `1e2` (spec-valid integers); lax mode alone accepts
  `True`, `"1"`, `"1.0"` (spec-invalid). The `BeforeValidator` is the
  only way to hit the spec exactly. Note: Python ints are unbounded, so
  the runtime helper also enforces the cross-language cap `±(2^53−1)`
  (Open question 1 — **resolved**): `abs(v) > 9007199254740991` → reject.
- **Java**: Jackson typed binding into POJOs (Java 8 floor; not records,
  see PRINCIPLES Java §1). `Long` fields get
  `@JsonDeserialize(using = SpecLongDeserializer.class)` from the
  generated runtime. The deserializer branches on `JsonToken`:
  ```java
  // Sketch — see runtime for full impl. CAP = 9007199254740991L (2^53-1).
  JsonToken t = p.currentToken();
  if (t == VALUE_NUMBER_INT) {                  // "1", ">2^53"
      BigInteger bi = p.getBigIntegerValue();
      if (bi.abs() > CAP) throw ...;            // ±(2^53-1) cap
      return bi.longValueExact();
  }
  if (t == VALUE_NUMBER_FLOAT) {                // "1.0", "1.5", "1e2"
      double d = p.getDoubleValue();
      if (NaN || inf || d != floor(d) || abs(d) > CAP) throw ...;
      return (long) d;
  }
  throw "expected number token, got " + t;      // rejects bool, string, null-as-value-here
  ```
  Rationale (empirically verified, Jackson 2.18): Jackson's defaults
  *silently truncate* `1.5`→`1` for `Long` fields — a P10 violation
  blocking shipping with defaults. `ACCEPT_FLOAT_AS_INT=false` fixes
  truncation but rejects spec-valid `1.0`/`1e2` and still coerces `"1"`.
  The custom deserializer is the only path that matches the spec.
  The `±(2^53−1)` cap (Open question 1 — **resolved**) is enforced
  explicitly above; `>2^63` would also trip Jackson's own range check,
  but our cap is tighter so ours fires first.
  `MismatchedInputException` converts to our `ValidationError`.
  Aggregation primitive TBD with the Java section of PRINCIPLES.md.

## Property-testing matrix

### Accepted values (positive tests)

| Shape | Values |
|---|---|
| Single primitive | `"null"`, `"boolean"`, `"object"`, `"array"`, `"number"`, `"string"`, `"integer"` |

### Rejected at load time (negative tests)

Loader must produce a clear, located diagnostic for each.

| Reason | Values |
|---|---|
| Array form (P5/P10) | `["string","null"]`, `["integer","number"]`, full 7-element union, `[]`, `["string"]` |
| Absent `type` (P10) | `{}`, `{"description":"…"}` |
| Object without shape (P10.1) | `{"type":"object"}` with no `properties`, `patternProperties`, or `additionalProperties` (spec says "any object"; we require explicit intent) |
| `"null"` standalone | `{"type":"null"}` anywhere except as a branch of the [[nullability]] `oneOf` pattern |
| Unknown type name | `"int"`, `"float"`, `"date"`, `"any"`, `"bigint"`, `"String"`, `"INTEGER"` |
| Wrong outer type | `5`, `null`, `true`, `{"type":"string"}` |
| Nested / malformed | `[["string"]]` |

### Runtime fixtures per accepted type (validator tests)

For each accepted `type`, fuzz over:
- **Canonical accept**: `"x"`, `1`, `1.5`, `true`/`false`, `{}`, `[]`, `null`.
- **Boundary accept**: `""`, `0`, `-0`, `1.0` (must satisfy `integer`), `1e2`.
- **Wrong-type reject**: every other primitive against this type — 7×6=42
  cross-reject cases.
- **`bool`-is-not-`integer` trap**: `true` against `"integer"` must reject
  in all four languages. Go/TS/Java reject naturally (`true` is not a
  number token); Python relies on the explicit `isinstance(v, bool)`
  reject inside `_parse_spec_integer`.
- **Large integers (cap = ±(2^53−1), resolved)**: accept
  `9007199254740991` (`2^53−1`, the boundary) and `-9007199254740991`;
  reject `9007199254740992` (`2^53`), `9007199254740993` (`2^53+1`,
  which TS silently rounds to `2^53` — must still reject), and
  `18014398509481985` (`2^54+1`). Same accept/reject set in all four
  languages.

## Interactions

- **Gates which assertions apply.** Spec §3.4 silently ignores
  type-mismatched keywords; per **P10.1** we instead **reject** mismatched
  combinations at generator time (e.g. `{type:"string", minimum: 5}`
  errors).
- **[[const]]**: per **P9.1**, emitted field type is `type`'s mapping
  (`string`, not `"v1"`). Validator checks the const value at runtime so
  bumping a const never breaks the type signature.
- **[[enum]]**: emitted type comes from `type`; `enum` narrows runtime
  values; per **P9** unknown enum values are preserved on deserialize.
- **[[oneOf]]**: rejected in general per **P5**. The **only** accepted
  shape is the [[nullability]] pattern — `oneOf` with exactly 2
  branches, one being `{"type":"null"}` (order-insensitive). This is
  also the only context in which `type:"null"` may appear.
- **[[properties]] / [[items]]**: only meaningful when `type` is `"object"`
  / `"array"`. Cross-product mismatches are generator-time errors.
  Object-shape decisions live in [[properties]] / [[additionalProperties]];
  in summary, **typed structs are open by default** (per JSON Schema
  spec and **P9** — accept and preserve extras into a catch-all),
  closed behavior requires explicit `additionalProperties: false`.
- **[[format]]**: format hints layer onto `type:"string"` (mostly); a
  format may pick a more specific emitted type (`time.Time` in Go for
  `format:"date-time"`) while staying gated by the underlying string type.
- **[[required]]** + [[nullability]] own optional/nullable wrapping;
  `type` only contributes the inner type.

## Ecosystem variance

| Source dialect | Action |
|---|---|
| JSON Schema 2020-12 | Native. Reject only documented out-of-subset cases. |
| OpenAPI 3.1         | Aligns with 2020-12. Native. |
| OpenAPI 3.0         | `nullable: true` → reject (P10.2). User must rewrite. |
| Swagger 2.0 / draft-4 | Same as OAS 3.0; no type arrays; nullable rewrite required. |

Pre-draft-4 union-of-schemas form (`type: [{...},{...}]`) is irrelevant —
no current toolchain emits it.

## Resolved questions

1. **Large integers — RESOLVED: hard `±(2^53−1)` cap (uniform across
   languages).** The cap is `Number.MAX_SAFE_INTEGER` =
   `9007199254740991`. Rationale:
   - It is the only cap TypeScript can defend **without** a third-party
     parser. Empirically verified (`/tmp/ts_cap_probe.mjs`) that a plain
     post-parse `Number.isSafeInteger(v)` is complete and sound: every
     integer literal past the cap rounds to a double that fails
     `isSafeInteger` (`9007199254740993` → `9007199254740992`, which is
     `> MAX_SAFE_INTEGER` → rejected), with zero leaks swept across
     `[2^53, 2^53+10^5)`. This keeps P4 (minimal TS runtime deps) intact.
   - Go (`int64`) and Java (`long`) hold ±2^63 natively but their
     validators reject past `±(2^53−1)`, so all four languages agree on
     the accepted set.
   - Python ints are unbounded; the runtime helper enforces the cap.

   Spec-compliant `1.0`-as-integer was resolved earlier — see runtime
   helpers per language.

## Open questions

1. **Cross-language conformance suite for the integer runtime helpers.**
   Each language's helper (`parseSpecInteger`, `_parse_spec_integer`,
   `SpecLongDeserializer`, and TS's `Number.isSafeInteger` use) must pass
   an identical fixture set: accept `1`, `1.0`, `1e2`, `-0`, the cap
   boundary `±(2^53−1)`; reject `1.5`, `true`/`false`, `"1"`, non-numeric
   strings, NaN, ±Infinity, and any magnitude past `±(2^53−1)`.

## See also

- [[enum]], [[const]] — other any-instance-type assertions.
- [[multipleOf]], [[minimum]], [[maximum]], [[exclusiveMinimum]],
  [[exclusiveMaximum]] — numeric assertions gated by `type`.
- [[format]] — string refinements layered on `type:"string"`.
- [[oneOf]] — currently rejected per P5; revisit if [[nullability]]
  requires it.
- [[required]], [[nullability]] — own optional/nullable wrapping.
