#!/usr/bin/env python3
"""Regenerates the benchmark table in docs/STATUS.md from `vmbench --list`."""
import json
import subprocess
import sys

binary = sys.argv[1] if len(sys.argv) > 1 else "target/x86_64-unknown-linux-gnu/release/vmbench"
status_path = sys.argv[2] if len(sys.argv) > 2 else "docs/STATUS.md"

raw = subprocess.run([binary, "--quiet", "--list"], capture_output=True, check=True).stdout
report = json.loads(raw)
benchmarks = report["benchmarks"]

lines = ["| id | verification | kernel |", "|----|--------------|--------|"]
for b in sorted(benchmarks, key=lambda x: x["id"]):
    lines.append("| `%s` | %s | `%s` |" % (b["id"], b.get("verification", "none"), b["kernel"]))
generated = "\n".join(lines)

doc = open(status_path).read()
begin = "<!-- BEGIN GENERATED -->"
end = "<!-- END GENERATED -->"
a = doc.index(begin) + len(begin)
b = doc.index(end)
doc = doc[:a] + "\n" + generated + "\n" + doc[b:]
open(status_path, "w", newline="\n").write(doc)
print("wrote %s (%d benchmarks)" % (status_path, len(benchmarks)))
