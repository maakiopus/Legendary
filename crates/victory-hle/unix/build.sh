#!/bin/sh
# build.sh <folder of victory.exe>: builds victory_pages.so there (Wine only; optional).
set -e
out=${1:?folder of victory.exe}
env -u CARGO_ENCODED_RUSTFLAGS rustc --edition 2024 --crate-type cdylib --target x86_64-unknown-linux-gnu \
    -O -C panic=abort -C link-arg=-nostdlib -C link-arg=-nostartfiles \
    -o "$out/victory_pages.so" "$(dirname "$0")/victory_pages.rs"
