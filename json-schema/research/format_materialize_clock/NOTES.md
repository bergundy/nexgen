# Clock materialization — findings

Backs a proposed `features/format` extension: materialize the temporal formats
as typed model fields. Question, per format, per model target: **can we emit a
typed construct AND have every materializing language re-serialize to the SAME
canonical wire bytes (P1)?** Toolchains as-run: go 1.26, node v25, python3
3.13, java 21, ruby 2.6, dotnet 8.

> **Bottom line.** `date` and `date-time` **can** be materialized with
> byte-identical cross-language output under a **UTC + millisecond + `Z`**
> canonical form — proven, zero mismatches over the corpus in Go/Java/Python/TS
> (and prospective Ruby/.NET). `time` can materialize in every target **except
> TS and Ruby** (no time-of-day type), so it is at best a partial materializer.
> Materialization is **not free**: it (1) **truncates to milliseconds and
> UTC-normalizes** (loses the original offset and sub-ms precision — acceptable
> only because it is *consistent*), and (2) **forces narrowing the grammar to
> reject leap-second `:60`** (native parsers can't store it; Ruby corrupts it).
> Recommended authority model is **(B) with guardrails**, with **(A) the
> lossless fallback** where exact round-trip matters.

---

## 1. Authority model — recommendation: (B) typed field is authoritative, with guardrails; (A) offered as an opt-out

**(A) String authoritative, typed value is a derived accessor.** The stored
field stays the validated `string`; a getter (`asDateTime()` / `AsTime()` /
`.as_datetime()`) parses it on demand. Wire round-trips **byte-exactly** (no
truncation, offset and precision preserved). Fully P1-safe because the wire is
never re-serialized from the typed value. Downside: the model field is still
`string` — **not** the idiomatic typed field the user asked for; the typed
value is convenience only.

**(B) Typed construct IS the field; wire is produced by re-serializing it.**
This is what "materialize as language constructs" means. Idiomatic — the model
carries a `time.Time` / `OffsetDateTime` / `datetime` / `Date`. The cost is
that re-serialization normalizes (offset folded to UTC, precision floored to
ms) and **leap seconds cannot survive**. P1 is preserved **not** by matching
the original wire bytes but by every materializing language emitting the
**identical canonical bytes** — which this study proves is achievable.

**Recommendation: (B), gated by a canonical serializer and a narrowed grammar
(below).** It delivers the idiomatic field the user wants, and the corpus shows
the canonical bytes are identical across all four model targets. Offer **(A) as
a per-node or per-generator opt-out** ("lossless string") for schemas where the
original offset/precision is contractually significant (e.g. an audit log that
must preserve the sender's local offset). This mirrors the spec's existing
open-question-#4 escape hatch, promoted from "string + accessor" to "typed +
opt-out-to-string".

The honest trade-off: **(B) changes the wire on round-trip.** A value that
comes in as `2021-06-15T12:30:45+02:00` goes out as `2021-06-15T10:30:45.000Z`.
Same instant, different bytes. This is the "truncation is OK if consistent"
the user accepted — but it is a real semantic change (the local offset is gone)
and must be documented loudly. Where that is unacceptable, use (A).

---

## 2. Canonical serialization + cross-language byte-equality proof

The canonical form is **dictated by JS `Date`**, the least-capable materializer:
`Date` holds a UTC instant at **millisecond** resolution, and
`Date.prototype.toISOString()` **always** emits exactly
`YYYY-MM-DDTHH:MM:SS.sssZ` — 3 fractional digits, literal `Z`, and the wire
offset is **not recoverable** (`Date` stores only epoch-ms; `getTimezoneOffset`
returns the *runtime* offset, not the input's). Proven directly:

```
2021-06-15T12:30:45Z        -> 2021-06-15T12:30:45.000Z
2021-06-15T12:30:45.5Z      -> 2021-06-15T12:30:45.500Z
2021-06-15T12:30:45.999999Z -> 2021-06-15T12:30:45.999Z   (µs floored to ms)
```

So the canonical rules that **every** materializing language can hit
byte-for-byte:

- **`date-time`** → `YYYY-MM-DDTHH:MM:SS.sssZ`: parse to instant, **UTC-
  normalize**, **truncate to milliseconds**, always `Z`, always exactly 3
  fractional digits.
- **`date`** → `YYYY-MM-DD`: lossless.
- **`time`** → `HH:MM:SS.sss`: **drop the offset** (wall clock), truncate to
  ms, always 3 fractional digits.

### Proof (`compare.py`, corpus of 24 rows)

**Zero byte mismatches** across Go/Java/Python/TS (and prospective Ruby/.NET)
on every materializable row. Representative canonical outputs, identical in
every materializing engine:

| id | wire | canonical (all engines identical) |
|---|---|---|
| `dt-z-nofrac` | `2021-06-15T12:30:45Z` | `2021-06-15T12:30:45.000Z` |
| `dt-offset-pos` | `2021-06-15T12:30:45+02:00` | `2021-06-15T10:30:45.000Z` |
| `dt-offset-neg` | `2021-06-15T12:30:45-05:00` | `2021-06-15T17:30:45.000Z` |
| `dt-offset-plus0000` | `…+00:00` | `2021-06-15T12:30:45.000Z` |
| `dt-offset-neg0000` | `…-00:00` | `2021-06-15T12:30:45.000Z` |
| `dt-frac-6` | `…12:30:45.123456Z` | `2021-06-15T12:30:45.123Z` (µs→ms) |
| `dt-frac-9` | `…12:30:45.123456789Z` | `2021-06-15T12:30:45.123Z` (ns→ms) |
| `dt-lowercase` | `2021-06-15t12:30:45z` | `2021-06-15T12:30:45.000Z` |
| `d-basic` | `2021-06-15` | `2021-06-15` |
| `t-offset-pos` | `12:30:45+02:00` | `12:30:45.000` (offset dropped) |

### The one parse-path wrinkle: lowercase `t`/`z`

The pinned grammar **accepts** lowercase `t`/`z` (RFC 3339 §5.6). But Go
`time.Parse`, Python `datetime.fromisoformat`, and Ruby `DateTime.rfc3339`
**reject** lowercase; Java/JS/.NET accept it. Since the value is already
validated, the fix is a trivial **case-normalize on the parse path**:
uppercase the string before feeding the native parser. Safe for these three
formats because their grammar has **no other letters** (offset is digits only,
no month names). The runners apply `strings.ToUpper` / `.upper()` / `.upcase`
and then all six engines produce identical bytes. **This is a required part of
the parse path in materializing languages** — without it, Go/Python throw on a
value the validator accepted.

### Precision-floor alternatives considered

- **Millisecond floor (chosen).** Forced by JS `Date`. Every other language
  truncates cleanly to ms. This is the only floor that works if TS materializes
  via `Date`.
- Microsecond floor (Python's native floor) would let Python/Go/Java/.NET
  agree but **TS cannot hit it** (`Date` is ms). Only viable if TS stays
  `string` for `date-time` — not recommended (TS is a model target and `Date`
  is the idiomatic construct).
- Second-only floor is lossy for sub-second timestamps and offers no
  portability benefit over ms; rejected.

### Offset policy considered

- **UTC-normalize (chosen).** Everyone emits `…Z`. Forced by `Date` losing the
  offset. Proven byte-identical.
- **Preserve the numeric offset (rejected for (B)-via-`Date`).** `Date` cannot
  store the wire offset, so preserving it would force TS to **not** use `Date`
  — e.g. a `{epochMillis, offsetMinutes}` pair or keeping `string` in TS. That
  breaks the "idiomatic typed field" goal in the one language whose only
  temporal type is `Date`. If offset preservation is a hard requirement, use
  authority model **(A)** (string authoritative) instead — do not try to
  materialize with a preserved offset, because it un-idiomizes TS.

`Z` vs `+00:00`: canonical is **`Z`** (that is what `toISOString` emits and
what all engines converge on). Fractional-when-zero: canonical **always emits
`.000`** (again `toISOString`'s behavior) — do **not** omit the fraction, or
JS will disagree with a "trim trailing zeros" implementation elsewhere.

---

## 3. Leap-second conflict — REQUIRED decision: NARROW the grammar to reject `:60` in materializing formats

The validator **accepts** `:60` (pinned edge decision). But:

- Go `time.Parse`, Java `OffsetDateTime.parse`, Python `datetime`, .NET
  `DateTimeOffset.Parse` — **all reject** `:60` (parse throws). A validated
  `:60` value would fail to materialize → a runtime error on a value the
  validator said was fine. Unacceptable.
- **Ruby `DateTime.rfc3339` silently CLAMPS `:60`→`:59`.** The probe emitted
  `2021-12-31T23:59:59.000Z` for input `2021-12-31T23:59:60Z` — a **different
  instant, no error**. Silent data corruption, and it would disagree
  byte-for-byte with any language that (hypothetically) preserved `:60`.

There is **no** way to store `:60` in the stdlib typed construct of any target.
So materialization is **incompatible** with accepting `:60`.

**Recommendation: if a `date-time`/`time` node materializes, narrow its grammar
to REJECT `:60`** (drop the `|60` alternative from the seconds group for that
node). This is a *consistent* narrowing — every language rejects `:60`
identically at validation time, before parse — and it squarely fits the user's
"truncation OK if consistent". It is strictly *more* restrictive than the
current spec, so no previously-rejected value becomes accepted.

Nuance: the narrowing must be **tied to materialization**, not global. Nodes
kept as `string` (authority model (A), or `time` in TS, or a non-materializing
generator mode) should keep accepting `:60` to preserve the current contract.
Practically: **the materialized variant asserts a `:60`-rejecting grammar; the
string variant keeps the `:60`-accepting grammar.** Do NOT special-case `:60`
at parse (clamp/roll) — that is exactly the silent corruption Ruby demonstrates.

---

## 4. `time` specifics — materializes in Go/Java/Python/.NET; NOT in TS or Ruby

| Target | time-of-day type | Verdict |
|---|---|---|
| Go | none (`time.Time` w/ phantom date `0000-01-01`) | materializes via `time.Time`, but the type is a phantom-date fit; offset dropped |
| Java | `LocalTime` (no offset) / `OffsetTime` (offset **required**) | RFC 3339 `time` has **optional** offset, so neither java.time type fits both cases; use `LocalTime` + drop offset (chosen) |
| Python | `datetime.time` (can hold tzinfo) | materializes; we drop offset for canonical wall-clock |
| .NET | `TimeOnly` (no offset) | materializes; offset dropped |
| **TS** | **none** — `new Date("12:30:45")` → Invalid Date | **cannot materialize; stays `string`** |
| Ruby | **none** — `Time.parse` fabricates today's date | **cannot materialize; stays `string`** |

Because RFC 3339 `time` offset is **optional** and none of the languages'
time-of-day types cleanly hold an optional offset, the canonical `time` form
**drops the offset** (wall clock). That is a **bigger** semantic loss than
`date-time`'s UTC-fold: `12:30:45+02:00` and `12:30:45-05:00` both canonicalize
to `12:30:45.000`, collapsing distinct logical values. For a bare
time-of-day with no date there is no correct way to UTC-normalize an offset
anyway (no reference date). So `time` materialization is **lossy in a way that
can merge distinct inputs** — a stronger reason to **keep `time` as `string`**.

**Recommendation: `time` stays `string` in all targets** (TS can't materialize
it regardless, so materializing elsewhere already breaks the "same field type"
half of P2 across languages, and the offset-collapse is a real information
loss). If `time` materialization is ever wanted, restrict it to the
offset-less local `partial-time` subset only, in the four capable languages —
but that is a narrower feature and out of scope here.

---

## 5. Per-language / per-format design table

Field type / parse path / serialize path / truncation. "string" = do not
materialize (keep current behavior).

### `date-time`  (canonical `YYYY-MM-DDTHH:MM:SS.sssZ`)

| Lang | field type | parse (validated str → typed) | serialize (typed → wire) | truncation |
|---|---|---|---|---|
| Go | `time.Time` | `time.Parse(RFC3339Nano, upper(s))` | `.UTC().Truncate(ms).Format("…000Z07:00")` | UTC, ms |
| Java | `OffsetDateTime` | `OffsetDateTime.parse(s)` (accepts lowercase) → `Instant` | `.atOffset(UTC).truncatedTo(MILLIS)`, format `…SSSZ` | UTC, ms |
| Python | `datetime` (aware) | `datetime.fromisoformat(upper(s))` | `.astimezone(utc)`, floor µs→ms, format `…mmmZ` | UTC, ms |
| TS | `Date` | `new Date(s)` (already validated) | `.toISOString()` (native `…sssZ`) | UTC, ms |
| Ruby* | `DateTime` | `DateTime.rfc3339(upcase(s))` **(clamps :60!)** | `.new_offset(0)`, ms | UTC, ms |
| .NET* | `DateTimeOffset` | `DateTimeOffset.Parse(s, RoundtripKind)` | `.ToUniversalTime()`, ms | UTC, ms |

Requires the **`:60`-rejecting grammar** on the node (§3). `*` = prospective.

### `date`  (canonical `YYYY-MM-DD`, lossless)

| Lang | field type | parse | serialize | truncation |
|---|---|---|---|---|
| Go | `time.Time`† | `time.Parse("2006-01-02", s)` | `.Format("2006-01-02")` | none (phantom time-of-day ignored) |
| Java | `LocalDate` | `LocalDate.parse(s)` | `String.format("%04d-%02d-%02d")` | none |
| Python | `datetime.date` | `date.fromisoformat(s)` | `f"{y:04}-{m:02}-{d:02}"` | none |
| TS | `Date`‡ | `new Date(s)` (UTC midnight) | Y-M-D from `getUTC*` | none (but see ‡) |
| Ruby* | `Date` | `Date.iso8601(s)` | format | none |
| .NET* | `DateOnly` | `DateOnly.ParseExact(s,"yyyy-MM-dd")` | format | none |

† Go has no date-only type — `time.Time` carries a phantom `00:00:00Z`.
‡ TS has no date-only type — `Date` is a UTC **instant** at midnight. It
round-trips Y-M-D correctly here, **but** the field is semantically an instant,
not a date (a consumer doing `.getHours()` gets 0, and any TZ-local read is
wrong). This is the honest wart of materializing `date` in TS. `date` in TS is
the weakest cell; consider keeping `date` as `string` in TS specifically.

### `time`  (recommend **string everywhere**; table shows feasibility if forced)

| Lang | field type | parse | serialize | truncation |
|---|---|---|---|---|
| Go | `time.Time`† | `time.Parse("15:04:05…Z07:00", upper(s))` | wall-clock `HH:MM:SS.sss` | offset **dropped**, ms |
| Java | `LocalTime` | strip offset, `LocalTime.parse(s)` | `HH:MM:SS.sss` | offset dropped, ms |
| Python | `datetime.time` | `time.fromisoformat(upper(s))` | wall-clock ms | offset dropped, ms |
| **TS** | **`string`** | — | — | cannot materialize |
| Ruby* | **`string`** | — | — | cannot materialize |
| .NET* | `TimeOnly` | strip offset, `TimeOnly.Parse` | `HH:MM:SS.sss` | offset dropped, ms |

Offset-drop can **merge distinct values** (§4) → keep `string`.

---

## Which formats/languages should materialize

| Format | Go | Java | Python | TS | Ruby* | .NET* | Recommendation |
|---|---|---|---|---|---|---|---|
| `date-time` | ✅ | ✅ | ✅ | ✅ | ✅ (clamp risk) | ✅ | **Materialize (B)** — best case; all 4 model targets agree byte-for-byte; requires `:60`-reject grammar |
| `date` | ✅ | ✅ | ✅ | ⚠️ instant | ✅ | ✅ | **Materialize** in Go/Java/Python; **TS is a judgment call** (`Date`-as-instant wart) — defensible to keep TS `date` as `string` |
| `time` | ⚠️ | ⚠️ | ⚠️ | ❌ | ❌ | ⚠️ | **Keep `string`** — TS/Ruby can't, and offset-drop merges values |

`date-time` is the clear win and almost certainly what the user primarily
wants. `date` materializes cleanly in 3 of 4 model targets; the TS `Date`
semantics are the one wart. `time` should stay `string`.

---

## Residual risks

1. **Wire changes on round-trip (model B).** Offset folded to UTC, sub-ms
   precision lost. Same instant, different bytes than came in. Consumers that
   compare wire bytes, or need the sender's local offset, are affected. → offer
   authority model (A) opt-out.
2. **`:60` grammar narrowing must be node-scoped.** A global narrowing would
   change the string-field contract too. Materialized nodes reject `:60`;
   string nodes keep accepting it. Two grammars to maintain.
3. **Sub-millisecond precision loss is silent.** A nanosecond timestamp
   (`.123456789Z`) becomes `.123Z`. If any schema carries high-precision
   timestamps this is real data loss — flag in generated docs; (A) preserves it.
4. **TS `date` is an instant, not a date** — TZ-local reads misbehave. Consider
   TS `date` = `string`.
5. **Ruby `date-time` leap clamp** — if Ruby is ever a real target and the
   `:60`-reject grammar is *not* applied for some reason, Ruby corrupts
   silently. The grammar narrowing (§3) is what makes Ruby safe; it must be
   mandatory for materialized nodes.
6. **`time` offset-drop merges distinct values** — the reason `time` stays
   string.
7. **Case-normalize is mandatory on the parse path** (Go/Python/Ruby) or
   validated lowercase values throw. Cheap, but must not be forgotten.
8. **Not tested: extreme years / BCE.** Corpus covers `0001`..`9999`. Years
   outside 4-digit range are already rejected by the pinned `date` regex, so
   no new risk, but `Date`/`time.Time` behavior at the extremes wasn't probed
   beyond the corpus bounds.
