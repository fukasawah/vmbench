#!/usr/bin/env python3
"""Prints raw runs for benchmarks matching a substring (dev helper)."""
import json
import sys

path = "/tmp/vmbench_report.json"
needle = sys.argv[1] if len(sys.argv) > 1 else ""
d = json.load(open(path))
for b in d["benchmarks"]:
    if needle and needle not in b["id"]:
        continue
    if needle and needle not in json.dumps(b.get("parameters", {})):
        pass
    print("==", b["id"], b.get("parameters", {}), b["status"])
    for r in b.get("runs", []):
        print("   dur_ns=%d iters=%d values=%s" % (r["duration_ns"], r["iterations"], r["values"]))
