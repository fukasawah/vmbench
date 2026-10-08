#!/usr/bin/env bash
# Cross-build vmbench for aarch64 and smoke-test it under qemu-user.
set -uo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="/mnt/c/Users/fukas/Documents/projects/linux-vps-benchmark"
TOOLBIN="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin"
export PATH="$TOOLBIN:$PATH"
cd "$ROOT"

echo "== rust-lld: $(rust-lld --version | head -1)"
if ! cargo build --release --target aarch64-unknown-linux-gnu 2>/tmp/arm.log; then
  echo "== build errors"
  grep -E "^error" -A 10 /tmp/arm.log | head -120
  exit 1
fi
echo "== build ok"
BIN="target/aarch64-unknown-linux-gnu/release/vmbench"
file "$BIN"
if nm -u "$BIN" | grep -q .; then
  echo "undefined symbols:"; nm -u "$BIN" | head
else
  echo "no undefined symbols"
fi

if command -v qemu-aarch64 >/dev/null 2>&1; then
  echo "== qemu smoke test"
  qemu-aarch64 "$BIN" --version
  qemu-aarch64 "$BIN" --quick --only cpu.int.throughput,cpu.fp,cpu.isa,linux.syscall,cache.bandwidth.l1 --output /tmp/arm_report.json
  python3 -m json.tool /tmp/arm_report.json > /dev/null && echo "ARM JSON_VALID"
  python3 - <<'PY'
import json
d = json.load(open("/tmp/arm_report.json"))
for b in d["benchmarks"]:
    print(b["id"], b["parameters"].get("op"), b["status"], b.get("median"))
PY
else
  echo "qemu-aarch64 not found; skipping execution test"
fi
