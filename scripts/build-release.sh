#!/usr/bin/env bash
# Build an optimized release binary.
#
# Usage:
#   ./scripts/build-release.sh                                    # native
#   ./scripts/build-release.sh --target x86_64-pc-windows-gnu     # cross-compile
set -euo pipefail

cd "$(dirname "$0")/../ebook-convert-rs"

TARGET=""
for arg in "$@"; do
  case "$arg" in
    --target=*) TARGET="${arg#--target=}" ;;
    --target) ;; # next arg handled below
    *)
      if [ -n "${PREV_WAS_TARGET:-}" ]; then
        TARGET="$arg"
        unset PREV_WAS_TARGET
      else
        echo "Unknown flag: $arg"; exit 1
      fi
      ;;
  esac
  [ "$arg" = "--target" ] && PREV_WAS_TARGET=1
done

if [ -n "$TARGET" ]; then
  echo "==> Building release binary for $TARGET..."
  cargo build --target "$TARGET" --release
  BINARY="target/${TARGET}/release/ebook-convert-rs"
else
  echo "==> Building release binary (native)..."
  cargo build --release
  BINARY="target/release/ebook-convert-rs"
fi

# Check for .exe variant on Windows target
for ext in "" ".exe"; do
  if [ -f "${BINARY}${ext}" ]; then
    SIZE=$(du -h "${BINARY}${ext}" | cut -f1)
    echo "==> Built: $(pwd)/${BINARY}${ext} ($SIZE)"
    exit 0
  fi
done

echo "==> Warning: binary not found at expected path"
