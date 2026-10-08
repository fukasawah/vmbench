#!/usr/bin/env bash
# Builds and audits a vmbench release for one target and packs a tar.gz.
#
# Usage:
#   tools/build.sh [target]
# Default target: x86_64-unknown-linux-gnu
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TARGET="${1:-x86_64-unknown-linux-gnu}"
BIN="target/$TARGET/release/vmbench"

echo "== building vmbench for $TARGET"
cargo build --release --target "$TARGET"

echo "== auditing $BIN"
"$ROOT/tools/audit.sh" "$BIN"

echo "== generating docs"
python3 "$ROOT/tools/gen_docs.py" "$BIN" "$ROOT/docs/BENCHMARKS.md"
python3 "$ROOT/tools/gen_status.py" "$BIN" "$ROOT/docs/STATUS.md"
python3 "$ROOT/tools/status_check.py" "$BIN" "$ROOT/docs/STATUS.md"

VER="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
NAME="vmbench-$VER-$TARGET"
STAGE="dist/$NAME"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp "$BIN" "$STAGE/vmbench"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
sha256sum "dist/$NAME.tar.gz" | tee "dist/$NAME.tar.gz.sha256"
echo "== done: dist/$NAME.tar.gz"
