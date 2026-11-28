# Thunking hot Wine memory calls to Linux

Status: proposal, 28 Nov 2026. No code changes in this pass.

## Conclusion

Do not replace Wine's memory API wholesale. The useful boundary is narrower:
the layer already owns the guard state, the write-watch journal, and the
texture staging state. A native call is safe only where that state proves the
Windows contract and Wine remains the fallback.

The first three prototypes should be:

1. **Guard-rearm feasibility: `VirtualProtect` -> `mprotect` plus a Wine vprot synchronization hook.**
2. **Batched texture readability: `K32QueryWorkingSetEx` -> `process_vm_readv`.**
3. **A write-watch refusal gate with Wine unchanged; native `PAGEMAP_SCAN` is a separate follow-up.**

P1 is a feasibility prototype until a Wine vprot synchronization hook exists.
The third prototype is deliberately last. `GetWriteWatch` has the largest
measured phase envelope, but the blanket native scan already measured
neutral/slower and the earlier layer-side view skip crashed two Proton runs.
P3a must remove refused askings without changing reset ownership or turning a
refusal into a false clean answer. P3b must prove mapping-generation and
partial-reset handling before it is enabled.

A native negative probe for `VirtualQuery` hole checks is promising, but the
current profile has only 28-29 hole questions/frame and no isolated timing. It
is a watchlist item, not a first prototype, until that cost is measured.

## Evidence and accounting

### Current world budget

`docs/20fps.md:58-79` is the current fixed-save, Series X, scale 0.667 budget:

| Work | Measured per frame | Scope |
|---|---:|---|
| frame | 136 ms | wall |
| preparing | 110.9 ms | submitting thread |
| copies in the card's memory | 40.3 ms over ~92 MB | submitting thread; GPU/copy work |
| list steps | 52.7 ms | submitting thread |
| refresh flushes within list steps | 46 ms | submitting thread |
| textures | 7.6 ms | layer texture line; not syscall-specific |
| write-watch askings | 44.0 ms; 428 refused | all threads |
| copy-back | 25.7 ms | separate relay/landing work |
| GPU wait | 3.0 ms | submitting thread |

These categories overlap. The 40.3 ms card copies and 46 ms refresh flushes
are not syscall costs. Within the `docs/20fps.md` budget, the 44.0 ms
write-watch line is the only number that directly names a Wine memory API.

The same document records the prior `victory_mem.so` result: direct pages and
native scan were neutral/slower with the patched rbtree wineserver, so the
library remains opt-in and off (`docs/20fps.md:100-103`).

### Profiled world snapshot

`/tmp/opencode/sweep-prof/app.log` reached a populated world window while this
proposal was being written. At 05:41:00-05:41:06 it reports 6,436 draws/frame
and a 141.3 ms frame (`app.log:21322`):

- `app.log:21322`: **114.6 ms preparing** on the submitting thread; card copies
  41.4 ms / 94.1 MB; textures 7.8 ms / 32.6 MB on that thread; GPU 2.9 ms;
  copy-back 24.9 ms.
- `app.log:21272`: **47.9 ms of preparing is “asking the write watch.”**
- `app.log:21273`: 476 kernel askings over 2,714 MB cost **7.1 ms across all
  threads**, with **420 refused**; 28 hole questions; 526 blocks made read-only;
  **10.4 ms waiting for the guards** and **15.6 ms changing protections**.
- `app.log:21307`: the next window has 461 askings / 7.2 ms, 411 refused,
  29 hole questions, 512 blocks, 10.0 ms guard wait, and **14.3 ms changing
  protections**.
- `app.log:21334`: 609 watch/guard operations cost **19.80 ms**, with 322 guard
  faults; moves cost 10.29 ms.
- `app.log:21335`: guard fallback work is **15.2 ms** across recording threads.
- `app.log:21337`: waits for the write trackers' locks are 16.57 ms for ranges,
  2.34 ms for journal reads, 0.90 ms for journal notes, and **28.48 ms for
  guards**, all threads.
- `app.log:21276`: texture refreshes cost **21.2 ms across all threads**, of
  which **15.7 ms is asking the write watch and the guards**. This is not a
  `K32QueryWorkingSetEx` timer.
- `app.log:21320`: only 2.9 write-watch askings/frame exceed 1 ms, totalling
  4.7 ms; the longest is 4.5 ms.
- `app.log:21259`, `21293`: landing spends 3.01-3.15 ms opening or noting guards.

The important split is now visible: the submitting thread's 47.9 ms
write-watch phase is not 47.9 ms of `GetWriteWatch` syscall work. The kernel
askings are 7.1 ms across all threads. Guard protection and guard-lock waits
are separately measured and larger; refused-call lock contention is a
plausible remaining share, but these overlapping counters do not isolate it.

