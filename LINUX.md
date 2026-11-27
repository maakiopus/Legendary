# VIctory on Linux (Proton)

The Windows build runs under Wine/Proton (Proton Experimental, AMD on Mesa/RADV
tested). This file is the Linux side: what had to change for the layer to run
there, how to start it, and what is measured.

**No game files are in this repository.** You need your own dump of the Xbox
Series build.

## What running under Wine needed

These are in the tree; none changes a Windows run.

- **Every loop one back edge** (`emit/bitcode.rs`, `Module::single_latches`).
  vkd3d-proton compiles our DXIL with dxil-spirv, which asserts on a loop header
  with two back edges and otherwise recurses without end in its dominator
  search. Five pipeline workers died in a stack overflow, their pipelines were
  never made and the world stayed black. DXC's loops always have one latch, and
  now ours do as well.
- **Our helper shaders are compiled with DXC into DXIL at build time**
  (`src/dxil.rs`, `build.rs`, `shaders/`). Wine's `d3dcompiler_47` (vkd3d-shader)
  defines no `Load4`, `Store` or `Interlocked*` on byte-address buffers, so the
  card compare, the up check and the argument clamps did not compile there: the
  card compare fell back to the PC's memory and indirect executions were
  dropped. Windows still compiles at run time as before, and a build without a
  DXC keeps that path too.
- **An empty host pipeline library through the vkd3d ABI** (`pipeline/library.rs`):
  vkd3d-proton refuses a null-length blob with a non-null pointer, which the
  binding passes; the library is asked for with a null pointer now, so pipelines
  are kept between runs.
- **A bound on stalled pipeline workers** (`pipeline/make.rs`): under Wine a
  worker sometimes never returns from the driver's compile; draws stop waiting
  for it after 15 s instead of spending every frame's budget, and the workers
  have 8 MiB stacks.
- **`VICTORY_KEYS` reach GameInput's pad** (`victory-hle/gameinput.rs`): the
  title reads its pad through GameInput, and the scripted keys only reached
  XInput before. The start screen needs an A press to load.
- **Texture fixes ported from victory-royale / aDefaultUser's Wine patch**: a
  guarded texture is hashed at every check for a while after a change its guard
  missed, every block of a texture's memory is asked (not only its ends), the
  texture queue serves first looks in turns, and a slot's host copy is let go
  only while the slot is still made from the texture the pass read.

## Build

Needs clang/lld (as on Windows) and a DXC to bundle the helper shaders.

```sh
cargo install cargo-xwin            # once
# a DXC for this host, from https://github.com/microsoft/DirectXShaderCompiler/releases
export VICTORY_BUILD_DXC=$HOME/.cache/dxc/bin/dxc
cargo xwin build --profile diag -p victory --features diagnostics \
    --target x86_64-pc-windows-msvc
```

The `diag` profile with `diagnostics` is what a test run wants: `VICTORY_KEYS`
and `VICTORY_CAPTURE` exist only there. `--release` runs the game too, but
cannot press a key or save a frame for you.

## Run

Steam with **Proton - Experimental**. No system Wine is used.

### Direct (recommended)

`tools/linux-run.sh` starts the layer itself: no cmd.exe window, the log is
tailed into this terminal, gamescope runs 1920x1080@75 fullscreen, and
MangoHud shows the clock, the frame rate, and the processor's and the card's
load, temperature and power. The 15 s huge-page loop starts with it.

```sh
tools/linux-run.sh /media/SSD/GTA6.Xbox.v1.0.2.1-CYBERLEEK/game_scarlett_master.exe
tools/linux-run.sh --dry-run /media/.../game_scarlett_master.exe   # plan only
```

| | |
|---|---|
| `VICTORY_HUD=0` | no overlay |
| `VICTORY_HUD=1` | compact, top-left: clock, fps/frametime, CPU/GPU load, temperatures, power (default) |
| `VICTORY_HUD=2` | full: the above plus GPU name, clocks, VRAM/RAM and the frame-time graph |
| `VICTORY_GAMESCOPE=0` | no gamescope: `MANGOHUD=1` and the MangoHud Vulkan layer draw into the game window (`mangohud` must be installed) |
| `VICTORY_THP=0` | no huge-page helper |

The launcher sets `MANGOHUD_CONFIG`, so a value in the host's environment is
replaced, not appended to, and the host's `MangoHud.conf` is not read; HUD
off clears `MANGOHUD` as well. Logs go to
`~/victory-logs/run-<stamp>.txt`, printed as `logs:` and followed in this
terminal; `tail -f` the same path from another one.

### Through cmd.exe (if you insist)

`tools/linux-run.sh --bat <game>` writes
`~/victory-logs/run-<stamp>.bat` and prints the one command that runs it
(`--print-bat` prints the file as well). The batch starts the layer directly
with no redirect, so the console carries the layer's stderr; it titles the
window `VIctory - <game> - log: Z:\...`, prints that path and the Linux
`tail -f` line before the game starts, and sets `VICTORY_LOG_FILE` so the
file still has every line. The `VICTORY_*` variables exported when the
batch was written are baked into it.

