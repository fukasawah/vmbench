#!/usr/bin/env bash
# Development build helper for /mnt/c (DrvFs has coarse mtimes; bumping the
# source timestamps into the future makes cargo always see the change).
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
find src build.rs Cargo.toml Cargo.lock -type f -exec touch -d '+2 minutes' {} +
cargo build --release "$@"
