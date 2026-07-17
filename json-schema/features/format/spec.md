# `format`

Source: JSON Schema 2020-12, Validation, §7 "Vocabularies for Semantic
Content With 'format'".

Annotates a string with a named semantic shape (`uuid`, `date-time`,
`ipv4`, …). In 2020-12 `format` is an **annotation by default** — the
`format-annotation` vocabulary is required and merely *collects* the value;
assertion is the *opt-in* `format-assertion` vocabulary. We deliberately
**opt into assertion behavior for a curated, provably-portable subset** and
**reject every other format at load**, because an accepted-but-unenforced
`format` is exactly the "looks constrained, silently isn't" footgun **P10**
(validation is enforced, not advisory) forbids. We do **not** delegate to
any target's native format validator — those are the single most divergent
corner of JSON Schema across implementations, which is *why* the spec made
assertion optional. Instead each supported format lowers to a
**generator-owned** check: a pinned portable regex through the [[pattern]]
RE2-safe gate, plus a shared calendar predicate where a regex alone is
insufficient. [[pattern]] anticipated this and points here for the regex
route.

## Spec summary

Verbatim (2020-12 validation, §7.1 / §7.2):

> The value of this keyword is called a format attribute. It MUST be a
> string.

> [Format attributes] generally … only [apply to specific instance types].
> If the type of the instance to validate is not in this set, validation
> for this format attribute and instance SHOULD succeed.

> [Format-Annotation, required] … the "format" keyword … MUST … be
> collected as an annotation … The implementation MUST provide options to
> enable and disable such [validation] evaluation and MUST be disabled by
> default.

> [Format-Assertion, optional] … implementations MUST provide full
> validation support for all of the formats defined by this specification …
> When the Format-Assertion vocabulary is specified, implementations MUST
> fail upon encountering unknown formats.

Distilled:
- Value MUST be a **string** naming a format.
- Two vocabularies: **`format-annotation`** (required, default) collects
  `format` as a **pure annotation** and does **not** validate;
  **`format-assertion`** (optional, opt-in) **validates** and **fails on
  unknown formats**.
- A format generally targets **one instance type** (the standard set all
  target `string`); on any other type it is a **no-op** per the spec.
- Standard formats (§7.3): `date-time`, `date`, `time`, `duration`
  (§7.3.1); `email`, `idn-email` (§7.3.2); `hostname`, `idn-hostname`
  (§7.3.3); `ipv4`, `ipv6` (§7.3.4); `uri`, `uri-reference`, `iri`,
  `iri-reference`, `uuid` (§7.3.5); `uri-template` (§7.3.6);
  `json-pointer`, `relative-json-pointer` (§7.3.7); `regex` (§7.3.8).

## Support decision

**Support:** partial — a **curated portable subset**, each **asserted**
(we adopt `format-assertion` semantics for it) by lowering to a
generator-owned check. Everything outside the subset — including the
spec-default annotation-only fallthrough — is **rejected at load**
(deferred, *not* a categorical **P6** exclusion).

- **Asserted (v1):**
  - `uuid`, `ipv4`, `ipv6` — **purely syntactic**, each a single pinned
    regex, zero calendar/semantic ambiguity. Provably portable with no new
    machinery beyond the [[pattern]] gate.
  - `date-time`, `date`, `time` — **RFC 3339** profile. A pinned regex
    enforces the syntax; a **shared calendar predicate** (day-in-month +
    leap-year, and the pinned edge decisions below) enforces the semantics
    a regex cannot. Owned end-to-end, so all four targets agree
    value-for-value.
- **Deferred (rejected at load, "not yet supported"):** `email`,
  `idn-email`, `hostname`, `idn-hostname`, `uri`, `uri-reference`, `iri`,
  `iri-reference`, `uri-template`, `duration`, `json-pointer`,
  `relative-json-pointer`, `regex`. Each has a real cross-implementation
  divergence (RFC 5322 vs practical email; RFC 3986 URI parsing; IDNA) or
  is niche; admitting them needs a portable owned check we don't yet commit
  to.
- **Unknown / non-standard format** (`format: "phone"`, a typo, a custom
  string) → **reject** with a fix-it listing the supported names. We do
  **not** silently accept it as an annotation: an unrecognized format is
  the ambiguity **P7.1** rejects loudly, and `format-assertion` itself
  mandates failing on unknown formats.
- **`format` on a non-string [[type]]** (`{type:"integer", format:"uuid"}`)
  → **reject** (**P7.1**). The spec would make it a vacuous no-op; a
  statically meaningless keyword is a load reject here, exactly as
  [[pattern]] / the count keywords treat a type mismatch. (No standard
  format targets a non-string type.)

