#!/usr/bin/env bash
# Build an optimized release binary for the current platform.
# Usage: ./scripts/build-release.sh
set -euo pipefail

cd "$(dirname "$0")/../ebook-convert-rs"

echo "==> Building release binary..."
cargo build --release

BINARY="target/release/ebook-convert-rs"
if [ -f "$BINARY" ]; then
  SIZE=$(du -h "$BINARY" | cut -f1)
  echo "==> Built: $(pwd)/$BINARY ($SIZE)"
elif [ -f "$BINARY.exe" ]; then
  SIZE=$(du -h "$BINARY.exe" | cut -f1)
  echo "==> Built: $(pwd)/$BINARY.exe ($SIZE)"
fi
