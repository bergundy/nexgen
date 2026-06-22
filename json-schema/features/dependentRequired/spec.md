# `dependentRequired`

Source: JSON Schema 2020-12, Validation vocabulary, §6.5.4
"Validation Keywords for Objects → dependentRequired".

Conditional presence: when a trigger member is present, a set of other
members becomes required. Pure runtime cross-field assertion — no type
impact.

## Spec summary

Verbatim (2020-12 validation, §6.5.4):

> The value of this keyword MUST be an object. Properties in this object,
> if any, MUST be arrays. Elements in each array, if any, MUST be
> strings, and MUST be unique.

> This keyword specifies properties that are required if a specific other
> property is present. Their requirement is dependent on the presence of
> the other property.

> Validation succeeds if, for each name that appears in both the instance
> and as a name within this keyword's value, every item in the
> corresponding array is also the name of a property in the instance.

> Omitting this keyword has the same behavior as an empty object.

Distilled:
- `{"a": ["b","c"]}` means: if `a` is present, `b` and `c` must also be
  present. If `a` is absent, no constraint.
- Names dependencies only — it never applies subschemas (contrast
  [[dependentSchemas]], which does and is rejected per **P5**).

## Support decision

**Support:** yes — runtime assertion.

Lowers to a boundary cross-field check in every language. It does **not**
change emitted types: every member involved (trigger and dependents)
stays **optional** at the type level, because the requirement is
conditional — a member that were unconditionally required would go in
[[required]] instead.

Rationale (citing [[PRINCIPLES.md]]):
- **P7 (enforced)**: the conditional requirement is checked at the
  boundary, aggregated per **P8**.
- This is the one *conditional* object keyword that lowers cleanly:
  unlike [[dependentSchemas]] / `if`-`then`-`else` (rejected per **P5**),
  it only tests name presence, never branches on subschema validation,
  so no language needs sum-type or conditional-shape machinery.

Loader behavior:
- Value not an object → reject.
- Any value not an array of unique strings → reject.
- A trigger name or any dependent name not declared in [[properties]] →
  reject per **P10.1** (presence check on an undeclared member is
  undecidable). Diagnostic names the offender.
- A dependent name that is **also** in [[required]] → reject as
  redundant (it is unconditionally required already; the dependency is
  vacuous). Diagnostic suggests removing it from `dependentRequired`.
- A trigger name in [[required]] → **reject**: if the trigger is always
  present, its dependents are always required, so they belong in
  [[required]] directly. Keeps one canonical spelling.
- Empty object / empty arrays → accepted (vacuous).

## Type mapping

None. All involved members keep their optional emitted form (see
[[required]] / [[nullability]]); the constraint is validator-only.

## Validator mapping

Per **P7**/**P8**. For each trigger present in the instance, verify each
dependent is also present.

| Language | Strategy |
|---|---|
| Go | In `UnmarshalJSON`, after decoding the shadow, for each present trigger check each dependent's shadow `!= nil`; missing → `ValidationError{Path:dependent, Reason:"required when <trigger> present"}`, joined via `errors.Join`. |
| TypeScript | For each present trigger key, assert each dependent `!== undefined`; push `ValidationError{path, reason}`, throw `AggregateError`. |
| Python | `model_validator(mode='wrap')` reading the raw dict: for each present trigger, raise `InitErrorDetails` for each absent dependent, merged into the aggregated `pydantic.ValidationError`. Dependency map stored as a `ClassVar` constant (per PRINCIPLES Python §5). |
| Java | post-bind check over the bound object's present-field set; reject per the Java aggregation primitive (TBD, PRINCIPLES Java §5). |

## Property-testing matrix

### Accepted (positive)

| Shape | Example |
|---|---|
| Single dependency | `{type:object, properties:{a:{…},b:{…}}, dependentRequired:{"a":["b"]}}` |
| Multiple dependents | `dependentRequired:{"a":["b","c"]}` |
| Empty (no-op) | `dependentRequired:{}` |

### Rejected at load time (negative)

| Reason | Example |
|---|---|
| Not object / not arrays | `dependentRequired:[]`, `{"a":"b"}` |
| Non-string / non-unique dependents | `{"a":[1]}`, `{"a":["b","b"]}` |
| Undeclared trigger/dependent (P10.1) | trigger or dependent absent from [[properties]] |
| Dependent already in `required` | `required:["b"], dependentRequired:{"a":["b"]}` |
| Trigger in `required` | `required:["a"], dependentRequired:{"a":["b"]}` |

### Runtime fixtures (validator)

- Trigger absent → no constraint (dependents may be absent).
- Trigger present + all dependents present → OK.
- Trigger present + a dependent absent → one
  `ValidationError{path:dependent, reason}`.
- Multiple triggers each missing dependents → all reported in one shot
  (P8).

## Interactions

- **[[required]]**: unconditional counterpart. A name can't be in both
  (`required` wins; the dependency would be vacuous → load error).
- **[[properties]]**: every trigger and dependent must be declared.
- **[[dependentSchemas]]**: the subschema-applying sibling — **rejected**
  per **P5** (conditional shape doesn't lower). `dependentRequired` is
  the supported subset of conditional object logic.
- **[[nullability]]**: independent — dependency is about presence, not
  null-ness.

## Ecosystem variance

| Source dialect | Action |
|---|---|
| JSON Schema 2020-12 | Native. |
| OpenAPI 3.1 | Aligns with 2020-12. Native. |
| OpenAPI 3.0 / Swagger 2.0 | No `dependentRequired`; draft-4..7 used `dependencies` (array form ≡ this). A `dependencies` array form → accept as `dependentRequired`; the schema form → reject (maps to [[dependentSchemas]]). |
| draft-4..7 | `dependencies` (merged keyword) — split: array form supported here, schema form rejected. |

## Open questions

- None.

## See also

- [[required]] — unconditional presence.
- [[dependentSchemas]] — conditional *subschema* application (rejected,
  P5).
- [[properties]] — declares the members named here.
