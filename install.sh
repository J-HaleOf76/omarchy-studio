#!/bin/sh
set -e

# omarchy-studio quick installer (v1.0)
# Fetches the latest release binary from GitHub and installs it to ~/.local/bin

REPO="arino08/omarchy-studio"
BIN_NAME="omarchy-studio"
ASSET_NAME="omarchy-studio-linux-x86_64"
INSTALL_DIR="$HOME/.local/bin"

echo "=> Fetching latest release of $BIN_NAME..."

# Use GitHub API to get the latest release URL
LATEST_URL=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep "browser_download_url.*$ASSET_NAME" | cut -d '"' -f 4 || true)

if [ -n "$LATEST_URL" ]; then
    echo "=> Downloading pre-built binary from $LATEST_URL..."
    mkdir -p "$INSTALL_DIR"
    curl -sL "$LATEST_URL" -o "$INSTALL_DIR/$BIN_NAME"
    chmod +x "$INSTALL_DIR/$BIN_NAME"
    echo "=> Success! $BIN_NAME installed/updated to $INSTALL_DIR/$BIN_NAME"
else
    echo "=> No pre-built binary found for the latest release."
    if command -v cargo >/dev/null 2>&1; then
        echo "=> Rust toolchain detected. Building and updating from source..."
        cargo install --git https://github.com/$REPO.git $BIN_NAME
        INSTALL_DIR="$HOME/.cargo/bin"
        echo "=> Success! $BIN_NAME installed/updated via cargo."
    else
        echo "Error: No pre-built binary available, and Rust (cargo) is not installed."
        echo "Please install Rust from https://rustup.rs and try again."
        exit 1
    fi
fi

# Check if INSTALL_DIR is in PATH
if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    echo ""
    echo "Note: $INSTALL_DIR is not in your PATH."
    echo "Add the following line to your ~/.bashrc or ~/.zshrc:"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
fi

echo ""
echo "Run '$BIN_NAME' to start."
