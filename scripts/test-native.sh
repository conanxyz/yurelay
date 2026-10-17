#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
task_binary="$(mktemp -t yurelay-native-test)"
trap 'rm -f "$task_binary"' EXIT HUP INT TERM
xcrun clang -fobjc-arc -fblocks -mmacosx-version-min=14.0 \
  -framework AppKit -framework ApplicationServices -framework Vision -framework NaturalLanguage \
  -framework ScreenCaptureKit -framework Foundation -framework CoreServices \
  crates/platform/native/tests.m -o "$task_binary"
"$task_binary"