`/tmp/opencode/sweep-prof/stalls.txt:77-109` contains no `mprotect` or `ioctl`
stall over the tracer's 5 ms threshold. That does **not** prove those calls are
cheap: `tools/perf/stalls.bt:87-113` records only individual syscalls over
5 ms, while the app counter aggregates many protection changes.

`/tmp/opencode/sweep-prof/stat.txt:4-16` is a 10 s CPU sample: 58.986 s
task-clock on 5.9 CPUs, 300,092 context switches, 151,882 migrations, and
498,487 page faults. It supports the CPU/thread-bound conclusion but contains
no syscall attribution.

`/tmp/opencode/sweep-prof/report.txt:14-17` has 35K samples and no lost
samples. The report header is `Children` then `Self` (`report.txt:19`). The
named memory paths' **cumulative** shares are `NtProtectVirtualMemory` at
**1.21%**, `get_working_set_ex` at **0.72%**, and `NtGetWriteWatch` at
**0.41%** (`report.txt:553-557`, `843-845`, `1155-1157`). Their displayed self
shares are only **0.02%**, **0.06%**, and **0.01%**; `set_protection` is 0.98%
cumulative and 0.00% self (`report.txt:631-633`). These are sample shares, not
milliseconds. Kernel symbols are unresolved (`report.txt:1-9`), so the report
cannot assign the kernel-side `mprotect` or `PAGEMAP_SCAN` time. Ranking below
therefore uses the app call timers, not these cumulative percentages.

The log-derived report gives a 142.6 ms median and 240.7 ms p95 over 37
windows (`perf-report.md:3-8`). Its p99 is dominated by a 199,176 ms end-of-run
stall (`perf-report.md:10-16`) and is not a useful thunk A/B metric. The run's
correctness verdict is PASS: 0 faults, 0 crashes, 0 black-frame warnings, and
304/311 nonblank world captures (`verdict.txt:4-13`).

## Ranked candidates

Costs are per frame. “All threads” is aggregate thread time, not submitting
thread wall time. An upper bound is not an expected saving. Rank follows the
measured call-specific cost; larger parent envelopes are shown but are not
mistaken for syscall cost. Cost rank is P1, P3a, P2. The first-prototype list
is P1, P2, P3a because P2 has lower semantic risk; P3b is not in that list.

