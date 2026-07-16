#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
PACKAGE_DIR="$ROOT_DIR/extensions/minitest-ruby"
ARTIFACT="$PACKAGE_DIR/target/wasm32-wasip1/release/ruby_fast_lsp_minitest_extension.wasm"

cd "$ROOT_DIR"
cargo test -p ruby-fast-lsp-minitest-extension
cargo build --release --target wasm32-wasip1 \
  --target-dir "$PACKAGE_DIR/target" \
  -p ruby-fast-lsp-minitest-extension

if command -v shasum >/dev/null 2>&1; then
  ACTUAL_CHECKSUM="$(shasum -a 256 "$ARTIFACT" | awk '{print $1}')"
else
  ACTUAL_CHECKSUM="$(sha256sum "$ARTIFACT" | awk '{print $1}')"
fi
EXPECTED_CHECKSUM="$(sed -n 's/^checksum_sha256 = "\([0-9a-f]*\)"$/\1/p' "$PACKAGE_DIR/extension.toml")"
if [[ "$ACTUAL_CHECKSUM" != "$EXPECTED_CHECKSUM" ]]; then
  echo "Minitest Rust Wasm checksum mismatch" >&2
  echo "expected: $EXPECTED_CHECKSUM" >&2
  echo "actual:   $ACTUAL_CHECKSUM" >&2
  echo "Update extensions/minitest-ruby/extension.toml after an intentional guest change." >&2
  exit 1
fi

cargo test -p ruby-fast-lsp-test-harness --test minitest_extension -- --nocapture
