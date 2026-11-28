# Legendary work log

Chronological record of the Linux/Proton port work: every run, what it tested,
what it measured, the fixes merged, the regressions found, and where it stands.
Companions: [`linux-research.md`](linux-research.md) (findings by topic),
[`linux-handoff.md`](linux-handoff.md) (state at hand-off), [`../LINUX.md`](../LINUX.md)
(how to build/run).

Machine: RX 6700 XT (RADV, Mesa 26.3.1), 6 cores/12 threads, 38 GB RAM.
Game: Xbox Series build, `/media/SSD/GTA6.Xbox.v1.0.2.1-CYBERLEEK`.
Layer builds: `cargo xwin`, `x86_64-pc-windows-msvc`, `diag` profile unless said.

## Runs, in order

| Run | Config | Result |
|---|---|---|
| `belobug-first` | release build, no keys, Experiment al | boots, splash art, fades to black; stuck at the start screen (no A press) |
| `sweep-baseline` | diag, scripted keys reaching only XInput | black at the start screen, 30 fps, 3 draws/frame |
| `keys1` | first GameInput key fix | **reaches the world**, black frames; root cause isolated: 5 pipeline workers overflowed their stacks (dxil-spirv) |
| `latch1` | single-latch DXIL + warm shader cache | world at frame 1386, PASS, 93 world captures, 0 overflows, 0 pipeline drops; 1.7 fps |
| `defaults2` | + absent-cache bookkeeping fixes, NO_WAIT default | PASS, 59 world captures, NO_WAIT wait **0.0 ms** |
| `perf1` | absent-region cache default on | stuck at the start screen — cache made memory unwatchable; made opt-in |
| `perf-nocache` | same binary, cache off | PASS, world at 1263 |
| `fast1` | ports 1/6/7 + archive switch set + THP | PASS, world at 8251, 3.4 fps last; one helper shader lost (`isnan`) → fixed |
| `p6w` | port 6 + `WIDE_THREADS=6` | PASS but 0.6 fps last / 0.2 worst — more threads hurt under Wine's locks |
| `shirt1` | forever recheck + `PAGE_WRITES=1` | PASS, world at 1429, 102 world captures; 242,989 looks, 179 changes caught; 2.3 fps |
| `stab1` | `PIPELINE_WAIT_MS=150` + warm book | never reached the world (1.6 fps, 2.46 s worst) — capped waits drop loading draws |
| `ge-run` | GE-Proton11-14 stock, defaults | never reached the world (0.9–2.1 fps) |
| `gevt1` | GE-Proton11-14 **with view-tree wineserver** | PASS, world at 984 (~2 min), 279 world captures, 3.3–3.8 fps; **write-watch asking 7–9 ms** (stock 50–132) |
| `gevts` | + Series S + copy switches | PASS, 4.4–4.6 fps; GPU 52–62 ms, copies 58–66 ms |
| `geport1` | + port 1 + THP + copy switches | PASS, 6.0 fps, but **water vanished** (port 1 private pages / copy compare) |
| `reg1` | GE-vt, defaults, Series X, new binary | PASS, water present, 3.3 fps / 300 ms, worst 2.6 / 383 ms (stable) |
| `reg2s` | Series S only | PASS, water present, 3.6 fps |
| `reg3s` | Series S + compose DXIL | crash: Rust panic (`asks()[13]` on a 13-array), 39 Breakpad reports |
| `launcher-run3` | launcher, HUD, new binary | same crash; gamescope closed after the title died |
| `stable1` | after view-ask default off | still crashed: the panic was independent of the switch |
| `stable2` | fresh pipeline library | still crashed (panic) |
| `stable3` | fresh cache + data dir | still crashed (panic) |
| `stable4` | **panic fixed** | PASS, world at 1090, 180 world captures, **12.9 fps / 77 ms last**, worst 2.6 / 380 ms; copies 41 ms / 21 MB, GPU 13 ms, write-watch 1.1 ms |
| `cmp1` | + `COPY_COMPARE=1` | PASS, 2.9 fps last; copies 64–69 ms, compare skipped 45 MB/frame |
| `ghost4` | + ghost fixes (copy→texture, full recheck) + `PAGE_WRITES=1` | world reached; 782,978 UAV/copy writes, 20 slots `shown from a texture the host wrote` (the bake atlas alias engages) |
| `vlaw-run` | the 19 fps archive build, Wine compiler | black world (helper `InterlockedOr` E5005; indirect dropped) |
| `vlaw-run2` | archive + Firefox 62 native `d3dcompiler_47.dll` | title crashed (Breakpad, exit 5) |
| `probe-run` | proton shader probe | `D3DCompile` of card compare fails (`Load4` E5005); its DXIL blob: ROOT OK, PSO OK |

