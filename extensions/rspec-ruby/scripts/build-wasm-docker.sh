#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE="${IMAGE:-ruby-fast-lsp-rspec-ruby-wasm-builder}"
MRUBY_VERSION="${MRUBY_VERSION:-4.0.0}"
WASI_SDK_VERSION="${WASI_SDK_VERSION:-33}"

docker build \
  --build-arg "MRUBY_VERSION=$MRUBY_VERSION" \
  --build-arg "WASI_SDK_VERSION=$WASI_SDK_VERSION" \
  -f "$ROOT/extensions/rspec-ruby/Dockerfile.build" \
  -t "$IMAGE" \
  "$ROOT"

docker run --rm \
  -e "WASI_ENABLE_SJLJ=${WASI_ENABLE_SJLJ:-0}" \
  -e "WASI_USE_CXX_EXCEPTION=${WASI_USE_CXX_EXCEPTION:-0}" \
  -e "WASI_TRAP_EXCEPTIONS=${WASI_TRAP_EXCEPTIONS:-1}" \
  -v "$ROOT:/workspace" \
  -w /workspace \
  "$IMAGE"
