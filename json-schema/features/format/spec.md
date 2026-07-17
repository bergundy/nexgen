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
assertion optional. Each supported format lowers to a **generator-owned**
check: a pinned portable regex through the [[pattern]] RE2-safe gate, plus —
where a regex alone is insufficient — a shared **length guard** or **calendar
predicate**, all plain arithmetic.

Beyond validation, the temporal formats (`date-time`, `date`, `time`,
`duration`) are **materialized** as idiomatic typed model fields (Go
`time.Time`, Java `OffsetDateTime`, Python `datetime`, …) rather than a bare
`string` — see **Materialization (type mapping)**. That is the one place
`format` departs from a pure assertion: the field carries a language-native
value, and the wire is produced by **re-serializing** it to a **pinned
canonical form**. Every rule below was verified value-for-value across all
four runtime targets **plus** the Rust gate and the prospective .NET / Ruby
targets by the corpora under `research/format_conformance/`,
`research/format_email/`, `research/format_hostname/`,
`research/format_duration/`, `research/format_uri/`,
`research/format_materialize_clock/`, and
`research/format_materialize_duration/`.

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

**Asserted (v1)** — grouped by the shape of the owned check:

- **Pinned regex only** (the syntax fully captures validity):
  - `uuid` — RFC 4122 8-4-4-4-12 hex.
  - `ipv4` — dotted-quad, each octet `0–255`, no leading zeros.
  - `ipv6` — RFC 4291 (full, `::`-compressed, and IPv4-tail forms).
  - `uri`, `uri-reference` — RFC 3986, ASCII-only, at high fidelity
    (`research/format_uri/`, 67 pairs, 7/7 agree). One documented gap: the
    IP-literal host is checked structurally, not semantically (below).
- **Pinned regex + a length guard** (RE2 has no total-length lookahead, so
  a cheap `code_point_count` check rides alongside the regex in the shared
  `Validate`):
  - `hostname` — RFC 1123 LDH labels; each label ≤63, **total ≤253**
    (`research/format_hostname/`, 41 pairs, 7/7 agree).
  - `email` — a well-defined **ASCII dot-atom** subset of RFC 5321 (no
    quoted locals, comments, IP-literals, or IDN); **total ≤254**, the
    guard runs *before* the regex to neutralize a Java matcher hazard
    (`research/format_email/`, 56 pairs, 7/7 agree).
- **Pinned regex (syntax) + a shared calendar/range predicate, and
  materialized** (below): `date`, `date-time`, `time`, `duration` — RFC 3339
  profile; the predicate enforces day-in-month, the Gregorian leap-year
  rule, and the offset numeric range
  (`research/format_conformance/`, 124 pairs, 7/7 agree).

**Deferred (rejected at load, "not yet supported"):** `idn-email`,
`idn-hostname`, `iri`, `iri-reference` (all need **IDNA / Unicode** handling
that diverges across engines — WHATWG punycodes, Ruby ASCII-rejects; the
asserted set is deliberately ASCII-only, the portable line); `uri-template`
(RFC 6570 templating grammar), `json-pointer`, `relative-json-pointer`, and
`regex` (niche; `regex` would additionally mean running the [[pattern]] gate
over the *value*). Each is deferred, *not* a categorical **P6** exclusion.

**Unknown / non-standard format** (`format: "phone"`, a typo, a custom
string) → **reject** with a fix-it listing the supported names. An
unrecognized format is the ambiguity **P7.1** rejects loudly, and
`format-assertion` itself mandates failing on unknown formats.

**`format` on a non-string [[type]]** (`{type:"integer", format:"uuid"}`)
→ **reject** (**P7.1**). The spec would make it a vacuous no-op; a
statically meaningless keyword is a load reject here, exactly as
[[pattern]] / the count keywords treat a type mismatch.

Grounding ([[PRINCIPLES.md]]): **P1** (identical cross-language accept /
reject **and** identical wire bytes — guaranteed by owning the check and the
canonical serializer, proven by the corpora, never by a native validator),
**P2** (a typed field is the idiomatic, hand-written-feeling shape — the
motivation for materialization), **P4** (only each stdlib's regex engine and
temporal types — no new dependency), **P10** (enforced at the boundary),
**P11** (aggregated), **P12** (see Validator mapping). The curated line is
the **P1** line, mirroring [[pattern]]'s "portable subset accepted, hazardous
form rejected, deferred not excluded".

