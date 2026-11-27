#!/usr/bin/env bash
# Runs the stall tracer beside a sweep: the sweep's own output plus the
# kernel's view of where the frame time goes.
#
#   tools/perf/perf-run.sh SWEEP_LABEL=slow ./tools/linux-sweep/run.sh
#
# Leading NAME=VALUE arguments are exported for the sweep (sudo strips the
# caller's environment, so this is how SWEEP_LABEL reaches it). The traces
# land in /tmp/opencode/sweep-<label>/stalls.txt, where <label> defaults to
# the sweep's own `sweep`, so the report lands in its directory. bpftrace
# runs through passwordless `sudo -n` when the caller is not root; without
# bpftrace it says what is missing and runs the sweep alone.
#
# A debug trace of a port that lost its device: SWEEP_DEBUG=1 (the sweep
# sets VICTORY_LOG=debug and VICTORY_D3D12_DEBUG=1, which name the command
# the removal was in) — the sweep script does it, and the log keeps the
# breadcrumb lines.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)

# NAME=VALUE before the command are environment for the sweep, not its argv.
ARGS=()
while [ $# -gt 0 ]; do
    case $1 in
    [A-Za-z_][A-Za-z0-9_]*=*) export "$1"; shift ;;
    *) ARGS+=("$1"); shift ;;
    esac
done
set -- "${ARGS[@]}"

if ! command -v bpftrace >/dev/null 2>&1; then
    echo "perf-run: no bpftrace; the sweep runs alone (install bpftrace, or read the BCC ones in tools/perf/README.md)" >&2
    exec "$@"
fi
LABEL=${SWEEP_LABEL:-sweep}
WORK=/tmp/opencode/sweep-$LABEL
mkdir -p "$WORK"

# One unmatched tracepoint aborts the whole script, and not every kernel has
# amdgpu_vm_fault (7.3.6 here does not): the runnable copy leaves that probe
# out when the event is absent.
TRACE=$ROOT/tools/perf/stalls.bt
RUN=$WORK/stalls.bt
if [ -e /sys/kernel/tracing/events/amdgpu/amdgpu_vm_fault ] ||
   [ -e /sys/kernel/debug/tracing/events/amdgpu/amdgpu_vm_fault ]; then
    cp "$TRACE" "$RUN"
else
    echo "perf-run: amdgpu_vm_fault is absent on this kernel; that probe is left out" >&2
    sed -e '/^tracepoint:amdgpu:amdgpu_vm_fault$/,/^}$/d' \
        -e '/^    print(@vm_faults);$/d' \
        -e '/^    clear(@vm_faults);$/d' \
        "$TRACE" > "$RUN"
fi

TRACER=(env TZ=UTC bpftrace "$RUN")
if [ "$(id -u)" != 0 ]; then
    echo "perf-run: bpftrace runs through sudo -n (passwordless)" >&2
    TRACER=(sudo -n env TZ=UTC bpftrace "$RUN")
fi

"${TRACER[@]}" > "$WORK/stalls.txt" 2>&1 &
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
grep -E '[0-9]+ us$' "$WORK/stalls.txt" 2>/dev/null | awk '{print $(NF-1), $0}' | sort -rn | head -8 | cut -d' ' -f2-
# When the sweep ran perf.py afterwards, its frame-time report is here too.
if [ -f "$WORK/perf-report.md" ]; then
    echo "perf-run: the frame-time report is in $WORK/perf-report.md"
fi
exit "$rc"
