# Legendary: Linux port research

A record of what was found, measured and rejected while making VIctory run on
Linux under Wine/Proton + vkd3d-proton, and of where the frame time goes. It
complements [`LINUX.md`](../LINUX.md) (how to build/run) and
[`linux-handoff.md`](linux-handoff.md) (the state at the last handoff).

## The machine and the route

| | |
|---|---|
| GPU / driver | AMD RX 6700 XT (RDNA 2), RADV, Mesa 26.3.1 |
| CPU | 6 cores / 12 threads |
| Proton | Proton Experimental (`experimental-11.0-20261112`), vkd3d-proton |
| Build | Windows target (`cargo xwin`, `x86_64-pc-windows-msvc`), diag profile |
| Title | Xbox Series build, run with `--console series-x`, 1920x1080 |

The layer is not a Linux program: it is the Windows build running in Wine,
talking to vkd3d-proton's D3D12. Everything below is about making that pairing
behave.

## Root causes found, in the order they blocked the run

1. **Black world: dxil-spirv recursion.** vkd3d-proton compiles our DXIL with
   dxil-spirv, which asserts on a loop header with two back edges and otherwise
   recurses without end in its dominator search. Five pipeline workers died with
   `has overflowed its stack` even on 8 MiB stacks; 800–900 pipelines were never
   made and hundreds of draws were dropped each frame. Fix: `Module::single_latches`
   in `victory-shader/src/emit/bitcode.rs`, ported from the Proton fork
   (`kroeeee-eeee/VIctory-Linux-Proton-Wine-7900XT-XTX@f5c30ef`). After it:
   0 overflows, 0 pipeline drops, the world renders.
2. **Start screen never left: input.** The title reads its pad through
   GameInput, and `VICTORY_KEYS` only reached XInput. Ported `scripted_keys` /
   `merge_scripted` from victory-royale; the start screen then loads the world.
3. **Helper shaders do not compile under Wine.** vkd3d-shader (Wine's
   `d3dcompiler_47`) defines no `Load4`/`Store`/`Interlocked*` on byte-address
   buffers, so the card compare, the up check and the argument clamps failed
   (`E5005`); title copies stayed in the PC's memory and indirect executions
   were dropped. Fix: compile our helper HLSL with DXC into DXIL at build time
   (`victory-gpu/build.rs`, `src/dxil.rs`, `shaders/`), used only under Wine.
   A Proton probe verified `CreateRootSignature` + `CreateComputePipelineState`
   accept the DXIL container and its embedded root signature.
   Later, `any(isnan(...))`/`isinf` in the ambient stand-in was another gap
   (fixed by testing the exponent bits), and the black shirt/body boxes were
   stale indexed textures, not a compile failure.
4. **Empty pipeline library refused.** vkd3d-proton rejects `CreatePipelineLibrary`
   with a non-null pointer and length 0 (the binding's dangling empty-slice
   pointer); the library was never made, so no pipeline was kept between runs.
   Fix: call the vtable method with `std::ptr::null()` for an empty blob.
5. **Pipeline workers never return.** Under Wine a worker can stay inside the
   driver's compile; every frame then spent its wait budget on it. Fix: a 15 s
   stall bound (`Pending::claimed`/`stalled`) and 8 MiB worker stacks.
6. **Remap-lock waits.** The title's streaming thread holds the title-memory
   remap window while mapping pages; refreshes waiting for it cost 294 ms/frame
   (up to 11.5 s summed over threads). Wine's `NO_WAIT` path (read through the
   kernel instead) is now the default under Wine; the wait measured **0.0 ms**
   afterwards. Windows keeps its default.
7. **Stale/black textures.** Two classes:
   - Indexed textures: the fingerprint recheck worked, but after five scheduled
     looks the slot left the queue for good, so a texture streamed/painted at
     the same address later kept its first host copy. Fix:
     `VICTORY_HEAP_RECHECK_FOREVER` (default on; a parked slot is looked at
     again every 400 frames) and `VICTORY_RELOOK_MS=20000` for slots without
     memory. In one run: 242,989 looks, 179 changes caught and re-uploaded.
   - GPU UAV writes through descriptor tables are invisible to the CPU write
     watch, which is the remaining ghosting. Fix in `54ebfc1`: address-backed
     UAV texture spans are marked, at most 32 slots made due per frame
     (`VICTORY_UAV_RECHECK`, default on).

