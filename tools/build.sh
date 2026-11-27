#!/usr/bin/env bash
# Builds this fork's Windows binary, and answers what a commit must pass
# before it is one (`tools/build.sh gate`).
#
# This tree carries no DXC bundling step: crates/victory-gpu/build.rs has no
# `VICTORY_BUILD_DXC`, and dxil.rs only has `on_wine()`. The helper shaders
# therefore compile at run time against a native `d3dcompiler_47.dll` staged
# beside the exe (`tools/d3dcompiler.sh`). A tree with commit b52f4e3 bundles
# them as DXIL, and then `tools/build.sh` looks up or fetches DXC
# (`tools/dxc.sh`) and passes it to the build.
#
#   tools/build.sh fast      the diag binary the runs use (default).
#   tools/build.sh debug     an unoptimized dev build with full debuginfo,
#                            for a debugger or a backtrace.
#   tools/build.sh test      compiles the tests for the Windows target (the
#                            running of them is CI's, on a Windows runner).
#   tools/build.sh gate      what a commit must pass: check, clippy -D
#                            warnings of both feature sets, and the tests'
#                            compile. Nothing is committed past a red gate
#                            (`tools/hooks/pre-commit` runs it).
#
# Modes for a run follow the sweep: `SWEEP_LOG=debug` and
# `VICTORY_D3D12_DEBUG=1` name a faulting command in the log (the
# diagnostics build; `docs/linux-worklog.md`).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
TARGET=${CARGO_TARGET_DIR:-$ROOT/target}
MODE=${1:-fast}

source tools/d3dcompiler.sh

# This tree's build.rs has no DXC step; a tree with commit b52f4e3 names
# `VICTORY_BUILD_DXC` in it and gets the compiler looked up or fetched.
bundling_supported() {
    grep -q "VICTORY_BUILD_DXC" "$ROOT/crates/victory-gpu/build.rs" 2>/dev/null
}

DXC=
if bundling_supported; then
    source tools/dxc.sh
    if ! DXC=$(find_dxc); then
        if fetch_dxc; then
            DXC=$(find_dxc)
        fi
    fi
    if [ -n "${DXC:-}" ]; then
        echo "dxc: $DXC (the helpers go into the exe as DXIL)"
    else
        echo "dxc: none — the helpers will not be bundled; fetch one (retry) or $DXC_LINUX" >&2
    fi
else
    echo "dxc: unused — this tree has no DXIL bundling step; helpers compile at run time"
fi

report_helpers() {
    local exe=$1
    if dxil_bundled "$exe"; then
        echo "helpers: bundled DXIL in the exe (victory-dxil-bundled)"
        return
    fi
    if d3dcompiler_native "$exe"; then
        echo "helpers: compiled at run time by the native d3dcompiler_47 beside the exe"
        return
    fi
    echo "helpers: not bundled — $exe carries no victory-dxil-bundled marker" >&2
    echo "         the helper shaders compile at run time: stage a native" >&2
    echo "         d3dcompiler_47.dll beside the exe with tools/d3dcompiler.sh" >&2
    echo "         (ensure_d3dcompiler_47 <exe>), or restore the DXC bundling" >&2
    echo "         commit b52f4e3 and rebuild (tools/build.sh finds dxc again)" >&2
}

CARGO="cargo xwin"
run_cargo() {
    local sub=$1
    shift
    if [ -n "${DXC:-}" ]; then
        VICTORY_BUILD_DXC=$DXC CARGO_TARGET_DIR=$TARGET $CARGO "$sub" --target x86_64-pc-windows-msvc "$@"
    else
        CARGO_TARGET_DIR=$TARGET $CARGO "$sub" --target x86_64-pc-windows-msvc "$@"
    fi
}

case $MODE in
fast)
    run_cargo build --profile diag -p victory --features diagnostics
    exe=$TARGET/x86_64-pc-windows-msvc/diag/victory.exe
    report_helpers "$exe"
    echo "exe: $exe"
    ;;
debug)
    run_cargo build -p victory --features diagnostics
    exe=$TARGET/x86_64-pc-windows-msvc/debug/victory.exe
    report_helpers "$exe"
    echo "exe: $exe"
    ;;
test)
    run_cargo test -p victory-gpu --features diagnostics --no-run
    echo "the tests compile (the running of them is CI's, on a Windows runner)"
    ;;
gate)
    run_cargo check --workspace --locked
    run_cargo clippy --workspace --all-targets --locked -- -D warnings
    run_cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
    run_cargo test -p victory-gpu --features diagnostics --no-run
    echo "gate: green (check, clippy of both feature sets, the tests' compile)"
    ;;
*)
    echo "usage: tools/build.sh [fast|debug|test|gate]" >&2
    exit 2
    ;;
esac
