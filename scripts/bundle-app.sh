#!/usr/bin/env bash
# Build a .app bundle for the SwiftUI app + CLI binary.
#
#   scripts/bundle-app.sh [release|debug] [version]
#
# Produces dist/GNDrive.app with both executables inside (the app binary as
# CFBundleExecutable, the CLI as gn-drive for launchd/service use).
set -euo pipefail

CONFIG="${1:-release}"
VERSION="${2:-dev}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

swift build -c "$CONFIG" --product GNDriveApp
swift build -c "$CONFIG" --product gn-drive

BIN_DIR="$(swift build -c "$CONFIG" --show-bin-path)"
APP_DIR="$ROOT/dist/GNDrive.app"

rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"

cp "$BIN_DIR/GNDriveApp" "$APP_DIR/Contents/MacOS/GNDriveApp"
cp "$BIN_DIR/gn-drive" "$APP_DIR/Contents/MacOS/gn-drive"

sed -e "s/\$(GN_VERSION)/$VERSION/" -e "s/\$(GN_BUILD)/$VERSION/" \
    scripts/packaging/Info.plist > "$APP_DIR/Contents/Info.plist"

echo "✓ built $APP_DIR ($CONFIG, $VERSION)"
echo "  Run: open $APP_DIR"
