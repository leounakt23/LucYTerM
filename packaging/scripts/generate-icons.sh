#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

converter=""
if command -v magick >/dev/null 2>&1; then converter=magick
elif command -v convert >/dev/null 2>&1; then converter=convert
elif command -v rsvg-convert >/dev/null 2>&1; then converter=rsvg-convert
else
  echo "install ImageMagick or librsvg to generate package icons" >&2
  exit 1
fi

for size in 48 128 256 512; do
  mkdir -p "target/package-icons/${size}x${size}"
  if [[ "$converter" == "rsvg-convert" ]]; then
    rsvg-convert -w "$size" -h "$size" assets/icon.svg -o "target/package-icons/${size}x${size}/remote-app.png"
  else
    "$converter" assets/icon.svg -background none -resize "${size}x${size}" "target/package-icons/${size}x${size}/remote-app.png"
  fi
done
