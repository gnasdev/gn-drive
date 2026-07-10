#!/usr/bin/env bash
# scripts/dev.sh — run Vue watch + Go hot reload (air) in parallel.
#
# Vue side:   `vite build --watch` rebuilds the SPA into
#             ../internal/webui/dist/ on every source change.
# Go side:    `air` watches Go files + internal/webui/dist/ and restarts
#             the server binary on any change.
# Browser:    served by the Go binary's SPA route on a fixed dev port
#             (default 53241, set in .air.toml --port). Fixed so air
#             restarts do not change the URL. No separate Vite dev server.
#
# Both processes are run in the background and their PIDs are tracked so
# we can shut them down together on Ctrl+C.
# Note: we deliberately avoid `set -e` because pnpm install prints
# non-fatal warnings (e.g. ERR_PNPM_IGNORED_BUILDS) that can return
# non-zero exit codes, and we want dev to keep running.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PID_VITE=""
PID_AIR=""

cleanup() {
  echo ""
  echo "dev: shutting down…"
  if [[ -n "$PID_VITE" ]] && kill -0 "$PID_VITE" 2>/dev/null; then
    kill "$PID_VITE" 2>/dev/null || true
  fi
  if [[ -n "$PID_AIR" ]] && kill -0 "$PID_AIR" 2>/dev/null; then
    # air's child process is the Go binary; ask air to stop, then hard-kill.
    kill "$PID_AIR" 2>/dev/null || true
  fi
  # Give them 2s, then force.
  sleep 2
  if [[ -n "$PID_VITE" ]] && kill -0 "$PID_VITE" 2>/dev/null; then
    kill -9 "$PID_VITE" 2>/dev/null || true
  fi
  if [[ -n "$PID_AIR" ]] && kill -0 "$PID_AIR" 2>/dev/null; then
    kill -9 "$PID_AIR" 2>/dev/null || true
  fi
  echo "dev: done."
}
trap cleanup EXIT INT TERM

# Pre-flight checks
command -v pnpm >/dev/null 2>&1 || { echo "ERROR: pnpm not installed (run: npm i -g pnpm)"; exit 1; }
command -v go >/dev/null 2>&1 || { echo "ERROR: go not in PATH"; exit 1; }

# Resolve air: prefer $GOPATH/bin/air, fall back to PATH.
AIR_BIN="${GOPATH:-$HOME/go}/bin/air"
if [[ ! -x "$AIR_BIN" ]]; then AIR_BIN="$(command -v air || true)"; fi
if [[ -z "$AIR_BIN" || ! -x "$AIR_BIN" ]]; then
  echo "ERROR: air not found. Install with: go install github.com/cosmtrek/air@latest"
  exit 1
fi

# Build the frontend once up-front so the Go server can serve a real
# index.html on its first start. air will pick up subsequent rebuilds
# (it watches internal/webui/dist/).
echo "dev: building frontend (initial)…"
# pnpm install prints warnings to stderr (e.g. ERR_PNPM_IGNORED_BUILDS) which
# pnpm treats as non-zero exit. We don't care about that for dev — only the
# build itself matters. So we run install with `|| true` to keep going.
( cd frontend && pnpm install --frozen-lockfile >/dev/null 2>&1 || pnpm install >/dev/null 2>&1 || true )
# Build directly into internal/webui/dist — the path the Go binary embeds via
# `//go:embed all:dist`. (The default `vite build` outDir is frontend/dist,
# which the Go embed never sees, so the dev build must override it here.)
# Use `pnpm exec vite` (not `pnpm run build -- …`): `pnpm run` injects a
# literal `--` separator that vite ignores, dropping the --outDir override.
( cd frontend && pnpm exec vite build --mode development --sourcemap --outDir ../internal/webui/dist --emptyOutDir )
echo "dev: initial build done."
echo ""

# --- Start Vite watch in background -----------------------------------
echo "dev: starting Vite --watch (foreground output piped to this terminal)…"
( cd frontend && pnpm exec vite build --watch --mode development --sourcemap --outDir ../internal/webui/dist --emptyOutDir ) &
PID_VITE=$!

# --- Start air in background ------------------------------------------
echo "dev: starting air (Go hot reload)…"
"$AIR_BIN" &
PID_AIR=$!

# --- Wait for either to exit -----------------------------------------
echo ""
DEV_PORT="${GN_DRIVE_DEV_PORT:-53241}"
echo "dev: both watchers up. Edit files to see live reload."
echo "dev: - Vue source (.vue/.ts in frontend/) → Vite rebuilds dist/ → Go restarts"
echo "dev: - Go source (.go) → air rebuilds and restarts the server"
echo "dev: - Open http://127.0.0.1:${DEV_PORT}/  (fixed port; air restarts keep this URL)"
echo "dev: Press Ctrl+C to stop both."
echo ""

# Wait for either backgrounded process to exit; if one dies, kill the other.
while true; do
  if ! kill -0 "$PID_VITE" 2>/dev/null; then
    echo "dev: Vite watch exited, stopping air…"
    break
  fi
  if ! kill -0 "$PID_AIR" 2>/dev/null; then
    echo "dev: air exited, stopping Vite…"
    break
  fi
  sleep 1
done

# trap will run cleanup
