# Where a frame's time goes, from the kernel's side

The port's own five-second lines say what its threads spent; the kernel
says where they waited. `tools/perf/stalls.bt` is a bpftrace script that
follows the run's processes and prints the futex waits, the long syscalls
and the card's work. `tools/perf/perf-run.sh` starts it beside a sweep:

```
sudo tools/perf/perf-run.sh SWEEP_LABEL=slow ./tools/linux-sweep/run.sh
```

Output lands beside the sweep's (`/tmp/opencode/sweep-<label>/stalls.txt`),
with the widest stalls echoed at the end.

## What each trace answers

| Trace | It is the wall when |
|---|---|
| futex waits (`victory.exe`) | the preparing thread's locks: the guards, the texture pass's queues, the shadow's |
| mmap/munmap/mprotect | the write watch's mapping work per texture (the absent-region cache was for this) |
| read/write (`wineserver`) | the stock wineserver walking views — the known 294 ms remap wait; the patched server (GE-Proton11-14-vt) should not appear |
| ioctl | the card's queue: submissions the driver takes long to bookkeep (the amdgpu userptr page walk, which the THP helper collapses) |
| amdgpu_cs_ioctl | how many submissions a frame makes; a burst is the relay's |
| amdgpu_vm_fault | the guards fault on purpose (the write watch); a flood outside a bake or a load is a mapping gone wrong |

Requires bpftrace and root (or `setcap cap_sys_bpf` on it). The counts are
sampled sums, one line a second — the biggest number of the second is the
wall to split.

## A debug trace of a lost device

The port names the command a device removal was in when it runs as a
diagnostics build with `VICTORY_D3D12_DEBUG=1`. The sweep does it with
`SWEEP_DEBUG=1` (it also takes the log to debug level):

```
SWEEP_DEBUG=1 SWEEP_LABEL=dbg tools/linux-sweep/run.sh
```

`analyze.py` prints the faults it found; the app.log keeps the
breadcrumb lines of the list that was executing.
