#!/usr/bin/env python3
"""Structured analysis of one fixed-loop victory run under Proton.

Reads the app log and capture directory produced by run.sh, then prints a
stable report. Every sweep uses the same checks so results from different
commits and different agents are comparable.

Ported from victory-royale tools/linux-sweep/analyze.py; the log patterns
here match this fork's wording (world marker without `frame=`, the per-five-
second fps line instead of present-interval windows) and add the failures
only Wine/vkd3d produce: the card compare's shader does not compile, the
empty host pipeline library is refused, and helper shaders of ours fail to
compile.
"""
from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from pathlib import Path

SHADER_DROP = re.compile(
    r"shader does not translate; its pipelines' draws are dropped "
    r'stage="(?P<stage>[^"]+)" hash=(?P<hash>[0-9a-f]+) error=(?P<error>.+)'
)
BLACK_FRAME = re.compile(
    r"a black frame after a frame that was not frame=(?P<frame>\d+) "
    r"resource=(?P<resource>0x[0-9a-f]+) state=(?P<state>0x[0-9a-f]+) "
    r"draws=(?P<draws>\d+) copied=(?P<copied>\w+) black_frames=(?P<count>\d+)"
)
# This fork: "... frames 6410 and 6414 are captured" (the qualifying frame
# is two before the first). victory-royale added a trailing `frame=N`.
WORLD_MARKER = re.compile(
    r"the first frame of \d+ draws or more: frames (?P<first>\d+) and "
    r"(?P<second>\d+) are captured(?: frame=(?P<qualifying>\d+))?"
)
# victory-royale's frame-time meter (kept for logs from that tree).
PRESENT = re.compile(
    r"present intervals: (?P<n>\d+) frames, mean (?P<mean>[\d.]+) ms, "
    r"median (?P<median>[\d.]+) ms, p95 (?P<p95>[\d.]+) ms, "
    r"longest (?P<longest>[\d.]+) ms, (?P<over100>\d+) over 100 ms, "
    r"(?P<over1s>\d+) over 1 s"
)
# This fork: one line per five-second report, "30.0 fps, 33.3 ms/frame".
FPS = re.compile(r"(?P<fps>[\d.]+) fps, (?P<ms>[\d.]+) ms/frame")
FAULT = re.compile(
    r"\bshader fault\b|\bdevice lost\b|\bhost GPU device removed\b|"
    r"\bdevice removed\b|\bDeviceLost\b",
    re.IGNORECASE,
)
CRASH = re.compile(r"\bunhandled exception\b|\bWER - Saving\b|\bBreakpad\b", re.IGNORECASE)
INDIRECT_FAIL = re.compile(r"no argument clamp: indirect execution dropped")
DEPTH_FAIL = re.compile(r"no pipeline to draw a depth plane")
# Wine-only degradations: vkd3d-shader refuses a helper shader or the empty
# library, so a feature silently falls back and frame time is not what a
# Windows run measures.
CARD_COMPARE_FAIL = re.compile(r"no host copies in the card's memory: the compare could not be made")
LIBRARY_FAIL = re.compile(r"no host pipeline library: pipelines are not kept")
HELPER_SHADER_FAIL = re.compile(r"a shader of ours: (?P<error>.+)")
UNIMPLEMENTED_PIPELINE = re.compile(
    r"a pipeline creation method that is not implemented: the title goes without this pipeline "
    r"slot=(?P<slot>0x[0-9a-f]+)"
)


