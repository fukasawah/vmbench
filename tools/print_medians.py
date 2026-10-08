#!/usr/bin/env python3
"""Prints benchmark medians from a vmbench JSON report (dev helper)."""
import json
import sys

path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/vmbench_report.json"
d = json.load(open(path))
for b in d["benchmarks"]:
    med = b.get("median", {})
    pretty = {}
    for k, v in med.items():
        if isinstance(v, list):
            pretty[k] = ["%s=%s" % (p.get("x"), p.get("y")) for p in v]
        elif isinstance(v, float):
            pretty[k] = round(v, 3)
        else:
            pretty[k] = v
    print(b["id"], b.get("parameters", {}), pretty)
