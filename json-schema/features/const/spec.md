# `const`

Source: JSON Schema 2020-12, Validation vocabulary, §6.1.3
"Validation Keywords for Any Instance Type → const".

Pins an instance to a single fixed value. The discriminator primitive:
a `const` string on a member is how a typed object announces its
variant on the wire. Supported for scalar values; the in-memory type
stays the underlying primitive (**P9.1**). `const` is a **pure
assertion** — equivalent to a single-value [[enum]] — checked in both
directions by the shared `Validate` layer (**P17**). It carries **no
serialize-side special-casing**: the value reaches the wire because it
is *set in memory* (by the TS type, by a Java `final` field, by a
Python `before`-validator inject, or by the Go consumer), not because
the serializer rewrites it.

## Spec summary

Verbatim (2020-12 validation, §6.1.3):

> The value of this keyword MAY be of any type, including null.

> Use of this keyword is functionally equivalent to an "enum" (Section
> 6.1.2) with a single value.

> An instance validates successfully against this keyword if its value
> is equal to the value of the keyword.

Distilled:
- A single-value assertion: the instance must **equal** the keyword's
  value (JSON equality — by type and value).
- Equivalent to `enum: [<value>]`; see [[enum]].
- Value may be any JSON type. In our subset only **scalar** consts
  (string / number / integer / boolean) are supported; `null` and
  composite (object / array) consts are handled below.
- It is an **assertion**, not an annotation — unlike [[default]], a
  non-matching value is a hard validation failure.

## Support decision

**Support:** yes (scalar values) — a runtime equality assertion, nothing
more. `const: null` and composite consts are rejected/deferred.

Rationale (citing [[PRINCIPLES.md]]):
- **P7 (enforced)**: the equality check runs at the (de)serializer
  boundary, aggregated per **P8**. It is a pure predicate over the
  decoded value, identical in both directions — the **shared `Validate`**
  layer of **P17**, with no serialize-side adapter logic of its own.
- **P9.1 (forward-compatible const)**: the emitted field type is the
  **underlying primitive** (`string`, not the literal type `"v1"`); the
  fixed value is validated at runtime. Bumping a const value in the
  schema never breaks the generated type signature — only the runtime
  check moves. This trades static discriminated-union ergonomics for
  forward compatibility, deliberately. TS and Go both soften the DX cost
  without closing the type (see Type mapping): TS with a
  `"v1" | (string & {})` hint, Go with a named alias `type X = string` +
  typed value consts — discoverable, still any-value-assignable.
