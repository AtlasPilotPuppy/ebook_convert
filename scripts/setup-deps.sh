#!/usr/bin/env bash
# Install external dependencies for ebook-convert-rs.
# Detects the current OS and installs poppler-utils accordingly.
# Usage: ./scripts/setup-deps.sh
set -euo pipefail

case "$(uname -s)" in
  Darwin)
    echo "==> macOS detected"
    if command -v brew &>/dev/null; then
      echo "Installing poppler via Homebrew..."
      brew install poppler
    else
      echo "Error: Homebrew not found. Install from https://brew.sh"
      exit 1
    fi
    ;;
  Linux)
    echo "==> Linux detected"
    if command -v apt-get &>/dev/null; then
      echo "Installing poppler-utils via apt..."
      sudo apt-get update && sudo apt-get install -y poppler-utils
    elif command -v dnf &>/dev/null; then
      echo "Installing poppler-utils via dnf..."
      sudo dnf install -y poppler-utils
    elif command -v pacman &>/dev/null; then
      echo "Installing poppler via pacman..."
      sudo pacman -S --noconfirm poppler
    else
      echo "Error: No supported package manager found (apt, dnf, pacman)"
      exit 1
    fi
    ;;
  MINGW*|MSYS*|CYGWIN*)
    echo "==> Windows detected"
    if command -v choco &>/dev/null; then
      echo "Installing poppler via Chocolatey..."
      choco install poppler -y
    elif command -v scoop &>/dev/null; then
      echo "Installing poppler via Scoop..."
      scoop install poppler
    else
      echo "Error: No supported package manager found (choco, scoop)"
      exit 1
    fi
    ;;
  *)
    echo "Error: Unsupported OS: $(uname -s)"
    exit 1
    ;;
esac

echo ""
echo "==> Verifying poppler installation..."
if command -v pdftohtml &>/dev/null; then
  echo "pdftohtml: $(which pdftohtml)"
else
  echo "Warning: pdftohtml not found on PATH"
fi
if command -v pdftoppm &>/dev/null; then
  echo "pdftoppm: $(which pdftoppm)"
else
  echo "Warning: pdftoppm not found on PATH"
fi

echo ""
echo "==> Dependencies installed!"
