#!/usr/bin/env python3
"""Checks the verification contract for every registered benchmark.

- docs/STATUS.md and the binary registry agree (ids, generated table),
- every benchmark has one of the four verification classifications
  (an id that matches no rule is "unclassified" and fails),
- every benchmark has an explicit dispatch arm in src/bench/verify.rs.

The runtime side is fail-closed: an id without a dispatch arm fails
verification, so a full quick run in CI rejects it. See docs/VERIFICATION.md
for the normative rules.
"""
import json
import re
import subprocess
import sys

binary = sys.argv[1] if len(sys.argv) > 1 else "target/x86_64-unknown-linux-gnu/release/vmbench"
status_path = sys.argv[2] if len(sys.argv) > 2 else "docs/STATUS.md"
verify_src = sys.argv[3] if len(sys.argv) > 3 else "src/bench/verify.rs"

KINDS = {"expected_value", "functional", "protocol", "none"}

raw = subprocess.run([binary, "--quiet", "--list"], capture_output=True, check=True).stdout
report = json.loads(raw)
ids = {b["id"] for b in report["benchmarks"]}

doc = open(status_path).read()
begin = "<!-- BEGIN GENERATED -->"
end = "<!-- END GENERATED -->"
section = doc[doc.index(begin) + len(begin):doc.index(end)]
table_ids = set(re.findall(r"^\| `([^`]+)` \|", section, re.M))

failed = False

missing = sorted(ids - table_ids)
extra = sorted(table_ids - ids)
if missing:
    print("STATUS FAIL: ids missing from docs/STATUS.md: %s" % ", ".join(missing))
if extra:
    print("STATUS FAIL: docs/STATUS.md lists unknown ids: %s" % ", ".join(extra))
failed = failed or bool(missing or extra)

bad_kinds = sorted(
    (b["id"], b.get("verification"))
    for b in report["benchmarks"]
    if b.get("verification") not in KINDS
)
if bad_kinds:
    print("STATUS FAIL: invalid verification classification:")
    for bid, kind in bad_kinds:
        print("  %s: %s" % (bid, kind))
    failed = True

src = open(verify_src).read()
dispatch = src[src.index("pub fn verify_benchmark"):]
no_arm = sorted(i for i in ids if ('"%s"' % i) not in dispatch)
if no_arm:
    print("STATUS FAIL: ids without a dispatch arm in %s: %s" % (verify_src, ", ".join(no_arm)))
    failed = True

if failed:
    sys.exit(1)

kinds = {}
for b in report["benchmarks"]:
    kinds[b.get("verification")] = kinds.get(b.get("verification"), 0) + 1
print("STATUS OK: %d ids, verification: %s" % (len(ids), ", ".join("%s=%d" % kv for kv in sorted(kinds.items()))))
