// Probe: MATERIALIZE model (B) in JS/TS. The only stdlib temporal type is
// `Date` (a UTC instant, millisecond floor, offset LOST). date-only and
// time-only have NO stdlib type. Parse each validated wire string, re-serialize
// to the CANONICAL form, emit the bytes.  node runner.mjs corpus.json
import { readFileSync } from "node:fs";

const ENGINE = "js";
const emit = (o) => console.log(JSON.stringify(o));

// date-time via Date. Date.parse is LAX (accepts calendar-invalid, missing
// offset) but here we only feed strings already validated by the pinned check,
// so the ONLY question is round-trip bytes. Date is UTC + ms floor.
function canonDateTime(wire) {
  const ms = Date.parse(wire);
  if (Number.isNaN(ms)) throw new Error("Date.parse -> NaN");
  const d = new Date(ms);
  // toISOString() is always "YYYY-MM-DDTHH:mm:ss.sssZ" (UTC, exactly 3 frac, Z)
  return d.toISOString();
}

// date-only: JS has NO date type. new Date("2021-06-15") is a UTC instant at
// midnight. We CAN recover Y-M-D from the UTC fields since the input midnight
// is interpreted as UTC. But this is materializing as a full instant, not a
// date. Emit Y-M-D from UTC parts to test byte-equality; flag it lossy.
function canonDate(wire) {
  const ms = Date.parse(wire); // "2021-06-15" -> UTC midnight
  if (Number.isNaN(ms)) throw new Error("Date.parse -> NaN");
  const d = new Date(ms);
  const y = String(d.getUTCFullYear()).padStart(4, "0");
  const mo = String(d.getUTCMonth() + 1).padStart(2, "0");
  const da = String(d.getUTCDate()).padStart(2, "0");
  return `${y}-${mo}-${da}`;
}

// time-only: JS has NO time type. new Date("12:30:45") -> Invalid Date.
// Cannot materialize as a language construct at all. Report unsupported.
function canonTime(_wire) {
  throw new Error("UNSUPPORTED: JS has no time-of-day type");
}

function run(rows, fmt, fn) {
  for (const r of rows) {
    try {
      emit({ id: r.id, engine: ENGINE, format: fmt, canonical: fn(r.wire), err: "" });
    } catch (e) {
      emit({ id: r.id, engine: ENGINE, format: fmt, canonical: "", err: String(e.message || e) });
    }
  }
}

const c = JSON.parse(readFileSync(process.argv[2], "utf8"));
run(c["date-time"], "date-time", canonDateTime);
run(c["date"], "date", canonDate);
run(c["time"], "time", canonTime);
