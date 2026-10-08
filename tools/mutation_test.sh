#!/usr/bin/env bash
# Proves that the expected-value verification has teeth: break a kernel
# constant in a scratch copy of the tree, build it, and require the affected
# benchmark to be rejected with status "verification_failed".
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="${TMPDIR:-/tmp}/vmbench-mutation"
rm -rf "$TMP"
mkdir -p "$TMP/.cargo"
cp -r "$ROOT/src" "$TMP/"
cp "$ROOT/Cargo.toml" "$ROOT/Cargo.lock" "$ROOT/build.rs" "$ROOT/rust-toolchain.toml" "$TMP/"
cp "$ROOT/.cargo/config.toml" "$TMP/.cargo/"
cd "$TMP"

# Change only the kernel constant; the verification reference keeps the
# original value, so the two must disagree.
sed -i 's/0x9e37_79b9_7f4a_7c15/0x9e37_79b9_7f4a_7c16/g' src/bench/cpu_int.rs
cargo build --release >/dev/null 2>&1

BIN="target/x86_64-unknown-linux-gnu/release/vmbench"
"$BIN" --quick --quiet --only cpu.int.throughput.add --output "$TMP/mut.json" >/dev/null 2>&1 || true
python3 - "$TMP/mut.json" <<'PY'
import json
import sys

d = json.load(open(sys.argv[1]))
b = d["benchmarks"][0]
if b["status"] != "verification_failed":
    print("MUTATION FAIL: status=%s (expected verification_failed)" % b["status"])
    sys.exit(1)
print("MUTATION OK: broken kernel rejected (%s)" % b.get("reason", ""))
PY
rm -rf "$TMP"