def analyse(log: Path, captures: Path) -> int:
    text = log.read_text(errors="replace") if log.exists() else ""
    lines = text.splitlines()

    shader_drops: list[tuple[str, str, str]] = []
    black_frames: list[tuple[int, str, int, int]] = []
    world: tuple[int, int, int] | None = None
    presents: list[dict[str, float]] = []
    fps_windows: list[tuple[float, float]] = []
    faults = 0
    crashes = 0
    indirect = 0
    depth = 0
    card_compare = 0
    library_fail = 0
    helper_errors: list[str] = []
    unimplemented: Counter[str] = Counter()

    for line in lines:
        clean = re.sub(r"\x1b\[[0-9;]*m", "", line)
        if m := SHADER_DROP.search(clean):
            shader_drops.append((m["stage"], m["hash"], m["error"]))
        if m := BLACK_FRAME.search(clean):
            black_frames.append((int(m["frame"]), m["resource"], int(m["count"]),
                                 int(m["draws"])))
        if m := WORLD_MARKER.search(clean):
            qualifying = int(m["qualifying"] or int(m["first"]) - 2)
            world = (qualifying, int(m["first"]), int(m["second"]))
        if m := PRESENT.search(clean):
            presents.append(
                {
                    "n": float(m["n"]),
                    "mean": float(m["mean"]),
                    "median": float(m["median"]),
                    "p95": float(m["p95"]),
                    "longest": float(m["longest"]),
                    "over100": float(m["over100"]),
                    "over1s": float(m["over1s"]),
                }
            )
        if m := FPS.search(clean):
            fps_windows.append((float(m["fps"]), float(m["ms"])))
        if FAULT.search(clean):
            faults += 1
        if CRASH.search(clean):
            crashes += 1
        if INDIRECT_FAIL.search(clean):
            indirect += 1
        if DEPTH_FAIL.search(clean):
            depth += 1
        if CARD_COMPARE_FAIL.search(clean):
            card_compare += 1
        if LIBRARY_FAIL.search(clean):
            library_fail += 1
        if m := HELPER_SHADER_FAIL.search(clean):
            helper_errors.append(m["error"])
        if m := UNIMPLEMENTED_PIPELINE.search(clean):
            unimplemented[m["slot"]] += 1

    # A one-off black frame during startup/menu is not a rendering failure in
    # the world. Keep warnings in or after the world, or from a scene already
    # drawing enough geometry to be the world.
    world_start = world[0] if world else None
    black_frames = [
        bf for bf in black_frames
        if (world_start is not None and bf[0] >= world_start) or bf[3] >= 100
    ]

    pngs = sorted(captures.glob("*.png")) if captures.exists() else []
    big = [p for p in pngs if p.stat().st_size > 500_000]

    def frame_of(name: str) -> int:
        """The frame a capture is named after (`frame-6416.png`), or -1."""
        match = re.search(r"frame-(\d+)", name)
        return int(match.group(1)) if match else -1

    # The first captures are the title's splash and loading art, not the
    # world: when the world marker is known, its captures are the evidence.
    # A black world frame is a few hundred kilobytes, so it would fall out of
    # `big`: measure every capture after the marker, whatever its size.
    if world_start is not None:
        shown = [p for p in pngs if frame_of(p.name) >= world_start]
    else:
        shown = big
    if not shown:
        shown = big

    # A large PNG can still be a black loading screen. Measure luminance of
    # the biggest captures so the sweep flags "reachable but renders nothing".
    brightness: list[tuple[str, float]] = []
    bright_fraction: list[float] = []
    tint: list[tuple[str, float, float, float, float]] = []
    try:
        from PIL import Image, ImageStat

        for p in sorted(shown, key=lambda x: x.stat().st_size, reverse=True)[:8]:
            rgb = Image.open(p).convert("RGB")
            stat = ImageStat.Stat(rgb)
            gray = rgb.convert("L")
            mean = ImageStat.Stat(gray).mean[0]
            brightness.append((p.name, mean))
            # A dark night scene still has lit sky, lamps and geometry; a
            # loading screen has almost no bright pixels. Use the fraction
            # above 80 to tell them apart, not the mean alone. Histograms
            # keep this fast for 4 MB captures.
            small = 240 * 135
            bright_fraction.append(sum(gray.resize((240, 135)).histogram()[80:]) / small)
            (r, g, b) = stat.mean
            from PIL import ImageChops
            (cr, cg, cb) = rgb.split()
            least = ImageChops.darker(ImageChops.darker(cr, cg), cb)
            blown = sum(least.resize((240, 135)).histogram()[240:]) / small
            tint.append((p.name, r, g, b, blown))
    except ImportError:
        pass
    if brightness:
        means = [m for _, m in brightness]
        print(f"capture_luminance min={min(means):.1f} max={max(means):.1f} "
              f"bright_ge80={sum(m >= 80 for m in means)}/{len(means)} "
              f"bright_frac={max(bright_fraction):.3f}")
        print(f"  darkest={min(brightness, key=lambda x: x[1])} "
              f"brightest={max(brightness, key=lambda x: x[1])}")
        if tint:
            # A healthy capture keeps its blue channel. A broken post-process
            # or pixel output drops it: R/G stay high while B collapses.
            cast = [max(r, g) / max(b, 1.0) for _, r, g, b, _ in tint]
            blown = [x[4] for x in tint]
            print(f"capture_color mean_r={sum(x[1] for x in tint)/len(tint):.1f} "
                  f"mean_g={sum(x[2] for x in tint)/len(tint):.1f} "
                  f"mean_b={sum(x[3] for x in tint)/len(tint):.1f} "
                  f"color_cast={max(cast):.2f} blown_frac={max(blown):.3f}")

    print(f"log_lines={len(lines)} captures={len(pngs)} nonblank={len(big)}")
    if world_start is not None:
        after = [p for p in pngs if frame_of(p.name) >= world_start]
        print(f"world_captures={len(after)} world_nonblank={sum(p.stat().st_size > 100_000 for p in after)}")
    print(f"world_marker={world or 'NONE'}")
    print(f"faults={faults} crashes={crashes} indirect_drops={indirect} depth_plane_drops={depth}")
    print(f"wine_shader_failures card_compare={card_compare} pipeline_library={library_fail} "
          f"helper_shaders={len(helper_errors)}")
    for error, count in Counter(helper_errors).most_common(5):
        print(f"  helper_shader count={count} err={error[:200]}")
    if unimplemented:
        slots = ", ".join(f"{s}={c}" for s, c in sorted(unimplemented.items()))
        print(f"unimplemented_pipeline_calls {slots}")
    last_black = black_frames[-1][:3] if black_frames else None
    print(f"black_frame_warnings={len(black_frames)} last={last_black}")
    print(f"shader_translation_drops={len(shader_drops)}")
    for (stage, h, err), count in Counter(shader_drops).most_common(12):
        print(f"  drop stage={stage} hash={h} count={count} err={err}")
    if shader_drops:
        print(f"  example_error={shader_drops[0][2][:200]}")
    if presents:
        worst = max(presents, key=lambda p: p["longest"])
        last = presents[-1]
        print(f"present_windows={len(presents)} worst_longest_ms={worst['longest']}")
        print(f"last_window n={last['n']:.0f} mean={last['mean']} median={last['median']} "
              f"p95={last['p95']} longest={last['longest']} over100={last['over100']:.0f} "
              f"over1s={last['over1s']:.0f}")
    if fps_windows:
        # Every five-second report, in order: the last one is the state the
        # run ended in, the slowest the worst the title held.
        lasts = fps_windows[-1]
        worst = min(fps_windows, key=lambda w: w[0])
        print(f"fps_windows={len(fps_windows)} last_fps={lasts[0]:.1f} "
              f"last_ms={lasts[1]:.1f} worst_fps={worst[0]:.1f} worst_ms={worst[1]:.1f}")

    # A sweep fails if anything that should render is missing or errors appear.
    # A dim night world still has some lit sky, lamps or windows. Loading
    # screens measured in past sweeps had bright_frac <= 0.017; require less
    # than 2% bright pixels before calling every capture a dark screen.
    dark_screen = bool(brightness) and all(
        m < 80 and f < 0.02 for (_, m), f in zip(brightness, bright_fraction)
    )
    # Warm night lighting also suppresses blue, but the whole image stays
    # dim. Only flag a collapsed blue channel when the capture is bright;
    # otherwise a valid night scene would fail the sweep.
    # Only the world's own colour is evidence: the splash art is warm and
    # would read as a cast on a run that never reached the world.
    color_cast = bool(world) and bool(tint) and all(
        max(r, g) >= 60 and max(r, g) / max(b, 1.0) > 1.6
        for _, r, g, b, _ in tint
    )
    blown = bool(world) and bool(tint) and max(x[4] for x in tint) > 0.25
    ok = (bool(world) and not faults and not crashes and not shader_drops
          and not black_frames and not dark_screen and not color_cast
          and not card_compare and not library_fail)
    if dark_screen:
        print("DARK_SCREEN: every large capture is a dark loading screen, not the world")
    if color_cast:
        print("COLOR_CAST: blue channel collapsed in every large capture; pixel "
              "output or post-processing is broken")
    if blown:
        print("BLOWN_HIGHLIGHTS: large fully-white regions in the captures")
    if card_compare:
        print("WINE_SHADER_FAIL: the card compare's HLSL did not compile under "
              "vkd3d's compiler; title copies stay in the PC's memory")
    if library_fail:
        print("WINE_LIBRARY_FAIL: vkd3d refused the empty host pipeline library; "
              "pipelines are not kept between runs")
    print(f"VERDICT={'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--log", type=Path, required=True)
    ap.add_argument("--captures", type=Path, required=True)
    args = ap.parse_args()
    return analyse(args.log, args.captures)


if __name__ == "__main__":
    sys.exit(main())