| Rank | Call and site | Measured cost and evidence | Native replacement | What the `.so` needs | Risk | Decision |
|---:|---|---|---|---|---|---|
| 1 | `VirtualProtect` in `watch::protect_alias`, `protect_here`, `unguard` (`crates/victory-gpu/src/watch.rs:3108-3179`, `3353-3382`) | **14.3-16.5 ms all threads** changing protections in populated windows (`app.log:21273`, `21307`, `21441`); 10.0-10.4 ms guard-lock wait in the same windows. Perf: `NtProtectVirtualMemory` 1.21% cumulative / 0.02% self; `set_protection` 0.98% cumulative / 0.00% self (`report.txt:553-557`, `631-633`) | `mprotect(PROT_READ)` / `mprotect(PROT_READ|PROT_WRITE)` on merged 64 KiB runs | Existing `victory_mem::protect` ABI is enough only for a shadow/feasibility prototype (`/tmp/opencode/port-linuxmem/crates/victory-hle/unix/victory_mem.rs:374-411`). A functional thunk also needs a Wine-side vprot synchronization hook; the `.so` cannot update Wine's static bookkeeping itself | High | Prototype P1 |
| 2 | `GetWriteWatch` / `ResetWriteWatch` in `watch::kernel_written` / `reset_found` (`crates/victory-gpu/src/watch.rs:649-753`) | **47.9 ms submitting-thread write-watch phase** (`app.log:21272`); **44.0 ms all threads; 428 refused/frame** in the fixed budget (`docs/20fps.md:69-79`). Current kernel askings are only **7.1 ms all threads, 420 refused** (`app.log:21273`), so the addressable syscall/lock share is unknown. Perf: `NtGetWriteWatch` 0.41% cumulative / 0.01% self (`report.txt:1155-1157`) | P3a: no syscall for proven views; Wine for watched/unknown. P3b: `ioctl(/proc/self/pagemap, PAGEMAP_SCAN)` only after mapping-generation and partial-reset fixes | P3a needs no `.so` change. P3b can reuse `scan` (`victory_mem.rs:230-276`); the Rust adapter must expose its existing `found`/`walk_end` as complete/refused/partial-or-unknown. Mapping synchronization belongs to Rust/HLE state | High | P3a first; P3b separate |
| 3 | `K32QueryWorkingSetEx` plus one-byte fallback reads in `texture::readable` (`crates/victory-gpu/src/texture.rs:1707-1751`) | **21.2 ms all threads** for texture refresh, **15.7 ms asking write watch/guards** (`app.log:21276`); the readability share is unknown. The fixed budget's 7.6 ms texture line is smaller but is not a readability timer (`docs/20fps.md:68`). Perf: `get_working_set_ex` 0.72% cumulative / 0.06% self (`report.txt:843-845`) | `process_vm_readv(getpid(), ...)`, one local byte per remote page/run | Existing `readable` call (`victory_mem.rs:279-343`) covers contiguous ranges. Add a versioned `readable_runs` call for dispersed texture probes, with tri-state error reporting and per-run results | Medium | Prototype P2 |
| 4 | `VirtualQuery` / `NtQueryVirtualMemory` for known holes in `uncommitted_end_asked` and `placeholder` (`crates/victory-gpu/src/watch.rs:2148-2164`, `3253-3302`) | **28-29 hole questions/frame in populated windows** (`app.log:21273`, `21307`); their time is not isolated. Historical 6,200/frame belongs to an older 930 ms configuration (`docs/linux-research.md:72-90`) and is not a current number | Negative `process_vm_readv` probe; any readable result or non-`EFAULT` error falls back to Wine | Extend the readable ABI with `readable` / `unreadable(EFAULT)` / `error(other)`; a batch form is desirable but must preserve independent outcomes | Medium | Watchlist |
| 5 | `ReadProcessMemory` / `NtWriteVirtualMemory` in `host::read` / `write_mapped` (`crates/victory-gpu/src/host.rs:8991-9004`, `9032-9077`) | **Not a default hot Wine path**: `safecopy::on()` defaults on and bypasses both (`crates/victory-gpu/src/safecopy.rs:379-401`; `host.rs:8996-9000`, `9041-9045`) | `process_vm_readv` / `process_vm_writev` | New bulk transfer ABI with partial-byte reporting; not needed for the default play configuration | Medium | Fallback-mode experiment only |
| 6 | `MapViewOfFile3` / `UnmapViewOfFile2` in `xmem` (`crates/victory-hle/src/modules/xmem.rs`) | Not isolated per frame. Historical mapping churn is described in `docs/linux-research.md:87-93`; current direct-page branch measured neutral/slower (`docs/20fps.md:100-103`) | `mremap(MREMAP_MAYMOVE|MREMAP_FIXED)` over a shared section view | Existing `show` call (`victory_mem.rs:184-227`) | High | Do not prioritize |
| 7 | `GetThreadTimes` in diagnostics (`crates/victory-gpu/src/copystat.rs:1115-1123`) | Diagnostics only; no frame-work saving | `getrusage(RUSAGE_THREAD)` | Existing `times` call (`victory_mem.rs:346-371`) | Low | Already prototyped on `linuxmem-port`; not a frame optimization |
| 8 | `QueryPerformanceCounter` in meters | Meter overhead only; no isolated frame cost | `rdtsc` with host anchoring | Separate clock facility already prototyped on `linuxmem-port` | Low-Medium | Not a memory thunk |

## Candidate contracts

### 1. Guard transitions: `VirtualProtect` -> `mprotect`

**Call path.** `watch::writes` falls back to `watch::guard` for view memory
(`watch.rs:1342-1382`). `guard` identifies written blocks, then `protect` /
`protect_here` makes them read-only. A later title fault or layer write calls
`open` / `unguard` and makes them writable again. Aliased physical pages must
be covered at every view (`watch.rs:3042-3149`).

**What the native call changes.** Only the kernel VMA protection. It does not
update Wine's `pages_vprot` table. Wine's own `NtProtectVirtualMemory` takes
`virtual_mutex`, validates that the whole range is one committed view, updates
that table, and calls the kernel. A direct `mprotect` intentionally skips the
Wine bookkeeping for a short interval in which the layer owns the guard.

**Who else reads the state.**

- `PROTECTED`, `Guards::protected`, `Guards::clean`, `PUBLISHED`, `BESIDE`, and
  `Guards::aliased` decide whether a page is clean, absent, or must fault.
- `watch::on_write` and `open_faulted` use those bits to reopen a page.
- `inplace` alias tracking decides which other views share the physical page.
- `title_memory_remapped`, `title_memory_unmapping`, and `title_memory_freed`
  invalidate guard state around mapping changes.
- Wine's `VirtualQuery` and a guest `VirtualProtect` can still observe or
  change Wine's protection bookkeeping while the native guard is active.

**Locking and ordering.**

- Keep the existing `GUARDS` write lock, or the shared lock plus ordered
  `Claims`, for the whole transition. Do not introduce a second lock order.