- **No auto-emit (the corrected decision).** `const` is treated exactly
  as a single-value [[enum]]: validate that the value equals the keyword
  on every model — whether constructed in-language or deserialized over
  the wire. The generator does **not** force-write the fixed value on
  serialize. The value lands on the wire because it is *set in memory*,
  and **presence is governed by [[required]]**, like every other field —
  a required+const is always present (so always emitted) for the same
  reason any required field is. Dropping force-write deletes a special
  case from every language's serializer and, more importantly, stops the
  serializer from **silently rewriting** a wrong in-memory value: setting
  `kind="admin"` on a type whose const is `"user"` now fails `Validate`
  loudly instead of being masked. The earlier "always-emit" design (and
  `/tmp/pyd_serialize_probe.py`'s omit-unset-drops-discriminator finding)
  was solving a presence problem that [[required]] already owns.

Loader behavior:
- `const` value type-incompatible with the declared [[type]] → reject
  per **P10.1** (`{type:"integer", const:"x"}` is statically
  unsatisfiable). The const value must validate against the **rest** of
  the field's own schema too (e.g. `{type:"string", minLength:5,
  const:"ab"}` → reject — the fixed value can never satisfy the field).
  **Called out, not yet fully specced (P10.4):** validating the const
  value against *constraint* keywords — `pattern`, `minLength`/
  `maxLength`, `minimum`/`maximum`, `multipleOf`, … — means running those
  keywords' own validators over the fixed value at load time. Those
  features are not specced yet, so the full check is **deferred to land
  with them**; today only `type`-compatibility is enforced here. Same
  obligation applies to [[default]] and [[enum]].
- `const` **and** [[default]] both present → reject. A const fixes the
  value; a default is then either redundant (equal) or contradictory
  (unequal). Diagnostic: drop the `default`; the const already
  determines the value.
- `const` **and** [[enum]] both present → reject as redundant (const is
  a single-value enum; pick one spelling). Diagnostic points at the
  equivalence.
- `const: null` → **reject**. A field that is *always* `null` carries no
  information — the same degenerate case as a standalone `{type:"null"}`
  (see [[type]]). If the intent is "nullable", use the [[nullability]]
  pattern; if "absent", omit the field.
- Composite const (`const` whose value is an **object or array**) →
  **temporarily unsupported**; reject at load with a "not yet supported"
  diagnostic (not a categorical P5 exclusion — see Open questions). It
  would require a deep structural-equality check on every (de)serialize;
  deferred past v1.

## Type mapping

The emitted **bare type is the underlying primitive of the const's
scalar type** (**P9.1**) — *not* a singleton/literal type. Optional vs
required wrapping is owned by [[required]] / [[nullability]].

| const value kind | Go | TypeScript | Python | Java |
|---|---|---|---|---|
| string  | `type X = string`  | `"<v>" \| (string & {})`  | `Literal["<v>"] \| str` | value class `X` |
| integer | `type X = int64`   | `<v> \| (number & {})`    | `Literal[<v>] \| int`   | `long` |
| number  | `type X = float64` | `<v> \| (number & {})`    | `float`                 | `double` |
| boolean | `type X = bool`    | `boolean` | `bool`  | `boolean` |

Notably **not** `kind: "user"` as a *closed* literal (TS), a *closed*
`Literal["user"]` (Python), or an enum singleton — all four keep any
value of the primitive assignable and enforce the fixed value in the
validator (**P9.1**). Each target instead emits an *open* form, closed
only at runtime: TS `"<v>" | (string & {})`, Go a named alias, Python the
open union `Literal["<v>"] | str`, Java a generated **value class** (it
has no structural literal type to lean on — see below). This keeps a
const *bump* ("v1"→"v2" in a new schema revision) a validator-only
change: regenerated code still compiles against the same field type, and
old typed call sites are unaffected at compile time (they fail the
runtime check against the new value, which is the correct, loud outcome —
not a type error).

**TypeScript autocomplete hint.** TS uses the `"<v>" | (string & {})`
idiom (and `<v> | (number & {})` for numerics): the `& {}` intersection
stops TS from collapsing the union back to the bare primitive, so editors
suggest the const value while *any* value of the primitive stays
assignable. This is "emit the primitive **+ a hint**" — strictly better
DX than bare `string` with the **identical** forward-compatibility
(boolean has only two values, so it needs no hint). const fields are also
emitted **`readonly`**: the value is fixed, mutating it in memory is
always a bug, and `readonly` catches that at compile time (compile-only;
no runtime or serialize effect).

**Go named alias.** Go emits a **type alias** over the primitive —
`type UserEventKind = string` — plus one typed value constant per const,
`const UserEventKindUser = UserEventKind("user")`; the field is typed
`UserEventKind`. The alias form (`=`, **not** a defined type
`type UserEventKind string`) is deliberate: it stays fully
interchangeable with the underlying `string`, so any value — including a
future/unknown one — remains assignable without a conversion (**P9.1**),
exactly mirroring the TS `string & {}` hint. It buys naming and
discoverability (the value consts group under one named type, IDE- and
doc-surfaced) without closing the type or forcing conversions at call
sites. This also generalizes cleanly to [[enum]], where the same alias
carries several value constants.

**Python open-enum hint.** Python mirrors this with an open union, named
in two parts (parallel to Go's type/value split):

```python
EventKindUser = Literal["user"]            # the specific value
EventKind     = Union[EventKindUser, str]  # the open field type
```

The field is typed `EventKind`. Because `str` is in the union, **any**
string stays assignable — including a future/unknown value (**P9.1**);
the union is what an editor reads to suggest `"user"`. Three honest
caveats, all verified in `/tmp/const_open_enum_probe.py`:
- It is **open, not closed** — to a static type checker the union is
  *semantically* just `str` (the `Literal` is absorbed), so it provides
  **no** compile-time closedness. That is exactly what we want for
  forward-compat; runtime correctness is the `model_validator`, never the
  annotation (the probe shows Pydantic accepting `"user_v2"` through the
  `str` arm when the validator is absent).
- Autocomplete of the literal is **best-effort and tool-dependent**
  (Pylance/Pyright surface it; type-only tools and `mypy` see plain
  `str`). It is a DX nicety, not a guarantee.
- **`float` consts get no hint** — `Literal` forbids float members — and
  `bool` needs none (two values), so both stay the plain primitive (see
  the type table).

**Java value class.** Java has no structural literal type, so the open
form is a generated **value class** wrapping the string — the same shape
the [[enum]] feature will emit, of which a const is the single-value
specialization. (Shown standalone for clarity; when the const is
**anonymous** on a property it is emitted **nested** as
`UserEvent.Kind` — see Naming and collisions below.)

```java
public final class UserEventKind {
    public static final UserEventKind USER = new UserEventKind("user");  // the known value
    private final String value;
    private UserEventKind(String value) { this.value = value; }

