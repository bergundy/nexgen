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
assertion optional (see the empirical divergence tables under **Ecosystem
variance**). Instead each supported format lowers to a **generator-owned**
check: a pinned portable regex through the [[pattern]] RE2-safe gate, plus —
where a regex alone is insufficient — a shared **length guard** or **calendar
predicate**, all plain arithmetic. [[pattern]] anticipated this and points
here for the regex route. Every rule below was verified value-for-value
across all four runtime targets **plus** the Rust gate and the prospective
.NET / Ruby targets by the conformance corpora under
`research/format_conformance/`, `research/format_email/`,
`research/format_hostname/`, `research/format_duration/`, and
`research/format_uri/`.

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
  - `duration` — RFC 3339 App. A / ISO 8601 duration; the week-vs-rest
    mutual exclusion and strict component nesting fall out of the
    alternation, so **no auxiliary predicate is needed**
    (`research/format_duration/`, 68 pairs, 7/7 agree).
  - `uri`, `uri-reference` — RFC 3986, ASCII-only, at high fidelity
    (`research/format_uri/`, 67 pairs, 7/7 agree). One documented gap: the
    IP-literal host is checked structurally, not semantically (below).
- **Pinned regex + a length guard** (RE2 has no total-length lookahead, so
  a cheap `code_point_count` check rides alongside the regex in the shared
  `Validate`):
  - `hostname` — RFC 1123 LDH labels; each label ≤63, **total ≤253**
    (`research/format_hostname/`, 41 pairs, 7/7 agree).
  - `email` — a well-defined **ASCII dot-atom** subset of RFC 5321 (no
    quoted locals, comments, IP-literals, or IDN); **total ≤254**, and the
    guard runs *before* the regex to neutralize a Java matcher hazard
    (below) (`research/format_email/`, 56 pairs, 7/7 agree).
- **Pinned regex (syntax) + a shared calendar/range predicate** (semantics
  a regex cannot express):
  - `date`, `date-time`, `time` — RFC 3339 profile; the predicate enforces
    day-in-month, the Gregorian leap-year rule, and the offset numeric
    range (`research/format_conformance/`, 124 pairs, 7/7 agree).

**Deferred (rejected at load, "not yet supported"):** `idn-email`,
`idn-hostname`, `iri`, `iri-reference` (all need **IDNA / Unicode** handling
that diverges across engines — WHATWG punycodes, Ruby ASCII-rejects; the
asserted set is deliberately ASCII-only, the portable line); `uri-template`
(RFC 6570 templating grammar), `json-pointer`, `relative-json-pointer`, and
`regex` (niche; `regex` would additionally mean running the [[pattern]] gate
over the *value*). Each is deferred, *not* a categorical **P6** exclusion —
admitting it needs a portable owned check we don't yet commit to.

**Unknown / non-standard format** (`format: "phone"`, a typo, a custom
string) → **reject** with a fix-it listing the supported names. We do
**not** silently accept it as an annotation: an unrecognized format is
the ambiguity **P7.1** rejects loudly, and `format-assertion` itself
mandates failing on unknown formats.

**`format` on a non-string [[type]]** (`{type:"integer", format:"uuid"}`)
→ **reject** (**P7.1**). The spec would make it a vacuous no-op; a
statically meaningless keyword is a load reject here, exactly as
[[pattern]] / the count keywords treat a type mismatch. (No standard
format targets a non-string type.)

Grounding ([[PRINCIPLES.md]]): **P1** (identical cross-language accept /
reject — guaranteed by owning the check, proven by the corpora, never by a
native validator), **P4** (the regex route needs only each stdlib's regex
engine, as [[pattern]] established; the length and calendar predicates are
plain arithmetic — no new dependency), **P10** (enforced at the boundary),
**P11** (aggregated), **P12** (a pure predicate over the decoded value in
the **shared `Validate`** layer, identical both directions — no per-adapter
logic). The curated line is the **P1** line, mirroring [[pattern]]'s
"portable subset accepted, hazardous form rejected, deferred not excluded"
and [[multipleOf]]'s fractional-divisor deferral: we assert only where every
target provably agrees.

