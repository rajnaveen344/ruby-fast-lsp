#!/bin/bash
set -e

# Configuration
EXTENSION_NAME="ruby-fast-lsp"
EXTENSION_VERSION=$(grep -m 1 "version" Cargo.toml | cut -d '"' -f 2)
BINARY_PATH="./target/release/ruby-fast-lsp"
EXTENSION_DIR="./vsix"
TARGET_DIR="./target"

# Check if the binary exists
if [ ! -f "$BINARY_PATH" ]; then
    echo "Binary not found at $BINARY_PATH. Building release version..."
    cargo build --release
fi

# Ensure directories exist
mkdir -p "$EXTENSION_DIR/bin/macos"
mkdir -p "$EXTENSION_DIR/bin/linux"
mkdir -p "$EXTENSION_DIR/bin/win32"
mkdir -p "$TARGET_DIR"

# Copy binary to extension directory based on platform
if [ "$(uname)" == "Darwin" ]; then
    cp "$BINARY_PATH" "$EXTENSION_DIR/bin/macos/"
elif [ "$(uname)" == "Linux" ]; then
    cp "$BINARY_PATH" "$EXTENSION_DIR/bin/linux/"
else
    # Assuming Windows
    cp "$BINARY_PATH.exe" "$EXTENSION_DIR/bin/win32/"
fi

# Navigate to extension directory and package
cd "$EXTENSION_DIR"
echo "Installing dependencies..."
npm install

echo "Packaging extension..."
vsce package

# Move the VSIX file to the target directory
mv *.vsix "../$TARGET_DIR/"

echo "VSIX package created successfully!"
echo "You can find the VSIX file in the target directory of your project."
echo "To install the extension, run: code --install-extension $TARGET_DIR/$EXTENSION_NAME-$EXTENSION_VERSION.vsix"
