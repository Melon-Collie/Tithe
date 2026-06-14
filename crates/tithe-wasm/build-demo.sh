#!/bin/bash
# Build the WASM module into the web UI folder, then serve it statically — the
# in-browser demo with NO server backend (the sim runs as WASM inside the page).
#
# Requires: wasm-pack (`cargo install wasm-pack`) and the wasm target
# (`rustup target add wasm32-unknown-unknown`). Serves with Python's http.server;
# any static file server works (the point is: no application backend).
set -euo pipefail
cd "$(dirname "$0")/../.."

echo "Building WASM (release) into crates/tithe-web/web/pkg ..."
wasm-pack build crates/tithe-wasm --target web --out-dir ../tithe-web/web/pkg --release

PORT="${1:-8000}"
echo
echo "Serving crates/tithe-web/web/ at http://localhost:${PORT}/  (Ctrl-C to stop)"
echo "The page loads ./pkg and runs the sim as WASM — no axum, no /api."
python -m http.server "$PORT" --directory crates/tithe-web/web
