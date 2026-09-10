#!/bin/sh
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
"$ROOT_DIR/editors/scripts/build_npm.sh" --current-only
node "$SCRIPT_DIR/smoke_npm_install.js"
