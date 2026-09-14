#!/usr/bin/env bash
# Assembles playr.app around a built playr-gui.
#
#   packaging/macos/bundle.sh RELEASE_DIR OUT_DIR VERSION
#
# RELEASE_DIR holds playr-gui; OUT_DIR/playr.app is replaced.
set -euo pipefail
release=$1
out=$2
version=$3
root=$(cd "$(dirname "$0")/../.." && pwd)
app="$out/playr.app/Contents"

rm -rf "$out/playr.app"
mkdir -p "$app/MacOS" "$app/Resources"
cp "$release/playr-gui" "$app/MacOS/"
cp "$root/crates/playr-gui/assets/playr.icns" "$app/Resources/"
sed "s/VERSION/$version/g" "$root/packaging/macos/Info.plist" > "$app/Info.plist"
