#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

# Keep the supplied image intact; derive desktop and UI sizes from one source.
task_icons="$(mktemp -d -t glossa-icons)"
trap 'rm -rf "$task_icons"' EXIT HUP INT TERM
sips -s format png assets/brand/glossa-original.jpeg --out app-icon.png
npm run tauri -- icon app-icon.png --output "$task_icons"
for task_icon in "$task_icons"/*.png "$task_icons"/*.icns "$task_icons"/*.ico; do
  cp "$task_icon" src-tauri/icons/
done
mkdir -p src/assets
cp "$task_icons/128x128@2x.png" src/assets/glossa-logo.png
