#!/bin/sh
# build.sh <folder of victory.exe>: builds victory_mem.so there (Wine only; optional:
# without it the runtime's memory calls go through Wine as on Windows).
# Needs a rustc with the x86_64-unknown-linux-gnu target; run it without a Windows
# cross-build environment in the shell (cargo-xwin's), or the .so comes out broken.
set -e
out=${1:?folder of victory.exe}
env -u CARGO_ENCODED_RUSTFLAGS -u RUSTFLAGS -u CC -u CXX -u AR \
    "${RUSTC:-rustc}" --edition 2024 --crate-type cdylib --target x86_64-unknown-linux-gnu \
    -O -C panic=abort -C link-arg=-nostdlib -C link-arg=-nostartfiles \
    -o "$out/victory_mem.so.new" "$(dirname "$0")/victory_mem.rs"
mv -f "$out/victory_mem.so.new" "$out/victory_mem.so"
