#!/usr/bin/env bash
# Builds the Debian package from an assembled release directory.
# The binaries should come from an Ubuntu 24.04 (glibc 2.39) build.
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
source_dir="${SOURCE_DIR:-dist/escargot-native-$version}"
pkgname="escargot-native"
root="dist/deb/${pkgname}_${version}_amd64"
output="dist/${pkgname}_${version}_amd64.deb"

test -x "$source_dir/escargot-native"
test -x "$source_dir/escargot-webview"

rm -rf "$root"
mkdir -p "$root/DEBIAN" \
    "$root/usr/bin" \
    "$root/usr/share/escargot-native" \
    "$root/usr/share/applications" \
    "$root/usr/share/icons/hicolor/256x256/apps" \
    "$root/usr/share/doc/$pkgname"

install -m755 "$source_dir/escargot-native" "$root/usr/bin/escargot-native"
install -m755 "$source_dir/escargot-webview" "$root/usr/bin/escargot-webview"
ln -s escargot-native "$root/usr/bin/msn"
cp -r "$source_dir/assets" "$root/usr/share/escargot-native/"
install -m644 "$source_dir/escargot-native.desktop" \
    "$root/usr/share/applications/escargot-native.desktop"
install -m644 "$source_dir/escargot-native.png" \
    "$root/usr/share/icons/hicolor/256x256/apps/escargot-native.png"

install -m644 "$source_dir/LICENSE" "$root/usr/share/doc/$pkgname/LICENSE"
install -m644 "$source_dir/LICENSE-Wine-Tahoma.txt" \
    "$root/usr/share/doc/$pkgname/LICENSE-Wine-Tahoma.txt"
install -m644 "$source_dir/README.md" "$root/usr/share/doc/$pkgname/README.md"

cat > "$root/usr/share/doc/$pkgname/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: escargot-native
Source: https://github.com/mafuzyk/escargot-native

Files: *
Copyright: 2026 mafuzyk
License: CC-BY-NC-4.0
 The full license text is in /usr/share/doc/$pkgname/LICENSE.
Comment: MSN Messenger and Windows Live Messenger, including the artwork,
 emoticons and sounds, are Microsoft property. This is an unofficial fan
 project for personal, non-commercial use.
EOF

cat > "$root/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi

exit 0
EOF

cat > "$root/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi

exit 0
EOF

chmod 755 "$root/DEBIAN/postinst" "$root/DEBIAN/postrm"

installed_size="$(du -sk "$root" | cut -f1)"

cat > "$root/DEBIAN/control" <<EOF
Package: $pkgname
Version: $version-1
Section: net
Priority: optional
Architecture: amd64
Maintainer: mafuzyk <247084418+mafuzyk@users.noreply.github.com>
Installed-Size: $installed_size
Depends: libc6 (>= 2.39), libgtk-3-0t64, libwebkit2gtk-4.1-0, libsecret-1-0, libxcb1, libxcb-xkb1, libxkbcommon0, libxkbcommon-x11-0, libvulkan1, libsqlite3-0, libgcc-s1, libstdc++6, hicolor-icon-theme
Recommends: pipewire
Homepage: https://github.com/mafuzyk/escargot-native
Description: Native Linux client for MSN Messenger 7.5 on the Escargot network
 A GTK/GPUI client for the Escargot network with the MSN 7.5 layout, emoticons,
 sounds, contact list, one-to-one conversations, local history and the three
 bundled games.
EOF

dpkg-deb --root-owner-group --build "$root" "$output" >/dev/null
sha256sum "$output"