## Fixes merged, in order

| Commit | What |
|---|---|
| `778b8a4` | Linux sweep harness (`run.sh`, `analyze.py`, `ab.py`) |
| `a1058fa` | empty pipeline library through the vkd3d ABI (null pointer), stalled-worker bound, 8 MiB worker stacks |
| `bdc1331` | `VICTORY_KEYS` reach GameInput's pad |
| `8a527c9` | guarded texture hashed every check after a change its guard missed |
| `70e4cba` | texture queue turns, slot let-go race, `mapped_bytes_between` |
| `b52f4e3` | helper shaders compiled to DXIL with DXC for Wine (card compare, up check, clamps) |
| `79b03c3` | one back edge per loop header for dxil-spirv (**the black-world fix**) |
| `fcae060`, `20c06c2` | analyzer judges the world's own captures; a fade is not a failed scene |
| `9269cba`, `c738b2c` | absent-region cache (opt-in), copy compare, wide threads follow the cache |
| `f934faa` | `VICTORY_LOG_FILE` |
| `4037e58` | renderer port 1 private pages under Wine + THP helper |
| `0929e8d` | port-1 private-page notification fix (unmapping, outside the remap lock) |
| `8cdd128` | launcher collapses port 1's huge pages every 15 s |
| `147d202`, `f250ac1`, `ecbf303`, `bebc78f` | launcher: no empty cmd window, live log tail, MangoHud switch, RAM/VRAM/IO/temps; `read_cfg=0` beats the host `no_display` |
| `c9d7ddc` | ambient stand-in tests NaN/Inf without isnan/isinf |
| `f085a93`, `064f1ad` | indexed texture stays on the 400-frame look; 20 s relook |
| `54ebfc1` | GPU-UAV writes mark address-backed texture spans due (≤32/frame) |
| `665ee20` | view-memory write-watch skip opt-in after it crashed the title |
| `a519aed` | compose DXIL reverted (title crashed) |
| `65…` → `3521f6e` | the frame-line counter array size (the panic that closed every window) |
| `65498fd`, `c2ab4b5` | copies into title textures told + placed view read from host-written memory (bake-atlas ghost); scheduled looks hash the whole texture |

## Regressions found and how they were caught

1. **Black world** (the first blocker): dxil-spirv recursion on multi-back-edge
   loops → stack overflows → pipelines never made. Caught by the 5 pipeline
   worker threads dying with `has overflowed its stack`; fixed by
   `single_latches`.
2. **Start screen never left**: scripted keys only reached XInput, the title
   reads GameInput. Caught by the log's `GameInputCreate` and the title's
   forever-black 30 fps start screen; fixed by `merge_scripted`.
3. **Absent-region cache default on**: run stayed at the start screen; caught
   because the same binary with `VICTORY_ABSENT_CACHE=0` reached the world.
   The cache stays opt-in until the memory path that bypasses `watch.rs` is
   found.
4. **`PIPELINE_WAIT_MS=150`**: no world in 600 s; caught by the run stopping
   at the loading screens with dropped draws. Reverted (warming instead).
5. **View-memory write-watch skip**: two runs crashed the title with Breakpad
   (39 reports, exit 99). First suspected; the actual cause was our own panic:
   the skip's new counter was printed as `a[13]` while `asks()` returned
   `[u64; 13]` → `index out of bounds` inside the frame-line thread, which the
   guest saw as a `rust_panic`. Fixed `asks()` to 14 (`3521f6e`); the skip
   itself is opt-in (`VICTORY_VIEW_ASK=1`).
6. **Compose DXIL bundling (Series S two-plane composer made to compile)**:
   title crashed. Reverted (`a519aed`). The composer failing (1,922 warnings)
   is known but the made version is worse under vkd3d for now.
7. **Water vanished** with `--console series-s` + port 1 + `COPY_COMPARE=1` +
   `RELAY=2` + `JOIN_COPIES=1`: the independent regression review blames
   `copy_compared` reading title memory without the remap guard and
   `copy_mapped` zero-filling unreadable pages, so a remap racing the compare
   sends zeros up (45 MB/frame decided that way). Port 1 is the other unique
   switch (placed windows never fetch CPU-side water data). Both stay off by
   default; Series S alone and defaults keep the water.

