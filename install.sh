#!/bin/bash
set -e

INSTALL_DIR="${HOME}/.local/bin"
BINARY_NAME="cashmere"
REPO="sidharthachatterjee/cashmere"

# Detect architecture
ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
    ARCH="aarch64"
elif [ "$ARCH" = "x86_64" ]; then
    ARCH="x86_64"
else
    echo "Unsupported architecture: $ARCH"
    exit 1
fi

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
if [ "$OS" = "darwin" ]; then
    OS="apple-darwin"
elif [ "$OS" = "linux" ]; then
    OS="unknown-linux-gnu"
else
    echo "Unsupported OS: $OS"
    exit 1
fi

ASSET_NAME="${BINARY_NAME}-${ARCH}-${OS}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${ASSET_NAME}"

# Create install directory
mkdir -p "$INSTALL_DIR"

echo "Downloading ${BINARY_NAME}..."
TMP_FILE=$(mktemp)
curl -fsSL "$DOWNLOAD_URL" -o "$TMP_FILE"
chmod +x "$TMP_FILE"
mv -f "$TMP_FILE" "${INSTALL_DIR}/${BINARY_NAME}"

echo "Installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"

# Check if INSTALL_DIR is in PATH
if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
    echo ""
    echo "Add the following to your shell profile (.bashrc, .zshrc, etc.):"
    echo "  export PATH=\"\$PATH:${INSTALL_DIR}\""
fi