```sh
tools/linux-run.sh --bat /media/.../game_scarlett_master.exe
# then the printed line, HUD and Proton included:
# MANGOHUD_CONFIG=... gamescope ... --mangoapp -- umu-run 'C:\windows\system32\cmd.exe' /c 'Z:\home\...\run-<stamp>.bat'
```

The title's start screen waits for A (`Enter` on the stand-in pad), so a run
without keys sits there forever on a black screen. `--console series-x`
renders one plane; the Series S profile is slower and softer here.

Proton's cmd.exe console is Wine's own window, and the layer's stderr is not
attached to it: if it stays empty, the log file is the whole story, and the
batch's title and banner point at it. The bat path has no 15 s THP loop; use
the direct path or run the helper by hand. The sweep runner is unchanged.

## Sweep and logs

`tools/linux-sweep/run.sh` runs one fixed scenario (same Proton, keys, captures
and duration) and `analyze.py` prints a stable verdict; `ab.py` puts two sweeps
side by side. See [`tools/linux-sweep/README.md`](tools/linux-sweep/README.md).

Useful switches under Proton:

| | |
|---|---|
| `VICTORY_LOG_FILE=<path>` | also write the log to a file (Proton gives the process its own console, so a shell redirect catches nothing) |
| `VICTORY_LOG=info` | the per-five-second frame line and the counters (debug is much heavier) |
| `VICTORY_COPY_COMPARE=1` | put in only the 4 KiB pieces a refresh finds changed; the frame line says how many MB were written and how many were unchanged (off by default) |
| `VICTORY_ABSENT_CACHE=1` | remember absent title-memory regions instead of asking Wine on every look (off by default: a run with it on stayed at the start screen where the same binary without it reached the world, so a commit path is still to be found) |
| `VICTORY_WIDE_THREADS=<n>` | threads of the wide refresh passes (default: a third of the logical processors, 2 to 8) |
| `VICTORY_TEXTURE_NO_WAIT=1` | do not wait for the title-memory remap lock during a texture refresh (the Proton fork's stall fix) |
| `VICTORY_INDIRECT_WIDE=0` | keep indirect group counts at 65,535 rather than AMD's 2^20 |

## Where it stands (RX 6700 XT, RADV 26.3.1, Proton Experimental 11.0, driver 26.3)

- Boot, splash and loading art, the prologue world: renders. A 10-minute sweep
  reaches the world (first frame of 500+ draws) about three minutes in and the
  captures measure mean luminance 80-105, colour cast 1.16-1.39, no shader
  translation drops, no faults, no device loss.
- Frame rate in the first world frames is 0.5-4 fps while the title's pipelines
  are made and its streamed textures are refreshed; the Proton fork reports
  16-19 fps with renderer ports 1, 2, 5, 6 and 7 on and huge pages for port 1's
  heap.
- The port-1 path itself works under Wine on this tree: with
  `VICTORY_PORT_CONSTANTS=1` the log names five private ranges of the 32-bit
  heap and `linux/thp/victory-thp-collapse` collapsed them ("88 collapsed,
  0 not"). With all five renderer ports on the same 10-minute run did not reach
  the world (0.9-1.9 fps, still streaming textures at the end), where the
  defaults reach it in about three minutes: the ports stay off by default here
  until each is measured one at a time on this GPU.
- Ray tracing is off: the structure builder is opt-in (`VICTORY_BVH_BUILDER=1`),
  and without its pipelines the title's ray passes are dropped by design
  (about 330 draws a frame).

## Renderer port 1 under Wine

vkd3d-proton opens a heap from an address by giving the pages to the AMD driver
as a user pointer, and the driver takes anonymous memory only. On a view of
Wine's sections (shared, file-backed memory) the card read nothing and the
picture stayed black. Under Wine the 32-bit heap's fresh pages are therefore
private memory, and the heap is opened on the title's own pages.

`VICTORY_PORT_PRIVATE=0` keeps the views as before. Port 1 stays opt-in: neither
path runs with `VICTORY_PORT_CONSTANTS` unset.

## Huge pages for renderer port 1 (recommended with port 1)

Under vkd3d-proton the port-1 heap is a userptr, and amdgpu checks every page of
it on every submission. With 4 KiB pages that kept the `vkd3d_queue` thread at
100 % of a core; as 2 MiB pages the same memory is ~512 times fewer entries.
`linux/thp/victory-thp-collapse` collapses the heap into 2 MiB pages from
outside the process (`process_madvise(MADV_COLLAPSE)`, Linux 6.1+). On the
reference tree (RX 7900 XT, Ryzen 7 5800X3D, Proton Experimental 11):
**~9 -> ~18-19 fps in the same scene**.

It needs one capability, not root:

```sh
cc -O2 -o linux/thp/victory-thp-collapse linux/thp/victory-thp-collapse.c
sudo setcap cap_sys_nice+ep linux/thp/victory-thp-collapse
```

`tools/linux-run.sh` starts it every 15 s (the bat path does not); run it by
hand with the command in [`linux/thp/README.md`](linux/thp/README.md).
