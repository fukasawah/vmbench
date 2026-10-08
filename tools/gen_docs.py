#!/usr/bin/env python3
"""Generates docs/BENCHMARKS.md from `vmbench --list` (dev/release helper)."""
import json
import subprocess
import sys

binary = sys.argv[1] if len(sys.argv) > 1 else "target/x86_64-unknown-linux-gnu/release/vmbench"
out_path = sys.argv[2] if len(sys.argv) > 2 else "docs/BENCHMARKS.md"

raw = subprocess.run([binary, "--quiet", "--list"], capture_output=True, check=True).stdout
report = json.loads(raw)
benchmarks = report["benchmarks"]

lines = []
lines.append("# Benchmark registry")
lines.append("")
lines.append("Generated from the binary (`vmbench --list`). `code_hash` is the")
lines.append("SHA-256 of the kernel function machine code as loaded at runtime.")
lines.append("")
lines.append("Suite version: %s" % report.get("benchmark_suite_version"))
lines.append("Binary: `%s`" % report.get("binary_sha256"))
lines.append("")
lines.append("| id | ver | kernel | isa | source | code_hash | description |")
lines.append("|----|-----|--------|-----|--------|-----------|-------------|")
for b in benchmarks:
    ch = (b.get("code_hash") or "unavailable")
    lines.append("| `%s` | %d | `%s` | %s | `%s` | `%s` | %s |" % (
        b["id"], b["version"], b["kernel"], b["isa"], b["source"], ch,
        b["description"].replace("|", "\\|")))
lines.append("")
lines.append("## Algorithms")
lines.append("")
for b in benchmarks:
    lines.append("- `%s` (`%s`): %s" % (b["id"], b["kernel"], b["algorithm"]))

open(out_path, "w", newline="\n").write("\n".join(lines) + "\n")
print("wrote", out_path, "(%d benchmarks)" % len(benchmarks))
