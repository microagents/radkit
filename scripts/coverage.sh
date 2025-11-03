#!/usr/bin/env bash
set -euo pipefail

# Native coverage
if command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "Running native coverage..."
  cargo llvm-cov --quiet --workspace --features runtime --html --output-path target/coverage/html
else
  echo "cargo-llvm-cov not installed; run 'cargo install cargo-llvm-cov' first" >&2
  exit 1
fi

# WASI coverage (requires wasmtime)
if command -v wasmtime >/dev/null 2>&1; then
  echo "Running wasm32-wasi coverage..."
  CARGO_TARGET_WASM32_WASI_RUNNER="wasmtime run --dir ." \
    cargo llvm-cov --quiet --workspace --features runtime --target wasm32-wasi \
      --lcov --output-path target/coverage/wasm.lcov
else
  echo "wasmtime not found; skipping wasm32-wasi coverage" >&2
fi
