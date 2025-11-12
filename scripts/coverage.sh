#!/usr/bin/env bash
set -euo pipefail

FEATURES="runtime test-support"

# Native coverage
if command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "Running native coverage..."
  cargo llvm-cov --quiet --workspace --features "$FEATURES" --html --output-dir target/coverage/html
else
  echo "cargo-llvm-cov not installed; run 'cargo install cargo-llvm-cov' first" >&2
  exit 1
fi

# WASIp1 coverage (disabled until profiler_builtins is available for the target)
# Uncomment once rustc ships `profiler_builtins` for wasm32-wasip1.
# if command -v wasmtime >/dev/null 2>&1; then
#   if rustc --print target-list | grep -q "wasm32-wasip1"; then
#     export CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime run --dir ."
#     echo "Running wasm32-wasip1 coverage..."
#     cargo llvm-cov --quiet --workspace --features "$FEATURES" --target wasm32-wasip1 \
#       --lcov --output-path target/coverage/wasm32-wasip1.lcov
#   else
#     echo "wasm32-wasip1 target not installed; skipping WASIp1 coverage" >&2
#   fi
# else
#   echo "wasmtime not found; skipping wasm32-wasip1 coverage" >&2
# fi