**Materialization narrows two grammars, node-scoped.** Materializing a
temporal into a native type means the native type must be able to *hold* the
value, which the full RFC 3339 / ISO 8601 grammar does not always allow. So a
**materialized** node asserts a **narrower** grammar than a **string-opt-out**
node (below):
- **Leap second `:60` is rejected** on a materialized `date-time` / `time`
  node. No stdlib temporal type can store `:60`; every native parser rejects
  it and **Ruby silently clamps** `:60`→`:59` (corruption). Rejecting it at
  validation, uniformly, is the only portable choice. A **string-opt-out**
  node keeps accepting `:60` (the current pure-assertion contract).
- **`duration` is narrowed to a time-only duration** — `PT`-forms of hours /
  minutes / seconds only. The calendar components (years, months, weeks,
  days) are **rejected** on a materialized node, because no stdlib
  fixed-duration type (`time.Duration`, `timedelta`, `java.time.Duration`,
  `TimeSpan`) can represent calendar-variable years/months without a
  reference date (`research/format_materialize_duration/`). A
  **string-opt-out** node keeps the full duration grammar.

Both narrowings are strictly *more* restrictive (no previously-rejected value
becomes accepted) and are the price of the idiomatic typed field.

**RFC 3339 edge decisions (pinned, temporal formats).** All targets follow
these because we own the check:
- **`date-time` offset is required** (`Z` or `±HH:MM`); a bare local
  `date-time` is invalid. `-00:00` is accepted. **`time` offset is optional**
  (RFC 3339 `partial-time`); an offset, when present, is range-checked.
- **Offset range** is enforced by the predicate: hours `00–23`, minutes
  `00–59`, so `+24:00` / `+01:60` are rejected.
- **Case** — `T` / `Z` separators are accepted in either case (RFC 3339
  §5.6). Materialized nodes **uppercase on the parse path** before the native
  parse (Go / Python / Ruby native parsers reject lowercase; safe because the
  grammar has no other letters).
- **Calendar validity** (`date`, and the date half of `date-time`) enforces
  month `01–12`, day within the month's length, and the Gregorian leap-year
  rule.
- **Leap second** — see the narrowing above: **rejected** on materialized
  nodes, **accepted** on string-opt-out nodes.

**Edge decisions for the string-shaped formats** (pinned, corpus-proven):
- **`hostname`**: a **trailing dot** is **rejected** (note `ajv` accepts it);
  an **all-numeric label / TLD** is **accepted** (RFC 1123's note is not
  RE2-expressible; documented residual risk); `xn--` A-labels pass as LDH
  (Punycode is `idn-hostname`, deferred).
- **`email`**: ASCII dot-atom local, single `@`, `hostname`-style domain of
  **≥2 labels** (`user@localhost` rejected). Quoted locals, comments,
  IP-literals, trailing dots, Unicode rejected. The **≤254 guard precedes the
  regex**: `java.util.regex` matches the nested dot-atom quantifier
  recursively and throws `StackOverflowError` on multi-thousand-char runs;
  the cap keeps every engine safe (RE2 engines are already linear/immune).
- **`uri` / `uri-reference`**: RFC 3986 faithful for scheme, percent-encoding
  (`%HEXDIG HEXDIG` only), the authority/path split, and ASCII-only
  enforcement. **One deliberate gap:** the IP-literal host `[…]` is validated
  *structurally*, so `http://[1::2::3]` (double `::`) is accepted; bounded,
  closable later by splicing in `ipv6`'s grammar.

## Materialization (type mapping)

The temporal formats carry a **typed model field**; the rest stay `string`.
The typed value is **authoritative** (authority model B): the parse path
turns the validated wire string into it, and the serialize path re-emits it
as a **pinned canonical string**. **P1 is preserved not by round-tripping the
original bytes but by every language emitting the identical canonical bytes**
— which the round-trip corpora prove. Where a language lacks a suitable
native type, the field stays a `string` **holding the canonical form** (the
parse adapter canonicalizes on ingest), so it emits the same bytes as the
materializing languages.

