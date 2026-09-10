#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
VSIX_VERSION="$(node -p 'require(process.argv[1]).version' "$ROOT_DIR/editors/vscode/vsix/package.json")"
VSIX_PATH="${1:-$ROOT_DIR/target/ruby-fast-lsp-$VSIX_VERSION.vsix}"

if [[ ! -f "$VSIX_PATH" ]]; then
  echo "Bundled-extension stress requires an assembled VSIX: $VSIX_PATH" >&2
  echo "Build it with ./editors/vscode/create_vsix.sh --current-platform-only" >&2
  exit 1
fi

EXTRACT_DIR="$(mktemp -d)"
trap 'rm -rf "$EXTRACT_DIR"' EXIT
unzip -q "$VSIX_PATH" -d "$EXTRACT_DIR"

export RUBY_FAST_LSP_BUNDLED_EXTENSION_ROOT="$EXTRACT_DIR/extension/extensions"
export RUBY_FAST_LSP_REQUIRE_BUNDLED_STRESS=1
export CARGO_INCREMENTAL=0

cd "$ROOT_DIR"
cargo test -p ruby-fast-lsp-test-harness \
  --test bundled_extension_stress \
  -- --nocapture
