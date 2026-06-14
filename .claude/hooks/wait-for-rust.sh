#!/bin/bash
# Blocks until the Rust toolchain provisioned by the async session-start hook is
# ready, then returns. Run this once before the first `cargo` command in a Claude
# Code web/mobile session (see CLAUDE.md > Workflow). No-op once cargo is ready.
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"
READY_MARKER="/tmp/.rust-toolchain-ready"
TIMEOUT_SECS="${1:-600}"
waited=0

rust_ready() { command -v cargo >/dev/null 2>&1; }

if rust_ready; then
  echo "[wait-for-rust] ready: $(cargo --version)"
  exit 0
fi

echo "[wait-for-rust] waiting for the background Rust toolchain install..."
while [ "$waited" -lt "$TIMEOUT_SECS" ]; do
  if [ -f "$READY_MARKER" ] && rust_ready; then
    echo "[wait-for-rust] ready after ${waited}s: $(cargo --version)"
    exit 0
  fi
  sleep 3
  waited=$((waited + 3))
done

echo "[wait-for-rust] timed out after ${TIMEOUT_SECS}s; Rust not ready." >&2
echo "[wait-for-rust] check the session-start hook log for install errors." >&2
exit 1
