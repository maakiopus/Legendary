# The 20 fps pass: profile, arms, and the ceiling

Status, 28 Nov: **20 fps was not reached.** On a fixed save, Series X, 1080p,
render scale 0.667, the median frame went from **157.7 ms to 135.7 ms**
(6.3 → 7.4 fps) with a set of memory-path switches. The GPU is idle behind
the CPU (3 ms of wait); what is left is the per-frame sync work on the
submitting thread, and switches moved it by milliseconds, not by the 86 ms
the target still needs.

## How it was measured

- Save: `GTA6SAVES/Manual Save`, copied over the prefix's
  `AppData/Local/victory/data/7952987A` before every arm, so each run starts
  from the same slot (the live save drifts between runs and changes the
  scene).
- `--console series-x`, 1920x1080, `VICTORY_RENDER_SCALE=0.667`, 480 s arms
  (660 s for the two validations), world period only: the median of the 5 s
  frame windows after the first window with more than 1,000 draws.
- Same Proton (GE-Proton11-14-vt), same machine (Ryzen 5 5600G, RX 6700 XT),
  same sweep (`tools/linux-sweep/run.sh`).

## Arms

Each row adds one switch to the row above; `median` is the world-period
median, `p95` the 95th percentile of the windows.

| arm | added | median | p95 | verdict |
|---|---|---|---|---|
| baseline | — | 157.7 | 193.3 | |
| relaycopy | `VICTORY_RELAY_COPY=1` | 154.4 | 205.5 | keep: copy-back 73.7 → 30.8 ms |
| copycompare | `VICTORY_COPY_COMPARE=1` | 145.9 | 194.4 | keep: 48 of 68 MB written found unchanged |
| copybeside | `VICTORY_COPY_BESIDE=1` | 143.1 | 177.8 | keep |
| storebeside | `VICTORY_STORE_BESIDE=1` | 141.4 | 184.2 | keep: guard wait 27.3 → 11.2 ms |
| late1024 | `VICTORY_LATE_WAIT_LISTS=1024` | 140.4 | 176.8 | keep |
| card64 | `VICTORY_CARD_BUFFERS_MIN_KB=64`, `_MB=1536` | 139.2 | 179.8 | keep |
| texturenowait | `VICTORY_TEXTURE_NO_WAIT=1` | 138.0 | 173.7 | keep |
| join8 | `VICTORY_JOIN_PARTS=8` | 156.6 | 221.0 | drop |
| light0 | `VICTORY_LIGHT_REFRESH=0` | 195.9 | 221.9 | drop |
| minmb16 | `VICTORY_CARD_BUFFERS_MIN_MB=16` | 215.4 | 233.2 | drop |
| wide4 / wide6 | `VICTORY_WIDE_THREADS=4/6` | 154.2 / 141.3 | 186.5 / 179.6 | 4 worse, 6 neutral |
| thin | `VICTORY_THIN=1` | 148.2 | 182.8 | drop |
| viewask | `VICTORY_VIEW_ASK=1` | 144.3 | 185.5 | drop |
| steps0 / steps4 | `VICTORY_CARD_STEPS=0/4` | 169.3 / 170.3 | 202.4 / 206.5 | drop; 2 is right |
| upq / askearly0 | `VICTORY_UP_QUEUE=1` / `ASK_EARLY=0` | 141.1 / 141.1 | 188.3 / 179.7 | neutral |
| indirect | `VICTORY_INDIRECT_BATCH=2`, `_WINDOW=1` | 138.8 | 172.8 | **rejected**: one black frame |
| spin | `VICTORY_WIDE_SPIN=100` | 141.0 | 179.7 | neutral |

Validations of the winning set with `tools/linux-sweep/run.sh`:

| set | duration | median | p95 | p99 | faults | non-blank |
|---|---|---|---|---|---|---|
| without `TEXTURE_NO_WAIT` | 660 s | 140.9 | 173.9 | 215.3 | 0 | 518/525 |
| final (all nine) | 660 s | **135.7** | 176.7 | 207.8 | 0 | 534/541 |

Both PASS, 0 crashes, 0 black-frame warnings. The final set is in
`run.sh`'s play defaults.

## Where the remaining time is

Final set, per frame (medians of the layer's own lines):

| | ms |
|---|---|
| frame | 136 |
| preparing (submitting thread) | 110.9 |
| — copies in the card's memory | 40.3 over ~92 MB |
| — list steps | 52.7 (refresh flushes 46) |
| — textures | 7.6 |
| write-watch askings | 44.0 (428 refused askings a frame) |
| copy-back | 25.7 |
| GPU wait | 3.0 |

The card is idle: the submitting thread is the wall, and its work is
page-granular memory syncing:

- 6,305 pages refreshed a frame, 5,361 of them found to copy nothing.
- ~43,000 tiny up-ranges for 26 of the ~92 MB, ~600 B each.
- 428 askings a frame that the kernel refuses (view memory), each taking
  Wine's process-wide mapping lock.

50 ms (20 fps) would need all three of those to fall by roughly 3×. No
switch does: each moved 1–9 ms, and the ones that touch the mechanism
directly (`VIEW_ASK`, `SEND_CHANGED=0`, `JOIN_PARTS=8`, `UP_QUEUE=1`) were
neutral or worse.

## What would move it (code, not switches)

1. **Whole-page sends where no shader wrote the page.** The 43,000 up-ranges
   exist because only changed dwords are sent. A page the GPU never wrote
   can be sent as one 4 KiB copy; the layer already tracks which card
   pieces shaders wrote (`pieces compared on the card`, `fetched`). This is
   the largest single cut available (~40 ms of copies).
2. **Skip refreshes that last time found nothing.** 5,361 of 6,305 pages
   copy nothing every frame; a per-page "unchanged since last compare"
   counter would take most of the 46 ms flush away. Must be bounded (the
   title can write the same value back).
3. **Compare on the card.** 317 hash and 95 compare passes a frame run on
   the host; moving them to compute passes would trade 10–20 ms of submit
   thread for GPU time that is currently idle.
4. **Native memory calls (`victory_mem.so`, ported).** Measured neutral
   here: our Proton already carries the rbtree wineserver, so direct pages
   remove little, and native scan's kernel askings cost what
   `GetWriteWatch`'s refusals cost. It stays opt-in and off.
5. **Nothing in the title's own rendering.** At scale 0.667 the GPU floor is
   ~3 ms of wait; raising or lowering the scale does not move the CPU wall.

## Reproduce

```sh
# reset the slot to the fixed save, then
SWEEP_LABEL=arm SWEEP_DURATION=480 SWEEP_EXE=<diag exe> \
SWEEP_EXTRA=$'set VICTORY_RENDER_SCALE=0.667\nset VICTORY_RELAY_COPY=1\n...' \
  ./tools/linux-sweep/run.sh
```

`tools/linux-sweep/perf.py` reports the window medians; the per-line
breakdown above comes from parsing the same `app.log` (`preparing per frame`,
`asking about title memory`, `host copies in the card's memory`, `landing per
frame`).