| Format | Go | Java | Python | TS | Canonical wire (identical in all targets) |
|---|---|---|---|---|---|
| `date-time` | `time.Time` | `OffsetDateTime` | `datetime` (aware) | `Date` | `YYYY-MM-DDTHH:MM:SS.sssZ` (UTC-normalized, ms floor, always `Z`, always 3 frac digits) |
| `date` | `time.Time`† | `LocalDate` | `date` | `string`‡ | `YYYY-MM-DD` (lossless) |
| `time` | `time.Time`† | `LocalTime` | `time` | `string` | `HH:MM:SS.sss` (offset **dropped**, ms floor) |
| `duration` | `time.Duration` | `Duration` | `timedelta` | `string` | `PTnHnMnS` (time-only; omit zero components; `PT0S` for zero) |
| `uuid` / `ipv4` / `ipv6` / `hostname` / `email` / `uri` / `uri-reference` | `string` | `string` | `String` | `string` | verbatim (no materialization) |

† Go has no date-only / time-of-day type; `time.Time` carries a phantom
time-of-day / date that the canonical serializer ignores. ‡ TS has no
date-only type — `Date` is a UTC **instant**, so a materialized TS `date`
would misread under a local-timezone `getHours()`; `date` therefore stays a
(canonical) `string` in TS specifically.

**The cost of materialization (model B), consistent across languages:**
- **`date-time` folds the offset to UTC and floors to milliseconds** — the
  wire changes on round-trip (`…T12:30:45+02:00` → `…T10:30:45.000Z`). Same
  instant, different bytes; the original offset and sub-ms precision are
  gone. This is the "consistent truncation" accepted for the idiomatic field.
  The millisecond floor is forced by JS `Date` (the least-capable
  materializer, whose `toISOString()` dictates the canonical form).
- **`time` drops the offset**, which can **merge distinct values**
  (`12:30:45+02:00` and `12:30:45-05:00` both → `12:30:45.000`). `time`
  materializes only in Go / Java / Python; TS / Ruby keep the canonical
  `string` (no time-of-day type).
- **`duration` canonicalizes** non-canonical inputs (`PT90M` → `PT1H30M`,
  `PT3600S` → `PT1H`) — consistent across languages. `duration` materializes
  natively in Go / Java / Python; TS keeps the canonical `string` (no stdlib
  duration type).

**String opt-out (authority model A).** A node may opt out of materialization
and keep a **verbatim `string`** in *every* language (byte-exact round-trip,
offset and precision preserved, `:60` and calendar durations accepted), with
an optional derived accessor (`asDateTime()` / `AsOffsetDateTime()` /
`.as_datetime()`) that parses on demand. Use it where the sender's exact
offset / sub-ms precision is contractually significant. The opt-out is
per-node (and available as a generator-wide mode); it keeps the *wider*
(pre-narrowing) grammar.

**Doc comment.** The materialized field's doc comment names the format and
the canonical behavior (`// format: date-time — UTC, millisecond precision`)
so the lossy round-trip is visible in the generated source (**P2**).

## Validator mapping

Per **P10** / **P11**. For a **string-shaped format** (`uuid`, `ipv4`,
`ipv6`, `hostname`, `email`, `uri`, `uri-reference`, and any opt-out
temporal) the check is a single predicate over the decoded `string`,
identical in both directions (shared `Validate`, **P12**): the pinned regex
compiled **once** ([[pattern]]'s machinery — the ASCII-class rule and the
per-target end-anchor `$`→`\Z`/`\z` normalization apply), plus the length
guard for `hostname` / `email`. Pinned patterns (written `^…$`; **emitted**
with the normalized anchors):

| Format | Pinned pattern / source | Auxiliary check |
|---|---|---|
| `uuid` | `^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$` | — |
| `ipv4` | `^(?:(25[0-5]\|2[0-4][0-9]\|1[0-9][0-9]\|[1-9]?[0-9])\.){3}(25[0-5]\|2[0-4][0-9]\|1[0-9][0-9]\|[1-9]?[0-9])$` | — |
| `ipv6` | RFC 4291 — see `research/format_conformance/` (authoritative form) | — |
| `hostname` | `^[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)*$` | length ≤253 |
| `email` | ASCII dot-atom — see `research/format_email/` | length ≤254 (runs **first**) |
| `uri` / `uri-reference` | RFC 3986 ASCII — see `research/format_uri/pinned_body.json` | — |

