#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SDK_DIR="$ROOT/extensions/mruby-sdk"
EXTENSION_PATH="${1:?usage: build-wasm.sh <extension-package-directory>}"
if [[ "$EXTENSION_PATH" = /* ]]; then
  EXT_DIR="$EXTENSION_PATH"
else
  EXT_DIR="$ROOT/$EXTENSION_PATH"
fi

MANIFEST="$EXT_DIR/extension.toml"
[[ -f "$MANIFEST" ]] || { echo "missing extension manifest: $MANIFEST" >&2; exit 1; }
[[ -f "$EXT_DIR/extension.rb" ]] || { echo "missing extension source: $EXT_DIR/extension.rb" >&2; exit 1; }
[[ -f "$EXT_DIR/runtime.rb" ]] || { echo "missing extension runtime: $EXT_DIR/runtime.rb" >&2; exit 1; }

EXTENSION_ID="$(sed -n 's/^id = "\([^"]*\)"/\1/p' "$MANIFEST")"
WASM_RELATIVE="$(sed -n 's/^wasm = "\([^"]*\)"/\1/p' "$MANIFEST")"
[[ -n "$EXTENSION_ID" ]] || { echo "manifest has no id: $MANIFEST" >&2; exit 1; }
[[ -n "$WASM_RELATIVE" ]] || { echo "manifest has no wasm path: $MANIFEST" >&2; exit 1; }
IREP_SYMBOL="$(printf '%s' "$EXTENSION_ID" | tr -c '[:alnum:]' '_')_mrb"

: "${MRUBY_ROOT:?MRUBY_ROOT must point to an mruby checkout}"
: "${WASI_SDK_PATH:?WASI_SDK_PATH must point to a wasi-sdk installation}"

MRBC="$MRUBY_ROOT/build/host/bin/mrbc"
[[ -x "$MRBC" ]] || MRBC="$MRUBY_ROOT/build/host/mrbc/bin/mrbc"
MRUBY_LIB="$MRUBY_ROOT/build/wasm32-wasip1/lib/libmruby.a"
if [[ ! -x "$MRBC" || ! -f "$MRUBY_LIB" ]]; then
  if [[ "${WASI_TRAP_EXCEPTIONS:-1}" == "1" ]]; then
    (cd "$MRUBY_ROOT" && patch -N -p1 < "$SDK_DIR/patches/mruby-wasi-trap-exceptions.patch" || true)
  fi
  (cd "$MRUBY_ROOT" && MRUBY_CONFIG="$SDK_DIR/build_config/wasm32-wasip1.rb" rake)
fi

GEN_DIR="$EXT_DIR/target/generated"
OUTPUT="$EXT_DIR/$WASM_RELATIVE"
mkdir -p "$GEN_DIR" "$(dirname "$OUTPUT")"
sed '/^require /d;/^require_relative /d' \
  "$SDK_DIR/ruby_fast_lsp_extension.rb" \
  "$EXT_DIR/extension.rb" \
  "$EXT_DIR/runtime.rb" > "$GEN_DIR/bundle.rb"
"$MRBC" -B "$IREP_SYMBOL" -o "$GEN_DIR/bundle.c" "$GEN_DIR/bundle.rb"

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
  -D"EXTENSION_IREP=$IREP_SYMBOL" \
  -I "$MRUBY_ROOT/include" \
  -I "$MRUBY_ROOT/build/wasm32-wasip1/include" \
  "$SDK_DIR/native/extension_shim.c" \
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
  -o "$OUTPUT"

echo "$OUTPUT"
