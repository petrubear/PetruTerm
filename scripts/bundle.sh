#!/usr/bin/env bash
# PetruTerm — macOS .app bundle script
# Usage: ./scripts/bundle.sh [--debug]
#
# Output: dist/PetruTerm.app
# Requires: Rust toolchain, codesign (Xcode CLT)
#
# To install: open dist/PetruTerm.app
# To run from Finder: double-click, or `open dist/PetruTerm.app`

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/PetruTerm.app"
CONTENTS="$APP/Contents"
MACOS_DIR="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"

# ── flags ────────────────────────────────────────────────────────────────────
PROFILE="release"
for arg in "$@"; do
    case $arg in
        --debug) PROFILE="debug" ;;
    esac
done

# ── build ─────────────────────────────────────────────────────────────────────
echo "==> Building PetruTerm ($PROFILE)..."
if [ "$PROFILE" = "release" ]; then
    cargo build --release --manifest-path "$ROOT/Cargo.toml"
    BINARY="$ROOT/target/release/petruterm"
else
    cargo build --manifest-path "$ROOT/Cargo.toml"
    BINARY="$ROOT/target/debug/petruterm"
fi

# ── scaffold ──────────────────────────────────────────────────────────────────
echo "==> Creating bundle structure..."
rm -rf "$APP"
mkdir -p "$MACOS_DIR" "$RESOURCES"

# ── binary ────────────────────────────────────────────────────────────────────
echo "==> Copying binary..."
cp "$BINARY" "$MACOS_DIR/petruterm"

if [ "$PROFILE" = "release" ]; then
    strip "$MACOS_DIR/petruterm"
fi

# ── PkgInfo ───────────────────────────────────────────────────────────────────
printf 'APPL????' > "$CONTENTS/PkgInfo"

# ── Info.plist ────────────────────────────────────────────────────────────────
echo "==> Writing Info.plist..."
VERSION="$(grep '^version' "$ROOT/Cargo.toml" | head -1 | sed 's/.*= "\(.*\)"/\1/')"

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>PetruTerm</string>
    <key>CFBundleDisplayName</key>
    <string>PetruTerm</string>
    <key>CFBundleIdentifier</key>
    <string>com.petruterm.app</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>petruterm</string>
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
    <!-- Icon — place AppIcon.icns in Resources/ to override -->
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
</dict>
</plist>
PLIST

# ── default config ────────────────────────────────────────────────────────────
echo "==> Copying default config..."
cp -r "$ROOT/config/default" "$RESOURCES/config"

# ── icon (optional) ──────────────────────────────────────────────────────────
# Place a 1024×1024 PNG at assets/AppIcon.png and re-run to generate the icns.
ICON_PNG="$ROOT/assets/AppIcon.png"
ICON_ICNS="$RESOURCES/AppIcon.icns"
if [ -f "$ICON_PNG" ]; then
    echo "==> Generating AppIcon.icns from $ICON_PNG..."
    ICONSET="$DIST/AppIcon.iconset"
    mkdir -p "$ICONSET"
    for size in 16 32 64 128 256 512; do
        sips -z $size $size "$ICON_PNG" --out "$ICONSET/icon_${size}x${size}.png" > /dev/null
        double=$((size * 2))
        sips -z $double $double "$ICON_PNG" --out "$ICONSET/icon_${size}x${size}@2x.png" > /dev/null
    done
    iconutil -c icns "$ICONSET" -o "$ICON_ICNS"
    rm -rf "$ICONSET"
else
    echo "    (no AppIcon.png found — skipping icon; add assets/AppIcon.png to include one)"
fi

# ── ad-hoc code signing ───────────────────────────────────────────────────────
echo "==> Signing (ad-hoc)..."
codesign --force --deep --sign - "$APP"

# ── summary ───────────────────────────────────────────────────────────────────
SIZE="$(du -sh "$APP" | cut -f1)"
echo ""
echo "  Bundle : $APP"
echo "  Size   : $SIZE"
echo "  Version: $VERSION"
echo ""
echo "  open \"$APP\""