For a **materialized temporal**, the pinned regex + calendar/range predicate
run in the **parse adapter over the wire string** (that is where `:60`,
offset, and precision are still observable) — a value that fails is one
aggregated `Violation`; a value that passes is then **uppercased and parsed
into the native construct** (UTC-normalized / offset-dropped / floored to ms
per the table). Pinned temporal patterns (the materialized, `:60`-rejecting
grammar):

| Format | Pinned pattern (materialized node) |
|---|---|
| `date` | `^[0-9]{4}-(0[1-9]\|1[0-2])-(0[1-9]\|[12][0-9]\|3[01])$` + calendar predicate |
| `time` | `^([01][0-9]\|2[0-3]):[0-5][0-9]:[0-5][0-9](\.[0-9]+)?(Z\|[+-]([01][0-9]\|2[0-3]):[0-5][0-9])?$` (offset optional; no `\|60`) |
| `date-time` | full-date `T` full-time, **offset required**, no `\|60` + calendar + range |
| `duration` | `^PT(?:[0-9]+H(?:[0-9]+M(?:[0-9]+S)?)?\|[0-9]+M(?:[0-9]+S)?\|[0-9]+S)$` (time-only) |

*(A string-opt-out temporal node keeps the wider grammar: `time` / `date-time`
add back the `|60` seconds alternative; `duration` uses the full
`PnYnMnDTnHnMnS` / `PnW` grammar from `research/format_duration/`.)*

| Language | Strategy (materialized temporal) |
|---|---|
| Go | Parse adapter: run the pinned regex + `validRFC3339(...)` over the wire string, pushing a `Violation` on failure; else `t, _ := time.Parse(RFC3339, strings.ToUpper(s))` → store `t.UTC().Truncate(time.Millisecond)` (`date-time`), or `time.Parse("2006-01-02", s)` (`date`); `duration` parses the `PT…` components into a `time.Duration`. Encode adapter: format the canonical string. `regexp.MustCompile` compiled once at init. |
| TypeScript | Parse adapter: pinned regex (`/…/u`) + calendar/range check, then `new Date(s)` (`date-time`); `date` / `time` / `duration` store the **canonicalized string** (no native type). Encode adapter: `date-time` → `.toISOString()`; others emit the stored canonical string. |
| Python | Parse adapter (an `AfterValidator` / model hook): regex + calendar over the wire string, then `datetime.fromisoformat(s.upper()).astimezone(utc)` floored to ms (`date-time`), `date.fromisoformat(s)` (`date`), `time.fromisoformat` (`time`), or parse `PT…` into a `timedelta` (`duration`). Encode: canonical via `@model_serializer`. We do **not** use Pydantic's native `datetime` coercion (it accepts a missing offset and normalizes differently). |
| Java | The per-POJO collecting deserializer (PRINCIPLES Java §5): regex + calendar over the `String`, then `OffsetDateTime.parse(s).atOffset(UTC).truncatedTo(MILLIS)` (`date-time`), `LocalDate.parse` / `LocalTime.parse`, or `Duration.parse` for the `PT…` form. The `Serializer` emits the **generator-owned** canonical string — **not** `Duration.toString()` for `.NET` parity and **not** the BCL serializer (`.NET XmlConvert` rolls `PT24H`→`P1D`). |

**Informative `reason` strings.** The `Violation` `reason` names the
**format and the offending value** (`must be a valid date-time, got "…"`),
per the [[maximum]] / [[pattern]] convention.

**Why compile-once.** As [[pattern]]: the pinned pattern is a package-level
compiled constant; the load gate proves it compiles, so the emitted
`MustCompile` / `Pattern.compile` is unconditional.

### Serialize-side (P12)

- **String-shaped formats:** the predicate **re-runs before emit** over the
  decoded string, so an in-memory value set to a non-UUID (etc.) fails
  serialize with the same aggregated primitive — real teeth in the
  statically-typed targets. The check is direction-agnostic.
