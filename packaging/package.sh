#!/usr/bin/env bash
# Packages the release builds for one target into an archive in the current
# directory: playr-TAG-TARGET.tar.gz, or .zip for Windows.
#
#   packaging/package.sh TARGET TAG
#
# Every archive holds playr, playr-gui and playr-server with README.md,
# CHANGELOG.md, LICENSE and docs/server-guide.md. On macOS playr-gui is inside
# playr.app; on Linux playr.desktop, playr.png and playr-server.service come
# with them.
set -euo pipefail
target=$1
tag=$2
root=$(cd "$(dirname "$0")/.." && pwd)
release="$root/target/$target/release"
name="playr-$tag-$target"

rm -rf "$name"
mkdir "$name"
mkdir "$name/docs"
cp "$root/README.md" "$root/CHANGELOG.md" "$root/LICENSE" "$name/"
# Where README.md links to it.
cp "$root/docs/server-guide.md" "$name/docs/"

case "$target" in
  *windows*)
    cp "$release/playr.exe" "$release/playr-gui.exe" "$release/playr-server.exe" "$name/"
    7z a "$name.zip" "$name" > /dev/null
    echo "$name.zip"
    ;;
  *apple*)
    cp "$release/playr" "$release/playr-server" "$name/"
    "$root/packaging/macos/bundle.sh" "$release" "$name" "$tag"
    tar czf "$name.tar.gz" "$name"
    echo "$name.tar.gz"
    ;;
  *)
    cp "$release/playr" "$release/playr-gui" "$release/playr-server" "$name/"
    cp "$root/packaging/linux/playr.desktop" "$root/crates/playr-gui/assets/playr.png" \
      "$root/packaging/linux/playr-server.service" "$name/"
    tar czf "$name.tar.gz" "$name"
    echo "$name.tar.gz"
    ;;
esac