**Why assert rather than annotate.** The spec default (`format-annotation`)
would have us accept any `format` and never check it. That collides with
this generator's mission — a `format: "uuid"` that lets a non-UUID through
is a silent wire-contract hole (**P10**). So for the subset we own we adopt
`format-assertion` semantics; for everything else we reject at load rather
than accept-and-ignore, keeping the "no silently-incorrect output"
guarantee (**P7.1**).

**Two portability rules every pinned pattern obeys** (both learned from
[[pattern]], both re-confirmed by these corpora — no new gate machinery):
- **Explicit ASCII classes, never `\d` / `\w` / `\s`.** The Rust `regex`
  crate (the load gate) makes `\d` Unicode-aware, so a bare `\d` would
  accept `P٣Y` / fullwidth digits in the gate; every pinned pattern spells
  `[0-9]` / `[A-Za-z0-9]` etc. so even the gate agrees without a per-engine
  flag.
- **Per-target end/start anchor.** A bare `$` matches before a trailing
  `\n` in Python / Java / .NET, so a pinned `^…$` initially let
  `"…uuid…\n"` through; every emitted pattern uses the strict end anchor
  ([[pattern]]'s existing rewrite — `\Z` Python, `\z` Java / Ruby / .NET,
  `$` Go / JS already strict) and Ruby's `\A` start anchor. This is why the
  inline patterns below are written with `$`/`^` but *emit* the normalized
  form.

**RFC 3339 edge decisions (pinned, temporal formats).** All targets follow
these because we own the check:
- **Leap second** `:60` in the seconds field is **accepted syntactically**
  (RFC 3339 permits it); we do not verify it against a real leap-second
  table (out of scope, and unportable). Native parsers split on this
  (Go / Java / Python / .NET reject `:60`; Ruby silently *clamps* it to
  `:59`) — one of the reasons we own the check.
- **`date-time` offset is required** (`Z` or `±HH:MM`) per RFC 3339; a
  bare local `date-time` is invalid. `-00:00` ("unknown offset") is
  accepted.
- **`time` offset is optional** — RFC 3339 `partial-time` permits a bare
  local `time` (`12:30:00`), which is accepted; an offset, when present, is
  range-checked like `date-time`'s.
- **Offset range** is enforced by the predicate, not just the regex:
  hours `00–23`, minutes `00–59`, so `+24:00` and `+01:60` are rejected.
- **Fractional seconds** are accepted at **any precision** (`.` followed by
  one or more digits); trailing precision is not normalized (native parsers
  truncate/pad — Python 9→6, Ruby pads to 9, .NET to 7 — which is why the
  string, not a parsed value, stays authoritative; see Type mapping).
- **Case** — the `T` / `Z` separators are accepted in either case
  (RFC 3339 §5.6 NOTE), pinned identically across targets.
- **Calendar validity** (`date`, and the date half of `date-time`) enforces
  month `01–12`, day within the month's length, and the Gregorian
  leap-year rule for February — so `2021-02-30` and `2021-13-01` are
  rejected, which a pure regex would miss.

