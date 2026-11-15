#!/usr/bin/env bash
# Build the library and generate UniFFI client bindings for Python, Kotlin and Swift
# into clients/<lang>/generated. Generated files are not committed.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --locked --lib
case "$(uname -s)" in
  Darwin) lib=target/debug/libprivdat.dylib ;;
  MINGW*|MSYS*|CYGWIN*) lib=target/debug/privdat.dll ;;
  *) lib=target/debug/libprivdat.so ;;
esac

for lang in python kotlin swift; do
  out="clients/$lang/generated"
  rm -rf "$out"
  cargo run --locked --quiet --bin uniffi-bindgen -- generate \
    --library "$lib" --language "$lang" --out-dir "$out"
done

# The Python package loads the native library from next to the generated module.
cp "$lib" clients/python/generated/
echo "bindings written to clients/{python,kotlin,swift}/generated"