- Keep `Asking` held where `writes` already holds it. A guard transition must
  not race another write-watch answer for the same range.
- Set `PROTECTED` before making a page read-only, as the current code does.
  Insert `g.protected` / `g.clean` only after the native call succeeds.
- Before the first alias becomes writable, clear `clean` and `PUBLISHED` for
  **every** alias in the transition. Otherwise `nothing_new()` can skip a copy
  through an alias that is already writable while the rest of the batch is
  still running.
- Clear `PROTECTED` and `g.protected` per block immediately after that block's
  `mprotect` succeeds. Do not defer all state changes to the end of the batch.
- On failure, restore protected state only for blocks proven still read-only.
  A block whose state is unknown becomes dirty and goes through the existing
  fault/open retry; it must never remain stale-clean.
- Preserve alias ordering: every view that must observe the write is opened
  before the layer reports the block written.
- Merge only adjacent, known-protected 64 KiB blocks with no hole. A merged
  `mprotect` failure falls back per run; it must not silently skip the failed
  range.

**Fallback.** Fall back to the existing `VirtualProtect` call when not under
Wine, the library/version/capability is absent, a run fails, or the range is
not wholly layer-owned committed read-write/read-only view memory. A partial
native batch reports completed runs; the failed run and the remainder go
through Wine. The fallback keeps the current old-protection validation; it
must not blindly force read-write on a range whose protection changed.

**Wine bookkeeping is a blocker, not a footnote.** A successful native rearm
leaves Wine's `pages_vprot` read-only. `VirtualQuery`, `writable()`, and stale
fault handling can then disagree with the kernel. Functional success requires
a Wine-side hook that synchronizes vprot before the block is reported
writable. The existing `.so` ABI cannot do that. Until the hook exists, P1 is a
shadow/feasibility experiment only and must not be enabled in the game.

**Observable-semantics risk.** This is the highest-risk prototype. A guest
`VirtualProtect` can update Wine's table and then the kernel without the
layer's bits changing. Required tests are partial-batch success, a stale fault
on a partially reopened alias, a guest query during the transition, and a
guest protect racing the transition. If guest queries/protects affect control
flow, the hook must cover them too or the thunk is not acceptable.

### 2. Texture readability: `K32QueryWorkingSetEx` -> `process_vm_readv`

**Call path.** `texture::committed` calls `readable`; `mapped_bytes` and
`mapped_bytes_between` use up to eight dispersed probes, then one probe per
64 KiB block (`texture.rs:1680-1751`, `1779-1880`). The current Windows path
asks `K32QueryWorkingSetEx` for up to 256 pages and performs a one-byte
`host::read_into` when the working-set bit is not valid.

**Contract to preserve.** The layer does not ask “is this page resident?” It
asks “can the following read or write use this page?” A committed but
unreadable page counts as absent. A readable read-only page counts as present.
A partial range must report the same pages as the current loop. This is why
`mincore` or `PAGEMAP_SCAN(PAGE_IS_PRESENT)` is not an equivalent replacement.

**Native shape.** `process_vm_readv` transfers directly between address spaces
and may return a partial page-granular result. The existing one-byte-per-page
implementation is a good semantic base. A `readable_runs` ABI should accept
run addresses/lengths, return a result per run, and stop or continue exactly
as the Rust caller requests. One unix-library dispatch can own several
syscalls; it must not pretend a partial transfer proves later runs unreadable.

**Who else reads the state.** Texture staging, `mapped_bytes`, resource
creation, list reads, and `memory` probes all consume the answer. The native
probe may fault in a never-touched readable page, which changes the working
set but not the logical committed/readable answer. Callers already tolerate a
mapping changing between the probe and the later read.

**Locking and ordering.** No Wine `virtual_mutex` is needed. Keep the existing
texture/staging locks; do not hold them across a new blocking fallback. A
native answer is a snapshot. A successful probe followed by a failed read must
take the existing missing-page path, not manufacture texture bytes.

**Fallback.** Use Wine when not under Wine, the probe/version fails, the local
iovec is invalid, or the syscall returns `EPERM`, `ENOMEM`, `EINVAL`, or
`ESRCH`. `EFAULT` is a valid “remote page unreadable” result only after the
local buffer/vector has been validated. Capability failure disables the
switch for the process and logs once.

**Prior result.** The contiguous `VICTORY_NATIVE_PRESENT` switch already
measured neutral/slower (`docs/20fps.md:100-103`). The new prototype must prove
that batching dispersed probes removes enough unix-dispatch and Wine-lock
overhead to change that result; otherwise stop.

### 3. Write watch: gate first, native scan separately