**Edge decisions for the string-shaped formats** (pinned, all corpus-proven):
- **`hostname`**: a **trailing dot** (`example.`) is **rejected** (matches
  the JSON-Schema-Test-Suite; note `ajv` accepts it — we deliberately
  don't). An **all-numeric label / TLD** (`999`, `123.456`) is **accepted**
  — RFC 1123's "never all-numeric" is an interpretive note, not
  RE2-expressible; documented residual risk. `xn--` A-labels pass as
  ordinary LDH labels (Punycode decoding is `idn-hostname`, deferred).
- **`email`**: the accepted language is an **ASCII dot-atom** local part
  (`atext`), a single `@`, and a `hostname`-style domain of **≥2 labels**
  (so `user@localhost` is rejected). Quoted locals, comments, IP-literal
  domains, trailing dots, whitespace, and all Unicode are rejected. The
  **≤254 length guard precedes the regex**: `java.util.regex` matches the
  nested dot-atom quantifier recursively and throws `StackOverflowError`
  (a crash, not a clean reject) on multi-thousand-char runs; RE2 engines
  are linear and immune, but the length cap keeps every engine safe and is
  independently the RFC 5321 mailbox limit. The regex is **not** ReDoS-prone
  (linear to 100k chars on all backtracking engines).
- **`uri` / `uri-reference`**: RFC 3986 faithful for scheme rules,
  percent-encoding (`%HEXDIG HEXDIG` only — bare/truncated `%` rejected),
  the authority/path split, port, query, and fragment char classes, and
  ASCII-only enforcement (non-ASCII ⇒ IRI ⇒ rejected). **One deliberate
  fidelity gap:** the IP-literal host `[…]` is validated *structurally*
  (brackets + allowed inner bytes), not *semantically*, so
  `http://[1::2::3]` (double `::`) is accepted. Bounded and closable later
  by splicing in `ipv6`'s pinned grammar; mirrors the leap-second "syntax,
  not full semantics" line.

## Type mapping

None. The emitted field type is [[type]]'s `string`; the format check lives
only in the validator. The format name is surfaced in the generated type's
**doc comment** (`// format: uuid` and analogues) so the intent survives
into the generated source (**P2**), but it changes no signature.

**Why not a typed field** (`time.Time`, `Date`, `UUID`, `IPAddress`, …).
Empirically ruled out across all seven languages
(`research/format_typed_repr/`): **no format** has a stdlib typed
representation available in *every* target (Rust `std` builds only the two
IP types; JS has none; `uuid` has no stdlib type in Go / JS / Rust / Ruby),
so a typed field would force a dependency (**P4**) somewhere. Worse, where a
native type *does* exist it **diverges from the pinned grammar** and/or
**normalizes on re-emit** — JS `Date` rolls `2021-02-30`→Mar 2 and drops to
ms; Ruby clamps leap seconds; Java `InetAddress` can do a **blocking DNS
lookup** and rewrites `01.2.3.4`; UUID parsers are uniformly lax and
lowercase-normalize; IPv6 recompresses. Handing the user a parsed value
would therefore break both **P1** (accept/reject) and the byte round-trip.
The stored `string` stays authoritative. A future opt-in *derived accessor*
(parse the already-validated string with **our** check, never re-serialize
from the parsed value) is the escape hatch — see Open questions.

## Validator mapping

Per **P10** / **P11**. A single "does the value satisfy `<format>`?"
predicate, identical in both directions (shared `Validate`, **P12**),
composed of a pinned regex and — for some formats — an auxiliary length or
calendar/range check, all in the same shared layer.

**Regex-lowered formats** reuse [[pattern]]'s machinery wholesale: the
format lowers to a **pinned portable pattern**, compiled **once**
(module/package init) with the same P1-pinned flags [[pattern]] uses (the
pinned patterns are fully anchored via the per-target end-anchor
normalization above; ASCII classes; code-point `.`). Because the patterns
are generator-authored they are RE2-safe by construction — no author-supplied
regex reaches the gate. Pinned patterns (written with `^…$`; **emitted**
with the normalized anchors):

| Format | Pinned pattern / source | Auxiliary check |
|---|---|---|
| `uuid` | `^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$` | — |
| `ipv4` | `^(?:(25[0-5]\|2[0-4][0-9]\|1[0-9][0-9]\|[1-9]?[0-9])\.){3}(25[0-5]\|2[0-4][0-9]\|1[0-9][0-9]\|[1-9]?[0-9])$` (the `[1-9]?[0-9]` alternative is what forbids leading zeros) | — |
| `ipv6` | RFC 4291 full / `::`-compressed / IPv4-tail — see `research/format_conformance/` (authoritative pinned form) | — |
| `date` | `^[0-9]{4}-(0[1-9]\|1[0-2])-(0[1-9]\|[12][0-9]\|3[01])$` | calendar predicate |
| `time` | `^([01][0-9]\|2[0-3]):[0-5][0-9]:([0-5][0-9]\|60)(\.[0-9]+)?(Z\|[+-]([01][0-9]\|2[0-3]):[0-5][0-9])?$` (offset optional) | range predicate |
| `date-time` | full-date `T` full-time with **offset required** | calendar + range predicate |
| `duration` | `^P(?:(?:[0-9]+Y(?:[0-9]+M(?:[0-9]+D)?)?\|[0-9]+M(?:[0-9]+D)?\|[0-9]+D)(?:T(?:[0-9]+H(?:[0-9]+M(?:[0-9]+S)?)?\|[0-9]+M(?:[0-9]+S)?\|[0-9]+S))?\|T(?:[0-9]+H(?:[0-9]+M(?:[0-9]+S)?)?\|[0-9]+M(?:[0-9]+S)?\|[0-9]+S)\|[0-9]+W)$` | — |
| `hostname` | `^[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)*$` | length ≤253 |
| `email` | `^[A-Za-z0-9!#$%&'*+/=?^_`{\|}~-]+(?:\.[…same…]+)*@[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)+$` — see `research/format_email/` | length ≤254 (runs **first**) |
| `uri` / `uri-reference` | RFC 3986 ASCII grammar — see `research/format_uri/pinned_body.json` (authoritative pinned form) | — |

**Auxiliary predicates** run in the same shared `Validate`, plain
arithmetic, no date/net library:
- **Length guards** (`hostname` ≤253, `email` ≤254) — a `code_point_count`
  comparison; for `email` it runs *before* the regex (Java stack-overflow
  mitigation, above).
- **Calendar / range predicate** (`date`, `date-time`, `time`) — day-in-month
  + Gregorian leap year + offset hour/minute range, over the fields the
  regex already grouped. We do **not** delegate to `time.Parse` /
  `LocalDate.parse` / `datetime.fromisoformat` / `Date`: their grammars and
  error surfaces diverge (110 native divergences recorded in
  `research/format_conformance/`), the exact P1 hazard the owned check
  avoids.

| Language | Strategy |
|---|---|
| Go | Package-level `var fmtRe = regexp.MustCompile(<pinned>)` (compiled once at init); the shared `Validate` runs any length guard, `if !fmtRe.MatchString(v) { push(Violation{Path, Reason: fmt.Sprintf("must be a valid %s, got %q", <format>, v)}) }`, then for temporal formats calls the shared `validRFC3339(...)` calendar/range helper. Collected into one `ValidationError`. |
| TypeScript | Module-level ``const FMT_RE = /<pinned>/u;`` (the `u` flag mandatory, as [[pattern]]). Length guard, then ``if (!FMT_RE.test(v)) push(Violation{path, reason: `must be a valid ${format}, got ${JSON.stringify(v)}`})``, plus the shared calendar/range check for temporal formats. One `ValidationError`. |
| Python | Module-level `FMT_RE = re.compile(<pinned>, re.ASCII)` and an `AfterValidator`: length guard, then `if FMT_RE.search(v) is None: raise ValueError(...)` (plus the calendar/range helper), aggregating into `pydantic.ValidationError`. We deliberately do **not** use Pydantic's `UUID` / `datetime` / `EmailStr` types: they coerce/normalize (a `UUID` object, a `datetime`, a lowercased domain) and shift the wire shape, add a dependency (`EmailStr`), and their grammars differ from the pinned one — the same reason [[pattern]] avoids the native `pattern=`. |
| Java | Static `private static final Pattern FMT_RE = Pattern.compile(<pinned>);` (default flags — ASCII classes). The per-POJO collecting deserializer (PRINCIPLES Java §5) reads the `String`, runs the length guard **first** (for `email`, mandatory — it caps the input below the matcher's recursion limit), checks `if (!FMT_RE.matcher(v).find())` (the pinned pattern is anchored) and the shared calendar/range helper, pushing a `Violation{path, "must be a valid " + <format> + ", got " + v}` into the single `ValidationException`. Not bean-validation `@Pattern` / `@Email`. |

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
| `ipv4` / `ipv6` | `{type:"string", format:"ipv4"}`, `{…, format:"ipv6"}` |
| `date-time` / `date` / `time` | `{type:"string", format:"date-time"}` |
| `duration` | `{type:"string", format:"duration"}` |
| `hostname` | `{type:"string", format:"hostname"}` |
| `email` | `{type:"string", format:"email"}` |
| `uri` / `uri-reference` | `{type:"string", format:"uri"}`, `{…, format:"uri-reference"}` |
| Combined with a length bound | `{type:"string", format:"uuid", maxLength:36}` |
| Combined with `pattern` | `{type:"string", format:"uuid", pattern:"^0"}` (value must satisfy both) |
| On a nullable string | `{oneOf:[{type:"string", format:"uuid"},{type:"null"}]}` |

### Rejected at load time (negative)

| Reason | Example |
|---|---|
| Value not a string | `format:5`, `format:true`, `format:["uuid"]` |
| Type mismatch (P7.1) | `{type:"integer", format:"uuid"}`, `{type:"boolean", format:"date"}` |
| Unknown / non-standard format | `{type:"string", format:"phone"}`, `{…, format:"datetime"}` (typo) |
| Deferred standard format (IDNA/Unicode or niche) | `{type:"string", format:"idn-email"}`, `{…, format:"iri"}`, `{…, format:"uri-template"}`, `{…, format:"json-pointer"}`, `{…, format:"regex"}` |
| Literal fails its format | `{type:"string", format:"uuid", const:"not-a-uuid"}`, `{…, default:"nope"}` |

### Runtime fixtures (validator)

Per-format accept/reject behavior is exercised by the conformance corpora
(counts below); those corpora are each keyword-shape's regression suite —
new edge cases are added there, not enumerated here. Representative cases:

- **`uuid`** (`format_conformance/`, 18): canonical OK, upper/lower hex OK;
  wrong length / non-hex / missing dashes / braces / `urn:` prefix → fail.
- **`ipv4`** (17): `"192.168.0.1"` OK; `"256.0.0.1"`, `"1.2.3"`,
  leading-zero `"01.2.3.4"` → fail.
- **`ipv6`** (20): full / `::`-compressed / IPv4-tail OK; double `::`, too
  many groups, zone id → fail.
- **`date-time`** (22): `"2021-02-28T23:59:60Z"` (leap second) OK;
  `"2021-02-30T…"` (calendar) → fail; missing offset → fail;
  `"…+24:00"` (offset range) → fail; lowercase `t`/`z` and `-00:00` OK.
- **`date`** (25) / **`time`** (22): Feb-29 leap-year OK / non-leap → fail;
  month 00/13, day 32 → fail; `time` may omit the offset (`"12:30:00"` OK).
- **`duration`** (`format_duration/`, 68): `P3Y6M4DT12H30M5S`, `P4W`, `PT1H`
  OK; `P`, `PT`, `P1H` (no `T`), `1Y`, `P1Y1W`, `PT1.5S` → fail.
- **`hostname`** (`format_hostname/`, 41): `a.b-c.example` OK; trailing dot,
  label >63, total >253, leading/trailing hyphen, `host_name` → fail.
- **`email`** (`format_email/`, 56): `a.b+c@x.example` OK; `user@localhost`
  (single-label domain), quoted local, IP-literal, trailing dot, Unicode →
  fail; a ≤254 guard precedes the match.
- **`uri`** (`format_uri/`, 67): absolute URIs with valid pct-encoding OK;
  truncated `%2`, raw space, non-ASCII → fail; `http://[1::2::3]` accepted
  (documented IP-literal structural gap).