- **Materialized temporals:** the model field is a **native type that cannot
  hold an invalid value** (a `time.Time` is always a valid instant; a
  `time.Duration` always a valid duration), so the type system replaces the
  serialize-side validator — there is no invalid state to catch. Serialize is
  therefore a pure **canonicalization** (typed → canonical wire), the one
  place `format` has genuine encode-adapter logic. The only parse-side guard
  beyond validation is a **duration overflow check** (the regex caps no digit
  count, so an adversarial `PT999999999999H` that overflows the native type
  pushes a `Violation`).

## Property-testing matrix

### Accepted (positive)

| Shape | Example |
|---|---|
| `uuid` / `ipv4` / `ipv6` | `{type:"string", format:"uuid"}` |
| `date-time` / `date` / `time` | `{type:"string", format:"date-time"}` (materialized) |
| `duration` (time-only) | `{type:"string", format:"duration"}` → accepts `PT1H30M`, `PT0S` |
| `hostname` / `email` | `{type:"string", format:"hostname"}` |
| `uri` / `uri-reference` | `{type:"string", format:"uri"}` |
| Combined with `pattern` | `{type:"string", format:"uuid", pattern:"^0"}` (value must satisfy both) |
| On a nullable string | `{oneOf:[{type:"string", format:"uuid"},{type:"null"}]}` |
| String opt-out keeps the wider grammar | opt-out `date-time` accepts `…T23:59:60Z`; opt-out `duration` accepts `P1Y` |

### Rejected at load time (negative)

| Reason | Example |
|---|---|
| Value not a string | `format:5`, `format:true`, `format:["uuid"]` |
| Type mismatch (P7.1) | `{type:"integer", format:"uuid"}`, `{type:"boolean", format:"date"}` |
| Unknown / non-standard format | `{type:"string", format:"phone"}`, `{…, format:"datetime"}` (typo) |
| Deferred standard format | `{…, format:"idn-email"}`, `{…, format:"iri"}`, `{…, format:"uri-template"}`, `{…, format:"regex"}` |
| Materialized narrowing: leap second | materialized `{…, format:"date-time"}` with `const:"2021-12-31T23:59:60Z"` |
| Materialized narrowing: calendar duration | materialized `{…, format:"duration"}` with `const:"P1Y"` / `"P4W"` / `"P1D"` |
| Literal fails its format | `{…, format:"uuid", const:"not-a-uuid"}`, `{…, default:"nope"}` |

### Runtime fixtures (validator + round-trip)

Per-format accept/reject is exercised by the conformance corpora
(`research/format_conformance/` 124, `format_email/` 56, `format_hostname/`
41, `format_duration/` 68, `format_uri/` 67); the materialization round-trips
by `research/format_materialize_clock/` and
`research/format_materialize_duration/`. Representative cases:

- **String formats** — `uuid` canonical OK, wrong length/non-hex → fail;
  `ipv4` `256.0.0.1` / `01.2.3.4` → fail; `email` `user@localhost` → fail;
  `uri` truncated `%2` / non-ASCII → fail; `http://[1::2::3]` accepted (gap).
- **`date-time` materialized round-trip** (byte-identical across Go/Java/
  Python/TS): `…+02:00` → `…-02h…Z`; `.123456Z` → `.123Z`; lowercase
  `t`/`z` → uppercase; `…T23:59:60Z` → **load reject** (materialized).
- **`date`**: `2020-02-29` OK; `2021-02-29` / `2021-13-01` → fail.
- **`time`**: `12:30:45+02:00` → `12:30:45.000` (offset dropped; materialized
  in Go/Java/Python).
- **`duration`**: `PT90M` → `PT1H30M`; `PT0S` OK; `P1Y` / `P4W` / `P1D` →
  **load reject** (materialized time-only); overflow `PT<huge>H` → `Violation`.
- Combined with a failing [[minLength]] / [[maxLength]] / [[pattern]] or a
  sibling field → **all** reported in one shot (**P11**).

## Interactions

- **[[pattern]]**: `format` reuses its RE2-safe gate, compile-once
  mechanism, ASCII-class rule, and end-anchor normalization for every
  regex-lowered format. Both may appear on one node — the value must satisfy
  **both**, aggregated independently.
- **[[type]]**: gates applicability — `format` is meaningful only for
  `string`; a mismatch is a load reject (**P7.1**). For a materialized
  temporal the **emitted field type is the native construct**, not `string`
  (the wire is still a JSON string).
