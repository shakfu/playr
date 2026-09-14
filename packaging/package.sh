#!/usr/bin/env bash
# Packages the release builds for one target into an archive in the current
# directory: playr-TAG-TARGET.tar.gz, or .zip for Windows.
#
#   packaging/package.sh TARGET TAG
#
# Every archive holds playr and playr-gui with README.md, CHANGELOG.md and
# LICENSE. On macOS playr-gui is inside playr.app; on Linux playr.desktop and
# playr.png come with it.
set -euo pipefail
target=$1
tag=$2
root=$(cd "$(dirname "$0")/.." && pwd)
release="$root/target/$target/release"
name="playr-$tag-$target"

rm -rf "$name"
mkdir "$name"
cp "$root/README.md" "$root/CHANGELOG.md" "$root/LICENSE" "$name/"

case "$target" in
  *windows*)
    cp "$release/playr.exe" "$release/playr-gui.exe" "$name/"
    7z a "$name.zip" "$name" > /dev/null
    echo "$name.zip"
    ;;
  *apple*)
    cp "$release/playr" "$name/"
    "$root/packaging/macos/bundle.sh" "$release" "$name" "$tag"
    tar czf "$name.tar.gz" "$name"
    echo "$name.tar.gz"
    ;;
  *)
    cp "$release/playr" "$release/playr-gui" "$name/"
    cp "$root/packaging/linux/playr.desktop" "$root/crates/playr-gui/assets/playr.png" "$name/"
    tar czf "$name.tar.gz" "$name"
    echo "$name.tar.gz"
    ;;
esac