- Combined with a failing [[minLength]] / [[maxLength]], [[pattern]], or
  sibling field → **all** reported in one shot (**P11**); serialize of a
  malformed in-memory value → rejected before emit (**P12**).

## Interactions

- **[[pattern]]**: the general-purpose regex keyword; `format` reuses its
  RE2-safe gate, compile-once mechanism, ASCII-class rule, and end-anchor
  normalization for every regex-lowered format. Both may appear on one
  node — the value must satisfy **both**, aggregated independently.
  [[pattern]] points here for the format route; this is the return edge.
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
  bound against a format's implied length (a `uuid` is always 36 chars, but
  a contradictory `maxLength:10` is not caught at load) — the same non-check
  stance [[pattern]] takes on regex↔length satisfiability. The format's own
  length guard (`hostname`/`email`) is internal, not a `maxLength`.
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
| OpenAPI 3.1 | Adopts 2020-12 `format`; same names. Native. OAS-specific formats (`int32`, `int64`, `float`, `double`, `password`, `byte`, `binary`) are **not** JSON Schema formats — treated as unknown and rejected (numeric width is a [[type]] concern; `password`/`byte`/`binary` deferred). |
| OpenAPI 3.0 / draft-4 | `format` present with the same intent; `date-time` / `date` / `uuid` / `email` / `uri` / `hostname` names carry over (`url` maps to `uri` intent). Deferred formats await wider support. |
| Swagger 2.0 | Same as OAS 3.0. |

