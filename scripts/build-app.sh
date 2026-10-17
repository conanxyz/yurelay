#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
# Keep development bundles out of Spotlight and avoid large debug symbols.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/artifacts/build.noindex}"
export CARGO_PROFILE_DEV_DEBUG=0
# macOS's stripped proc-macro dylibs can fail to load with malformed LINKEDIT.
export CARGO_PROFILE_DEV_STRIP=none
export CARGO_INCREMENTAL=0
npm run tauri -- build --debug --bundles app