Grounding ([[PRINCIPLES.md]]): **P1** (identical cross-language accept /
reject — guaranteed by owning the check, never by a native validator),
**P4** (the regex route needs only each stdlib's regex engine, as
[[pattern]] established; the calendar predicate is plain arithmetic — no
new dependency), **P10** (enforced at the boundary), **P11** (aggregated),
**P12** (a pure predicate over the decoded value in the **shared
`Validate`** layer, identical both directions — no per-adapter logic). The
curated line is the **P1** line, mirroring [[pattern]]'s "portable subset
accepted, hazardous form rejected, deferred not excluded" and
[[multipleOf]]'s fractional-divisor deferral: we assert only where every
target provably agrees.

**Why assert rather than annotate.** The spec default (`format-annotation`)
would have us accept any `format` and never check it. That collides with
this generator's mission — a `format: "uuid"` that lets a non-UUID through
is a silent wire-contract hole (**P10**). So for the subset we own we adopt
`format-assertion` semantics; for everything else we reject at load rather
than accept-and-ignore, keeping the "no silently-incorrect output"
guarantee (**P7.1**).

**RFC 3339 edge decisions (pinned, temporal formats).** All four targets
follow these because we own the check:
- **Leap second** `:60` in the seconds field is **accepted syntactically**
  (RFC 3339 permits it); we do not attempt to verify it against a real
  leap-second table (out of scope, and unportable).
- **`date-time` offset is required** (`Z` or `±HH:MM`) per RFC 3339; a
  bare local `date-time` is invalid. `-00:00` ("unknown offset") is
  accepted.
- **Fractional seconds** are accepted at **any precision** (`.` followed by
  one or more digits); trailing precision is not normalized.
- **Case** — the `T` / `Z` separators are accepted in either case
  (RFC 3339 §5.6 NOTE), pinned identically across targets.
- Calendar validity (**`date`** and the date half of **`date-time`**)
  enforces month `01–12`, day within the month's length, and the Gregorian
  leap-year rule for February — so `2021-02-30` and `2021-13-01` are
  rejected, which a pure regex would miss.

## Type mapping

None. The emitted field type is [[type]]'s `string`; the format check lives
only in the validator. The format name is surfaced in the generated type's
**doc comment** (`// format: uuid` and analogues) so the intent survives
into the generated source (**P2**), but it changes no signature.

## Validator mapping

Per **P10** / **P11**. A single "does the value satisfy `<format>`?"
predicate, identical in both directions (shared `Validate`, **P12**).

**Regex-lowered formats** (`uuid`, `ipv4`, `ipv6`, and the *syntactic* pass
of the temporal formats) reuse [[pattern]]'s machinery wholesale: the
format lowers to a **pinned portable pattern**, compiled **once**
(module/package init) with the same P1-pinned flags [[pattern]] uses
(unanchored is irrelevant here — the pinned patterns are fully anchored;
ASCII classes; code-point `.`). Because the patterns are generator-authored
they are RE2-safe by construction — no author-supplied regex reaches the
gate. Pinned patterns:

| Format | Pinned pattern (anchored) |
|---|---|
| `uuid` | `^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$` |
| `ipv4` | dotted-quad, each octet `0–255` (no leading zeros) |
| `ipv6` | RFC 4291 (full, `::`-compressed, and IPv4-tail forms) |
| `date` / `time` / `date-time` | RFC 3339 grammar (syntax only; calendar checked separately) |

**Temporal formats** additionally run a **shared calendar predicate** over
the parsed fields (month/day/leap-year, per the pinned edge decisions
above) — plain integer arithmetic in every target, so it agrees
value-for-value without a date library. We do **not** delegate to
`time.Parse` / `LocalDate.parse` / `datetime.fromisoformat` / `Date`: their
accepted grammars and error surfaces diverge (e.g. `Date` is famously lax),
which is the exact P1 hazard the owned check exists to avoid.

