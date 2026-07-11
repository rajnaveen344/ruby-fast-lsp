#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EXTENSION_PATH="${1:?usage: build-wasm-docker.sh <extension-package-directory>}"
IMAGE="${IMAGE:-ruby-fast-lsp-mruby-wasm-builder}"

docker build \
  --build-arg "MRUBY_VERSION=${MRUBY_VERSION:-4.0.0}" \
  --build-arg "WASI_SDK_VERSION=${WASI_SDK_VERSION:-33}" \
  -f "$ROOT/extensions/mruby-sdk/Dockerfile.build" \
  -t "$IMAGE" \
  "$ROOT"

docker run --rm \
  -e "WASI_TRAP_EXCEPTIONS=${WASI_TRAP_EXCEPTIONS:-1}" \
  -e "WASI_ENABLE_SJLJ=${WASI_ENABLE_SJLJ:-0}" \
  -e "WASI_USE_CXX_EXCEPTION=${WASI_USE_CXX_EXCEPTION:-0}" \
  -v "$ROOT:/workspace" \
  -w /workspace \
  "$IMAGE" \
  extensions/mruby-sdk/scripts/build-wasm.sh "$EXTENSION_PATH"
