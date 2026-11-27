# Fixed Linux graphics and performance sweep

Runs this fork through one deterministic scenario on GE-Proton11-14-vt (the
patched-server configuration the reference runs use; PROTONPATH overrides it)
and reports graphics and frame-time defects in a stable format.
Every sweep uses the same Proton, keys, capture points and duration, so
results from different commits and different agents are comparable.

Ported from `victory-royale`'s sweep (`tools/linux-sweep/`); the log patterns
in `analyze.py` match this fork's wording and add the Wine-only failures the
other tree never logged.

**No system Wine is used.** The only launch path is `umu-run` with `PROTONPATH`
pointing at Steam GE-Proton11-14-vt (default; PROTONPATH overrides it);
UMU-Proton 10/11 emit invalid SPIR-V that RADV faults on in
`radv_shader_spirv_to_nir`.

## Usage

```
cargo xwin build --profile diag -p victory --features diagnostics \
    --target x86_64-pc-windows-msvc
SWEEP_LABEL=baseline ./tools/linux-sweep/run.sh
```

The binary must be the `diag` profile: `VICTORY_KEYS` and `VICTORY_CAPTURE`
exist only in builds with the `diagnostics` feature. A release build runs,
but the sweep cannot press a key or capture a frame.

Overrides: `SWEEP_EXE`, `SWEEP_GAME`, `SWEEP_CONSOLE` (default `series-x`),
`SWEEP_DURATION` (default 660 s), `SWEEP_KEYS` (default: press A — Enter on
the stand-in pad — every 10 s from 10 s to 200 s), `SWEEP_LOG` (default
`info`), `SWEEP_CACHE` (default `~/.cache/victory-sweep`), `SWEEP_EXTRA`
(extra `set VICTORY_*=...` lines), `SWEEP_KILL=0` (leave stale runs alone),
`PROTONPATH` (default `GE-Proton11-14-vt`), `STEAM_COMPAT_INSTALL_PATH`,
`STEAM_COMPAT_LIBRARY_PATHS`.

Output goes to `/tmp/opencode/sweep-$SWEEP_LABEL/`: `app.log` (app output),
`runner.log` (umu output), `capture/` (PNG captures) and the structured
verdict from `analyze.py`. When `tools/linux-sweep/perf.py` exists, the
sweep runs it afterwards and adds a frame-time report: `frametimes.csv`
(one row per five-second window), `spikes.json` (the spike trace), `perf-summary.json`
(the machine-readable summary: `windows`, `median_ms`, `p95_ms`, `p99_ms`,
`last_fps`, `spikes`, `top_stall`, `spike_windows`) and `perf-report.md`
(the readable report), with a short summary teed into the sweep output.
A `perf.py` error never fails the sweep — the verdict stands. Its spike
windows are the same five-second windows `analyze.py` reports, and
`tools/perf/stalls.txt` runs with `TZ=UTC`, the clock `app.log` and the
report use, so a spike names the seconds and the stall trace names what
the kernel saw in them. `p95_ms`/`p99_ms` are percentiles of the
five-second windows, not per-frame percentiles.

A run that reaches the title screen needs the A presses; the retail game does
not leave it without one. Two presses were not enough in the other tree's
sweeps, so the default holds the key every 10 s for the first 200 s.

## Compare two sweeps

```
python3 tools/linux-sweep/ab.py baseline after-fix
python3 tools/linux-sweep/ab.py /tmp/opencode/sweep-a /tmp/opencode/sweep-b
```

Runs `analyze.py` for each and prints the same metrics side by side, plus
the perf-summary columns (`median_ms`, `p95_ms`, `p99_ms`, `spikes`,
`top_stall`) when that sweep dir has a `perf-summary.json` from `perf.py`;
a dash means the sweep ran without it.

## Checks

`analyze.py` flags:

- shader translation drops (stage, hash, converter error)
- black-frame warnings (only in or after the world, or where a frame already
  draws 100+ times: loading screens are black by design)
- shader faults, crashes and a removed host device
- indirect/depth-plane pipeline drops
- world marker (the first frame of 500 draws or more) and nonblank captures
- present/fps windows: last and worst five-second report (worst includes loading)
- capture luminance, bright-pixel fraction, colour cast and blown highlights
- Wine-only failures: the card compare's HLSL not compiling, the empty host
  pipeline library refused by vkd3d, helper shaders of ours that do not
  compile, and the console pipeline methods the fork does not implement

`VERDICT=PASS` requires a world marker, no faults, no crashes, no shader
drops, no black-frame warnings in the world, no Wine shader/library failure
(helper shaders included), and captures that are neither a dark loading
screen nor colour-cast. `run.sh` exits with the verdict's code so a sweep
loop can chain fix attempts automatically.

A large PNG does **not** prove the world rendered: a dark loading screen was
~6 KB to 60 KB in past sweeps and a real frame 4 MB+. The luminance and
bright-fraction checks are the evidence; read the brightest frame yourself
when a verdict is close.
