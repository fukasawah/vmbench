#!/usr/bin/env python3
"""Human-readable summary for vmbench reports.

Prints one headline metric per benchmark in human units, a few derived
scaling numbers, and (with --baseline) category indices computed the way SPEC
CPU and Geekbench aggregate results: per-test ratio to a baseline report, then
geometric mean. Storage results are grouped by target name and measurement
method (blocking streams vs io_uring); the two methods are never mixed.

Usage:
    tools/summary.py result.json
    tools/summary.py candidate.json --baseline reference.json
    tools/summary.py candidate.json --baseline reference.json --full
"""
import argparse
import json
import math
import sys

# Categories: (label, predicate over benchmark id). Order is report order.
CATEGORIES = [
    ("cpu single int", lambda i: i.startswith("cpu.int.")),
    ("cpu single fp", lambda i: i.startswith("cpu.fp.")),
    ("cpu isa", lambda i: i.startswith("cpu.isa.")),
    ("cpu branch/load-store", lambda i: i.startswith("cpu.branch.") or i.startswith("cpu.loadstore.")),
    ("cpu multi/atomic", lambda i: i.startswith("cpu.multicore.") or i.startswith("cpu.atomic.")),
    (
        "memory latency",
        lambda i: i.startswith(
            ("memory.latency.", "memory.mlp.", "memory.tlb.", "memory.page_size.",
             "memory.numa.", "cache.latency.")
        ),
    ),
    (
        "memory bandwidth",
        lambda i: i.startswith(("memory.bandwidth.", "cache.bandwidth.")),
    ),
    ("storage: seq/sync", lambda i: i.startswith("storage.") and _is_fixed(i)),
    ("storage: streams", lambda i: i.startswith("storage.streams.")),
    ("storage: io_uring", lambda i: i.startswith("storage.uring.")),
    ("linux/scheduler", lambda i: i.startswith("linux.")),
]

SPECIAL_HEADLINES = {
    "cpu.mix.v1": ("iterations_per_sec", "high"),
    "cpu.sustained.v1": ("throughput_median", "high"),
    "cpu.core2core.0_1.v1": ("ns_per_roundtrip", "low"),
    "cpu.core2core.0_2.v1": ("ns_per_roundtrip", "low"),
    "cpu.core2core.0_half.v1": ("ns_per_roundtrip", "low"),
    "cpu.core2core.0_last.v1": ("ns_per_roundtrip", "low"),
    "cpu.core2core.smt_siblings.v1": ("ns_per_roundtrip", "low"),
    "linux.syscall.getpid.v1": ("ns_per_syscall", "low"),
    "linux.ctxswitch.futex.v1": ("ns_per_switch", "low"),
    "linux.wakeup.futex.v1": ("wakeup_ns_median", "low"),
    "linux.scheduler.jitter.v1": ("jitter_ns_median", "low"),
    "linux.pagefault.minor.v1": ("ns_per_fault", "low"),
    "storage.seq.read.v1": ("iops", "high"),
    "storage.seq.write.v1": ("iops", "high"),
    "storage.sync.fdatasync.4k.v1": ("iops", "high"),
    "storage.sync.dsync.4k.v1": ("iops", "high"),
    "storage.buffered.read.warm.v1": ("bytes_per_sec", "high"),
    "cache.latency.curve.v1": ("effective_capacity_bytes", "high"),
}

# Headline concurrency for storage sweeps. Both families use the same level so
# the table stays comparable, but scoring never crosses the two families.
SWEEP_HEADLINE_X = 16


def _is_fixed(i):
    return (
        i.startswith("storage.seq.")
        or i.startswith("storage.sync.")
        or i.startswith("storage.sustained.")
        or i.startswith("storage.buffered.")
    )


def headline(b):
    """Returns (metric_key, curve_x, direction, unit)."""
    bid = b["id"]
    if bid in SPECIAL_HEADLINES:
        key, direction = SPECIAL_HEADLINES[bid]
        unit = next((m["unit"] for m in b["metrics"] if m["key"] == key), "")
        return key, None, direction, unit
    for m in b["metrics"]:
        if m["kind"] != "scalar":
            continue
        unit = m["unit"]
        if m["key"] == "ops_per_sec":
            return "ops_per_sec", None, "high", unit
        if m["key"] == "iterations_per_sec":
            return "iterations_per_sec", None, "high", unit
        if m["key"] == "ns_per_op":
            return "ns_per_op", None, "low", unit
        if m["key"] == "ns_per_access":
            return "ns_per_access", None, "low", unit
        if m["key"] == "bytes_per_sec":
            return "bytes_per_sec", None, "high", unit
    for m in b["metrics"]:
        if m["kind"] == "curve" and m["key"] == "iops":
            return "iops", SWEEP_HEADLINE_X, "high", m["unit"]
    for m in b["metrics"]:
        if m["kind"] == "curve" and m["key"] == "ns_per_access":
            return "ns_per_access", "max", "low", m["unit"]
    return None, None, None, None