**Call path.** `kernel_written` calls `GetWriteWatch`; `writes` first checks
the journal and guarded/published bits, then calls the kernel, then falls back
to `guard` on refusal (`watch.rs:1301-1413`). `ask_ahead` merges nearby ranges
but a refusal recursively splits them (`watch.rs:756-835`, `1265-1283`).

**Contract to preserve.**

- A successful answer is an ordered list of written 4 KiB pages, with reset
  semantics when requested.
- A range that is not one write-watched reservation is refused. Refusal is not
  an empty clean answer.
- A resetting scan and a Wine `GetWriteWatch` must never both consume the same
  dirty state.
- `writes(..., guarded=false)` must return `None` for a view so texture refresh
  enters its own guard/hash path (`watch.rs:1333-1356`). Only
  `writes(..., guarded=true)` may take the layer's guard path.
- A partial native scan can have reset completed chunks. The caller must keep
  those pages and conservatively handle the failed chunk.

**What already exists.** `victory_mem::scan` implements `PAGEMAP_SCAN`, and
`linuxmem::written` adapts runs to the layer's page/part journal. The probe
checks that Wine's `MEM_WRITE_WATCH` allocations are the userfaultfd async
write-protect kind before enabling it (`linuxmem.rs:591-621`). Its current
`Some(())` result does not distinguish complete from partial/unknown
(`linuxmem.rs:624-671`), so it is not sufficient for P3b yet.

**P3a: refusal gate only; Wine stays unchanged.**

1. Journal hit: no kernel call.
2. Proven view and `guarded=true`: take the existing guard path.
3. Proven view and `guarded=false`: return `None`; texture refresh keeps its
   own guard/hash path.
4. Proven watched allocation or unknown: call Wine `GetWriteWatch` exactly as
   today.

The gate may use only state invalidated on every map/unmap/free path.
`VIEWS`, `PROTECTED`, and `Shadow::watched` are leads, not proof by themselves.
P3a must also capture the mapping/allocation generation and either hold mapping
state stable through classify -> branch or validate it unchanged before taking
the guard/`None` path; otherwise it falls back to Wine. The earlier
`VICTORY_VIEW_ASK=1` skip crashed two Proton runs (`watch.rs:851-871`), so the
proof obligation is the missing invalidation or misclassification, not the
syscall. P3a adds no reset backend and requires no native syscall count to rise.

**P3b: native scan, only after P3a and as a separate arm.**

- Use `PAGEMAP_SCAN` only for a proven single write-watched allocation.
- `Asking` does not exclude map/free. A resetting scan must hold mapping state
  stable across classify -> scan -> journal note, using
  `title_memory_reading()` with a proven lock order or a per-allocation
  generation lock. A post-scan generation check alone is too late: the scan
  may already have reset a reused allocation.
- Replace the `Some(())` adapter result with:
  `complete`, `refused-before-reset`, or `partial-or-unknown` plus `walk_end`
  and the uncertain range. Completed pages are journaled; uncertain pages are
  marked dirty and are never reset-retried.
- Capability failure, mixed ranges, and unknown classification stay on Wine.

**Locking and ordering.** P3a keeps `Asking` and journal ordering unchanged.
P3b also needs the mapping-generation synchronization above; avoiding Wine's
`virtual_mutex` does not remove the need to serialize askings and prevent
address reuse during a reset.

**Fallback.** P3a uses Wine exactly once for every unknown or watched range.
P3b may use Wine only when no native reset can have occurred. If a native reset
may already have happened, use the explicit partial/unknown result and mark
the uncertain chunk dirty; never blindly retry the whole range with Wine.

**Expected value.** P3a addresses only the refused-call and Wine-lock share of
the 47.9 ms submitting-thread phase; the kernel askings themselves are 7.1 ms
across all threads (`app.log:21272-21273`). That share is not measured yet.
P3b is a separate, unproven experiment: the prior blanket native scan measured
neutral/slower, and only 56 of 476 askings in this profile were non-refused.

### 4. Watchlist: negative hole probe

`uncommitted_end_asked` uses `VirtualQuery` to find the end of a reserved or
free region; `placeholder` uses it to distinguish a placeholder from ordinary
private no-access memory (`watch.rs:2148-2164`, `3289-3302`). A native read can
prove only one useful negative: a previously absent block is still unreadable.
It cannot distinguish reserved no-access from committed no-access, and it
cannot reconstruct Wine's region type.

A safe prototype would therefore be:

- `process_vm_readv` says unreadable with `EFAULT`: keep the block absent.
- Readable, or any other error: call the existing Wine query and use its full
  answer.

