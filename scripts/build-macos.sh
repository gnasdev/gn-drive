#!/bin/bash

# NG Drive macOS Production Build Script
# Creates a signed .app bundle for macOS distribution

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
APP_NAME="ng-drive"
APP_DISPLAY_NAME="NG Drive"
BUNDLE_ID="com.ngdrive.app"
VERSION="${VERSION:-1.0.0}"
COPYRIGHT="Copyright $(date +%Y)"

# Paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
DESKTOP_DIR="$PROJECT_ROOT/desktop"
FRONTEND_DIR="$DESKTOP_DIR/frontend"
BUILD_DIR="$DESKTOP_DIR/bin"
APP_BUNDLE="$BUILD_DIR/$APP_NAME.app"
OUTPUT_APP="$PROJECT_ROOT/$APP_NAME.app"

# Functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_prerequisites() {
    log_info "Checking prerequisites..."

    # Check Go
    if ! command -v go &> /dev/null; then
        log_error "Go is not installed. Please install Go 1.25+"
        exit 1
    fi
    GO_VERSION=$(go version | grep -oE '[0-9]+\.[0-9]+' | head -1)
    log_info "Go version: $GO_VERSION"

    # Check Node.js
    if ! command -v node &> /dev/null; then
        log_error "Node.js is not installed. Please install Node.js 18+"
        exit 1
    fi
    NODE_VERSION=$(node --version)
    log_info "Node.js version: $NODE_VERSION"

    # Check npm
    if ! command -v npm &> /dev/null; then
        log_error "npm is not installed. Please install npm"
        exit 1
    fi

    # Check/Install wails3
    if ! command -v wails3 &> /dev/null; then
        log_warning "wails3 not found. Installing..."
        go install github.com/wailsapp/wails/v3/cmd/wails3@latest
        export PATH="$PATH:$(go env GOPATH)/bin"
    fi
    WAILS_VERSION=$(wails3 version 2>/dev/null | head -1 || echo "unknown")
    log_info "Wails version: $WAILS_VERSION"

    log_success "All prerequisites satisfied"
}

clean_build() {
    log_info "Cleaning previous build artifacts..."
    rm -rf "$BUILD_DIR"
    rm -rf "$OUTPUT_APP"
    rm -rf "$FRONTEND_DIR/dist"
    mkdir -p "$BUILD_DIR"
    log_success "Clean complete"
}

generate_bindings() {
    log_info "Generating Wails bindings..."
    cd "$DESKTOP_DIR"
    wails3 generate bindings -ts ./...

    # Create wailsjs symlink if not exists
    if [ ! -L "$FRONTEND_DIR/wailsjs" ]; then
        cd "$FRONTEND_DIR"
        ln -sf bindings wailsjs
    fi

    log_success "Bindings generated"
}

build_frontend() {
    log_info "Building frontend..."
    cd "$FRONTEND_DIR"

    # Install dependencies
    npm ci --legacy-peer-deps 2>/dev/null || npm install --legacy-peer-deps

    # Build production
    npm run build

    log_success "Frontend built"
}

build_backend() {
    log_info "Building backend binary..."
    cd "$DESKTOP_DIR"

    # Build with optimizations
    CGO_ENABLED=1 go build \
        -trimpath \
        -ldflags="-s -w -X main.Version=$VERSION" \
        -o "$BUILD_DIR/$APP_NAME"

    log_success "Backend binary built"
}

create_app_bundle() {
    log_info "Creating macOS app bundle..."

    # Create directory structure
    mkdir -p "$APP_BUNDLE/Contents/MacOS"
    mkdir -p "$APP_BUNDLE/Contents/Resources"

    # Copy binary
    cp "$BUILD_DIR/$APP_NAME" "$APP_BUNDLE/Contents/MacOS/"

    # Create Info.plist
    cat > "$APP_BUNDLE/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
    <dict>
        <key>CFBundlePackageType</key>
        <string>APPL</string>
        <key>CFBundleName</key>
        <string>$APP_DISPLAY_NAME</string>
        <key>CFBundleExecutable</key>
        <string>$APP_NAME</string>
        <key>CFBundleIdentifier</key>
        <string>$BUNDLE_ID</string>
        <key>CFBundleVersion</key>
        <string>$VERSION</string>
        <key>CFBundleGetInfoString</key>
        <string>$APP_DISPLAY_NAME - Cloud Storage Sync</string>
        <key>CFBundleShortVersionString</key>
        <string>$VERSION</string>
        <key>CFBundleIconFile</key>
        <string>iconfile</string>
        <key>LSMinimumSystemVersion</key>
        <string>10.13.0</string>
        <key>NSHighResolutionCapable</key>
        <true/>
        <key>NSHumanReadableCopyright</key>
        <string>$COPYRIGHT</string>
    </dict>
</plist>
EOF

    # Create PkgInfo
    echo -n "APPL????" > "$APP_BUNDLE/Contents/PkgInfo"

    # Create app icon
    create_app_icon

    log_success "App bundle created"
}