def headline_value(b):
    key, x, direction, unit = headline(b)
    if key is None:
        return None, None, None, None
    med = b.get("median", {})
    v = med.get(key)
    if v is None:
        return None, None, None, None
    if x is not None:
        if not isinstance(v, list) or not v:
            return None, None, None, None
        if x == "max":
            best = max(v, key=lambda p: p.get("x", 0))
        else:
            best = min(v, key=lambda p: abs(p.get("x", 0) - x))
        v = best.get("y")
    if not isinstance(v, (int, float)) or v <= 0:
        return None, None, None, None
    return v, direction, unit, x


def curve_at(b, key, x):
    pts = b.get("median", {}).get(key)
    if not isinstance(pts, list) or not pts:
        return None
    best = min(pts, key=lambda p: abs(p.get("x", 0) - x))
    return best.get("y")


def entry_key(b):
    t = b.get("parameters", {}).get("target")
    return (b["id"], t)


def index(d):
    out = {}
    for b in d["benchmarks"]:
        out[entry_key(b)] = b
    return out


def fmt_rate(v):
    if v is None:
        return "-"
    if v >= 1e9:
        return "%.2f G/s" % (v / 1e9)
    if v >= 1e6:
        return "%.1f M/s" % (v / 1e6)
    if v >= 1e3:
        return "%.1f k/s" % (v / 1e3)
    return "%.1f /s" % v


def fmt_bytes(v):
    if v is None:
        return "-"
    if v >= 2**30:
        return "%.2f GiB/s" % (v / 2**30)
    if v >= 2**20:
        return "%.1f MiB/s" % (v / 2**20)
    if v >= 2**10:
        return "%.1f KiB/s" % (v / 2**10)
    return "%.0f B/s" % v


def fmt_ns(v):
    if v is None:
        return "-"
    if v >= 1e6:
        return "%.2f ms" % (v / 1e6)
    if v >= 1e3:
        return "%.2f us" % (v / 1e3)
    return "%.2f ns" % v


def fmt_size(v):
    if v is None:
        return "-"
    if v >= 2**30:
        return "%.2f GiB" % (v / 2**30)
    if v >= 2**20:
        return "%.1f MiB" % (v / 2**20)
    if v >= 2**10:
        return "%.1f KiB" % (v / 2**10)
    return "%.0f B" % v


def fmt_value(v, unit):
    if v is None:
        return "-"
    if unit == "IOPS" or unit == "ops/s" or unit == "iter/s":
        return fmt_rate(v)
    if unit == "bytes/s":
        return fmt_bytes(v)
    if unit == "ns":
        return fmt_ns(v)
    if unit == "cycles" or unit in ("", "count"):
        return "%.3g" % v
    return "%.4g %s" % (v, unit)


def table(rows, headers):
    out = ["| " + " | ".join(headers) + " |", "|" + "|".join(["---"] * len(headers)) + "|"]
    out += ["| " + " | ".join(r) + " |" for r in rows]
    return "\n".join(out) + "\n"


def ratio_for(base_b, cand_b):
    vb, db, ub, xb = headline_value(base_b)
    vc, dc, uc, xc = headline_value(cand_b)
    if vb is None or vc is None or db != dc:
        return None
    if db == "high":
        return vc / vb
    return vb / vc


def geomean(values):
    values = [v for v in values if v is not None and v > 0]
    if not values:
        return None, 0
    return math.exp(sum(math.log(v) for v in values) / len(values)), len(values)


