#!/usr/bin/env bash
# Renders crates/playr-gui/assets/playr.svg into the icons each platform needs:
# playr.png for the window and Linux, playr.icns for the macOS bundle, and
# playr.ico for the Windows executable. Needs rsvg-convert and python3, and
# iconutil, which only macOS has.
set -euo pipefail
cd "$(dirname "$0")/../crates/playr-gui/assets"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

rsvg-convert -w 256 -h 256 playr.svg -o playr.png

mkdir "$work/playr.iconset"
for size in 16 32 128 256 512; do
  rsvg-convert -w $size -h $size playr.svg -o "$work/playr.iconset/icon_${size}x${size}.png"
  rsvg-convert -w $((size * 2)) -h $((size * 2)) playr.svg -o "$work/playr.iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$work/playr.iconset" -o playr.icns

# An .ico of PNG images, which Windows reads from Vista on.
for size in 16 24 32 48 256; do
  rsvg-convert -w $size -h $size playr.svg -o "$work/$size.png"
done
python3 - "$work" <<'PY'
import struct, sys
work = sys.argv[1]
sizes = [16, 24, 32, 48, 256]
images = [open(f"{work}/{s}.png", "rb").read() for s in sizes]
out = struct.pack("<HHH", 0, 1, len(images))
offset = 6 + 16 * len(images)
for size, data in zip(sizes, images):
    side = 0 if size == 256 else size
    out += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
open("playr.ico", "wb").write(out + b"".join(images))
PY
