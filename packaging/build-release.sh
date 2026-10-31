#!/usr/bin/env bash
# Builds the release binaries and assembles the Linux x86_64 tarball.
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
jobs="${CARGO_BUILD_JOBS:-4}"
out="dist/escargot-native-$version"
tarball="dist/escargot-native-$version-x86_64.tar.gz"

if [ "${SKIP_BUILD:-0}" != "1" ]; then
    cargo build --release --locked -j "$jobs"
fi

rm -rf "$out" "$tarball"
mkdir -p "$out/assets"

strip --strip-unneeded target/release/escargot-native -o "$out/escargot-native"
strip --strip-unneeded target/release/escargot-webview -o "$out/escargot-webview"
chmod 755 "$out/escargot-native" "$out/escargot-webview"
ln -s escargot-native "$out/msn"

install -m755 packaging/release/msn2 "$out/msn2"
install -m644 packaging/release/escargot-native.desktop "$out/escargot-native.desktop"
install -m644 packaging/release/escargot-native-secondary.desktop "$out/escargot-native-secondary.desktop"

cp -r assets/img assets/emoticons assets/sounds assets/wallpapers assets/games "$out/assets/"
find "$out/assets" -name '*.md' -delete
magick assets/icon.png -resize 256x256 "$out/escargot-native.png"
chmod 644 "$out/escargot-native.png"

cp LICENSE README.md "$out/"
install -m644 assets/fonts/LICENSE.txt "$out/LICENSE-Wine-Tahoma.txt"

tar -czf "$tarball" -C dist "escargot-native-$version"
sha256sum "$tarball"
