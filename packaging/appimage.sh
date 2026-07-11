#!/usr/bin/env bash
# Build Rustpad-<version>-x86_64.AppImage from the release binary.
# Used both locally and by the GitHub release workflow.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
APPDIR=target/appimage/Rustpad.AppDir
TOOL=target/appimage/appimagetool

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/icons/hicolor/256x256/apps"

cp target/release/rustpad-gui "$APPDIR/usr/bin/"
cp assets/rustpad.desktop "$APPDIR/"
cp assets/rustpad-256.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/rustpad.png"
cp assets/rustpad-256.png "$APPDIR/rustpad.png"
cp assets/rustpad-256.png "$APPDIR/.DirIcon"

cat > "$APPDIR/AppRun" << 'EOF'
#!/bin/sh
HERE=$(dirname "$(readlink -f "$0")")
exec "$HERE/usr/bin/rustpad-gui" "$@"
EOF
chmod +x "$APPDIR/AppRun"

if [ ! -x "$TOOL" ]; then
    curl -fsSL -o "$TOOL" \
        https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$TOOL"
fi

# --appimage-extract-and-run works without FUSE (containers, CI)
ARCH=x86_64 "$TOOL" --appimage-extract-and-run "$APPDIR" "Rustpad-${VERSION}-x86_64.AppImage"
echo "Built Rustpad-${VERSION}-x86_64.AppImage"
