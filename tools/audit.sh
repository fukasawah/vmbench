#!/usr/bin/env bash
# Verifies that a vmbench binary is self-contained and that every benchmark
# kernel's machine code matches the hash reported by the binary itself.
#
# Usage: tools/audit.sh [path-to-vmbench]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${1:-$ROOT/target/x86_64-unknown-linux-gnu/release/vmbench}"
QEMU_BIN="${QEMU:-}"

if [ ! -x "$BIN" ]; then
  echo "audit: binary not found: $BIN" >&2
  exit 1
fi

run_bin() {
  if [ -n "$QEMU_BIN" ]; then
    "$QEMU_BIN" "$BIN" "$@"
  else
    "$BIN" "$@"
  fi
}

echo "== dynamic dependencies"
if readelf -d "$BIN" 2>/dev/null | grep -q NEEDED; then
  echo "audit: FAIL: binary has dynamic dependencies" >&2
  exit 1
fi
echo "ok: no dynamic dependencies"

echo "== undefined symbols"
if nm -u "$BIN" | grep -q .; then
  echo "audit: FAIL: undefined symbols:" >&2
  nm -u "$BIN" >&2
  exit 1
fi
echo "ok: no undefined symbols"

echo "== PLT / GOT relocations"
if readelf -r "$BIN" 2>/dev/null | grep -qE "JUMP_SLOT|GLOB_DAT"; then
  echo "audit: FAIL: dynamic relocations present" >&2
  exit 1
fi
echo "ok: no dynamic relocations"

echo "== kernel machine-code hashes and call checks"
LIST="$(mktemp)"
run_bin --quiet --list > "$LIST"
OBJDUMP_BIN="${OBJDUMP:-}"
if [ -z "$OBJDUMP_BIN" ]; then
  if objdump -d "$BIN" >/dev/null 2>&1; then
    OBJDUMP_BIN=objdump
  else
    for cand in aarch64-linux-gnu-objdump llvm-objdump rust-objdump; do
      if command -v "$cand" >/dev/null 2>&1 && "$cand" -d "$BIN" >/dev/null 2>&1; then
        OBJDUMP_BIN="$cand"
        break
      fi
    done
  fi
fi
python3 "$ROOT/tools/audit_elf.py" "$BIN" "$LIST" "${OBJDUMP_BIN:-none}"
rm -f "$LIST"
