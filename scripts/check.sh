#!/usr/bin/env bash
# Run the full CI check suite locally (fmt, clippy, test, build).
# Usage: ./scripts/check.sh [--release]
set -euo pipefail

cd "$(dirname "$0")/../ebook-convert-rs"

RELEASE=false
for arg in "$@"; do
  case "$arg" in
    --release) RELEASE=true ;;
    *) echo "Unknown flag: $arg"; exit 1 ;;
  esac
done

echo "==> Checking formatting..."
cargo fmt --all -- --check

echo ""
echo "==> Running clippy..."
cargo clippy --lib --bins --tests -- -D warnings

echo ""
echo "==> Running tests..."
cargo test --lib --bins

if [ "$RELEASE" = true ]; then
  echo ""
  echo "==> Building release binary..."
  cargo build --release
  echo "Binary: $(pwd)/target/release/ebook-convert-rs"
else
  echo ""
  echo "==> Building debug..."
  cargo build --all-targets
fi

echo ""
echo "==> All checks passed!"
