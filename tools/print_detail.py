#!/usr/bin/env python3
"""Prints median/variation and chunk-rate details for selected benchmark ids."""
import json
import sys

path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/normal.json"
d = json.load(open(path))
want = set(sys.argv[2:]) if len(sys.argv) > 2 else {
    "cpu.overhead.loop.v1",
    "cpu.int.latency.add.v1",
    "cpu.int.throughput.add.v1",
    "memory.latency.dram.v1",
    "memory.bandwidth.seq.read.1.v1",
    "cpu.atomic.fetch_add.all.v1",
    "cpu.multicore.all.v1",
}
for b in d["benchmarks"]:
    if b["id"] not in want:
        continue
    med = {k: (round(v, 4) if isinstance(v, float) else v) for k, v in b["median"].items()}
    var = {k: round(v, 2) for k, v in b.get("variation_pct", {}).items()}
    print(b["id"], med, "var%", var)
    for r in b["runs"]:
        cr = r.get("chunk_rate")
        if cr:
            print("   run dur=%.3fs chunks=%d rate min/med/max=%.3g/%.3g/%.3g" % (
                r["duration_ns"] / 1e9, cr["count"], cr["min"], cr["median"], cr["max"]))