**Why native validators can't serve as the oracle** (empirical, recorded in
the corpora): they diverge and/or mutate, so delegating would break **P1**
or the wire round-trip.
- **Temporal** (`format_conformance/`): JS `Date` accepts `2021-02-30` and
  a missing offset; Go `time.Parse` rejects lowercase `t`/`z` and leap
  seconds but accepts `+24:00`; Ruby clamps `:60`→`:59`.
- **`email`** (`format_email/`): `.NET MailAddress` accepts `user@localhost`,
  IP-literals, quoted locals, and full IDN; Python `parseaddr` never really
  rejects; Pydantic `EmailStr` adds a dependency **and** normalizes the
  domain.
- **`hostname`** (`format_hostname/`): `.NET Uri.CheckHostName` accepts
  underscores and trailing dots and calls `999` an IPv4; even `ajv` accepts
  a trailing dot.
- **`duration`** (`format_duration/`): Java `Duration.parse` (no Y/M,
  accepts fractions) and `Period.parse` (accepts negatives, expands `P1W`)
  disagree with each other; `.NET XmlConvert` rejects the valid `P1W`.
- **`uri`** (`format_uri/`): 27 of 57 tricky inputs get divergent verdicts
  across the seven native parsers — Python `urllib` accepts nearly
  everything, WHATWG `URL` silently normalizes, `.NET` reinterprets a
  path as `file:///…`.

