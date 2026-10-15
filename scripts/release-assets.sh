#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${RUST_TARGET:?}" "${RELEASE_TAG:?}" "${GH_REPO:?}"
case "$RUST_TARGET" in aarch64-apple-darwin|x86_64-apple-darwin) ;; *) exit 1 ;; esac
bundle="target/$RUST_TARGET/release/bundle"
archive="Glossa_${RELEASE_TAG}_${RUST_TARGET}.app.tar.gz"
checksums="SHA256SUMS-${RUST_TARGET}.txt"
# This ordinary application archive needs no updater signing key.
tar -czf "$bundle/$archive" -C "$bundle/macos" Glossa.app
(
  cd "$bundle"
  {
    shasum -a 256 "$archive"
    (cd dmg && shasum -a 256 ./*.dmg)
  } > "$checksums"
)
gh release upload "$RELEASE_TAG" "$bundle/$archive" "$bundle/$checksums" --clobber