## What a world frame costs (all ports on, 930 ms median)

| Cost | Measured | Site |
|---|---|---|
| Texture refresh, of which 294 ms waiting for the remap lock | 397 ms | `texture.rs` |
| Lists' steps on the submitting thread (6,475 refreshes in 46 flushes) | 244 ms | `host.rs` submit |
| Write watch: 436 `GetWriteWatch` + 6,200 hole `VirtualQuery` a frame | 132 ms | `watch.rs` |
| CPU snapshot + AVX2 compare (454 MB compared, 424 MB in ≥1 MiB copies) | 60 ms | `shadow.rs` |
| Title→host copies (~700 MB/s) | 105 ms / 73 MB | `host.rs::copy_mapped` |
| Texture detile + upload, 23.7 k slots waiting | 112 ms / 28.5 MB | `texture.rs`, `draw.rs` |
| Waiting for flights on the submitting thread | 93 ms | `host.rs::relay_wait` |
| Landing/copy-back | 69 ms / 10.6 MB | `land.rs` |
| Guard lock | 43.5 ms over 55 waits | `watch.rs::guards()` |
| GPU | 93 ms (19 % busy, 740 ms nothing queued) | not the wall |

The frame is CPU- and lock-bound. Under Wine every `VirtualQuery`,
`VirtualProtect`, `GetWriteWatch` and map/unmap request goes to the
single-threaded **wineserver**, which walks the process's whole view list each
time: tens of thousands of views, ~2,400 remap notifications a frame. That is
why more threads (6 cores/12 threads, `WIDE_THREADS=6`) and port 6 measured
*slower*, and why the frame times are streaky: median hundreds of ms, tails of
seconds while the title streams.

## Renderer ports: status under Wine

| Port | State | Evidence |
|---|---|---|
| 1 `PORT_CONSTANTS` | engages (5 private ranges, 185 MB heap, THP "88 collapsed") | makes the descriptor heap unwatchable, so port 2's heap sync is skipped while it is on |
| 2 `PORT_DESCRIPTORS` | patches 6/6 stores, 1.25 M marks recorded, **0 taken with port 1 on** | needs a port-2-alone run; heap is `MEM_PRIVATE` so `watch::track` refuses it |
| 5 `PORT_CBV` | delivers with port 1: 14,542 CBs read in place, 5.6 MB/frame not copied | the only additive port win seen |
| 6 `PORT_GUARD` | delivers: 218 faults shared, 420 refused kernel asks removed | with `WIDE_THREADS=6`: world PASS but 0.6 fps last / 0.2 worst — slower |
| 7 `PORT_JOURNAL` | journal half delivers (526 shared notings, 0.52 ms waits) | heap half inert without `VICTORY_ASYNC_SUBMIT=1` |

None is default-on. Measured configs on this GPU: defaults 2–4 fps world
(PASS); all five ports + archive switch set 0.9–1.9 fps and no world in 10 min;
fast1 (ports 1/6/7 + switches + THP) 3.4 fps last; port 6 + 6 threads 0.6 fps.

## Rejected or deferred paths

- **The 19 fps archive** (`victory-linux-amd-work-20261127.zip`): its helper
  shaders need a native `d3dcompiler_47.dll` (black world with Wine's
  compiler), and with Firefox 62's native DLL the title crashed (Breakpad, exit
  5). Its fps come from its `fps20-wip` code plus a 7900 XTX and a warm seed of
  85,933 pipelines — not from the switch set (which measured slower on our
  tree). Its useful ideas were taken: texture rechecks, private pages, THP.
- **GE-Proton11-14 without the patched wineserver**: never reached the world
  here (0.9–2.1 fps). Proton Experimental stays the working Proton.
