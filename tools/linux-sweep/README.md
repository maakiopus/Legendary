# Fixed Linux graphics and performance sweep

Runs this fork through one deterministic scenario on Proton Experimental and
reports graphics and frame-time defects in a stable format. Every sweep uses
the same Proton, keys, capture points and duration, so results from different
commits and different agents are comparable.

Ported from `victory-royale`'s sweep (`tools/linux-sweep/`); the log patterns
in `analyze.py` match this fork's wording and add the Wine-only failures the
other tree never logged.

**No system Wine is used.** The only launch path is `umu-run` with `PROTONPATH`
pointing at Steam Proton Experimental (UMU-Proton 10/11 emit invalid SPIR-V
that RADV faults on in `radv_shader_spirv_to_nir`).

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
`PROTONPATH`, `STEAM_COMPAT_INSTALL_PATH`, `STEAM_COMPAT_LIBRARY_PATHS`.

Output goes to `/tmp/opencode/sweep-$SWEEP_LABEL/`: `app.log` (app output),
`runner.log` (umu output), `capture/` (PNG captures) and the structured
verdict from `analyze.py`.

A run that reaches the title screen needs the A presses; the retail game does
not leave it without one. Two presses were not enough in the other tree's
sweeps, so the default holds the key every 10 s for the first 200 s.

## Compare two sweeps

```
python3 tools/linux-sweep/ab.py baseline after-fix
python3 tools/linux-sweep/ab.py /tmp/opencode/sweep-a /tmp/opencode/sweep-b
```

Runs `analyze.py` for each and prints the same metrics side by side.

## Checks

`analyze.py` flags:

- shader translation drops (stage, hash, converter error)
- black-frame warnings (only in or after the world, or where a frame already
  draws 100+ times: loading screens are black by design)
- shader faults, crashes and a removed host device
- indirect/depth-plane pipeline drops
- world marker (the first frame of 500 draws or more) and nonblank captures
- present/fps windows: last and worst five-second report
- capture luminance, bright-pixel fraction, colour cast and blown highlights
- Wine-only failures: the card compare's HLSL not compiling, the empty host
  pipeline library refused by vkd3d, helper shaders of ours that do not
  compile, and the console pipeline methods the fork does not implement

`VERDICT=PASS` requires a world marker, no faults, no crashes, no shader
drops, no black-frame warnings in the world, no Wine shader/library failure,
and captures that are neither a dark loading screen nor colour-cast. It exits
non-zero on failure so a sweep loop can chain fix attempts automatically.

A large PNG does **not** prove the world rendered: a dark loading screen was
~6 KB to 60 KB in past sweeps and a real frame 4 MB+. The luminance and
bright-fraction checks are the evidence; read the brightest frame yourself
when a verdict is close.
