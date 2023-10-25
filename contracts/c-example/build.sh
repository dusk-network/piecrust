#!/usr/bin/env sh

BASEDIR=$(dirname "$0")

clang -nostdlib -Os \
   --target=wasm64 \
   -Wl,--allow-undefined \
   -Wl,--no-entry \
   -Wl,--export=A \
   -Wl,--export=increment_and_read \
   -Wl,--export=out_of_bounds \
   "$BASEDIR/contract.c" \
   -o "$BASEDIR/../../target/wasm64-unknown-unknown/release/c-example.wasm"