| Language | Strategy |
|---|---|
| Go | Package-level `var fmtRe = regexp.MustCompile(<pinned>)` (compiled once at init); the shared `Validate` checks `if !fmtRe.MatchString(v) { push(Violation{Path, Reason: fmt.Sprintf("must be a valid %s, got %q", <format>, v)}) }`, then for temporal formats calls the shared `validRFC3339Date(...)` calendar helper. Collected into one `ValidationError`. |
| TypeScript | Module-level ``const FMT_RE = /<pinned>/u;`` (the `u` flag mandatory, as [[pattern]]). ``if (!FMT_RE.test(v)) push(Violation{path, reason: `must be a valid ${format}, got ${JSON.stringify(v)}`})``, plus the shared calendar check for temporal formats. One `ValidationError`. |
| Python | Module-level `FMT_RE = re.compile(<pinned>, re.ASCII)` and an `AfterValidator`: `if FMT_RE.search(v) is None: raise ValueError(...)` (plus the calendar helper), aggregating into `pydantic.ValidationError`. We deliberately do **not** use Pydantic's `UUID`/`datetime` types or `AwareDatetime`: they coerce/normalize (a `UUID` object, a `datetime`) and shift the wire shape, and their grammars differ from the pinned one — the same reason [[pattern]] avoids the native `pattern=`. |
| Java | Static `private static final Pattern FMT_RE = Pattern.compile(<pinned>);` (default flags — ASCII classes). The per-POJO collecting deserializer (PRINCIPLES Java §5) reads the `String`, checks `if (!FMT_RE.matcher(v).find())` (the pinned pattern is anchored) and the shared calendar helper, pushing a `Violation{path, "must be a valid " + <format> + ", got " + v}` into the single `ValidationException`. Not bean-validation `@Pattern` / `@Email`. |

**Informative `reason` strings.** The `Violation` `reason` names the
**format and the offending value** (`must be a valid uuid, got "xyz"`), per
the [[maximum]] / [[pattern]] convention — never a bare keyword.

**Why compile-once.** Identical to [[pattern]]: the pinned pattern is a
package-level compiled constant reused across calls, not recompiled per
(de)serialize. The patterns are generator-authored and RE2-safe, so the
`MustCompile` / `Pattern.compile` is unconditional.

### Serialize-side (P12)

The format check is a shared-`Validate` predicate, so it **re-runs before
emit** over the decoded value — a model constructed in memory with a
malformed string (a Go `string` / Java `String` / Python `str` set to a
non-UUID) fails serialize with the same aggregated primitive rather than
writing an invalid value. Real teeth in the statically-typed targets, where
in-memory construction is unchecked. The value is the same string in memory
as on the wire, so the check is the identical predicate in both directions —
no parse-adapter-only or encode-adapter-only logic.

## Property-testing matrix

### Accepted (positive)

| Shape | Example |
|---|---|
| `uuid` | `{type:"string", format:"uuid"}` |
| `ipv4` | `{type:"string", format:"ipv4"}` |
| `ipv6` | `{type:"string", format:"ipv6"}` |
| `date-time` | `{type:"string", format:"date-time"}` |
| `date` | `{type:"string", format:"date"}` |
| `time` | `{type:"string", format:"time"}` |
| Combined with a length bound | `{type:"string", format:"uuid", maxLength:36}` |
| Combined with `pattern` | `{type:"string", format:"uuid", pattern:"^0"}` (value must satisfy both) |
| On a nullable string | `{oneOf:[{type:"string", format:"uuid"},{type:"null"}]}` |

### Rejected at load time (negative)

| Reason | Example |
|---|---|
| Value not a string | `format:5`, `format:true`, `format:["uuid"]` |
| Type mismatch (P7.1) | `{type:"integer", format:"uuid"}`, `{type:"boolean", format:"date"}` |
| Unknown / non-standard format | `{type:"string", format:"phone"}`, `{…, format:"datetime"}` (typo) |
| Deferred standard format | `{type:"string", format:"email"}`, `{…, format:"uri"}`, `{…, format:"hostname"}`, `{…, format:"duration"}` |
| Literal fails its format | `{type:"string", format:"uuid", const:"not-a-uuid"}`, `{…, default:"nope"}` |

### Runtime fixtures (validator)

- Well-formed value → OK (both directions); malformed → one
  `ValidationError` naming the format (`must be a valid uuid, got "xyz"`).
- `uuid`: `"f81d4fae-7dec-11d0-a765-00a0c91e6bf6"` OK; wrong length /
  non-hex / missing dashes → fail. Both upper- and lower-case hex accepted.
- `ipv4`: `"192.168.0.1"` OK; `"256.0.0.1"`, `"1.2.3"`, leading-zero
  `"01.2.3.4"` → fail.
- `date-time`: `"2021-02-28T23:59:60Z"` (leap second) OK; `"2021-02-30T…"`
  (calendar) → fail; missing offset → fail; `"…T…+00:00"` and `-00:00` OK.
- `date`: `"2020-02-29"` (leap year) OK; `"2021-02-29"` → fail;
  `"2021-13-01"` → fail.
