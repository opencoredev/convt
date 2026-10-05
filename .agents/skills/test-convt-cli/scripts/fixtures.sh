#!/usr/bin/env bash
# Writes small synthetic inputs into an empty directory. Never points at user files.
#   bash .agents/skills/test-convt-cli/scripts/fixtures.sh <dir>
# Needs ffmpeg. sample.pdf also needs soffice and a built convt; it is skipped without them.
set -euo pipefail
dir=${1:?usage: fixtures.sh <dir>}
mkdir -p "$dir"
if [ -n "$(ls -A "$dir")" ]; then
  echo "refusing: $dir is not empty" >&2
  exit 1
fi
root=$(cd "$(dirname "$0")/../../../.." && pwd)
ff() { ffmpeg -v error -nostdin -y "$@"; }

ff -f lavfi -i testsrc2=size=320x240 -frames:v 1 "$dir/sample.png"
ff -f lavfi -i testsrc2=duration=3:size=320x240:rate=25 -f lavfi -i sine=duration=3 \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest "$dir/sample.mp4"
ff -f lavfi -i sine=frequency=440:duration=2 "$dir/sample.wav"
cat > "$dir/sample.svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120" viewBox="0 0 200 120">
  <rect width="200" height="120" fill="#0b0d0c"/>
  <circle cx="60" cy="60" r="40" fill="#74d39b"/>
  <text x="110" y="68" font-family="sans-serif" font-size="24" fill="#eef9f1">convt</text>
</svg>
SVG
printf 'convt fixture\nSecond line.\n' > "$dir/sample.txt"

convt="$root/target/debug/convt"
if command -v soffice >/dev/null && [ -x "$convt" ]; then
  "$convt" "$dir/sample.txt" --to pdf 2>/dev/null
else
  echo "skipped sample.pdf (needs soffice and target/debug/convt)" >&2
fi
ls -l "$dir"
