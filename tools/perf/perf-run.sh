#!/usr/bin/env bash
# Runs the stall tracer beside a sweep: the sweep's own output plus the
# kernel's view of where the frame time goes.
#
#   sudo tools/perf/perf-run.sh <sweep arguments...>
#
# The sweep's arguments pass through (SWEEP_LABEL=slow ./tools/linux-sweep/run.sh
# …); the traces land in /tmp/opencode/sweep-<label>/stalls.txt. Requires
# bpftrace and root (or cap_sys_bpf); without them it says what is missing
# and runs the sweep alone.
#
# A debug trace of a port that lost its device: SWEEP_DEBUG=1 (the sweep
# sets VICTORY_LOG=debug and VICTORY_D3D12_DEBUG=1, which name the command
# the removal was in) — the sweep script does it, and the log keeps the
# breadcrumb lines.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)

if ! command -v bpftrace >/dev/null 2>&1; then
    echo "perf-run: no bpftrace; the sweep runs alone (install bpftrace, or read the BCC ones in tools/perf/README.md)" >&2
    exec "$@"
fi
if [ "$(id -u)" != 0 ]; then
    echo "perf-run: not root — the traces need the kernel's tracepoints (sudo, or setcap cap_sys_bpf)" >&2
fi

LABEL=${SWEEP_LABEL:-perf}
WORK=/tmp/opencode/sweep-$LABEL
mkdir -p "$WORK"

bpftrace "$ROOT/tools/perf/stalls.bt" > "$WORK/stalls.txt" 2>&1 &
tracer=$!
trap 'kill "$tracer" 2>/dev/null' EXIT INT TERM
sleep 1

"$@"
rc=$?
sleep 1
kill "$tracer" 2>/dev/null
wait "$tracer" 2>/dev/null

lines=$(wc -l < "$WORK/stalls.txt" 2>/dev/null || echo 0)
echo "perf-run: the sweep exited $rc; $lines lines of stalls in $WORK/stalls.txt"
echo "perf-run: the widest of them:"
grep -vE "^(@|stalls:|Attaching)" "$WORK/stalls.txt" 2>/dev/null | sort -t' ' -k6 -rn 2>/dev/null | head -8
exit "$rc"
