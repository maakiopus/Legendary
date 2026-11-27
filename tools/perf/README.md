# Where a frame's time goes, from the kernel's side

The port's own five-second lines say what its threads spent; the kernel
says where they waited. `tools/perf/stalls.bt` is a bpftrace script that
follows the run's processes and prints the futex waits, the long syscalls
and the card's work. `tools/perf/perf-run.sh` starts it beside a sweep:

```
tools/perf/perf-run.sh SWEEP_LABEL=slow ./tools/linux-sweep/run.sh
```

`perf-run.sh` runs bpftrace through passwordless `sudo -n` (the sweep stays
your user). Output lands beside the sweep's (`/tmp/opencode/sweep-<label>/stalls.txt`;
the label defaults to the sweep's own `sweep`), with the widest stalls
echoed at the end. Every printed line is stamped with a `time("%H:%M:%S ")`
prefix, and `perf-run.sh` runs bpftrace with `TZ=UTC`, the clock `app.log`
and `perf-report.md` use, so a stall is matched to the sweep's five-second
windows and spike windows. The one-second sums print their stamp
on its own line first, so the block after a stamp is that second's sums.

## What each trace answers

| Trace | It is the wall when |
|---|---|
| futex waits (`victory.exe`) | the preparing thread's locks: the guards, the texture pass's queues, the shadow's |
| mmap/munmap/mprotect | the write watch's mapping work per texture (the absent-region cache was for this) |
| read/write (`wineserver`) | the stock wineserver walking views — the known 294 ms remap wait; the patched server (GE-Proton11-14-vt) should not appear |
| ioctl | the card's queue: submissions the driver takes long to bookkeep (the amdgpu userptr page walk, which the THP helper collapses) |
| amdgpu_cs_ioctl | how many submissions a frame makes; a burst is the relay's |
| amdgpu_vm_fault | the guards fault on purpose (the write watch); a flood outside a bake or a load is a mapping gone wrong |

Requires bpftrace; `perf-run.sh` runs it through passwordless `sudo -n`. The
counts are sampled sums, one line a second — the biggest number of the second
is the wall to split.

`amdgpu_vm_fault` is absent on some kernels (7.3.6 here). `perf-run.sh`
leaves that probe out of the runnable copy when the event is missing; a
manual `bpftrace tools/perf/stalls.bt` needs the same edit or the whole
script aborts on the unmatched tracepoint.

## The sweep's frame-time report

When `tools/linux-sweep/perf.py` exists, `run.sh` runs it after the verdict
against the same app log and captures. It writes four files into the sweep
dir — `frametimes.csv` (one frame a row), `spikes.json` (the spike trace),
`perf-summary.json` (the machine-readable summary) and `perf-report.md`
(the readable report) — and tees a short summary into the sweep output.
`perf-summary.json` carries `windows`, `median_ms`, `p95_ms`, `p99_ms`,
`last_fps`, `spikes`, `top_stall` and `spike_windows`; `ab.py` picks up
`median_ms`, `p95_ms`, `p99_ms`, `spikes` and `top_stall` as extra columns
whenever the summary is present (a dash otherwise).

The report's spike windows are five-second windows, the same granularity as
the present/fps windows `analyze.py` prints, and the stall tracer's stamps
(`stalls.bt`, `time("%H:%M:%S ")`) are on the same wall clock — a spike
window names the seconds, `stalls.txt` names what the kernel saw in them,
and `perf-report.md` reads the two against each other.

## A debug trace of a lost device

The port names the command a device removal was in when it runs as a
diagnostics build with `VICTORY_D3D12_DEBUG=1`. The sweep does it with
`SWEEP_DEBUG=1` (it also takes the log to debug level):

```
SWEEP_DEBUG=1 SWEEP_LABEL=dbg tools/linux-sweep/run.sh
```

`analyze.py` prints the faults it found; the app.log keeps the
breadcrumb lines of the list that was executing.