## Open questions

1. **Remaining deferred formats.** `idn-email`, `idn-hostname`, `iri`,
   `iri-reference` await a portable **IDNA / Unicode** story (the empirical
   punycode-vs-ASCII divergence is why they're out for now); `uri-template`,
   `json-pointer`, `relative-json-pointer`, and `regex` are niche. Each is a
   candidate for later admission once a portable owned check is pinned and
   corpus-proven — mirroring [[pattern]]'s subset-widening question. (The
   v1 candidates `email` / `hostname` / `duration` / `uri` / `uri-reference`
   are now **resolved** and asserted.)
2. **Close the `uri` IP-literal semantic gap.** `uri` / `uri-reference`
   currently accept a structurally-valid but semantically-invalid
   IP-literal host (`[1::2::3]`); splicing in `ipv6`'s pinned grammar would
   close it. Deferred as a bounded, documented limitation.
3. **Calendar-semantic depth.** v1 pins leap-second acceptance and skips a
   real leap-second table; whether to tighten is revisited on demand.
4. **Opt-in typed accessor.** The model field stays `string` (Type mapping).
   If a typed *accessor* is ever wanted (`asTime()`, `asUUID()`), it must
   parse the already-validated string with **our** pinned check — never a
   native parser — and never re-serialize from the parsed value, so the
   stored string stays authoritative and P1 holds. Gated on demand; the
   `research/format_typed_repr/` matrix is the feasibility record.
5. **Declaring `format-assertion`.** If the IDE-support JSON Schema for
   `*.nexusrpc.yaml` documents ever needs to signal that we assert, it can
   reference the `format-assertion` vocabulary in `$vocabulary`; today the
   assertion behavior is implicit in the generator.

## See also

- [[pattern]] — the regex keyword whose RE2-safe gate, compile-once
  mechanism, ASCII-class rule, and end-anchor normalization `format`
  reuses; owns the regex-lowering route.
- [[type]] — supplies the emitted `string`; gates applicability to
  `string`.
- [[const]] / [[default]] / [[enum]] — supplied string literals validated
  against the format at load.
- [[minLength]] / [[maxLength]] — the other string assertions; independent,
  and not cross-checked against a format's implied length.
- [[multipleOf]] — the sibling "support the portable subset, reject the
  hazardous form, deferred not excluded" decision posture.
- [[maximum]] — the `reason`-string convention.
