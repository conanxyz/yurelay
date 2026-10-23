#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${RUST_TARGET:?}" "${RELEASE_TAG:?}" "${GH_REPO:?}"
case "$RUST_TARGET" in aarch64-apple-darwin|x86_64-apple-darwin) ;; *) exit 1 ;; esac
bundle="target/$RUST_TARGET/release/bundle"
checksums="SHA256SUMS-${RUST_TARGET}.txt"
installers=()
for installer in "$bundle"/dmg/*.dmg; do
  [ -f "$installer" ] && installers+=("$installer")
done
if [ "${#installers[@]}" -ne 1 ]; then
  echo 'Expected exactly one DMG installer for this architecture.' >&2
  exit 1
fi
installer="${installers[0]}"
(
  cd "$bundle/dmg"
  shasum -a 256 "${installer##*/}" > "../$checksums"
)
gh release upload "$RELEASE_TAG" "$installer" "$bundle/$checksums" --clobber
