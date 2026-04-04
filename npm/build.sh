#!/bin/sh
set -e

# Build binaries and copy them into the npm platform packages.
# Usage:
#   ./npm/build.sh                    # build all platforms
#   ./npm/build.sh --current-only     # build current platform only

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION=$(grep -m 1 "version" "$ROOT_DIR/Cargo.toml" | cut -d '"' -f 2)

echo "Building ruby-fast-lsp v${VERSION}"

get_target() {
  case "$1" in
    darwin-arm64) echo "aarch64-apple-darwin" ;;
    darwin-x64)   echo "x86_64-apple-darwin" ;;
    linux-x64)    echo "x86_64-unknown-linux-gnu" ;;
    win32-x64)    echo "x86_64-pc-windows-gnu" ;;
  esac
}

# Determine which platforms to build
if [ "$1" = "--current-only" ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)  PLATFORMS="darwin-arm64" ;;
    Darwin-x86_64) PLATFORMS="darwin-x64" ;;
    Linux-x86_64)  PLATFORMS="linux-x64" ;;
    *) echo "Unknown platform: $(uname -s)-$(uname -m)"; exit 1 ;;
  esac
else
  PLATFORMS="darwin-arm64 darwin-x64 linux-x64 win32-x64"
fi

for PLATFORM in $PLATFORMS; do
  TARGET=$(get_target "$PLATFORM")
  echo ""
  echo "==> Building for ${PLATFORM} (${TARGET})"

  BIN_NAME="ruby-fast-lsp"
  case "$PLATFORM" in win32-*) BIN_NAME="ruby-fast-lsp.exe" ;; esac

  cargo build --release --target "$TARGET"

  # Copy binary into npm package
  mkdir -p "$SCRIPT_DIR/$PLATFORM/bin"
  cp "$ROOT_DIR/target/$TARGET/release/$BIN_NAME" "$SCRIPT_DIR/$PLATFORM/bin/$BIN_NAME"

  echo "    -> $SCRIPT_DIR/$PLATFORM/bin/$BIN_NAME"
done

# Sync version across all package.json files
for PKG in ruby-fast-lsp darwin-arm64 darwin-x64 linux-x64 win32-x64; do
  sed -i '' "s/\"version\": \".*\"/\"version\": \"${VERSION}\"/" "$SCRIPT_DIR/$PKG/package.json" 2>/dev/null || \
  sed -i "s/\"version\": \".*\"/\"version\": \"${VERSION}\"/" "$SCRIPT_DIR/$PKG/package.json"
done

# Sync optionalDependencies versions in main package
for DEP in darwin-arm64 darwin-x64 linux-x64 win32-x64; do
  sed -i '' "s|\"@ruby-fast/lsp-${DEP}\": \".*\"|\"@ruby-fast/lsp-${DEP}\": \"${VERSION}\"|" "$SCRIPT_DIR/ruby-fast-lsp/package.json" 2>/dev/null || \
  sed -i "s|\"@ruby-fast/lsp-${DEP}\": \".*\"|\"@ruby-fast/lsp-${DEP}\": \"${VERSION}\"|" "$SCRIPT_DIR/ruby-fast-lsp/package.json"
done

echo ""
echo "Done. Built $(echo $PLATFORMS | wc -w | tr -d ' ') platform(s)."
echo "To publish: ./npm/publish.sh"