def category_name(bid):
    for label, pred in CATEGORIES:
        if pred(bid):
            return label
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("report")
    ap.add_argument("--baseline")
    ap.add_argument("--full", action="store_true", help="also list excluded/mismatched tests")
    args = ap.parse_args()

    cand = json.load(open(args.report, encoding="utf-8"))
    base = json.load(open(args.baseline, encoding="utf-8")) if args.baseline else None
    C = index(cand)
    B = index(base) if base else {}

    lines = []
    lines.append("# vmbench summary\n")
    lines.append("report: `%s`  (suite %s, %.0f s)\n" % (
        args.report, cand.get("benchmark_suite_version"), cand.get("duration", {}).get("total_s", 0)))
    if base:
        lines.append("baseline: `%s`  (suite %s)\n" % (
            args.baseline, base.get("benchmark_suite_version")))

    # ---- headline table -------------------------------------------------
    lines.append("## Headline metrics\n")
    headers = ["benchmark", "target", "value"]
    if base:
        headers += ["baseline", "cand/base"]
    rows = []
    for b in cand["benchmarks"]:
        bid = b["id"]
        if not (bid.startswith(("cpu.", "memory.", "cache.", "linux.", "storage."))):
            continue
        if b["status"] != "ok":
            continue
        v, direction, unit, x = headline_value(b)
        label = bid
        if x is not None:
            label += " @x=%s" % (x if x == "max" else "%g" % x)
        target = b.get("parameters", {}).get("target", "")
        row = [label, target or "-", fmt_value(v, unit)]
        if base:
            bb = B.get(entry_key(b))
            vb = None
            r = None
            if bb is not None and bb["status"] == "ok":
                vb, _, _, _ = headline_value(bb)
                r = ratio_for(bb, b)
            if vb is not None and r is not None:
                mark = ""
                if direction == "low":
                    mark = " (faster)" if r > 1 else " (slower)"
                row.append(fmt_value(vb, unit))
                row.append("%.2fx%s" % (r, mark))
            else:
                row += ["-", "-"]
        rows.append(row)
    lines.append(table(rows, headers))

    # ---- category indices ----------------------------------------------
    if base:
        lines.append("## Category index (geometric mean of per-test ratios)\n")
        lines.append("A value > 1.0 means the candidate is faster. Equal weight per test;")
        lines.append("storage rows are split by target and method and never mixed.\n")
        groups = {}
        mismatched = []
        for key, b in C.items():
            bid, target = key
            label = category_name(bid)
            if label is None or b["status"] != "ok":
                continue
            bb = B.get(key)
            if bb is None or bb["status"] != "ok":
                continue
            cb = b.get("implementation", {}).get("code_hash")
            rb = bb.get("implementation", {}).get("code_hash")
            if cb and rb and cb != rb:
                mismatched.append(bid)
                continue
            r = ratio_for(bb, b)
            if r is None:
                continue
            gname = label
            if target:
                gname = "%s [%s]" % (label, target)
            groups.setdefault(gname, []).append(r)
        rows = []
        for gname in sorted(groups):
            g, n = geomean(groups[gname])
            rows.append([gname, str(n), "%.3fx" % g if g else "-"])
        lines.append(table(rows, ["category", "tests", "candidate/baseline"]))
        if mismatched:
            lines.append("WARNING: %d test(s) skipped because the kernel code hash differs:" % len(set(mismatched)))
            lines.append(", ".join(sorted(set(mismatched))) + "\n")

    # ---- derived scaling ------------------------------------------------
    lines.append("## Derived scaling\n")
    rows = []
    mc1 = C.get(("cpu.multicore.1.v1", None))
    online = cand.get("cpu", {}).get("online_cpus") or 1
    if mc1 and mc1["status"] == "ok":
        base_rate = mc1["median"].get("ops_per_sec")
        for k, bid in [(2, "cpu.multicore.2.v1"), (4, "cpu.multicore.4.v1"),
                       (8, "cpu.multicore.8.v1"), (online, "cpu.multicore.all.v1")]:
            b = C.get((bid, None))
            if not b or b["status"] != "ok" or not base_rate:
                continue
            rate = b["median"].get("ops_per_sec")
            if not rate:
                continue
            eff = rate / (base_rate * min(k, online))
            rows.append(["multicore x%d" % min(k, online), "%.0f%% of linear" % (eff * 100)])
    for fam in ("streams", "uring"):
        wid = "storage.%s.read.4k.sweep.v1" % fam
        entries = [v for (bid, t), v in sorted(C.items(), key=lambda kv: (kv[0][0], kv[0][1] or "")) if bid == wid]
        for b in entries:
            if b["status"] != "ok":
                continue
            t = b.get("parameters", {}).get("target", "?")
            r1 = curve_at(b, "iops", 1)
            levels = [p["x"] for p in b["median"].get("iops", [])]
            if not r1 or not levels:
                continue
            top = max(levels)
            rt = curve_at(b, "iops", top)
            if rt:
                rows.append(["storage %s read [%s]" % (fam, t),
                             "x%d = %.2fx of x1" % (top, rt / r1)])
    cl = C.get(("cache.latency.curve.v1", None))
    if cl and cl["status"] == "ok":
        cap = cl["median"].get("effective_capacity_bytes")
        if cap:
            rows.append(["effective cache capacity", fmt_size(cap)])
    lines.append(table(rows, ["metric", "value"]))

    if args.full:
        lines.append("## All entries\n")
        rows = []
        for b in cand["benchmarks"]:
            rows.append([b["id"], b.get("parameters", {}).get("target", "-"), b["status"]])
        lines.append(table(rows, ["benchmark", "target", "status"]))

    sys.stdout.write("\n".join(lines))


if __name__ == "__main__":
    main()
