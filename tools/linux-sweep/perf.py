#!/usr/bin/env python3
"""Frametime trace and stall-point analysis for one victory sweep run.

Reads only the app log; captures are not needed. Every five seconds the host
prints one line like

    30.0 fps, 33.3 ms/frame (9.4 preparing: copies 0.2 ms 0.2 MB, textures
    9.2 ms 12.7 MB; 0.0 on the GPU; 0.0 copying back 0.0 MB), 6 draws/frame,
    dropped 0 without a host pipeline (0 being made) and 0 indirect; ...

so a whole run turns into one frametime row per report window. Once a second
the host also prints the cost lines (the card's clock, preparing per frame,
asking about title memory, texture refreshes, the relay, submissions sent
ahead, write-watch askings, host copies in the card's memory); the ones
printed since the previous fps line belong to that window.

Outputs, written into --out (default: the log's directory):
  frametimes.csv     one row per five-second report window
  spikes.json        windows over max(2 x median, 100 ms), >1 s marked apart
  perf-summary.json  headline numbers and the top stall point
  perf-report.md     frame time, spike and stall report with switch experiments

Stdout stays at a few summary lines so a sweep can call this next to
analyze.py without burying its verdict.
"""
from __future__ import annotations

import argparse
import csv
import json
import re
import statistics
import sys
from pathlib import Path

ANSI = re.compile(r"\x1b\[[0-9;]*m")
TIMESTAMP = re.compile(r"^(?P<t>\d{4}-\d{2}-\d{2}T[\d:.]+Z)\s")
# The fork's five-second report: one line, so the regex needs no DOTALL.
FPS_LINE = re.compile(
    r"(?P<fps>[\d.]+) fps, (?P<ms>[\d.]+) ms/frame "
    r"\((?P<preparing>[\d.]+) preparing: copies (?P<copies_ms>[\d.]+) ms "
    r"(?P<copies_mb>[\d.]+) MB, textures (?P<textures_ms>[\d.]+) ms "
    r"(?P<textures_mb>[\d.]+) MB; (?P<gpu_ms>[\d.]+) on the GPU; "
    r"(?P<copyback_ms>[\d.]+) copying back (?P<copyback_mb>[\d.]+) MB\), "
    r"(?P<draws>\d+) draws/frame, dropped (?P<dropped>\d+)"
)
# One cost line a second; the representative figure is the largest ms value on
# the line, which is that category's dominant component in that second.
COST_SPECS: list[tuple[str, str]] = [
    ("preparing per frame", r"preparing per frame, the submitting thread:"),
    ("asking about title memory", r"asking about title memory per frame"),
    ("write-watch askings", r"write-watch askings of a millisecond or more"),
    ("texture refreshes per frame", r"texture refreshes per frame, all threads:"),
    ("the relay per frame", r"the relay per frame \(VICTORY_RELAY=1\):"),
    ("submissions sent ahead", r"submissions sent ahead per frame:"),
    ("the card's clock", r"the card's clock per frame:"),
    ("host copies in the card's memory", r"host copies in the card's memory:"),
]
COST_LINES = [(name, re.compile(marker)) for name, marker in COST_SPECS]
MS_VALUE = re.compile(r"([\d.]+) ms")
CARD_WAIT = re.compile(r"stood waiting for the card ([\d.]+) ms")
GPU_CATEGORIES = ["copies", "textures", "GPU", "copy-back"]
SPIKE_OVER_1S = 1000.0
SWITCH_EXPERIMENTS = [
    ("VICTORY_LIGHT_REFRESH=0",
     "light refresh keeps texture copies small; turn it off to compare the "
     "full refresh path when textures dominate preparing"),
    ("VICTORY_ASK_EARLY=0",
     "the early thread asks for pieces of 2 MiB before the lists' steps; turn "
     "it off when the write watch or the copies in preparing dominate"),
    ("VICTORY_VIEW_ASK=1",
     "guards hold title memory as view memory; ask the kernel instead when "
     "'asking about title memory' dominates"),
]
SWITCH_EXPERIMENTS = SWITCH_EXPERIMENTS[:3]


