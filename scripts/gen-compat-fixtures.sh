#!/usr/bin/env bash
# Generate / verify Swift<->Rust compatibility fixtures for GN Drive.
#
# Usage (run from anywhere; script cd's to repo root):
#   scripts/gen-compat-fixtures.sh
#       Regenerate crates/gn-drive-core/tests/fixtures/{locked,plain,expected,expected.json}
#       using the real Swift GNDriveCore in temp dirs (MemorySecureStore; no keychain,
#       no ~/.config/gn-drive, no rclone, no network). Password: compat-test-pw-123.
#   scripts/gen-compat-fixtures.sh --verify <config-dir-copy>
#       Open a config dir with the Swift code (unlock with the password above if it has
#       auth.json, open gn-drive.db) and write <dir>/swift-dump.json (same shape as
#       expected.json, plus rcloneConf/authSetup). Unlock REWRITES files, so pass a
#       temp COPY; the committed fixtures locked/ and plain/ are refused.
#
# Implementation: Tests/GNDriveCoreTests/CompatFixtureGenerator.swift, skipped unless
# GN_DRIVE_GEN_FIXTURES / GN_DRIVE_VERIFY_DIR is set. First build is slow (SwiftPM).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
FIX="$ROOT/crates/gn-drive-core/tests/fixtures"

run() { echo "+ $*" >&2; "$@"; }

if [[ "${1:-}" == "--verify" ]]; then
  [[ $# -eq 2 ]] || { echo "usage: $0 --verify <config-dir-copy>" >&2; exit 2; }
  [[ -d "$2" ]] || { echo "not a directory: $2" >&2; exit 2; }
  DIR="$(cd "$2" && pwd -P)"
  for bad in "$FIX/locked" "$FIX/plain" "$HOME/.config/gn-drive"; do
    if [[ -d "$bad" && "$DIR" == "$(cd "$bad" && pwd -P)" ]]; then
      echo "refusing to verify in place ($DIR); pass a temp copy" >&2; exit 2
    fi
  done
  run env GN_DRIVE_VERIFY_DIR="$DIR" swift test --filter CompatFixtureGenerator/testVerifyConfigDir
  echo "wrote $DIR/swift-dump.json" >&2
elif [[ $# -eq 0 ]]; then
  mkdir -p "$FIX"
  run env GN_DRIVE_GEN_FIXTURES="$FIX" swift test --filter CompatFixtureGenerator/testGenerateFixtures
  echo "fixtures in $FIX" >&2
else
  echo "usage: $0 [--verify <config-dir-copy>]" >&2; exit 2
fi
