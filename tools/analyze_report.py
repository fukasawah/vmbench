#!/usr/bin/env python3
"""Analyzes a vmbench report: per-entry run durations and totals."""
import json
import sys

path = sys.argv[1] if len(sys.argv) > 1 else "test.json"
d = json.load(open(path))
print("total duration: %.1f s" % d["duration"]["total_s"])
print("quick mode:", "unknown")
total = 0.0
rows = []
for b in d["benchmarks"]:
    dur = sum(r["duration_ns"] for r in b.get("runs", [])) / 1e9
    total += dur
    rows.append((dur, b["id"], json.dumps(b.get("parameters", {}), ensure_ascii=False), b["status"]))
rows.sort(reverse=True)
print("sum of benchmark run durations: %.1f s" % total)
print("\ntop 15 by measured run time:")
for dur, bid, params, status in rows[:15]:
    print("%8.2f s  %-28s %s %s" % (dur, bid, params, status))
print("\nentry counts by id:")
from collections import Counter
c = Counter(b["id"] for b in d["benchmarks"])
for k, v in c.most_common():
    if v > 1:
        print("  %-32s x%d" % (k, v))
