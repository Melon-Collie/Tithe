#!/bin/bash
# SessionStart hook: provision the Rust toolchain so `cargo` works in Claude Code
# on the web / mobile sessions. Runs only in the remote environment; locally the
# user already has rustup + cargo, so this is a no-op.
#
# The sim uses recent stable Rust (e.g. `Option::is_none_or`), so an old distro
# `cargo` won't compile it — we provision the channel pinned by
# `rust-toolchain.toml` via rustup.
set -euo pipefail

# Async: the session starts immediately while this installs in the background.
# Anything needing `cargo` must first block on readiness via
# .claude/hooks/wait-for-rust.sh (see CLAUDE.md > Workflow).
echo '{"async": true, "asyncTimeout": 600000}'

READY_MARKER="/tmp/.rust-toolchain-ready"
rm -f "$READY_MARKER"

# Only run in Claude Code on the web/mobile; no-op on the user's local machine.
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

# Make a standard rustup/cargo install visible for the rest of this script.
export PATH="$HOME/.cargo/bin:$PATH"

# Install rustup if missing (its installer also lays down a default stable).
if ! command -v rustup >/dev/null 2>&1; then
  echo "[session-start] Installing rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --no-modify-path --profile minimal
  export PATH="$HOME/.cargo/bin:$PATH"
fi

# Pre-install the toolchain pinned by rust-toolchain.toml (channel + components),
# so the first `cargo` in the session doesn't pay for a lazy install mid-command.
echo "[session-start] Provisioning the pinned Rust toolchain..."
(cd "${CLAUDE_PROJECT_DIR:-.}" && rustup show >/dev/null 2>&1 || true)
rustup component add rustfmt clippy >/dev/null 2>&1 || true

# Signal readiness so wait-for-rust.sh can release any blocked cargo commands.
touch "$READY_MARKER"
echo "[session-start] Rust ready: $(cargo --version 2>/dev/null || echo unknown)"
