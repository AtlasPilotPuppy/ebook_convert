#!/usr/bin/env bash
# Build release binaries for all supported platforms.
#
# Usage:
#   ./scripts/build-all.sh                                  # build all targets
#   ./scripts/build-all.sh --targets native,linux-x86,windows  # specific targets
#
# Prerequisites: ./scripts/setup-cross.sh
#
# Targets:
#   native           - current host platform
#   linux-x86        - x86_64-unknown-linux-gnu
#   linux-arm        - aarch64-unknown-linux-gnu
#   windows          - x86_64-pc-windows-gnu
#   all (default)    - all of the above
set -euo pipefail

cd "$(dirname "$0")/../ebook-convert-rs"

BINARY_NAME="ebook-convert-rs"

# Parse arguments
REQUESTED_TARGETS=""
for arg in "$@"; do
  case "$arg" in
    --targets=*) REQUESTED_TARGETS="${arg#--targets=}" ;;
    *) echo "Unknown flag: $arg"; exit 1 ;;
  esac
done

# Resolve target name to triple (empty = native)
resolve_target() {
  case "$1" in
    native)     echo "NATIVE" ;;
    linux-x86)  echo "x86_64-unknown-linux-gnu" ;;
    linux-arm)  echo "aarch64-unknown-linux-gnu" ;;
    windows)    echo "x86_64-pc-windows-gnu" ;;
    *)          echo "UNKNOWN" ;;
  esac
}

# Determine which targets to build
if [ -z "$REQUESTED_TARGETS" ]; then
  TARGETS=("native" "linux-x86" "linux-arm" "windows")
else
  IFS=',' read -ra TARGETS <<< "$REQUESTED_TARGETS"
fi

OUTDIR="$(pwd)/target/dist"
mkdir -p "$OUTDIR"

SUCCEEDED=()
FAILED=()

for target_name in "${TARGETS[@]}"; do
  target_triple=$(resolve_target "$target_name")
  if [ "$target_triple" = "UNKNOWN" ]; then
    echo "Unknown target: $target_name"
    echo "Available: native, linux-x86, linux-arm, windows"
    exit 1
  fi

  echo ""
  if [ "$target_triple" = "NATIVE" ]; then
    HOST=$(rustc -vV | grep host | cut -d' ' -f2)
    echo "==> Building: native ($HOST)"
    if cargo build --release 2>&1; then
      if [ -f "target/release/${BINARY_NAME}" ]; then
        cp "target/release/${BINARY_NAME}" "$OUTDIR/${BINARY_NAME}-${HOST}"
        SUCCEEDED+=("native ($HOST)")
      elif [ -f "target/release/${BINARY_NAME}.exe" ]; then
        cp "target/release/${BINARY_NAME}.exe" "$OUTDIR/${BINARY_NAME}-${HOST}.exe"
        SUCCEEDED+=("native ($HOST)")
      fi
    else
      FAILED+=("native ($HOST)")
    fi
  else
    echo "==> Building: $target_name ($target_triple)"
    if cargo build --target "$target_triple" --release 2>&1; then
      if [ -f "target/${target_triple}/release/${BINARY_NAME}" ]; then
        cp "target/${target_triple}/release/${BINARY_NAME}" "$OUTDIR/${BINARY_NAME}-${target_triple}"
        SUCCEEDED+=("$target_name ($target_triple)")
      elif [ -f "target/${target_triple}/release/${BINARY_NAME}.exe" ]; then
        cp "target/${target_triple}/release/${BINARY_NAME}.exe" "$OUTDIR/${BINARY_NAME}-${target_triple}.exe"
        SUCCEEDED+=("$target_name ($target_triple)")
      fi
    else
      echo "    FAILED (is the cross-toolchain installed? Run ./scripts/setup-cross.sh)"
      FAILED+=("$target_name ($target_triple)")
    fi
  fi
done

echo ""
echo "========================================"
echo "Build Summary"
echo "========================================"
echo "Output directory: $OUTDIR"
echo ""
if [ ${#SUCCEEDED[@]} -gt 0 ]; then
  echo "Succeeded:"
  for s in "${SUCCEEDED[@]}"; do
    echo "  + $s"
  done
fi
if [ ${#FAILED[@]} -gt 0 ]; then
  echo ""
  echo "Failed:"
  for f in "${FAILED[@]}"; do
    echo "  - $f"
  done
fi

echo ""
echo "Binaries:"
ls -lh "$OUTDIR"/${BINARY_NAME}* 2>/dev/null || echo "  (none)"

# Exit with error if any failed
[ ${#FAILED[@]} -eq 0 ]
