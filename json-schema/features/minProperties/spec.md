# `minProperties`

Source: JSON Schema 2020-12, Validation vocabulary, §6.5.2
"Validation Keywords for Objects → minProperties".

Sets a floor on the number of members an object instance must have. A
pure runtime count assertion — no type impact. Mirror of
[[maxProperties]].

## Spec summary

Verbatim (2020-12 validation, §6.5.2):

> The value of this keyword MUST be a non-negative integer.

> An object instance is valid against "minProperties" if its number of
> properties is greater than, or equal to, the value of this keyword.

> Omitting this keyword has the same behavior as a value of 0.

Distilled:
- Counts **all** members including preserved extras.
- Omission ≡ `0` (no floor).

## Support decision

**Support:** yes — runtime assertion.

Lowers to a boundary count check; no effect on emitted types. Citing
[[PRINCIPLES.md]]: **P7**, **P8**.

Loader behavior:
- Value not a non-negative integer (honors `1.0`-as-integer + **P16**
  cap) → reject.
- `minProperties: 0` → accepted (no-op; equals omission).
- `minProperties > maxProperties` (both present) → reject
  (unsatisfiable).
- `minProperties` greater than the number of members the schema can
  ever have — i.e. a closed object ([[additionalProperties]] `false`)
  with fewer declared [[properties]] than `minProperties` → reject
  (unsatisfiable). Diagnostic names the gap.

## Type mapping

None. Constraint lives only in the validator.

## Validator mapping

Per **P7**/**P8**. The "number of properties" is the count of **distinct
member keys present on the wire**, taken at the deserialize boundary
**before** default population (**P11**) — a default-filled key is never on
the wire and does not count (see Interactions). Count the wire object as a
single number; do **not** sum a declared-fields bucket and an extras
bucket separately (case-mapping can route a key to either, and in Pydantic
the two sets overlap — verified `/tmp/pyd_minprops_probe.py`). Same
per-language strategy as [[maxProperties]] with `< min` as the failing
comparison:

| Language | Strategy |
|---|---|
| Go | count decoded members in `UnmarshalJSON` (wire keys, pre-population); `< min` → `ValidationError{Reason:"minProperties"}`, `errors.Join`. |
| TypeScript | `Object.keys(parsed).length < min` on the raw parsed wire object (before defaults applied) → push + `AggregateError`. |
| Python | `model_validator`; `len(model_fields_set) < min` — `model_fields_set` already includes extras and excludes default-filled fields, so it is the exact wire-key count; raise into aggregated `ValidationError`. |
| Java | count distinct JSON field names seen during bind (`< min`); **not** POJO fields + any-setter map summed post-bind. Reject per Java aggregation primitive (TBD). |

## Property-testing matrix

### Accepted (positive)

| Shape | Example |
|---|---|
| Floor satisfiable | `{type:object, additionalProperties:true, minProperties:1}` |
| Floor = 0 | `{type:object, properties:{a:{type:string}}, minProperties:0}` |

### Rejected at load time (negative)

| Reason | Example |
|---|---|
| Not non-negative integer | `minProperties:-1`, `minProperties:1.5`, `minProperties:"1"` |
| `> maxProperties` | `minProperties:5, maxProperties:2` |
| Unsatisfiable on closed object | `properties:{a:{…}}, additionalProperties:false, minProperties:2` |

### Runtime fixtures (validator)

- Member count `== min` → OK (≥ inclusive).
- Member count `min-1` → one `ValidationError{reason:"minProperties"}`.
- Open struct reaching the floor via extras → OK (extras count).

## Interactions

- **[[maxProperties]]**: paired bound over the same member set;
  `min > max` is a load error.
- **[[required]]**: required members count toward the floor but
  `minProperties` may demand *more* than the required set names —
  satisfiable only if extras are permitted ([[additionalProperties]]
  not `false`, or enough optional [[properties]]).
- **[[additionalProperties]] `false`**: caps how many members can exist;
  a `minProperties` above the declared count is then unsatisfiable
  (load error).
- **`default`**: `default` is an annotation, not an assertion — a
  default-filled key is never on the wire, so it does **not** count
  toward the floor. The count is taken before default population
  (**P11**); a client sending fewer than `minProperties` keys is invalid
  regardless of server-side defaults.
- **`const`** (future feature): an object-level `const` pins the exact
  member set, making `minProperties` statically decidable — a const
  object with fewer members than `minProperties` is unsatisfiable (load
  reject), mirroring the [[additionalProperties]] `false` case.
  Property-level `const` only constrains a value *if present*, so it has
  no count impact unless paired with [[required]]. Enforcement deferred
  to the `const` spec.

## Ecosystem variance

| Source dialect | Action |
|---|---|
| JSON Schema 2020-12 | Native. |
| OpenAPI 3.1 / 3.0 | `minProperties` identical. Native. |
| Swagger 2.0 / draft-4 | `minProperties` identical. Native. |

## Open questions

- None.

## See also

- [[maxProperties]] — upper bound on member count.
- [[required]], [[additionalProperties]] — interact with satisfiability.