- Combined with a failing [[minLength]] / [[maxLength]], [[pattern]], or
  sibling field → **all** reported in one shot (**P11**).
- Serialize of a malformed in-memory value → rejected before emit
  (**P12**), not silently written.

*(The per-format accept/reject corpus is the verification vehicle — see
Open questions. It plays the role [[pattern]]'s `pattern_conformance/`
corpus plays there, run through all four runtimes plus the Rust gate.)*

## Interactions

- **[[pattern]]**: the general-purpose regex keyword; `format` reuses its
  RE2-safe gate and compile-once mechanism for the regex-lowered formats.
  Both may appear on one node — the value must satisfy **both**, aggregated
  independently. [[pattern]] points here for the format route; this is the
  return edge.
- **[[type]]**: gates applicability — `format` is meaningful only for
  `string`; a mismatch is a load reject (**P7.1**). The emitted type stays
  `string`.
- **[[const]] / [[default]] / [[enum]]**: a supplied string literal on the
  same node MUST satisfy the format at load (e.g. a `const`/`default` UUID
  must be a valid UUID; every `enum` member must match) — the format arm of
  the deferred literal-vs-constraint obligation, exactly as [[pattern]]
  validates a literal against the regex.
- **[[minLength]] / [[maxLength]]**: independent string assertions; all
  present keywords apply and aggregate. We do **not** cross-check a length
  bound against a format's implied fixed length (a `uuid` is always 36
  chars, but a contradictory `maxLength:10` is not caught at load) — the
  same non-check stance [[pattern]] takes on regex↔length satisfiability.
- **[[nullability]]**: orthogonal — the format constrains a *present,
  non-null* string; if the field is the nullable [[nullability]] pattern, a
  `null` skips the format check (nothing to validate), and a present string
  is checked.
- **[[required]]**: orthogonal — `required` decides presence; `format`
  shapes the value when present.

## Ecosystem variance

| Source dialect | Action |
|---|---|
| JSON Schema 2020-12 | `format-annotation` is the default (collect only); we opt into `format-assertion` for the curated subset and reject the rest. Native format names, no rewrite. |
| OpenAPI 3.1 | Adopts 2020-12 `format`; same names. Native. OAS-specific formats (`int32`, `int64`, `float`, `double`, `password`, `byte`, `binary`) are **not** JSON Schema formats — treated as unknown and rejected (numeric width is a [[type]] concern; `password`/`byte`/`binary` are deferred). |
| OpenAPI 3.0 / draft-4 | `format` present with the same intent; `date-time`/`date`/`uuid` names carry over. Non-subset formats need to await wider support. |
| Swagger 2.0 | Same as OAS 3.0. |

## Open questions

1. **Widen the asserted subset.** `email`, `hostname`, the URI/IRI family,
   and `duration` are the next candidates — each admissible once a portable,
   generator-owned check is pinned (a deliberately-simple email regex; an
   RFC 3986 URI grammar; an ISO 8601 duration regex). Gated on the format
   conformance corpus agreeing across all targets (incl. prospective
   .NET / Ruby), mirroring [[pattern]]'s subset-widening question.
2. **Build the format conformance corpus.** A per-format `(value, verdict)`
   corpus run through all four runtime engines plus the Rust gate — the
   regression guard for the pinned patterns and the calendar predicate,
   modeled on `research/pattern_conformance/`. It does not exist yet; the
   pinned patterns above are the design, the corpus is the proof.
3. **Calendar-semantic depth.** v1 pins leap-second acceptance and skips a
   real leap-second table; whether to tighten (or to add `duration`'s
   `PnW`-vs-`PnDT…` mutual-exclusion rule) is revisited on demand.
4. **Declaring `format-assertion`.** If the IDE-support JSON Schema for
   `*.nexusrpc.yaml` documents ever needs to signal that we assert, it can
   reference the `format-assertion` vocabulary in `$vocabulary`; today the
   assertion behavior is implicit in the generator.

## See also

- [[pattern]] — the regex keyword whose RE2-safe gate and compile-once
  mechanism `format` reuses; owns the regex-lowering route.
- [[type]] — supplies the emitted `string`; gates applicability to
  `string`.
- [[const]] / [[default]] / [[enum]] — supplied string literals validated
  against the format at load.
- [[minLength]] / [[maxLength]] — the other string assertions; independent,
  and not cross-checked against a format's implied length.
- [[multipleOf]] — the sibling "support the portable subset, reject the
  hazardous form, deferred not excluded" decision posture.
- [[maximum]] — the `reason`-string convention.
