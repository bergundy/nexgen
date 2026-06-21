# `nullability` (cross-cutting design note)

Not a JSON Schema keyword. Captures the generator's per-language convention
for how **optionality** ("key may be absent") and **nullability** ("value
may be `null`") are encoded in emitted code. Per **P10.2** these are two
different concerns and we keep them distinct.

## Concept matrix

| Concern | JSON Schema source | Wire shape | Type-level question |
|---|---|---|---|
| **Optional** | property name *not* in `required` array of the enclosing object | key may be absent | "How do we represent absent?" |
| **Nullable** | (convention TBD; spec form is `["T","null"]` which we reject per [[type]] support decision) | value present, equal to JSON `null` | "How do we represent JSON null?" |

This doc owns the optionality conventions per language. Nullability
conventions are stubbed below pending the nullability-design decision.

## Optionality conventions per language

### Java

Primitive type for required fields; boxed type for optional. Emitted as
a POJO (Java 8 floor — **not** a record; see PRINCIPLES Java §1), so the
fields below are private with generated getters/constructor. The
package is `@NullMarked` (JSpecify; non-null by default), so optional
reference fields carry `@Nullable` and required ones need no annotation
(see PRINCIPLES Java §2):

```java
@NullMarked
public final class User {
    private final long      id;        // required: integer
    private final @Nullable Long   nickname;  // optional: integer — null if absent
    private final String    name;      // required: string (non-null by default; validator enforces)
    private final @Nullable String email;     // optional: string
    // generated constructor, getters, equals/hashCode/toString
}
```

| `type` token | required | optional |
|---|---|---|
| `"integer"` | `long`    | `@Nullable Long`    |
| `"number"`  | `double`  | `@Nullable Double`  |
| `"boolean"` | `boolean` | `@Nullable Boolean` |
| `"string"`  | `String` *(non-null validator)* | `@Nullable String` |
| `"object"`  | `T` *(non-null validator)* | `@Nullable T` |
| `"array"`   | `List<T>` *(non-null validator)* | `@Nullable List<T>` |

Required-field validators must still check absence and `null` explicitly
for reference types (`String`, `List<T>`, object types) — the type
system can't carry that constraint by itself. The `@Nullable`/
`@NullMarked` annotations are **complementary**: they track in-memory
post-construction nullness (required → non-null; optional → may be
null when the key is absent) and propagate it into the consumer's
static null-analysis. They are compile-time only (CLASS retention →
no runtime dependency, **P4**) and do **not** encode the wire-level
nullable distinction — every optional reference field is `@Nullable`
regardless of whether it is optional-non-nullable or optional+nullable.

### Go

Pointer to primitive for optional fields; bare type for required.

```go
type User struct {
    ID       int64   `json:"id"`              // required
    Nickname *int64  `json:"nickname,..."`    // optional — nil if absent
    Name     string  `json:"name"`            // required
    Email    *string `json:"email,..."`       // optional
}
```

| `type` token | required | optional |
|---|---|---|
| `"integer"` | `int64`   | `*int64`   |
| `"number"`  | `float64` | `*float64` |
| `"boolean"` | `bool`    | `*bool`    |
| `"string"`  | `string`  | `*string`  |
| `"object"`  | `T`       | `*T`       |
| `"array"`   | `[]T`     | `[]T` *(nil-slice = absent)* |

Pointer-from-literal ergonomics: Go 1.26 extended the `new` builtin
to take an expression — `new(expr)` allocates a variable of the
inferred type, initializes it to `expr`'s value, and returns `*T`.
The release notes explicitly call out optional-pointer JSON fields
as the motivating use case.

```go
// Pre-1.26 (verbose):
tmp := int64(42)
u := User{Nickname: &tmp}

// 1.26+ (the convention we emit at call sites):
u := User{Nickname: new(int64(42))}
u := User{Nickname: new(req.Count)}        // arbitrary expressions work
u := User{Nickname: new(yearsSince(t))}    // including function calls
```

Generator constructors and builders prefer `new(expr)` for
ergonomics. Go 1.26+ is **not** a hard requirement — on older
toolchains the equivalent verbose form (`tmp := 42; u.Nickname = &tmp`)
compiles correctly. The generated *user-facing* API surface is
identical either way; only the call-site idiom shown in examples and
emitted constructors differs.

`[]T` for optional arrays: a `nil` slice and an empty slice are
distinguishable in Go (`s == nil` vs `len(s) == 0`), so a pointer
wrapper is unnecessary; the runtime check is `if user.Tags != nil`.

### TypeScript

Optional fields use the `?` modifier; the field type stays the bare
type. Absence is `undefined`, not `null`.

```ts
interface User {
  id: number;          // required
  nickname?: number;   // optional — undefined if absent
  name: string;        // required
  email?: string;      // optional
}
```

| `type` token | required | optional |
|---|---|---|
| any                 | `T`       | `T` with `?` on the field |

Validator emits `if (parsed.x === undefined) ...` for required-presence
checks. We never use `T | null` for the optional-only case — `?`/
`undefined` is the absence channel; `null` is a value reserved for the
nullability convention (`x?: T | null` is the optional+nullable form).

### Python

Use `Optional[T]` (alias for `Union[T, None]`) on the field type.
Default value is `None` for absence.

```python
from typing import Optional
from pydantic import BaseModel

class User(BaseModel):
    id: int                           # required
    nickname: Optional[int] = None    # optional — None if absent
    name: str                         # required
    email: Optional[str] = None       # optional
```

| `type` token | required | optional |
|---|---|---|
| any | `T` | `Optional[T]` (with `= None` default) |

Pydantic strict mode + `Optional[T]` accepts `None` for the optional case;
absence and explicit `null` collapse to the same Python value (`None`).
**Implication:** in Python alone, the "absent vs `null`" distinction is
lost at the model boundary — the user can't tell which one the client
sent. If that distinction matters for a feature, we need a wrapper type
(tracked as an open question).

## Nullability convention

The only accepted source-level expression for "this field's value may
be `null`" is the JSON Schema 2020-12 canonical idiom:

```json
{ "oneOf": [{"type": "<T>"}, {"type": "null"}] }
```

Order is insensitive — `[{"type":"null"}, {"type":"<T>"}]` is
equivalent. The non-null branch is a full subschema and may carry any
sibling keyword recognized for that `type`:

```json
{ "oneOf": [
    {"type": "string", "format": "email", "minLength": 5},
    {"type": "null"}
]}
```

This is a **narrow exemption to P5** — we still reject `oneOf` in the
general case; only this exact recognized shape is accepted.

### Pattern acceptance rules

The generator accepts a `oneOf` schema iff:
- exactly 2 branches;
- exactly one branch is the literal `{"type": "null"}` with no sibling
  keywords on the null branch;
- the other branch declares a recognized [[type]] (must not itself be
  `"null"` — `{type:"null"}` paired with `{type:"null"}` is a
  tautology and rejected).

Any other `oneOf` shape → reject at load time per **P5** with a
diagnostic naming the recognized nullability form.

### Required + nullable is forbidden

A field listed in the enclosing object's `required` array whose schema
matches the nullable pattern is **rejected at load time** per
**P10.2**.

Rationale: required+nullable encodes "must be present, may be `null`"
— a 2-state space (`null` or T) operationally indistinguishable from
optional+non-nullable (absent or T). Java/Go/Python collapse the
distinction at the type level anyway (see below), so we force users
to pick the simpler form. TypeScript could in principle express the
distinction (`x: T \| null` vs `x?: T`), but we keep the rule uniform
across languages.

### Per-language emitted type (optional + nullable)

Since required+nullable is forbidden, the only valid nullable shape
is **optional+nullable**: a field where the JSON value may be absent,
null, or a value of T.

| `type` token | Java | Go | TypeScript | Python |
|---|---|---|---|---|
| `"integer"` | `@Nullable Long`    | `*int64`   | `x?: number \| null`  | `Optional[int]` |
| `"number"`  | `@Nullable Double`  | `*float64` | `x?: number \| null`  | `Optional[float]` |
| `"boolean"` | `@Nullable Boolean` | `*bool`    | `x?: boolean \| null` | `Optional[bool]` |
| `"string"`  | `@Nullable String`  | `*string`  | `x?: string \| null`  | `Optional[str]` |
| `"object"`  | `@Nullable T`       | `*T`       | `x?: T \| null`       | `Optional[T]` |
| `"array"`   | `@Nullable List<T>` | `[]T` (nil = absent or null) | `x?: T[] \| null` | `Optional[list[T]]` |

(Java optional column is `@Nullable` for both the optional-non-nullable
and optional+nullable cases — the annotation tracks in-memory nullness,
not the wire distinction; see the optionality section above and
PRINCIPLES Java §2.)

**Collapse note (Java / Go / Python):** the in-memory representations
of "absent" and "JSON null" are the same (`null`, `nil`, `None`). The
validator preserves the schema-level distinction at the *boundary*
(rejection happens before the field is populated), but post-
validation user code can't recover which one came through the wire.
This matches **P10.2**'s framing — optional and nullable are distinct
*schema* concerns; runtime collapse is acceptable when the language
can't represent the difference.

TypeScript is the exception: `undefined` vs `null` gives natural
three-state, so the validator can and does enforce the distinction at
read time.

### Diagnostics

Wire form → required generator output:

| Source form | Action |
|---|---|
| `"type": ["T", "null"]` (array form) | Reject. Diagnostic suggests `oneOf: [{type:"T"}, {type:"null"}]`. |
| `{type:"T", "nullable": true}` (OAS 3.0) | Reject. Diagnostic suggests `oneOf: [{type:"T"}, {type:"null"}]`. |
| `oneOf` with `{type:"null"}` branch where field is in `required` | Reject (P10.2). Diagnostic suggests dropping from `required`. |
| `oneOf` of 3+ branches with `{type:"null"}` among them | Reject (P5). Diagnostic distinguishes from the supported 2-branch form. |

## Validator implications

Three schema states × four languages. Required+nullable is forbidden,
so the matrix has nine cells.

| State | Java | Go | TS | Python |
|---|---|---|---|---|
| **Required, non-nullable** — must be present, must be T | type is `long`/`String`/etc.; emit `field == null` reject + type binding | type is `int64`/`string`/etc.; shadow `*T` field, reject on `nil` | type is `x: T`; emit `parsed.x === undefined \|\| parsed.x === null` reject | Pydantic field with no default → strict mode raises automatically |
| **Optional, non-nullable** — absent OK, T OK, explicit `null` rejected | strict-variant custom deserializer (see strategy below) | shadow `*json.RawMessage` with explicit `bytes.Equal(*raw, []byte("null"))` reject | `parsed.x === null` rejected; `=== undefined` OK | `model_validator(mode='before')` rejects keys present with `None` |
| **Optional + nullable** — absent OK, `null` OK, T OK | type is `Long`/`String`/etc.; no extra check beyond type binding | type is `*int64`/`*string`/etc.; no extra check beyond type binding | type is `x?: T \| null`; both `undefined` and `null` accepted | `Optional[T] = None`; both forms accepted |

## Strict enforcement of optional-non-nullable

**Decision:** explicit `"key": null` is rejected when the schema is
optional-non-nullable (the JSON-Schema-default case — a `{type: "T"}`
field not listed in `required`, where T is not the nullability
pattern). This honors the spec: `null` is not a valid value of any
non-`null` type, so a bare `{type: "string"}` doesn't admit `null`.

### Java

The runtime ships two deserializer variants per primitive:

- `SpecLongDeserializer`         — accepts `null` → `null` (used for
  optional+nullable fields)
- `SpecLongStrictDeserializer`   — overrides `getNullValue(ctxt)` to
  call `ctxt.reportInputMismatch(...)`, throwing when Jackson sees
  `VALUE_NULL` (used for optional-non-nullable fields, and as a
  redundant defense for required fields)

The generator picks the annotation based on the field's nullability
declaration. Absence-vs-present is already separated by Jackson —
the deserializer only fires when a token exists.

### Go

The shadow struct uses `*json.RawMessage` for every field (not just
numeric). Per field, the generated `UnmarshalJSON`:

1. `shadow.Foo == nil`              → key absent
   (required → emit error; optional → leave field zero)
2. `bytes.Equal(*shadow.Foo, []byte("null"))` → explicit `null`
   (optional-non-nullable → emit error; nullable → accept)
3. otherwise                        → delegate to the type-specific
   runtime helper (e.g., `parseSpecInteger`)

### TypeScript

Natural three-way via `=== undefined` vs `=== null`:

```typescript
if (parsed.x === null) {
    errors.push(new ValidationError("x", "explicit null not allowed"));
} else if (parsed.x !== undefined) {
    // validate value
}
```

No runtime helper needed.

### Python

A model-level `model_validator(mode='wrap')` wraps Pydantic's
field-validation pass. It pre-scans the raw input dict for
optional-non-nullable keys present with `None`, runs the inner
handler, and combines any pre-errors with field-validation errors
into a single `ValidationError` — preserving P8 aggregation across
both sources.

Each generated model carries a `ClassVar[frozenset]` listing the
affected field names. The `ClassVar` annotation is required —
without it Pydantic treats `_NAME` as a private model attribute and
the validator can't iterate it.

```python
from typing import ClassVar, Optional
from pydantic import BaseModel, ValidationError, model_validator
from pydantic_core import InitErrorDetails, PydanticCustomError

class User(BaseModel):
    id: SpecInt
    nickname: Optional[SpecInt] = None        # optional, non-nullable
    bio: Optional[str] = None                 # optional + nullable

    _OPTIONAL_NON_NULLABLE: ClassVar[frozenset] = frozenset({"nickname"})

    @model_validator(mode="wrap")
    @classmethod
    def _reject_explicit_null(cls, data, handler):
        pre_errs = []
        if isinstance(data, dict):
            pre_errs = [
                InitErrorDetails(
                    type=PydanticCustomError(
                        "null_for_nonnullable", "explicit null not allowed"
                    ),
                    loc=(f,),
                    input=None,
                )
                for f in cls._OPTIONAL_NON_NULLABLE
                if f in data and data[f] is None
            ]
        try:
            instance = handler(data)
        except ValidationError as e:
            field_errs = [
                InitErrorDetails(
                    type=PydanticCustomError(err["type"], err["msg"]),
                    loc=err["loc"],
                    input=err.get("input"),
                )
                for err in e.errors()
            ]
            raise ValidationError.from_exception_data(
                title=cls.__name__, line_errors=pre_errs + field_errs
            ) from None
        if pre_errs:
            raise ValidationError.from_exception_data(
                title=cls.__name__, line_errors=pre_errs
            )
        return instance
```

Empirically verified (Pydantic 2.13) against `model_validate` (dict
input) and `model_validate_json` (JSON input) with identical
behavior; aggregation works across both pre-errors and Pydantic
field errors (e.g. `{"nickname": null}` with missing `id` produces
both error entries in one shot).

Why `mode='wrap'` rather than `mode='before'`: a `mode='before'`
validator that raises short-circuits Pydantic's own field validation,
breaking P8 aggregation across error sources. `mode='wrap'` lets us
run the inner handler, catch its errors, and combine.

Why not a `BeforeValidator` per field: `BeforeValidator` receives
only the value, not "was the key present" — the absent-vs-explicit-
`None` distinction is only recoverable at the dict level.

## See also

- [[type]] — emitted bare type per `type` token; this doc wraps that.
- [[required]] — owns *which* fields are optional (the JSON Schema
  side of the decision).
- [[oneOf]] — currently rejected per **P5**; the nullability open
  question may carve out a narrow exemption.
- [[PRINCIPLES.md]] — **P10.2** (optional ≠ nullable), **P12**
  (distinguish absent from zero value), **P1** (hand-written feel).
