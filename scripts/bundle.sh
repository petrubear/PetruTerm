#!/usr/bin/env bash
# PetruTerm — macOS .app bundle script
# Usage: ./scripts/bundle.sh [--debug]
#
# Builds both binaries and packages each into its own .app bundle, so they
# can be installed side by side:
#   - petruterm      (wgpu/winit)  -> dist/PetruTerm.app
#   - gpui-petruterm (gpui)        -> dist/PetruTerm-gpui.app
#
# Requires: Rust toolchain, full Xcode.app (not just Command Line Tools) --
# the gpui dependency's build script compiles Metal shaders via
# `xcrun -sdk macosx metal`, which CLT alone does not provide.
#
# To install: open dist/PetruTerm.app / dist/PetruTerm-gpui.app

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST="$ROOT/dist"

# ── flags ────────────────────────────────────────────────────────────────────
PROFILE="release"
for arg in "$@"; do
    case $arg in
        --debug) PROFILE="debug" ;;
    esac
done

VERSION="$(grep '^version' "$ROOT/Cargo.toml" | head -1 | sed 's/.*= "\(.*\)"/\1/')"

# ── per-binary bundle ────────────────────────────────────────────────────────
# Args: bin_name app_dir_name bundle_id display_name icon_png
build_bundle() {
    local BIN_NAME="$1" APP_DIR_NAME="$2" BUNDLE_ID="$3" DISPLAY_NAME="$4" ICON_PNG_NAME="$5"
    local APP="$DIST/$APP_DIR_NAME"
    local CONTENTS="$APP/Contents"
    local MACOS_DIR="$CONTENTS/MacOS"
    local RESOURCES="$CONTENTS/Resources"

    echo "==> Building $BIN_NAME ($PROFILE)..."
    if [ "$PROFILE" = "release" ]; then
        # RUSTFLAGS: target-cpu=apple-m1 enables AMX, SHA3, and other M1 ISA extensions.
        # Produces a binary optimised for Apple Silicon — NOT portable to Intel Macs.
        RUSTFLAGS="-C target-cpu=apple-m1" cargo build --release --bin "$BIN_NAME" --manifest-path "$ROOT/Cargo.toml"
        BINARY="$ROOT/target/release/$BIN_NAME"
    else
        cargo build --bin "$BIN_NAME" --manifest-path "$ROOT/Cargo.toml"
        BINARY="$ROOT/target/debug/$BIN_NAME"
    fi

    echo "==> Creating $APP_DIR_NAME structure..."
    rm -rf "$APP"
    mkdir -p "$MACOS_DIR" "$RESOURCES"

    echo "==> Copying binary..."
    cp "$BINARY" "$MACOS_DIR/$BIN_NAME"
    if [ "$PROFILE" = "release" ]; then
        strip "$MACOS_DIR/$BIN_NAME"
    fi

    printf 'APPL????' > "$CONTENTS/PkgInfo"

    echo "==> Writing Info.plist..."
    cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>${DISPLAY_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${DISPLAY_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>${BIN_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <!-- Retina / HiDPI: required for correct scale_factor on Apple Silicon -->
    <key>NSHighResolutionCapable</key>
    <true/>
    <!-- GPU switching: let macOS pick the best GPU -->
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>CFBundleSupportedPlatforms</key>
    <array>
        <string>MacOSX</string>
    </array>
    <!-- Required by UNUserNotificationCenter: OS won't show permission dialog without it -->
    <key>NSUserNotificationUsageDescription</key>
    <string>PetruTerm uses notifications to alert you when long-running commands finish.</string>
    <!-- Icon — place AppIcon.icns in Resources/ to override -->
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
</dict>
</plist>
PLIST

    echo "==> Copying default config..."
    cp -r "$ROOT/config/default" "$RESOURCES/config"

    local ICON_PNG="$ROOT/assets/$ICON_PNG_NAME"
    local ICON_ICNS="$RESOURCES/AppIcon.icns"
    if [ -f "$ICON_PNG" ]; then
        echo "==> Generating AppIcon.icns from $ICON_PNG_NAME..."
        local ICONSET="$DIST/${APP_DIR_NAME%.app}.iconset"
        mkdir -p "$ICONSET"
        for size in 16 32 64 128 256 512; do
            sips -z $size $size "$ICON_PNG" --out "$ICONSET/icon_${size}x${size}.png" > /dev/null
            double=$((size * 2))
            sips -z $double $double "$ICON_PNG" --out "$ICONSET/icon_${size}x${size}@2x.png" > /dev/null
        done
        iconutil -c icns "$ICONSET" -o "$ICON_ICNS"
        rm -rf "$ICONSET"
    else
        echo "    (no $ICON_PNG_NAME found — skipping icon)"
    fi

    echo "==> Signing (ad-hoc)..."
    codesign --force --deep --sign - "$APP"

    local SIZE="$(du -sh "$APP" | cut -f1)"
    echo ""
    echo "  Bundle : $APP"
    echo "  Size   : $SIZE"
    echo "  Version: $VERSION"
    echo ""
}

build_bundle "petruterm" "PetruTerm.app" "com.petruterm.app" "PetruTerm" "AppIcon.png"
build_bundle "gpui-petruterm" "PetruTerm-gpui.app" "com.petruterm.gpui.app" "PetruTerm (gpui)" "AppIcon-gpui.png"

echo "  open \"$DIST/PetruTerm.app\""
echo "  open \"$DIST/PetruTerm-gpui.app\""
