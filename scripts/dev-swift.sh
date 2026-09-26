#!/usr/bin/env bash
# Dev loop: rebuild + relaunch the SwiftUI app whenever Sources/ change.
# Uses fswatch (brew install fswatch); falls back to 1s polling.
set -uo pipefail

cd "$(dirname "$0")/.."
BIN_DIR="$(swift build --show-bin-path)"
APP_BIN="$BIN_DIR/GNDriveApp"
APP_PID=""

rebuild() {
  if swift build --product GNDriveApp; then
    [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null
    "$APP_BIN" &
    APP_PID=$!
    echo "→ relaunched GNDriveApp (pid $APP_PID)"
  else
    echo "✗ build failed — app left running"
  fi
}

stop() {
  [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null
}
trap stop EXIT INT TERM

rebuild

if command -v fswatch >/dev/null 2>&1; then
  fswatch -o -r Sources -e '\.build' | while read -r _; do
    rebuild
  done
else
  echo "(fswatch not found — polling every 1s)"
  sig() { find Sources -name '*.swift' -exec stat -f %m {} + | sort -n | tail -1; }
  LAST="$(sig)"
  while sleep 1; do
    CUR="$(sig)"
    if [ "$CUR" != "$LAST" ]; then
      LAST="$CUR"
      rebuild
    fi
  done
fi
