# Linux/Proton handoff (2026-11-27)

Where the fork stands after the Linux port work: the game **runs** under
Proton Experimental + vkd3d and renders the world. The renderer ports are
ported but **not integrated**: the measurement matrix is half done and the
biggest lever (huge pages) has not been measured on a world run.

## Verified state

- Defaults (all ports off): 10-minute sweep, world marker at frame 1527
  (~3 min), 142 world captures, mean luminance 99-104, colour cast 1.15,
  0 faults / shader drops / indirect drops / black frames. **VERDICT=PASS**
  (`sweep-final1`; app.log was truncated by a /tmp quota recovery, captures
  remain in `/tmp/opencode/sweep-final1/capture`).
- Root cause of the earlier black world: dxil-spirv recursed on loop headers
  with two back edges. `Module::single_latches()` (79b03c3) fixed it: 0 stack
  overflows, 0 pipeline drops.
- GE-Proton11-14 (downloaded, extracted at
  `~/.local/share/Steam/steamapps/common/GE-Proton11-14-x86_64`): 600 s run
  with defaults did **not** reach the world (0.9-2.1 fps, still streaming
  textures at the end; log `/tmp/opencode/ge-run/app.log`). Experimental is
  the working Proton; GE needs its patched wineserver to be worth testing.
- Port 1 + private pages + THP: private pages engaged (5 ranges), THP helper
  collapsed "88 collapsed, 0 not" — but the run was only 116 s, so it says
  nothing about world fps. `VICTORY_TEXTURE_NO_WAIT` is now on by default
  under Wine (cc808fe), removing the remap-lock wait that dominated it.

## What the reviews found (agents, not yet all acted on)

- **Frame is CPU/lock-bound, not GPU**: ports run ~930 ms/frame, GPU 93 ms,
  card 19 % busy. Top items: texture refresh waiting for the remap lock
  (294 ms median), list steps (244 ms), write watch (132 ms + 6,200
  VirtualQuery hole questions), CPU snapshots/compare (60 ms, 454 MB), copies
  (105 ms, 73 MB), flights wait (93 ms).
- **Do not chase a port-1 reader leak**: the private pages are created once
  at startup and never remapped; the same waits happen with ports off. The
  wait is the stock wineserver's view-tree walk under the remap write lock.
- Reviewer fixes already committed: absent-cache bookkeeping no-op + epoch
  swap + cap (c738b2c), `mapped_bytes_between` eight-probe short circuit
  (6d008bc), wide-thread default follows the cache, copy-compare scratch no
  longer cleared (b90c4c7), NO_WAIT under Wine (cc808fe).
- Latent bug found, not fixed: `release_private` in
  `crates/victory-hle/src/modules/xmem.rs` calls `title_memory_remapped` for
  freed placeholders (should be `title_memory_unmapping` + `port_private_gone`),
  which would scan up to 100 MB inside the remap write lock if a port-1
  stretch is ever released.

## Next measurements (in order)

1. Build diag with the merged fixes, run **defaults 600 s, captures off** to
   confirm the world and record fps (`VICTORY_TEXTURE_NO_WAIT` now defaults
   on under Wine).
2. Run **port 1 only + THP collapsed** 600 s, same cache: run
   `linux/thp/victory-thp-collapse` every ~15 s on the ranges the log names.
   Expected: fps toward the defaults, `vkd3d_queue` no longer ~100 % of a core.
3. If port 1 holds, add ports 6 and 7, then 2 and 5 one at a time.
4. Try the reference script switches together: `VICTORY_COPY_COMPARE=1`,
   `VICTORY_MAP_THREADS=1`, `VICTORY_CARD_BUFFERS_MIN_MB=1`
   (+`VICTORY_CARD_BUFFERS_MB=512`), `VICTORY_JOIN_COPIES=1`,
   `VICTORY_CARD_REHASH_RUNS=1`, `VICTORY_RELAY=2`, `VICTORY_CARD_STEPS=2`.
5. `release_private` is fixed (commit 0929e8d); keep an eye on port 1
   teardown under load rather than treating it as a blocker.
6. The patched-wineserver view tree is in this tree
   (`linux/wineserver/wineserver-view-tree.patch`) and a 938-protocol server
   is built in `~/.local/share/Steam/steamapps/common/GE-Proton11-14-vt`.
   See `linux/wineserver/README.md`. This is now the primary server for
   GE runs, not an optional experiment.

## Running it

`LINUX.md` has build/run/switch docs. The new `tools/linux-run.sh` starts the
layer directly (no empty cmd.exe window), writes the log through
`VICTORY_LOG_FILE`, and shows clock + CPU/GPU use through MangoHud
(gamescope `--mangoapp`). `exerun.sh` on this host now asks MangoHud for the
clock and CPU/GPU stats too.
