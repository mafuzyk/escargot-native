#!/usr/bin/env bash
# Builds the x86_64 AppImage from an assembled release directory.
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
source_dir="${SOURCE_DIR:-dist/escargot-native-$version}"
appdir="dist/appimage/AppDir"
output="dist/escargot-native-$version-x86_64.AppImage"
tool="${APPIMAGETOOL:-appimagetool}"

test -x "$source_dir/escargot-native"
test -x "$source_dir/escargot-webview"

rm -rf "$appdir" "$output"
mkdir -p "$appdir/usr/bin" \
    "$appdir/usr/share/escargot-native" \
    "$appdir/usr/share/applications" \
    "$appdir/usr/share/icons/hicolor/256x256/apps"

install -m755 "$source_dir/escargot-native" "$appdir/usr/bin/escargot-native"
install -m755 "$source_dir/escargot-webview" "$appdir/usr/bin/escargot-webview"
ln -s escargot-native "$appdir/usr/bin/msn"
cp -r "$source_dir/assets" "$appdir/usr/share/escargot-native/"

install -m644 "$source_dir/escargot-native.desktop" \
    "$appdir/escargot-native.desktop"
install -m644 "$source_dir/escargot-native.desktop" \
    "$appdir/usr/share/applications/escargot-native.desktop"
sed -i 's#^Exec=/usr/bin/escargot-native$#Exec=escargot-native#' \
    "$appdir/escargot-native.desktop" "$appdir/usr/share/applications/escargot-native.desktop"
install -m644 "$source_dir/escargot-native.png" "$appdir/escargot-native.png"
install -m644 "$source_dir/escargot-native.png" \
    "$appdir/usr/share/icons/hicolor/256x256/apps/escargot-native.png"

cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
set -eu

here="$(dirname "$(readlink -f "$0")")"
export ESCARGOT_NATIVE_ASSETS="${ESCARGOT_NATIVE_ASSETS:-$here/usr/share/escargot-native}"
exec "$here/usr/bin/escargot-native" "$@"
EOF
chmod 755 "$appdir/AppRun"

APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$tool" --no-appstream "$appdir" "$output" >/dev/null
chmod 755 "$output"
sha256sum "$output"
