#!/usr/bin/env bash
# Builds this fork's Windows binary, and answers what a commit must pass
# before it is one (`tools/build.sh gate`).
#
#   tools/build.sh fast      the diag binary the runs use (default). DXC is
#                            found or fetched (`tools/dxc.sh`); without it
#                            Wine cannot compile the helper shaders at run
#                            time and the world's indirect records drop.
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
TARGET=${CARGO_TARGET_DIR:-$HOME/.cache/main-target}
MODE=${1:-fast}

# The compiler of the helper shaders, found or fetched.
source tools/dxc.sh
if ! DXC=$(find_dxc); then
    if fetch_dxc; then
        DXC=$(find_dxc)
    fi
fi
if [ -n "${DXC:-}" ]; then
    echo "dxc: $DXC (the helpers go into the exe as DXIL)"
else
    echo "dxc: none — Wine cannot compile the helpers at run time; the world's indirect records drop" >&2
    echo "     fetch one: tools/build.sh fast (retry after the download), or" >&2
    echo "     $DXC_LINUX" >&2
fi

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
    if dxil_bundled "$exe"; then
        echo "helpers: bundled DXIL in the exe"
    else
        echo "helpers: none in the exe — fetch dxc and build again (tools/dxc.sh)" >&2
    fi
    echo "exe: $exe"
    ;;
debug)
    run_cargo build -p victory --features diagnostics
    echo "exe: $TARGET/x86_64-pc-windows-msvc/debug/victory.exe"
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
