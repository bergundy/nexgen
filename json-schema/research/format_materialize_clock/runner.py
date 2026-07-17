#!/usr/bin/env python3
"""Probe: MATERIALIZE model (B) in Python via stdlib datetime/date/time.
Parse each validated wire string into the typed construct, re-serialize to the
CANONICAL form, emit the bytes. python3 runner.py corpus.json"""
import json, sys
from datetime import datetime, date, time, timezone, timedelta

ENGINE = "py"

def emit(o):
    print(json.dumps(o))

def canon_datetime(wire):
    # datetime.fromisoformat in 3.11+ handles Z and offsets, but rejects :60
    # and rejects lowercase t/z. Normalize case first to isolate the leap issue.
    # parse-path pre-normalization: uppercase case-insensitive t/z (pinned
    # grammar accepts lowercase; fromisoformat rejects it). Safe: date-time has
    # no other letters (offset is digits only).
    s = wire.upper()
    dt = datetime.fromisoformat(s)  # may raise
    if dt.tzinfo is None:
        raise ValueError("missing offset (naive)")
    dt = dt.astimezone(timezone.utc)
    # truncate to ms
    dt = dt.replace(microsecond=(dt.microsecond // 1000) * 1000)
    ms = dt.microsecond // 1000
    return f"{dt.year:04d}-{dt.month:02d}-{dt.day:02d}T{dt.hour:02d}:{dt.minute:02d}:{dt.second:02d}.{ms:03d}Z"

def canon_date(wire):
    d = date.fromisoformat(wire)
    return f"{d.year:04d}-{d.month:02d}-{d.day:02d}"

def canon_time(wire):
    t = time.fromisoformat(wire.upper())  # handles Z (3.11+) and offsets; rejects :60
    ms = t.microsecond // 1000
    return f"{t.hour:02d}:{t.minute:02d}:{t.second:02d}.{ms:03d}"

def run(rows, fmt, fn):
    for r in rows:
        try:
            emit({"id": r["id"], "engine": ENGINE, "format": fmt,
                  "canonical": fn(r["wire"]), "err": ""})
        except Exception as e:
            emit({"id": r["id"], "engine": ENGINE, "format": fmt,
                  "canonical": "", "err": f"{type(e).__name__}: {e}"})

def main():
    c = json.load(open(sys.argv[1]))
    run(c["date-time"], "date-time", canon_datetime)
    run(c["date"], "date", canon_date)
    run(c["time"], "time", canon_time)

if __name__ == "__main__":
    main()