- **[[const]] / [[default]] / [[enum]]**: a supplied string literal MUST
  satisfy the format at load; on a **materialized** node it must also be
  **materializable** (a `const` `date-time` cannot be `:60`; a `const`
  `duration` must be time-only) and is stored/echoed in its **canonical**
  form.
- **[[minLength]] / [[maxLength]]**: independent string assertions; not
  cross-checked against a format's implied length. On a materialized temporal
  they constrain the *wire* string; the internal length guards
  (`hostname` / `email`) are separate.
- **[[nullability]]**: orthogonal — a `null` skips the format check and is not
  materialized; a present value is checked (and materialized).
- **[[required]]**: orthogonal — presence vs value shape.

## Ecosystem variance

| Source dialect | Action |
|---|---|
| JSON Schema 2020-12 | `format-annotation` is the default (collect only); we opt into `format-assertion` for the curated subset and reject the rest. Native format names, no rewrite. |
| OpenAPI 3.1 | Adopts 2020-12 `format`; same names. Native. OAS-specific formats (`int32`, `int64`, `float`, `double`, `password`, `byte`, `binary`) are **not** JSON Schema formats — treated as unknown and rejected. |
| OpenAPI 3.0 / draft-4 | `format` present with the same intent; `date-time` / `date` / `uuid` / `email` / `uri` / `hostname` names carry over (`url` → `uri`). Deferred formats await wider support. |
| Swagger 2.0 | Same as OAS 3.0. |

**Why native validators / parsers can't serve as the oracle** (empirical, in
the corpora): they diverge and/or mutate, so delegating would break **P1** or
the wire round-trip. Highlights: JS `Date` accepts `2021-02-30` and a missing
offset; Ruby clamps `:60`→`:59`; `.NET MailAddress` accepts `user@localhost`
and full IDN; `.NET Uri.CheckHostName` accepts underscores/trailing dots;
Java `Duration.parse`/`Period.parse` disagree on Y/M and `P1W`; `.NET
XmlConvert` collapses `P1Y`→365d and rolls `PT24H`→`P1D`; 27/57 tricky URIs
get divergent verdicts across the seven native URI parsers.

## Open questions

1. **Remaining deferred formats.** `idn-email`, `idn-hostname`, `iri`,
   `iri-reference` await a portable **IDNA / Unicode** story; `uri-template`,
   `json-pointer`, `relative-json-pointer`, `regex` are niche. Candidates for
   later admission once a portable owned check is corpus-proven.
2. **Full-grammar `duration` via a component struct.** The materialized
   `duration` is narrowed to time-only so it can be a native type. To also
   support calendar durations (`P1Y`, `P4W`), a **generated component struct**
   (`{years,months,weeks,days,hours,minutes,seconds}`) round-trips the full
   grammar byte-identically in all six languages
   (`research/format_materialize_duration/`, design B) — a candidate
   representation for a node that needs Y/M/W, or the behavior behind the
   string opt-out's accessor. Deferred pending demand.
3. **Materialize `time`, and TS `date`.** `time` materializes only in
   Go/Java/Python (offset-drop can merge values; TS/Ruby have no type) and
   TS `date` stays a string (`Date`-as-instant footgun). A cleaner `time`
   (offset-less `partial-time` subset only) or a dedicated TS date/time type
   would let these join the native set uniformly.
4. **Close the `uri` IP-literal semantic gap** — splice in `ipv6`'s grammar.
5. **Declaring `format-assertion`.** The IDE-support schema for
   `*.nexusrpc.yaml` could reference the `format-assertion` vocabulary in
   `$vocabulary`; today the assertion behavior is implicit.

## See also

- [[pattern]] — the regex keyword whose RE2-safe gate, compile-once
  mechanism, ASCII-class rule, and end-anchor normalization `format` reuses.
- [[type]] — supplies the base `string`; gates applicability; a materialized
  temporal replaces the field type with a native construct.
- [[const]] / [[default]] / [[enum]] — supplied literals validated (and, when
  materialized, canonicalized) against the format at load.
- [[minLength]] / [[maxLength]] — independent string assertions.
- [[nullability]] — a `null` is neither validated nor materialized.
- [[multipleOf]] — the sibling "support the portable subset, reject the
  hazardous form, deferred not excluded" decision posture.
- [[maximum]] — the `reason`-string convention.
