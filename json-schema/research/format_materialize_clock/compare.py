#!/usr/bin/env python3
"""Cross-language canonical-serialization byte-equality harness for the clock
materialization probes. Runs each language runner over corpus.json, collects
the emitted CANONICAL string per (format, id, engine), and checks that every
MATERIALIZING language emits byte-identical output. A language that errors or
emits "" for a row is recorded as non-materializing for that row (expected for
JS date/time, Ruby time, Go/Java/Py leap second, etc.).

Usage: python3 compare.py [--with-ruby] [--with-dotnet]
Exit 0 always (report-only); disagreements are printed as MISMATCH lines.
"""
import json, subprocess, sys, os

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "corpus.json")

# base runners = the 4 model targets
RUNNERS = [
    ("go",   ["go", "run", "runner.go", CORPUS]),
    ("js",   ["node", "runner.mjs", CORPUS]),
    ("py",   ["python3", "runner.py", CORPUS]),
    ("java", ["java", "Runner.java", CORPUS]),
]

def run(engine, cmd):
    try:
        p = subprocess.run(cmd, cwd=HERE, capture_output=True, text=True, timeout=180)
    except Exception as e:
        print(f"!! {engine} failed to run: {e}", file=sys.stderr)
        return []
    if p.returncode != 0 and not p.stdout.strip():
        print(f"!! {engine} exited {p.returncode}: {p.stderr[:400]}", file=sys.stderr)
        return []
    rows = []
    for line in p.stdout.splitlines():
        line = line.strip()
        if not line or not line.startswith("{"):
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    return rows

def main():
    engines = list(RUNNERS)
    if "--with-ruby" in sys.argv:
        engines.append(("ruby", ["ruby", "runner.rb", CORPUS]))
    if "--with-dotnet" in sys.argv:
        engines.append(("dotnet", ["dotnet", "run", "--project", "dotnet_runner", "--", CORPUS]))

    # results[(fmt,id)][engine] = (canonical, err)
    results = {}
    engine_names = []
    for engine, cmd in engines:
        engine_names.append(engine)
        for r in run(engine, cmd):
            results.setdefault((r["format"], r["id"]), {})[r["engine"]] = (r.get("canonical",""), r.get("err",""))

    corpus = json.load(open(CORPUS))
    order = []
    for fmt in ("date-time", "date", "time"):
        for row in corpus[fmt]:
            order.append((fmt, row["id"]))

    mismatches = 0
    print(f"engines: {', '.join(engine_names)}\n")
    for (fmt, rid) in order:
        cell = results.get((fmt, rid), {})
        # who materialized (non-empty canonical, no err)?
        materialized = {e: v[0] for e, v in cell.items() if v[0] and not v[1]}
        errored = {e: v[1] for e, v in cell.items() if v[1] or not v[0]}
        vals = set(materialized.values())
        # A row where SOME engines materialize and others reject is a
        # PARTIAL divergence (leap-second is the canonical case): the
        # materializers among the 4 model targets must all agree AND the
        # count matters. Flag if the model targets disagree in count.
        model_targets = {"go", "js", "py", "java"}
        model_mat = {e for e in materialized if e in model_targets}
        if len(vals) > 1:
            status = "MISMATCH"
            mismatches += 1
        elif materialized and any(e in model_targets for e in errored) and model_mat:
            # some model target materialized, another model target rejected
            status = "PARTIAL"
            mismatches += 1
        else:
            status = "OK   " if materialized else "SKIP "
        canon = next(iter(vals)) if len(vals) == 1 else ("<none>" if not vals else "AMBIGUOUS")
        mat = ",".join(sorted(materialized))
        print(f"[{status}] {fmt:9s} {rid:16s} canonical={canon!r:40s} materialized={{{mat}}}")
        if status == "MISMATCH":
            for e in sorted(materialized):
                print(f"          {e:8s} -> {materialized[e]!r}")
        if errored:
            for e in sorted(errored):
                msg = errored[e][:70]
                print(f"          (no-mat) {e:8s}: {msg}")
    print(f"\nTotal MISMATCH rows: {mismatches}")

if __name__ == "__main__":
    main()
