# Materializing `date` / `time` / `date-time` as language constructs

Empirical study for a proposed extension of `features/format`: can the three
**temporal** formats be **materialized** — emitted as an idiomatic in-memory
typed construct in the generated model (Go `time.Time`, Java
`OffsetDateTime`/`LocalDate`/`LocalTime`, Python `datetime`/`date`/`time`,
TS `Date`) — instead of the current `string` field, while keeping the **wire
bytes byte-identical across every materializing language** (PRINCIPLES **P1**)?

This is the follow-up the [`format` spec's Type-mapping][spec] and
[`research/format_typed_repr/`][typed] left open (open question #4). That prior
study answered a **stricter** question — *"can ALL 7 languages build a typed
value on stdlib AND round-trip the EXACT original wire bytes?"* — and correctly
said **no** (Rust has no temporal std type; native re-emit normalizes offsets
and precision). This study relaxes two of those constraints in line with the
user's ask:

1. **Only the 4 model targets** (Go/Java/Python/TS) need to materialize — Rust
   is the load-time gate, not a model target. Ruby/.NET are prospective.
   Materializing in only SOME languages (others keep `string`) is acceptable.
2. **Truncation/normalization is acceptable IF it is consistent across
   languages** — we do **not** require the re-emitted bytes to equal the
   *original* wire bytes; we require every materializing language to emit the
   **same canonical bytes** for the same logical value. That is the real P1
   bar for a materialized field.

Validation is **unchanged**: the owned pinned regex + calendar predicate
([`research/format_conformance/`][conf]) still decides accept/reject. This
study only concerns (b) the parse path (validated string → typed) and (c) the
serialize path (typed → canonical wire).

## The canonical serialization under test

The canonical form is **dictated by the least-capable materializer, JS
`Date`**, which stores a UTC instant at **millisecond** resolution and whose
`toISOString()` **always** emits exactly `YYYY-MM-DDTHH:MM:SS.sssZ` (3
fractional digits, `Z`, offset lost). So:

| Format | Canonical wire (materialized) | Truncation / normalization applied |
|---|---|---|
| `date-time` | `YYYY-MM-DDTHH:MM:SS.sssZ` | UTC-normalized (offset folded into the instant, then dropped); **millisecond** floor; always `Z`; always exactly 3 fractional digits |
| `date` | `YYYY-MM-DD` | none (lossless) |
| `time` | `HH:MM:SS.sss` | offset **dropped** (wall clock); millisecond floor; always 3 fractional digits |

Every runner parses the validated wire string into its stdlib typed construct
and re-emits **this** canonical form. The harness checks byte-equality across
languages.

## Files

- `corpus.json` — validated wire strings spanning the axes (offset ±, `+00:00`,
  `-00:00`, fractional 1/3/6/9 digits, lowercase `t`/`z`, midnight, leap
  `:60`, `date` bounds, `time` local/offset/frac).
- `runner.go`, `runner.mjs`, `runner.py`, `Runner.java` — the 4 model targets.
- `runner.rb`, `dotnet_runner/` — prospective Ruby / .NET.
- `compare.py` — runs the runners, collects each emitted canonical string, and
  checks byte-equality across every materializing language. Flags `MISMATCH`
  (materializers disagree on bytes — a P1 break) and `PARTIAL` (a model target
  materializes a row another model target rejects — the "some string, some
  typed" case).

## Run

```sh
cd json-schema/research/format_materialize_clock
python3 compare.py                       # go / js / python / java (the 4 model targets)
python3 compare.py --with-ruby --with-dotnet
```

Each runner is standalone: `go run runner.go corpus.json`,
`node runner.mjs corpus.json`, `python3 runner.py corpus.json`,
`java Runner.java corpus.json`, `ruby runner.rb corpus.json`,
`dotnet run --project dotnet_runner -- corpus.json`.

## Result (summary — full detail in NOTES.md)

- **`date-time` and `date` materialize with ZERO byte mismatches** across all
  4 model targets (and prospective Ruby/.NET) for every non-leap row —
  including offset folding (`+02:00`→`Z`), `±00:00`→`Z`, fractional truncation
  (6/9→3 digits), and lowercase `t`/`z` (after a case-normalize on the parse
  path in Go/Python, whose native parsers reject lowercase).
- **`time` materializes in Go/Java/Python/.NET but NOT in TS or Ruby** (no
  time-of-day type). So `time` is a **partial** materializer: it must stay
  `string` in TS. Whether to materialize it at all is a policy call (see NOTES).
- **Leap second `:60` cannot be materialized.** All 4 model targets' native
  temporal parsers **reject** `:60`; **Ruby silently CLAMPS** it to `:59` (the
  probe emits `2021-12-31T23:59:59.000Z` for the `:60` input — a *different
  instant*, no error). Materializing therefore **forces narrowing the grammar
  to reject `:60`**. See NOTES for the recommendation.

[spec]: ../../features/format/spec.md
[typed]: ../format_typed_repr/NOTES.md
[conf]: ../format_conformance/README.md
