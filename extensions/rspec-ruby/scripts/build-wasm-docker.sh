#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/extensions/mruby-sdk/scripts/build-wasm-docker.sh" extensions/rspec-ruby
