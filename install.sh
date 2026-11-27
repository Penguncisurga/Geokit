#!/usr/bin/env sh
# GeoKit Linux / macOS One-Line Installer (with SHA256 checksum verification)
# Usage: curl -fsSL https://raw.githubusercontent.com/Penguncisurga/Geokit/main/install.sh | sh

set -e

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    ASSET="geokit-linux-x86_64"
    ;;
  Darwin)
    if [ "$ARCH" = "arm64" ]; then
      ASSET="geokit-macos-arm64"
    else
      echo "No prebuilt Intel macOS binary yet. Please build from source: cargo install --git https://github.com/Penguncisurga/Geokit"
      exit 1
    fi
    ;;
  *)
    echo "Unsupported OS: $OS. Use install.ps1 on Windows."
    exit 1
    ;;
esac

INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR"
BASE_URL="https://github.com/Penguncisurga/Geokit/releases/latest/download"
URL="$BASE_URL/$ASSET"
SUMS_URL="$BASE_URL/SHA256SUMS.txt"
TMP_BIN="$INSTALL_DIR/geokit.part"
TMP_SUMS="$INSTALL_DIR/SHA256SUMS.txt"

echo "Downloading $URL ..."
curl -fsSL "$URL" -o "$TMP_BIN"

if curl -fsSL "$SUMS_URL" -o "$TMP_SUMS" 2>/dev/null; then
  EXPECTED="$(grep " $ASSET\$" "$TMP_SUMS" | awk '{print $1}' | head -n 1)"
  rm -f "$TMP_SUMS"
  if [ -n "$EXPECTED" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
      ACTUAL="$(sha256sum "$TMP_BIN" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
      ACTUAL="$(shasum -a 256 "$TMP_BIN" | awk '{print $1}')"
    else
      ACTUAL=""
    fi
    if [ -n "$ACTUAL" ]; then
      if [ "$ACTUAL" != "$EXPECTED" ]; then
        rm -f "$TMP_BIN"
        echo "ERROR: SHA256 mismatch for $ASSET (expected $EXPECTED, got $ACTUAL)" >&2
        exit 1
      fi
      echo "[OK] SHA256 checksum verified ($ACTUAL)"
    fi
  fi
fi

chmod +x "$TMP_BIN"
mv -f "$TMP_BIN" "$INSTALL_DIR/geokit"

echo "[OK] Installed geokit -> $INSTALL_DIR/geokit"
"$INSTALL_DIR/geokit" --version
"$INSTALL_DIR/geokit" doctor
echo "Make sure $INSTALL_DIR is in your PATH."