def percentile(values: list[float], pct: float) -> float:
    ordered = sorted(values)
    if not ordered:
        return 0.0
    if len(ordered) == 1:
        return ordered[0]
    rank = (len(ordered) - 1) * pct / 100.0
    low = int(rank)
    high = min(low + 1, len(ordered) - 1)
    weight = rank - low
    return ordered[low] * (1.0 - weight) + ordered[high] * weight


def cost_of(line: str) -> float:
    """The dominant ms figure on a cost line; the clock line reports the
    submitting thread's wait for the card, not the card's idle time."""
    if "the card's clock per frame:" in line:
        match = CARD_WAIT.search(line)
        return float(match.group(1)) if match else 0.0
    values = [float(v) for v in MS_VALUE.findall(line)]
    return max(values) if values else 0.0


def parse_windows(text: str) -> list[dict]:
    """One record per fps line, with that window's cost lines folded in."""
    windows: list[dict] = []
    pending: dict[str, float] = {}
    for raw in text.splitlines():
        line = ANSI.sub("", raw)
        if match := FPS_LINE.search(line):
            timestamp = TIMESTAMP.match(line)
            windows.append(
                {
                    "t": timestamp["t"] if timestamp else "",
                    "fps": float(match["fps"]),
                    "ms": float(match["ms"]),
                    "preparing": float(match["preparing"]),
                    "copies_ms": float(match["copies_ms"]),
                    "copies_mb": float(match["copies_mb"]),
                    "textures_ms": float(match["textures_ms"]),
                    "textures_mb": float(match["textures_mb"]),
                    "gpu_ms": float(match["gpu_ms"]),
                    "copyback_ms": float(match["copyback_ms"]),
                    "copyback_mb": float(match["copyback_mb"]),
                    "draws": int(match["draws"]),
                    "dropped": int(match["dropped"]),
                    "costs": pending,
                    "raw": line,
                }
            )
            pending = {}
            continue
        for name, marker in COST_LINES:
            if marker.search(line):
                value = cost_of(line)
                pending[name] = max(pending.get(name, 0.0), value)
    return windows


def dominant_category(window: dict) -> tuple[str, float]:
    """The largest of copies/textures/GPU/copy-back in a window."""
    options = [
        ("copies", window["copies_ms"]),
        ("textures", window["textures_ms"]),
        ("GPU", window["gpu_ms"]),
        ("copy-back", window["copyback_ms"]),
    ]
    return max(options, key=lambda pair: pair[1])


def dominant_cost_line(window: dict) -> tuple[str, float]:
    if not window["costs"]:
        return "", 0.0
    return max(window["costs"].items(), key=lambda pair: pair[1])


def spike_cause(window: dict) -> str:
    name, value = dominant_cost_line(window)
    if name and value > 0.0:
        return f"{name} {value:.1f} ms"
    category, value = dominant_category(window)
    return f"{category} {value:.1f} ms"


def find_spikes(windows: list[dict], median_ms: float) -> list[dict]:
    threshold = max(2.0 * median_ms, 100.0)
    spikes = []
    for window in windows:
        if window["ms"] <= threshold:
            continue
        category, value = dominant_category(window)
        cost_name, cost_value = dominant_cost_line(window)
        spikes.append(
            {
                "t": window["t"],
                "ms_frame": window["ms"],
                "ratio_to_median": round(window["ms"] / median_ms, 2) if median_ms else 0.0,
                "over_1s": window["ms"] > SPIKE_OVER_1S,
                "dominant_cost": category,
                "dominant_cost_ms": value,
                "dominant_cost_line": cost_name,
                "dominant_cost_line_ms": cost_value,
                "cause": spike_cause(window),
                "raw_line": window["raw"],
            }
        )
    return spikes


