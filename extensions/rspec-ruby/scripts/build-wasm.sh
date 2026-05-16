#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EXT_DIR="$ROOT/extensions/rspec-ruby"
SDK_DIR="$ROOT/extensions/mruby-sdk"
TARGET_DIR="$EXT_DIR/target"
GEN_DIR="$TARGET_DIR/generated"

: "${MRUBY_ROOT:?MRUBY_ROOT must point to an mruby checkout}"
: "${WASI_SDK_PATH:?WASI_SDK_PATH must point to a wasi-sdk installation}"

MRBC="$MRUBY_ROOT/build/host/bin/mrbc"
if [[ ! -x "$MRBC" ]]; then
  MRBC="$MRUBY_ROOT/build/host/mrbc/bin/mrbc"
fi
MRUBY_LIB="$MRUBY_ROOT/build/wasm32-wasip1/lib/libmruby.a"
MRUBY_INCLUDE="$MRUBY_ROOT/include"

if [[ ! -x "$MRBC" || ! -f "$MRUBY_LIB" ]]; then
  echo "building mruby host mrbc + wasm32-wasip1 libmruby.a"
  if [[ "${WASI_TRAP_EXCEPTIONS:-1}" == "1" ]]; then
    (cd "$MRUBY_ROOT" && patch -N -p1 < "$EXT_DIR/patches/mruby-wasi-trap-exceptions.patch" || true)
  fi
  (cd "$MRUBY_ROOT" && MRUBY_CONFIG="$EXT_DIR/build_config/wasm32-wasip1.rb" rake)
fi

mkdir -p "$GEN_DIR" "$TARGET_DIR/wasm32-wasip1/release"

cat \
  "$SDK_DIR/ruby_fast_lsp_extension.rb" \
  "$EXT_DIR/extension.rb" \
  "$EXT_DIR/runtime.rb" \
  | sed '/^require /d;/^require_relative /d' \
  > "$GEN_DIR/bundle.rb"

"$MRBC" -B rspec_ruby_mrb -o "$GEN_DIR/bundle.c" "$GEN_DIR/bundle.rb"

SJLJ_FLAGS=()
if [[ "${WASI_ENABLE_SJLJ:-0}" == "1" ]]; then
  SJLJ_FLAGS=(-fwasm-exceptions -mllvm -wasm-enable-sjlj)
fi
LINKER="$WASI_SDK_PATH/bin/clang"
if [[ "${WASI_USE_CXX_EXCEPTION:-0}" == "1" ]]; then
  LINKER="$WASI_SDK_PATH/bin/clang++"
  SJLJ_FLAGS=(-fwasm-exceptions)
fi

"$LINKER" \
  --target=wasm32-wasip1 \
  "${SJLJ_FLAGS[@]}" \
  -Oz \
  -I "$MRUBY_INCLUDE" \
  -I "$MRUBY_ROOT/build/wasm32-wasip1/include" \
  "$EXT_DIR/native/extension_shim.c" \
  "$GEN_DIR/bundle.c" \
  "$MRUBY_LIB" \
  -Wl,--no-entry \
  -Wl,--export=memory \
  -Wl,--export=alloc \
  -Wl,--export=dealloc \
  -Wl,--export=abi_version \
  -Wl,--export=indexed_call_names \
  -Wl,--export=index_call \
  -Wl,--export=handle_event \
  -Wl,--allow-undefined \
  -o "$TARGET_DIR/wasm32-wasip1/release/rspec-ruby.wasm"

echo "$TARGET_DIR/wasm32-wasip1/release/rspec-ruby.wasm"
