#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Keep local builds compact and out of Spotlight. CI uses its cached target/.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/artifacts/verify.noindex}"
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_PROFILE_DEV_STRIP=none
export CARGO_PROFILE_TEST_STRIP=none
export CARGO_INCREMENTAL=0
export CARGO_TERM_COLOR=always
printf '\n[1/7] Repository hygiene and metadata\n'
python3 scripts/audit.py
python3 -m unittest discover -s tests/tooling
printf '\n[2/7] Rust formatting\n'
cargo fmt --all -- --check
printf '\n[3/7] Rust Clippy (zero warnings)\n'
cargo clippy --workspace --all-targets --locked -- -D warnings
printf '\n[4/7] Rust workspace tests\n'
cargo test --workspace --locked
printf '\n[5/7] Frontend types and production build\n'
npm run check
npm run build
printf '\n[6/7] Frontend unit tests\n'
npm run test:unit
printf '\n[7/7] Browser interaction tests\n'
npm test
printf '\nAll checks passed.\n'