- **`VICTORY_ABSENT_CACHE=1`** (remember absent regions, 6,200 queries/frame
  saved): one run stayed at the start screen, so it is opt-in until the commit
  path that bypasses `watch.rs` is found.
- **`VICTORY_PIPELINE_WAIT_MS=150`** to smooth spikes: drops the loading draws,
  no world in 600 s. Pipeline warming (`victory warm --used`) cut first binds
  3,214 → 25–42; do that instead.
- **Port 1 with the archive switch set** as a "fast config": slower than
  defaults on this GPU.
- **`VICTORY_COPY_COMPARE`** targets the move half of copies; the write watch
  is ~96 % of that cost, so it cannot fix the stalls.

## Next lever: wineserver view tree

`kroeeee-eeee/...@a14cfcbf` patches `server/mapping.c` to keep mapped views in
a red-black tree keyed by (process, base). Measured there: **8.3 → 17.7 fps**,
write watch 17–26 → 4.6 ms, launch-to-world 112–120 → 43–60 s. It must be built
from the Wine source of the Proton in use (server protocol match) and swapped
into a copy of the Proton install; a stock Wine 11.0 server is being built
against GE-Proton11-14 first (both report Wine 11.0), then the same for
Experimental if the protocol matches.

Built here (2026-11-27): the server protocol must match the Proton's Wine
exactly, and it differs far more than the version string suggests — plain Wine
11.0 answers protocol **930**, Wine 11.0 + staging 11.0 **931**, Proton
Experimental **931**, GE-Proton11-14 **938** (GE's patch series bumps
`include/wine/server_protocol.h`, and `tools/make_requests` adds one). The
patched server was therefore built from GE's actual tree: ValveSoftware/wine
`a47c73f` + the wine block of GE's `protonprep-valve-staging.sh` (2078 patches,
0 failed), the view-tree patch applied with no fuzz, `make server/wineserver`
(5,388,848 B, sha256 `9d4cfcdc…52cd5a`), installed in
`~/.local/share/Steam/steamapps/common/GE-Proton11-14-vt`; the original
`GE-Proton11-14-x86_64` is untouched and the protocol check
(`umu-run cmd /c echo ok`) passes. Experimental keeps its stock server: its
931 handshake rejects the 938 binary, so it needs a second build from its own
Valve wine commit. A/B line: `PROTONPATH=…/GE-Proton11-14-vt` against
`…/GE-Proton11-14-x86_64`.

Secondary, not yet ported: aDefaultUser `853d7af1` (never ask the kernel about
view memory the guards already hold — 4,950 refused asks/frame) and `6dea2976`
(write route when `ptrace_scope=2` refuses Windows write calls).

## Tools

- `tools/linux-sweep/run.sh` — fixed 600 s Proton scenario: scripted keys,
  captures, `analyze.py` verdict; `ab.py` compares two sweeps.
- `tools/linux-run.sh` — direct launch (no cmd.exe), log file + live tail,
  MangoHud (clock, CPU/GPU), THP collapse loop.
- `linux/thp/victory-thp-collapse.c` — `process_madvise(MADV_COLLAPSE)` on
  port 1's userptr heap (`setcap cap_sys_nice+ep`).
- `crates/victory-gpu/shaders/` — the helper shaders compiled by DXC at build
  time for Wine.

## References

- `kroeeee-eeee/VIctory-Linux-Proton-Wine-7900XT-XTX` — single latch, port-1
  private pages, absent cache, copy compare, wide threads, log file, THP idea,
  wineserver patch.
- `aDefaultUser/VIctory-linux` — view-memory write-watch skip, ptrace write
  route, Wine compatibility patch (blend normalisation, empty pipeline cache,
  bounded waits).
- `Belobug/VIctory-Belobug-AMD` — base tree and its Windows performance work.
- `avatarmanz/VIctory-7900XTX`, `zm0kiemedia/Neptune_VI`, `DexNom/VIctory` —
  surveyed; nothing else met the bounded-memory bar for porting.
