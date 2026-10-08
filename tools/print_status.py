#!/usr/bin/env python3
"""Prints status counts and medians for a vmbench report.

With --require-ok, exits nonzero if any benchmark failed verification
(status "verification_failed") or failed at runtime (status "failed", for
example a truncated measurement window). Environment-dependent "unsupported"
results are allowed, so this is usable as a CI gate.
"""
import json
import sys

args = [a for a in sys.argv[1:] if a != "--require-ok"]
require_ok = len(args) != len(sys.argv) - 1

path = args[0] if len(args) > 0 else "/tmp/full_quick.json"
prefix = args[1] if len(args) > 1 else ""
d = json.load(open(path))
bad = [b for b in d["benchmarks"] if b["status"] != "ok"]
print("entries:", len(d["benchmarks"]), "not-ok:", len(bad))
for b in bad:
    print("  NOT OK:", b["id"], b["status"], b.get("reason"), b.get("parameters"))
failed = [b for b in d["benchmarks"] if b["status"] == "verification_failed"]
runtime_failed = [b for b in d["benchmarks"] if b["status"] == "failed"]
if require_ok and failed:
    print("FAIL: %d benchmark(s) failed verification" % len(failed))
    sys.exit(1)
if require_ok and runtime_failed:
    print("FAIL: %d benchmark(s) failed at runtime" % len(runtime_failed))
    sys.exit(1)
for b in d["benchmarks"]:
    if prefix and not b["id"].startswith(prefix):
        continue
    med = b.get("median", {})
    show = {k: (round(v, 4) if isinstance(v, float) else v) for k, v in med.items()}
    print(b["id"], b["parameters"], show)
