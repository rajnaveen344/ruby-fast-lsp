#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EDITORS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NPM_DIR="$EDITORS_DIR/npm"

# Publish platform packages first, then the main CLI package.
# Usage:
#   ./editors/scripts/publish_npm.sh              # publish to npm
#   ./editors/scripts/publish_npm.sh --dry-run    # preview without publishing

DRY_RUN=""
if [ "$1" == "--dry-run" ]; then
  DRY_RUN="--dry-run"
  echo "DRY RUN MODE"
fi

# Platform packages first (order matters — main package depends on these)
for PKG in darwin-arm64 darwin-x64 linux-x64 win32-x64; do
  echo "Publishing @ruby-fast/lsp-${PKG}..."
  cd "$NPM_DIR/$PKG"
  npm publish --access public $DRY_RUN
  cd "$SCRIPT_DIR"
done

# Main CLI package last
echo "Publishing @ruby-fast/lsp..."
cd "$NPM_DIR/ruby-fast-lsp"
npm publish --access public $DRY_RUN

echo "Done."