The state readers are `Guards::absent`, `held`, `foreign`, `PUBLISHED`, and the
texture/copy paths that skip absent pages. Mapping notifications and the
once-per-frame hole recheck remain mandatory. Failure to notice a newly mapped
page can skip a texture upload or copy until the next notification/recheck;
the failure looks like stale or black texture data, not a syscall error.

The world-period counter is now 28-29 questions/frame, but there is no
`VirtualQuery` timing breakdown. Do not build this until that breakdown shows
a worthwhile target.

## The three first prototypes

| Prototype | Expected ms | Risk | Smallest proof | Game failure mode |
|---|---|---|---|---|
| **P1: guard-rearm feasibility (`mprotect` + Wine vprot hook)** | **Estimate: no functional ms claim until the Wine hook exists. A shadow run can measure only the syscall/lock ceiling, bounded by the observed 16.5 ms all-thread protection-change envelope (14.3-16.5 ms).** | High | `membench`-style two-alias test with a partial batch: clear all aliases dirty before the first rearm, reopen one at a time, inject an unmapped run, force a stale fault, and query/protect through Wine mid-transition. Prove vprot synchronization and state rollback before any fixed-save arm | Wine and the kernel disagree about protection; `nothing_new()` skips a newly writable alias; a guest protect races the guard; a page stays read-only or a write is not noticed |
| **P2: batched `readable_runs` for texture probes** | **Estimate: no demonstrated positive saving; plan for 0 ms until the readable share is timed. The parent texture-refresh envelope is 21.2 ms all threads, and the prior native-present switch was neutral/slower.** | Medium | Answer-equivalence matrix for readable, read-only, reserved, no-access, partially mapped, never-touched, and unmap-race ranges; include dispersed runs and forced `EPERM`/`ENOMEM` fallback. Then one fixed-save A/B | A readable page is called absent, so a texture is skipped or left neutral/black; or probing faults in too many pages and causes memory-pressure spikes |
| **P3a: write-watch refusal gate, Wine unchanged** | **Estimate: only the refused/lock-contention share of the 47.9 ms submitting-thread write-watch phase; currently unmeasured.** The kernel askings themselves are 7.1 ms all threads. No native-scan gain is claimed | High | Exhaustive state test over watched allocations, guarded views, aliases, placeholders, map/unmap/free, and writes through every alias; assert `guarded=false` returns `None` for views and Wine remains the only reset backend. Then one fixed-save A/B. P3b's mapping-reuse and partial-reset tests are separate | A view is misclassified, the texture guard/hash path is bypassed, or a dirty page is missed; result is stale/black textures, missing buffer updates, or the earlier exit-99 class of crash |

P1, P2, and P3a must be tested in separate arms. P3b is a later, separate
native-scan experiment and is not part of these first three prototypes.

## What not to thunk

| Call or work | Why not |
|---|---|
| Card copies, hash/compare passes, refresh flushes | The 40.3 ms copies and 46 ms flushes are bandwidth, comparison, and GPU synchronization work (`docs/20fps.md:66-77`), not Wine syscall overhead. A Linux syscall cannot remove that work. |
| General `VirtualQuery` / `NtQueryVirtualMemory` | Wine synthesizes allocation base, region type, commit state, protection, and placeholder information from its own view/vprot state. `/proc/self/maps` and Linux VMA permissions are not equivalent. The direct `mremap` branch already deliberately makes Windows see a private committed “lie.” |
| `MapViewOfFile3`, `UnmapViewOfFile2`, `VirtualAlloc2`, `VirtualFree` | Section objects, placeholders, aliases, allocation requirements, and wineserver protocol are part of the contract. The existing direct-page mechanism measured neutral/slower here and should remain opt-in. |
| Generic guest `VirtualProtect` | Only transitions on ranges the layer has proven it owns are candidates, and a functional native path still needs Wine vprot synchronization. A generic replacement would desynchronize Wine's protection table from the kernel. |
| `ReadProcessMemory` / `NtWriteVirtualMemory` in the default configuration | `VICTORY_SAFE_COPY` defaults on and uses the layer's fault-safe `rep movsb` paths. There is no default Wine call to remove. A process-vm backend is a fallback-mode experiment only. |
| `ResetWriteWatch` by itself | Reset consumes state. It must use the same backend and ownership rules as the preceding `GetWriteWatch`; mixing backends can erase dirty pages. |
| `GetWriteWatch` refusal rewritten as success | A view or plain allocation is not write-watched. A false empty answer bypasses the guard path and loses writes. |
| `K32QueryWorkingSetEx` rewritten as `mincore` / present-page scan | Residency is not readability or commit. Never-touched readable pages and unreadable committed pages make these APIs non-equivalent. |
| `GetThreadTimes`, process times, `QueryPerformanceCounter` | These are meters. Native replacements can reduce diagnostics overhead but do not reduce the game's frame work. |
| amdgpu ioctls and GPU waits | The card is idle behind the CPU (`docs/20fps.md:73-79`). Driver submit bookkeeping is not the current wall. |

