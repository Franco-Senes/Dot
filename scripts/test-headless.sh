#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKTREE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TOOLCHAIN_ROOT="/home/franco/search-linux-2/.toolchain"

PROFILE="${CARGO_PROFILE:-release}"
CARGO_FLAGS=(--manifest-path "$WORKTREE_ROOT/Cargo.toml" --target-dir "$WORKTREE_ROOT/target")

if [[ "$PROFILE" == "release" ]]; then
  "$TOOLCHAIN_ROOT/sdk" cargo build --release -p search "${CARGO_FLAGS[@]}"
else
  "$TOOLCHAIN_ROOT/sdk" cargo build -p search "${CARGO_FLAGS[@]}"
fi

export SEARCH_BIN="$WORKTREE_ROOT/target/$PROFILE/search"

"$TOOLCHAIN_ROOT/sdk-xvfb" \
  cargo test -p search-bench --test headless_integration "${CARGO_FLAGS[@]}" -- \
  --ignored --nocapture --test-threads=1
