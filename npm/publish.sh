#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Publish platform packages first, then the main CLI package.
# Usage:
#   ./npm/publish.sh              # publish to npm
#   ./npm/publish.sh --dry-run    # preview without publishing

DRY_RUN=""
if [ "$1" == "--dry-run" ]; then
  DRY_RUN="--dry-run"
  echo "DRY RUN MODE"
fi

# Platform packages first (order matters — main package depends on these)
for PKG in darwin-arm64 darwin-x64 linux-x64 win32-x64; do
  echo "Publishing @ruby-fast-lsp/${PKG}..."
  cd "$SCRIPT_DIR/$PKG"
  npm publish --access public $DRY_RUN
  cd "$SCRIPT_DIR"
done

# Main CLI package last
echo "Publishing ruby-fast-lsp..."
cd "$SCRIPT_DIR/ruby-fast-lsp"
npm publish --access public $DRY_RUN

echo "Done."
