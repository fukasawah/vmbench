#!/usr/bin/env bash
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="/mnt/c/Users/fukas/Documents/projects/linux-vps-benchmark"
BIN="$ROOT/target/x86_64-unknown-linux-gnu/release/vmbench"
mkdir -p "$HOME/vmbench-run"
cd "$HOME/vmbench-run"
cp "$BIN" ./vmbench
echo "== native ext4 run"
timeout 900 ./vmbench --quick --quiet --only storage --output /tmp/storage_native.json
echo "rc=$?"
python3 - <<'PY'
import json
try:
    d = json.load(open("/tmp/storage_native.json"))
except Exception as e:
    print("no report:", e); raise SystemExit
for b in d["benchmarks"]:
    print(b["id"], b["status"], b["parameters"], b["median"])
PY
ls -la