## What the frame costs (measured)

- Settled scene (stable4): 77 ms/frame = preparing 56.7 (copies **41.4 ms /
  21 MB**, textures 0.7) + GPU 12.9 + copy-back 5.4. Copy rate ~500 MB/s
  through `ReadProcessMemory` under Wine.
- Streaming scene: 300–380 ms/frame, copies 55–65 ms / 100+ MB, copy-back
  60 ms / 20 MB, GPU 50–100 ms.
- Ports run (all five, archive switches): 930 ms median, GPU 93 ms (19 %),
  294 ms of remap-lock wait (before NO_WAIT), 132 ms write watch, lists 244 ms.
- Wineserver view tree (GE-Proton11-14-vt): write-watch asking 7–9 ms vs
  50–132 ms; world in ~2 min vs ~3.
- Server protocols measured: plain Wine 11.0 = 930, staging/Experimental =
  931, GE-Proton11-14 = 938 (its patch series bumps `server_protocol.h`).
  Only a server built from its own Wine tree passes the handshake.

## Agent investigations (what each settled)

- **port-triage**: port 1 engages but makes the descriptor heap unwatchable
  (port 2's marks never taken, 1.25 M); port 5 delivers with port 1 (14,542
  CBs in place); port 6 delivers (420 refused asks gone) but is slower with
  more threads; port 7's journal half delivers, its heap half needs
  `VICTORY_ASYNC_SUBMIT=1`. The `release_private` notification bug was fixed.
- **hotpath-audit**: frame is CPU/lock-bound; the 294 ms remap wait is the
  stock wineserver walking views, not port 1; THP + patched server are the
  levers. Ranked the top costs per frame.
- **perf-review**: caught the absent-cache bookkeeping cost, the
  `mapped_bytes_between` full scan for away textures, the wide-thread default
  depending on a disabled cache, and the copy-compare scratch memset. All
  fixed or made dependent on the cache.
- **missed-opt**: surveyed six forks; the only large remaining win is the
  wineserver view tree (built here), then aDefaultUser's view-memory skip
  (ported, opt-in after the crash) and the ptrace write route (not ported).
- **tex-recheck**: proved the indexed-texture schedule dropped slots forever
  after five looks, and fixed that plus full-texture scheduled looks. Its
  bake-atlas theory (a copy into a title texture whose placed view read
  title memory) did not survive the 27 Nov runs; see `ghost-27nov`.
- **ghost-27nov**: relaxed `alias_of`'s `fills` reject and added why/epoch
  diagnostics; the ghost stayed. Run `alias-fix1` had 59 aliases, all
  R32_UINT→R9G9B9E5 light probes, 397 placed views with no host-written
  source, 3 not-placed views, and 0 `shown from a texture the host wrote`.
  The atlas is absent from the 2048×2048 misses, so it is not the placed
  view being rejected; render-target writes from the bake are not marked
  host-written at all. A shape bridge (HOST_WRITTEN_DESC) and P1 upload
  guard did not boot and were reverted. Full write-up:
  `docs/ghost-fix.md`; patch:
  `docs/ghost-notes/2026-11-27-alias-attempt.patch`.
- **regression-review**: no bug in the shipped defaults; ranked the water
  suspects (copy-compare zero-fill, port 1) and left two latent bugs:
  `copy_compared` lacks the remap guard, and the water readback kind fails
  silently (`texture.rs:683` `!addressable()`).
- **wineserver-build**: rebuilt GE-Proton11-14's wine with the view-tree
  patch, protocol check passed, original untouched.
- **console-hud**: launcher with no cmd.exe, live log tail, HUD switch, and
  the `read_cfg=0` fix for a host config carrying `no_display`.

## Where it stands

- Build note, 27 Nov, corrected the same day: build WITH the helpers
  bundled (`VICTORY_BUILD_DXC=$HOME/.cache/dxc/bin/dxc`, or
  `tools/build.sh fast`, which finds or fetches dxc). Wine's
  `d3dcompiler_47` (vkd3d-shader) cannot compile them at run time (the
  byte-address intrinsics fail with E5005), and without the bundled ones
  the argument clamp never exists: the world's indirect records drop — a
  black world behind a working HUD (three runs saw it), or, with the
  fallback, the GPU takes the records the clamp would have cut (a device
  removed, 0x887A0005). The earlier note suspected the DXIL-bundled diag
  build of unlit models; a bundled build is what rendered the world all
  along (sweep-ghost4, alias-fix1), and the exe itself now carries the
  bytes `victory-dxil-bundled` when the helpers are in it, which the run
  scripts check. Without dxc a native Microsoft `d3dcompiler_47` beside
  the exe answers the run-time compile instead (`tools/d3dcompiler.sh`).
  This tree does not carry the bundling commit: it compiles the helpers at
  run time with a modern native `d3dcompiler_47` beside the exe (the 2013
  winetricks cab rejects `cs_5_1`; see `docs/ghost-fix.md`).