create_app_icon() {
    log_info "Creating app icon..."

    local ICON_SOURCE="$DESKTOP_DIR/build/appicon.png"
    local ICONSET_DIR="/tmp/ng-drive-icons.iconset"
    local ICON_DEST="$APP_BUNDLE/Contents/Resources/iconfile.icns"

    if [ ! -f "$ICON_SOURCE" ]; then
        log_warning "App icon not found at $ICON_SOURCE, skipping icon creation"
        return
    fi

    # Create iconset directory
    rm -rf "$ICONSET_DIR"
    mkdir -p "$ICONSET_DIR"

    # Generate icon sizes
    sips -z 16 16 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_16x16.png" > /dev/null 2>&1
    sips -z 32 32 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_16x16@2x.png" > /dev/null 2>&1
    sips -z 32 32 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_32x32.png" > /dev/null 2>&1
    sips -z 64 64 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_32x32@2x.png" > /dev/null 2>&1
    sips -z 128 128 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_128x128.png" > /dev/null 2>&1
    sips -z 256 256 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_128x128@2x.png" > /dev/null 2>&1
    sips -z 256 256 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_256x256.png" > /dev/null 2>&1
    sips -z 512 512 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_256x256@2x.png" > /dev/null 2>&1
    sips -z 512 512 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_512x512.png" > /dev/null 2>&1
    sips -z 1024 1024 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_512x512@2x.png" > /dev/null 2>&1

    # Convert to icns
    iconutil -c icns "$ICONSET_DIR" -o "$ICON_DEST"

    # Cleanup
    rm -rf "$ICONSET_DIR"

    log_success "App icon created"
}

sign_app() {
    log_info "Signing app bundle..."

    local SIGNING_IDENTITY="${SIGNING_IDENTITY:--}"

    if [ "$SIGNING_IDENTITY" = "-" ]; then
        log_info "Using ad-hoc signing (local development)"
    else
        log_info "Using signing identity: $SIGNING_IDENTITY"
    fi

    codesign --force --deep --sign "$SIGNING_IDENTITY" "$APP_BUNDLE"

    # Verify signature
    codesign --verify --verbose "$APP_BUNDLE"

    log_success "App signed successfully"
}

copy_to_output() {
    log_info "Copying app bundle to project root..."
    rm -rf "$OUTPUT_APP"
    cp -R "$APP_BUNDLE" "$OUTPUT_APP"
    log_success "App bundle available at: $OUTPUT_APP"
}

show_summary() {
    echo ""
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}       Build Complete!${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo ""
    echo -e "App Bundle: ${BLUE}$OUTPUT_APP${NC}"
    echo -e "Version:    ${BLUE}$VERSION${NC}"
    echo -e "Bundle ID:  ${BLUE}$BUNDLE_ID${NC}"
    echo ""
    echo -e "To run the app:"
    echo -e "  ${YELLOW}open $OUTPUT_APP${NC}"
    echo ""
    echo -e "To install to Applications:"
    echo -e "  ${YELLOW}cp -R $OUTPUT_APP /Applications/${NC}"
    echo ""

    # Show signature info
    echo -e "Signature info:"
    codesign -dvv "$OUTPUT_APP" 2>&1 | grep -E "(Identifier|Format|Signature)" | sed 's/^/  /'
    echo ""
}

# Main execution
main() {
    echo ""
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}  NG Drive macOS Production Build${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""

    check_prerequisites
    clean_build
    generate_bindings
    build_frontend
    build_backend
    create_app_bundle
    sign_app
    copy_to_output
    show_summary
}

# Run main function
main "$@"