## Measurement plan

The baseline profile is complete. Run candidate arms only after implementation
is approved. Do not combine switches.

### Fixed conditions

For every arm:

- Save: copy `GTA6SAVES/Manual Save` over the prefix's
  `AppData/Local/victory/data/7952987A` immediately before launch.
- Console: `--console series-x`, 1920x1080, `VICTORY_RENDER_SCALE=0.667`.
- Proton: the same GE-Proton11-14-vt build and patched wineserver.
- Executable, helper shaders, shader cache, keys, capture points, and logging
  level: identical.
- Keep the winning play defaults from `run.sh:88-102`; add exactly one
  candidate switch.
- Use the same world-period rule as `docs/20fps.md:12-20`: median of the 5 s
  windows after the first window with more than 1,000 draws.

### Arms

For each prototype, use an A/B/B/A order: baseline, candidate, candidate,
baseline. That gives two 480 s arms per side and controls for time-order drift.
After screening, re-run the winner and baseline for 660 s each, alternating
order. Never enable P1, P2, and P3a in the same arm. P3b is a later,
separate native-scan arm.

Use 480 s for screening arms, matching the existing sweep method. A candidate
that changes only startup or loading is not a world-period result.

### Required observations

For every arm, retain:

- `perf.py` frame median, p95, p99, spikes, and the `preparing` submitting-thread
  number.
- `asking about title memory per frame`: kernel askings, refused askings, hole
  questions, blocks made read-only, guard wait, and protection-change ms.
- `texture refreshes per frame`: total and the asking-write-watch/guards share.
- `copies per frame`: queries, moves, watches, guard time, and fallbacks.
- `landing per frame`: guard-opening/noting time and direct versus beside
  writes.
- `frame account per frame`: shader-write find/put-back time and host-list
  time.
- New native counters: attempts, successes, fallbacks, errno, merged runs,
  pages/bytes, and partial-reset chunks. P3a instead counts refused askings
  avoided and proves the native-scan count stays flat. Log aggregates once per
  second, not per call.
- `analyze.py` verdict: crashes, device loss, black frames, shader failures,
  nonblank captures, and indirect drops.
- A separate short correctness arm with `VICTORY_REFRESH_CHECK=1` for every
  finalist. It is not a performance arm; it makes stale host copies observable
  when the normal warning path is silent (`shadow.rs:4096-4142`).

For kernel attribution, count **all** `mprotect`, `process_vm_readv`,
`process_vm_writev`, and `PAGEMAP_SCAN` calls by `victory.exe` thread, and
record per-syscall latency histograms. Also time wineserver socket reads and
writes. The existing `stalls.bt` is insufficient because it only sums
syscalls over 5 ms. Capture a `perf record` sample in the settled world window
and keep `perf report --stdio` output with the arm. Match its UTC window to
`app.log` and `stalls.txt`.

### Acceptance gates

A prototype proceeds only if all are true:

- The answer-equivalence test passes, including every forced fallback.
- No new crash, device loss, black frame, missing-write warning, or shader
  failure appears.
- The targeted counter changes in the expected direction: P1 reduces Wine
  `VirtualProtect` only after vprot synchronization; P2 reduces
  `K32QueryWorkingSetEx`; P3a reduces refused Wine `GetWriteWatch` calls while
  native scans stay flat. P3b alone must show native scans rising.
- The candidate-specific app counter falls by a measured amount.
- The world-frame median improves beyond the baseline arm spread. A syscall
  count reduction with no frame improvement is not a merge case.
- p95 and the long-stall count do not regress. Report p99, but do not use it as
  the sole gate: this profile's p99 is dominated by a 199 s end-of-run stall.

If P3a cannot prove its classification/invalidation invariant, stop. P3b also
stops unless mapping reuse is excluded during reset and partial/unknown scans
preserve every dirty page. Do not weaken either gate to make the 44.0 ms line
disappear.

## Tweet method and `gameos.xvd`

The X post was reachable. Its body only names the prompt; the `t.co` link
resolves to a public gist, revision
`2f516433e8b876712d6258ed3d8204c48d81d9dc`:

- Post: <https://x.com/in19h/status/2125780258485919948>
- Prompt: <https://gist.github.com/19h/a9d2f994358773fdf98dba12a795b600>

The useful method is not “see an API name, replace it.” It is:

1. Declare dump coverage and exact binary/build identity.
2. Normalize lossy IDA pseudocode without inventing missing fields or paths.
3. Trace entry points to terminal effects and recover each interface contract:
   parameters, flags, return/error behavior, state owners, synchronization,
   lifetime, and consumers.
