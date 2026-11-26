#!/usr/bin/env sh
# GeoKit Linux / macOS One-Line Installer
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
URL="https://github.com/Penguncisurga/Geokit/releases/latest/download/$ASSET"

echo "Downloading $URL ..."
curl -fL "$URL" -o "$INSTALL_DIR/geokit"
chmod +x "$INSTALL_DIR/geokit"

echo "[OK] Installed geokit -> $INSTALL_DIR/geokit"
"$INSTALL_DIR/geokit" --version
"$INSTALL_DIR/geokit" doctor
echo "Make sure $INSTALL_DIR is in your PATH."