- Series X, defaults, GE-Proton11-14-vt: PASS, world ~2 min, settled
  **12.9 fps**, stable frame times, water, no crashes.
- Ghosting: fixed (27 Nov). Cause: the 2013 Windows 8 `d3dcompiler_47.dll`
  (winetricks' cab) rejects `cs_5_1` with `X3506`; the helper shaders fell
  back, the card compare and clamps were not built, and the atlas host
  copies went stale. A modern `d3dcompiler_47` (Edge 148 tested) resolves
  it; `tools/d3dcompiler.sh` now refuses the 2013 build by SHA-256. The
  alias/`fills` investigation below is history (`docs/ghost-fix.md`).
- 20 fps: not reached. Copy path (41 ms at 21 MB) is the next cut: guard
  `copy_compared` with the remap reader, then a safe `COPY_COMPARE=1` A/B;
  then card-side compare and relay. Series X GPU floor is ~13 ms settled,
  ~50 ms streaming.
- 20 fps pass, 28 Nov: fixed save (`GTA6SAVES/Manual Save`, reset before each
  arm), Series X, 1080p, scale 0.667, 480 s arms, world-period medians:
  baseline 157.7 -> `RELAY_COPY=1` 154.4 (copy-back 73.7 -> 30.8 ms, fetch
  wait 61.9 -> 16.1) -> `+COPY_COMPARE=1` 145.9 (48 of 68 MB written found
  unchanged and not sent) -> `+COPY_BESIDE=1` 143.1 -> `+STORE_BESIDE=1`
  141.4 (guard wait 27.3 -> 11.2 ms) -> `+LATE_WAIT_LISTS=1024` 140.4 ->
  `+CARD_BUFFERS_MIN_KB=64, _MB=1536` 139.2 (p99 192) -> `+TEXTURE_NO_WAIT=1`
  138.0. 660 s validation of that set: PASS, **135.7 ms**, p95 176.7,
  p99 207.8, 0 faults, 534/541 world captures non-blank, 0 black frames. Losing arms: `JOIN_PARTS=8` 156.6,
  `LIGHT_REFRESH=0` 195.9, `CARD_BUFFERS_MIN_MB=16` 215.4,
  `WIDE_THREADS=4` 154.2, `THIN=1` 148.2, `VIEW_ASK=1` 144.3;
  `INDIRECT_BATCH=2` 138.8 failed on one black frame and is excluded;
  `CARD_STEPS=0/4` 169.3/170.3, `UP_QUEUE=1` and `ASK_EARLY=0` neutral.
  20 fps not reached: what is left is the sync work itself (refresh flush
  46 ms over 6,305 pages, write-watch 44 ms with 428 refused askings,
  copies 42 ms over ~93 MB), all on the submitting thread.
- 29 Nov: `VICTORY_WC_NO_BACK` ported from scarlet-fps and merged (`24664d0`):
  shader-written write-combined copies stay on the card instead of being
  fetched back. A/B/A on the fixed save: controls 171.4 / 170.6 ms (0.8 ms
  apart), ported 166.0, p95 170.0 vs 172.9 / 203.8, PASS; the arm left
  1,982 MB declared WC on the card and the confirmation run shows 7,942.9 MB
  over its windows. Smaller than Scarlet's win because our card-fetch path
  was already ~0 ms; the rest of our copy-back is PC-side copies.
- Reference comparison, same save and console: Scarlet-FPS (Y kekai 3070 base +
  VIctory-NVIDIA lighting) runs 13.3 fps / 75.0 ms on this card against our
  135.7 ms; its copies are 18.5 MB vs our 92 MB and copy-back 0.1 MB vs 15 MB.
  Profiling both shows the same CPU use (6.9 vs 6.4 cores): the difference is
  overlap and copied volume, not less work. Ports from it in flight:
  `SOFT_VIEWS` (texture-refresh asking 19.6 ms vs its 2.8), `GPU_BACK_MAX_KB`,
  `HEAP_SYNC_AT_SUBMIT`, `TEXBACK_ASYNC`.