def aggregate_costs(windows: list[dict]) -> list[dict]:
    """Rank the cost lines by their total across the windows they appear in."""
    totals: dict[str, float] = {}
    counts: dict[str, int] = {}
    worst: dict[str, tuple[str, float]] = {}
    for window in windows:
        for name, value in window["costs"].items():
            totals[name] = totals.get(name, 0.0) + value
            counts[name] = counts.get(name, 0) + 1
            if name not in worst or value > worst[name][1]:
                worst[name] = (window["t"], value)
    ranked = []
    for name, total in totals.items():
        count = counts[name]
        ranked.append(
            {
                "cost_line": name,
                "total_ms": round(total, 1),
                "mean_ms_per_window": round(total / count, 2) if count else 0.0,
                "windows": count,
                "worst_window_t": worst[name][0],
                "worst_window_ms": round(worst[name][1], 1),
            }
        )
    ranked.sort(key=lambda item: item["total_ms"], reverse=True)
    return ranked


def top_stall(windows: list[dict], spikes: list[dict]) -> str:
    ranked = aggregate_costs(windows)
    if ranked:
        return ranked[0]["cost_line"]
    if spikes:
        return spikes[0]["dominant_cost"]
    return ""


def write_csv(path: Path, windows: list[dict]) -> None:
    header = [
        "timestamp", "fps", "ms_frame", "preparing_ms", "copies_ms",
        "copies_mb", "textures_ms", "textures_mb", "gpu_ms", "copyback_ms",
        "copyback_mb", "draws", "dropped",
    ]
    with path.open("w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(header)
        for window in windows:
            writer.writerow(
                [
                    window["t"], window["fps"], window["ms"],
                    window["preparing"], window["copies_ms"],
                    window["copies_mb"], window["textures_ms"],
                    window["textures_mb"], window["gpu_ms"],
                    window["copyback_ms"], window["copyback_mb"],
                    window["draws"], window["dropped"],
                ]
            )


def write_summary(path: Path, summary: dict) -> None:
    path.write_text(json.dumps(summary, indent=2) + "\n")


def share(total_ms: float, frames_ms: float) -> float:
    return 100.0 * total_ms / frames_ms if frames_ms else 0.0


def category_shares(windows: list[dict]) -> dict[str, float]:
    frames_ms = sum(window["ms"] for window in windows)
    return {
        "preparing": share(sum(w["preparing"] for w in windows), frames_ms),
        "copies": share(sum(w["copies_ms"] for w in windows), frames_ms),
        "textures": share(sum(w["textures_ms"] for w in windows), frames_ms),
        "GPU": share(sum(w["gpu_ms"] for w in windows), frames_ms),
        "copy-back": share(sum(w["copyback_ms"] for w in windows), frames_ms),
    }


def write_report(path: Path, log: Path, summary: dict, spikes: list[dict],
                 ranked_costs: list[dict]) -> None:
    shares = summary["shares"]
    over_1s = sum(1 for spike in spikes if spike["over_1s"])
    median_line = "- median frame time: n/a"
    if summary["median_ms"]:
        median_line = (f"- median frame time: {summary['median_ms']:.1f} ms "
                       f"({1000.0 / summary['median_ms']:.1f} fps)")
    lines = [
        f"# Performance report — {log.name}",
        "",
        "## Frame time",
        f"- windows: {summary['windows']}",
        median_line,
        f"- p95 frame time: {summary['p95_ms']:.1f} ms",
        f"- p99 frame time: {summary['p99_ms']:.1f} ms",
        f"- last window fps: {summary['last_fps']:.1f}",
        "",
        "## Spikes",
        f"- {len(spikes)} windows over max(2 x median, 100 ms) "
        f"= {summary['spike_threshold_ms']:.1f} ms; {over_1s} over 1 s",
    ]
    if spikes:
        lines.append("")
        lines.append("| timestamp | ms/frame | x median | cause |")
        lines.append("|---|---|---|---|")
        for spike in sorted(spikes, key=lambda s: s["ms_frame"], reverse=True)[:5]:
            lines.append(
                f"| {spike['t']} | {spike['ms_frame']:.1f} | "
                f"{spike['ratio_to_median']:.1f} | {spike['cause']} |"
            )
    lines += [
        "",
        "## Share of frame time",
        f"- preparing: {shares['preparing']:.1f}% "
        f"(copies {shares['copies']:.1f}%, textures {shares['textures']:.1f}% of it)",
        f"- on the GPU: {shares['GPU']:.1f}%",
        f"- copying back: {shares['copy-back']:.1f}%",
        "",
        "Note: copies and textures are parts of preparing, so the three do not add up to 100%.",
        "",
        "## Top stall points",
    ]
    if ranked_costs:
        lines += [
            "| cost line | total ms | mean ms/window | windows | worst window |",
            "|---|---|---|---|---|",
        ]
        for item in ranked_costs[:8]:
            lines.append(
                f"| {item['cost_line']} | {item['total_ms']:.1f} | "
                f"{item['mean_ms_per_window']:.2f} | {item['windows']} | "
                f"{item['worst_window_t']} ({item['worst_window_ms']:.1f} ms) |"
            )
    else:
        lines.append("- no per-second cost lines in this log")
    lines += ["", "## Switch experiments"]
    for index, (switch, why) in enumerate(SWITCH_EXPERIMENTS, start=1):
        lines.append(f"{index}. `{switch}` — {why}. "
                     "Run again and compare perf-summary.json: median_ms, "
                     "p95_ms and the spike count decide.")
    lines.append("")
    path.write_text("\n".join(lines))


def analyse(log: Path, out: Path) -> int:
    text = log.read_text(errors="replace") if log.exists() else ""
    if not text.strip():
        print(f"perf: log is empty or missing: {log}")
        return 0
    windows = parse_windows(text)
    if not windows:
        print(f"perf: no five-second fps lines found in {log}")
        return 0

    frame_ms = [window["ms"] for window in windows]
    median_ms = statistics.median(frame_ms)
    p95_ms = percentile(frame_ms, 95)
    p99_ms = percentile(frame_ms, 99)
    spikes = find_spikes(windows, median_ms)
    ranked_costs = aggregate_costs(windows)
    stall = top_stall(windows, spikes)
    shares = category_shares(windows)
    summary = {
        "windows": len(windows),
        "median_ms": round(median_ms, 2),
        "p95_ms": round(p95_ms, 2),
        "p99_ms": round(p99_ms, 2),
        "last_fps": windows[-1]["fps"],
        "spikes": len(spikes),
        "spike_threshold_ms": round(max(2.0 * median_ms, 100.0), 1),
        "top_stall": stall,
        "shares": {name: round(value, 2) for name, value in shares.items()},
        "spike_windows": [
            {"t": spike["t"], "ms": spike["ms_frame"], "cause": spike["cause"]}
            for spike in spikes
        ],
    }

    out.mkdir(parents=True, exist_ok=True)
    write_csv(out / "frametimes.csv", windows)
    (out / "spikes.json").write_text(json.dumps(spikes, indent=2) + "\n")
    write_summary(out / "perf-summary.json", summary)
    write_report(out / "perf-report.md", log, summary, spikes, ranked_costs)

    worst = max(windows, key=lambda window: window["ms"])
    print(f"perf: windows={len(windows)} median={median_ms:.1f}ms "
          f"p95={p95_ms:.1f}ms p99={p99_ms:.1f}ms")
    print(f"perf: fps last={summary['last_fps']:.1f} worst={worst['fps']:.1f}")
    print(f"perf: spikes={len(spikes)} over_1s={sum(1 for s in spikes if s['over_1s'])} "
          f"worst={worst['ms']:.1f}ms at {worst['t']}")
    print(f"perf: top_stall={stall or 'none'}")
    print(f"perf: wrote frametimes.csv spikes.json perf-summary.json "
          f"perf-report.md in {out}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--log", type=Path, required=True)
    ap.add_argument("--captures", type=Path, default=None)
    ap.add_argument("--out", type=Path, default=None)
    args = ap.parse_args()
    out = args.out if args.out is not None else args.log.parent
    return analyse(args.log, out)


if __name__ == "__main__":
    sys.exit(main())