    @JsonCreator                              // deserialize: string -> instance
    public static UserEventKind fromString(String v) {
        if (v == null) return null;
        return switch (v) {
            case "user" -> USER;
            default -> new UserEventKind(v);  // open: any value representable (P9.1)
        };
    }
    @JsonValue public String getValue() { return value; }   // serialize: instance -> string
    public boolean isUnrecognized() { return this != USER; }
    // equals/hashCode/toString by value (omitted)
}
```

The class is **open** — `fromString` represents any wire value, so a
future/unknown one round-trips at the type level, exactly like the other
three targets. const and [[enum]] share this class and differ **only in
the validator**: const is *closed*, so its check **rejects** an
unrecognized value (`!UserEventKind.USER.equals(v)` → aggregated const
error); [[enum]] is *forward-compatible* (**P9**), so it **preserves**
the unrecognized instance. Jackson maps the field via `@JsonCreator` /
`@JsonValue`, so the wire form stays the bare string. Only **string**
consts use the value class; `integer`/`number`/`boolean` consts stay the
plain primitive (a value class buys nothing there), matching the type
table. This is heavier than a bare `String` field for a single-value
const, accepted for cross-language and [[enum]] consistency.

### Naming and collisions (P18)

A scalar `const` synthesizes identifiers that do not exist in the input
schema and so can collide with declared types or other synthesized names.
**Type-name derivation follows the [[properties]] resolved policy:** reuse
the `$defs` name when the const is a **named** definition; when it is
**anonymous** (inline on a property), **nest the synthesized type inside
its enclosing model where the language allows it**, so it leaves the
package/module namespace. Per-target:

| Target | Synthesized identifier(s) | Placement / scope |
|---|---|---|
| Java | value class `Kind` (member `USER` class-scoped) | **nested** `UserEvent.Kind` (verified `/tmp/nestprobe/java`) |
| Python | `Kind = Union[…]` + the `Literal[…]` arm; `KIND_CONST` value | **nested** `ClassVar` on the model (`/tmp/nestprobe/pynest.py`) |
| TypeScript | only the validator's `KIND_CONST` constant (the type is inline `"v" \| (string & {})`) | module |
| Go | type alias `UserEventKind` **+** value const `UserEventKindUser` | **flat package** (Go has no nested types — `/tmp/nestprobe/nest.go`); P18 backstop |

Per **P18** every synthesized name still enters the **same per-scope
namespace** as the declared names and as one another; the generator runs
a single collision pass (after case-mapping) and **rejects at load** with
a fix-it diagnostic on any coincidence. Nesting **shrinks** that surface
(a nested `UserEvent.Kind` cannot clash with a top-level `UserEventKind`,
verified) but does not remove the pass — **Go especially** still composes
a flat package-level `UserEventKind` that can collide with a declared
type, caught and rejected, resolvable via the [[properties]] `x-go-name`
override on the declaring member. **No auto-mangling** — a synthesized
`EventKind2` would be unstable across schema revisions (P9).

**Java value class — two surfaces.** The shared value class (Type
mapping above) collides on two levels, both under **P18**: its **name**
(`UserEventKind`) is *package*-scoped (vs declared types / other value
classes), and its **value constants** are *class-body*-scoped. A scalar
const has exactly one member (`USER`), so it can never self-collide on
the second surface — but [[enum]] synthesizes **many** members into one
class and is the first feature to exercise it: two enum values that
case-map to the same Java identifier (`"user"` + `"USER"` → both `USER`)
are a **hard compile error** (verified `/tmp/javacollide` Case A) → load
reject. The class-body pass only has to police **member-vs-member**: the
fixed scaffolding (`value` field, `fromString`/`getValue`/
`isUnrecognized` methods) does *not* constrain member names, because
members are UPPER_SNAKE while the scaffolding is lowerCamel (Case B
compiles) and Java permits a same-name field+method anyway (Case C,
unlike Go's hard field/method clash). So the class-body collision pass is
just the [[properties]] case-mapping collision policy applied within the
value class. [[enum]] inherits all of this unchanged (a const is its
single-value specialization).

## Validator mapping

Per **P7**/**P8**. A single equality check against the fixed value,
identical in both directions (it is a pure predicate over the decoded
value — the **shared `Validate`** layer of **P17**).

| Language | Strategy |
|---|---|
| Go | In `UnmarshalJSON`, after decoding, compare the field to the typed value constant (`if v != UserEventKindUser { … ValidationError{Path, Reason:"const"} }`), `errors.Join`. Emitted as `type UserEventKind = string` + `const UserEventKindUser = UserEventKind("user")` — the typed const is also the idiomatic way to set it (`UserEvent{Kind: UserEventKindUser}`). |
| TypeScript | `if (v !== KIND_CONST) push(ValidationError{path, reason:"const"})`, throw `AggregateError`. Fixed value emitted as `const KIND_CONST = "user"`. |
| Python | a field/`model_validator` checking `== <const>`, raising `InitErrorDetails` into the aggregated `pydantic.ValidationError`. Field typed as the **open** union `Literal["user"] \| str` (`EventKind = Union[EventKindUser, str]`), **not** a *closed* `Literal["user"]` — a closed `Literal` would make a const *bump* a type-level break, against **P9.1**; the open union keeps any str assignable and only hints the value (see Type mapping). |
| Java | the field is the generated value class (`UserEventKind`), whose own `@JsonCreator fromString` converts the wire string. The per-POJO collecting deserializer (PRINCIPLES Java §5) reads the node, builds the value, checks `UserEventKind.USER.equals(v)` (equivalently `!v.isUnrecognized()`), and pushes a `Violation` on mismatch into the single `ValidationException`. The known value is the `public static final UserEventKind USER` constant. (For `integer`/`number`/`boolean` consts the field is the plain primitive and the check is a bare `==`.) |

### Serialize-side (P17)

There is **no const-specific serialize logic**. `const` rides the same
encode path as every other field: a set field is emitted, an unset
optional field is omitted. Presence is owned entirely by [[required]] —
a required+const is always present (so always emitted), an optional+const
emits iff the consumer opted it in. The fixed value reaches the wire
because each language guarantees it is *set in memory*, never because the
adapter rewrites it:

| declaration | in-memory | serialize |
|---|---|---|
| required + const | always set (by type / `final` / `before`-inject / consumer) | emitted by the normal encode path |
| optional + const | present iff the consumer opts the member in | emitted **if set**, omitted if unset (normal omit-unset) |

How each language guarantees "set in memory" — and validates on the way
in:

| Language | Mechanism |
|---|---|
| Go | Plain field typed as the alias (`Kind UserEventKind`), no force-write. The consumer sets it idiomatically via the typed value constant: `UserEvent{Kind: UserEventKindUser}` (`type UserEventKind = string`; `const UserEventKindUser = UserEventKind("user")`). A forgotten field is the zero value (`""`), which the shared `Validate` rejects **loudly** on serialize — consistent with how Go treats every required field (no compile enforcement; validation catches it). No `readonly` exists in Go, so `Validate` is the whole guard. optional+const uses `*UserEventKind`+`,omitempty`, validated when non-nil. |
| TypeScript | `readonly kind: "user" \| (string & {})`. Required+const is always set by the type, emitted by the normal `serializeX`; the validator rejects a non-const value (and `readonly` blocks in-memory mutation at compile time). optional+const emits when not `undefined`. No unconditional literal write. |
| Python | The generated `@model_serializer(mode='wrap')` keeps **only** `model_fields_set` — **no** `const_fields` union. A `model_validator(mode='before')` injects the value when absent (`data[field]=<const>`), which makes it *provided* → in `model_fields_set` → emitted by the normal keep-set; and enforces `==` when present. The field carries **no** Pydantic `default` (a default isn't in `model_fields_set` and would be dropped). Verified end-to-end in `/tmp/const_fields_set_probe.py`: before-inject lands in `model_fields_set`, the const emits under plain `to_json` (the **default Temporal converter** path), a wrong value is rejected, and a genuinely-absent optional field stays omitted. |
| Java | `private final UserEventKind kind = UserEventKind.USER;` initialized to the known constant, getter only, **no setter**. The field is always `USER`, so Jackson's getter (via `@JsonValue`) emits `"user"` by the normal path — this is *not* force-write, the field simply cannot hold a wrong value. On the way in, the per-POJO collecting deserializer (PRINCIPLES Java §5) reads `kind`, checks `UserEventKind.USER.equals(v)`, and pushes a `Violation` on mismatch into the single `ValidationException`. The `final`-initializer is required+const only; optional+const is a normal nullable field, validated if non-null. **No builders** — they are a model-wide decision, deferred, and const does not justify them. |

The serialize equality check has teeth only where a wrong value can be
set in memory before emit: an optional+const set to a wrong value, a Go
zero-value/mutated field, or a Python `model_construct` bypass. In TS and
Java required+const the value cannot be wrong in memory (type / `final`),
so the check is effectively a deserialize-direction guard there.

## Property-testing matrix

### Accepted (positive)

| Shape | Example |
|---|---|
| String discriminator | `{type:"string", const:"user"}` |
| Integer const | `{type:"integer", const:1}` |
| Boolean const | `{type:"boolean", const:true}` |
| Number const (`1.0` ≡ integer-valued) | `{type:"number", const:1.5}` |

### Rejected at load time (negative)

| Reason | Example |
|---|---|
| Type-incompatible (P10.1) | `{type:"integer", const:"x"}` |
| Fails own subschema — *constraint check, deferred (P10.4)* | `{type:"string", minLength:5, const:"ab"}` |
| With `default` | `{type:"string", const:"v1", default:"v1"}` |
| With `enum` (redundant) | `{type:"string", enum:["a"], const:"a"}` |
| `const: null` (degenerate) | `{type:"null", const:null}` |
| Composite const (deferred) | `{type:"object", const:{a:1}}`, `{type:"array", const:[1]}` |
| Synthesized-name collision (P18) | Go flat `UserEventKind` (anonymous const) ⨯ a declared top-level `UserEventKind`; a `$defs`-named const reusing an existing type name; two consts colliding on one value-const name. (Nesting removes the Java/Python anonymous case; Go stays flat → still caught.) |

### Runtime fixtures (validator)

- Wire value equals const → OK (both directions).
- Wire value present but `!= const` → one
  `ValidationError{reason:"const"}`.
- required+const **absent on the wire** → required violation (see
  [[required]]), reported as a presence error, not a const error.
- Serialize of a correctly-set required const → the fixed value on the
  wire (TS/Java cannot be wrong; Python before-inject guarantees it; Go
  set via the value constant).
- Serialize of a Go zero-value / bypassed required const (`Kind == ""`)
  → rejected **loudly** by `Validate`, not silently rewritten — the key
  behavioral change from the dropped auto-emit.
- Serialize after mutating an optional+const to a wrong value → rejected
  before emit (**P17**).

## Interactions

- **[[enum]]**: `const` ≡ a single-element `enum` (spec §6.1.3). We
  reject the two together; `const` is the canonical spelling for the
  one-value case, [[enum]] for the multi-value case. They share the
  emitted open representation per language (TS `… | (string & {})`, Go
  alias, Python `Literal[…] | str`, Java the value class) — a const is
  the single-value specialization. Forward-compat handling (preserve
  unknown values, **P9**) is where they **diverge in the validator**, not
  the type: an *unknown enum* value is preserved, an *off-const* value is
  a hard reject — const is a closed contract. (In Java this is literally
  the same value class with `isUnrecognized()`; enum keeps an unrecognized
  instance, const rejects it.)
- **[[type]]**: the const value must be assignable to the declared type;
  mismatch is a load-time reject (**P10.1**). The emitted type is
  `type`'s primitive mapping, not a literal (**P9.1**).
- **[[required]]**: owns presence entirely. required+const is always set
  in memory (so always emitted) — the discriminator — for the same reason
  any required field is; optional+const is validated-if-present and
  emit-if-set. const itself adds no serialize behavior; it only asserts
  the value.
- **[[default]]**: mutually exclusive (load reject). `const` fixes the
  value; `default` supplies one for absence — combining them is
  redundant or contradictory. The two sit at opposite ends: a required
  `const` is always present and asserted; a `default` value is *off the
  wire* (omit-unset) and only materialized on read. That opposition is
  exactly why they don't co-occur.
- **[[oneOf]] / discriminated unions** (future): a per-branch `const` on
  a shared member name is the natural discriminator a future tagged-union
  feature would key on. `const` specifies the *value* contract today;
  the union *dispatch* convention is deferred to that feature. The
  **P9.1** "emit primitive, not literal" rule is what lets a discriminator
  value be bumped without breaking branch types.
- **[[minProperties]] / [[maxProperties]]**: an **object-level** const
  would pin the exact member set, making the count statically decidable
  (noted in both specs) — but object-level const is deferred here (see
  Open questions), so that interaction is dormant in v1. A
  **property-level** const only constrains a value if present; it affects
  the count only when paired with [[required]].
- **[[nullability]]**: orthogonal. `const: null` is rejected (degenerate);
  a nullable field that must equal a non-null const is just that scalar
  const (nullability adds nothing). 

## Ecosystem variance

| Source dialect | Action |
|---|---|
| JSON Schema 2020-12 | Native (scalar). Native. |
| OpenAPI 3.1 | Adopts 2020-12 — `const` native. |
| OpenAPI 3.0 | No `const` keyword; the idiom is `enum: [<value>]`. A single-element `enum` → accept as the equivalent const (single-value [[enum]] handling). |
| Swagger 2.0 / draft-4 | No `const` (draft-6+); single-element `enum` → same as OAS 3.0. |

## Open questions

1. **Composite (object/array) const.** Currently rejected as
   "temporarily unsupported." Lowering is mechanical — emit the
   structural type and validate by **deep structural equality** against
   the fixed value in the shared `Validate` (same validate-only model as
   scalar const; no force-write) — but the deep-equals cost and the
   rarity of composite discriminators put it past v1. Revisit on demand.
   (Contrast
   [[default]], which explicitly avoids deep-equals; for `const` the
   deep-equals is a genuine assertion, not an omission heuristic, so it
   would be correct, just costly.)

## See also

- [[enum]] — the multi-value sibling; `const` ≡ single-element enum.
- [[properties]] — owns the identifier case-mapping + collision/escape-hatch
  policy that governs const's synthesized type/value-const names (P18).
- [[type]] — supplies the emitted primitive type; gates value
  compatibility.
- [[required]] — owns presence; a required+const is the always-present
  discriminator (because it is required, not because const says so).
- [[default]] — the semantic opposite (off-the-wire/omit-unset vs
  always-present-and-asserted); mutually exclusive with `const`.
- [[nullability]] — `const: null` rejected; otherwise orthogonal.
- [[oneOf]] — future discriminated-union dispatch keys on `const`.
- [[minProperties]] / [[maxProperties]] — object-level const (deferred)
  would make counts static.
