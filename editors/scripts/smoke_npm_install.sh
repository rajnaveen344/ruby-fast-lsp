#!/bin/sh
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TEMP_DIR"' EXIT

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) PLATFORM="darwin-arm64" ;;
  Darwin-x86_64) PLATFORM="darwin-x64" ;;
  Linux-x86_64) PLATFORM="linux-x64" ;;
  *) echo "Unsupported npm smoke-test platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

node "$ROOT_DIR/editors/check_package_versions.js"
npm pack "$ROOT_DIR/editors/npm/$PLATFORM" --pack-destination "$TEMP_DIR" >/dev/null
npm pack "$ROOT_DIR/editors/npm/ruby-fast-lsp" --pack-destination "$TEMP_DIR" >/dev/null

cd "$TEMP_DIR"
npm init --yes >/dev/null
PLATFORM_TARBALL="$(find . -maxdepth 1 -name "ruby-fast-lsp-$PLATFORM-*.tgz" -print -quit)"
ROOT_TARBALL="$(find . -maxdepth 1 -name "ruby-fast-lsp-[0-9]*.tgz" -print -quit)"
if [ -z "$PLATFORM_TARBALL" ] || [ -z "$ROOT_TARBALL" ]; then
  echo "Expected npm tarballs were not produced" >&2
  exit 1
fi
npm install --ignore-scripts --no-audit "$PLATFORM_TARBALL" "$ROOT_TARBALL" >/dev/null
node "$ROOT_DIR/editors/scripts/smoke_npm.js" "$TEMP_DIR/node_modules/.bin/ruby-fast-lsp"