4. Separate observed, inferred, speculative, and unknown claims; cite function
   addresses and evidence.
5. Try to disprove the proposed equivalence, especially on failure and race
   paths.
6. Only then map the proven contract to a native syscall and a fallback.

`gameos.xvd` is not present in this worktree. If it becomes available, apply
that method first to the exact-build `xmem` / `xgameruntime` allocation and
query paths, then to `d3d12_xs` memory/view setup. For each hot export, keep
the DLL hash, architecture, exports, callers, pseudocode address, recovered
structure offsets, lock order, error mapping, and a list of other readers of
the state. A thunk proposal is accepted only when the pseudocode and
disassembly agree on the contract and the native test covers its failure
behavior. Until then, this document is bounded by the layer source, Linux
semantics, and the measured profile, not by the console oracle.

## References

### Layer and template

- `/tmp/opencode/sweep-prof/app.log:21239-21339` — populated-world counters
  used for the call-specific costs.
- `/tmp/opencode/sweep-prof/stat.txt:4-16` — 10 s CPU sample; no syscall
  attribution.
- `/tmp/opencode/sweep-prof/stalls.txt:77-109` — no long `mprotect`/`ioctl`
  after startup under the tracer's 5 ms threshold.
- `/tmp/opencode/sweep-prof/report.txt:14-19`, `553-557`, `631-633`,
  `843-845`, `1155-1157` — perf sample counts, column order, and named
  memory-path cumulative/self shares; kernel symbols unresolved.
- `/tmp/opencode/sweep-prof/perf-report.md:3-16` and `verdict.txt:4-13` —
  frame summary, end stall, and baseline correctness verdict.
- `docs/20fps.md:58-103` — current world budget and prior native-memory result.
- `crates/victory-gpu/src/watch.rs:649-753` — `GetWriteWatch` and reset path.
- `crates/victory-gpu/src/watch.rs:1301-1413` — journal, kernel, and guard
  fallback ordering.
- `crates/victory-gpu/src/watch.rs:3042-3382` — guard protection and alias
  handling.
- `crates/victory-gpu/src/watch.rs:3253-3302` — hole/region query path.
- `crates/victory-gpu/src/texture.rs:1707-1880` — readability and mapped-byte
  probes.
- `crates/victory-gpu/src/host.rs:8806-9077` — mapped copies and kernel writes.
- `crates/victory-gpu/src/safecopy.rs:379-401` — default fault-safe copy path.
- `/tmp/opencode/port-linuxmem/crates/victory-hle/unix/victory_mem.rs` — unix
  library ABI and syscall implementations.
- `/tmp/opencode/port-linuxmem/crates/victory-gpu/src/linuxmem.rs` — loading,
  capability probes, fallback policy, and Rust adapters.
- `tools/perf/stalls.bt:64-141` — current long-syscall tracer and its 5 ms
  threshold.

### Linux semantics

- `process_vm_readv(2)` / `process_vm_writev(2)`: direct transfer, partial
  page-granular results, `EFAULT`, permission checks, and no atomicity.
  <https://man7.org/linux/man-pages/man2/process_vm_readv.2.html>
- `mprotect(2)`: page alignment, kernel SIGSEGV behavior, and mapped-range
  errors. <https://man7.org/linux/man-pages/man2/mprotect.2.html>
- `mincore(2)`: residency only; not commit or readability.
  <https://man7.org/linux/man-pages/man2/mincore.2.html>

### Wine state ownership

Wine master was inspected only to locate state and lock ownership; it is not
the exact Proton build and is not used as the console contract.

- `dlls/ntdll/unix/virtual.c`: `NtProtectVirtualMemory` takes
  `virtual_mutex` and updates vprot; `NtGetWriteWatch` validates one
  write-watched range and uses `PAGEMAP_SCAN` when available;
  `NtReadVirtualMemory` has a direct self path, while the inspected
  `NtWriteVirtualMemory` sends a wineserver request.
  <https://github.com/wine-mirror/wine/blob/master/dlls/ntdll/unix/virtual.c>
- `dlls/kernelbase/memory.c`: Win32 wrappers for `GetWriteWatch`,
  `ReadProcessMemory`, `VirtualProtect`, and `VirtualQuery`.
  <https://github.com/wine-mirror/wine/blob/master/dlls/kernelbase/memory.c>
- `dlls/ntdll/unix/thread.c`: Linux thread times are read from
  `/proc/<pid>/task/<tid>/stat`.
  <https://github.com/wine-mirror/wine/blob/master/dlls/ntdll/unix/thread.c>
