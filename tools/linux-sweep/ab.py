#!/usr/bin/env python3
"""Compare two or more fixed-loop sweeps (run.sh) side by side.

    python3 tools/linux-sweep/ab.py baseline after-fix
    python3 tools/linux-sweep/ab.py /tmp/opencode/sweep-a /tmp/opencode/sweep-b

Each argument is either a sweep label (looked up as /tmp/opencode/sweep-<label>)
or a directory holding app.log and capture/. Runs analyze.py for each, keeps
the verdict lines, and prints one table so a before/after is read at a glance.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ANALYZE = HERE / "analyze.py"
LABEL_ROOT = Path("/tmp/opencode")

# (name, pattern, how to pick the number out of the match)
METRICS = [
    ("world_marker", re.compile(r"^world_marker=(.+)$")),
    ("verdict", re.compile(r"^VERDICT=(\w+)$")),
    ("faults", re.compile(r"^faults=(\d+)")),
    ("crashes", re.compile(r"^faults=\d+ crashes=(\d+)")),
    ("indirect_drops", re.compile(r"indirect_drops=(\d+)")),
    ("depth_drops", re.compile(r"depth_plane_drops=(\d+)")),
    ("black_frames", re.compile(r"^black_frame_warnings=(\d+)")),
    ("shader_drops", re.compile(r"^shader_translation_drops=(\d+)")),
    ("card_compare_fail", re.compile(r"card_compare=(\d+)")),
    ("library_fail", re.compile(r"pipeline_library=(\d+)")),
    ("helper_shader_fail", re.compile(r"helper_shaders=(\d+)")),
    ("captures", re.compile(r"^log_lines=\d+ captures=(\d+)")),
    ("nonblank", re.compile(r"nonblank=(\d+)")),
    ("luminance", re.compile(r"^capture_luminance (.+)$")),
    ("fps", re.compile(r"^fps_windows=.*last_fps=([\d.]+) last_ms=([\d.]+)")),
    ("present", re.compile(r"^last_window (.+)$")),
]

# The frame-time columns come from the sweep dir's perf-summary.json (written
# by perf.py); a sweep without one shows a dash.
PERF_KEYS = ("median_ms", "p95_ms", "p99_ms", "spikes", "top_stall")


def resolve(argument: str) -> Path:
    direct = Path(argument)
    if (direct / "app.log").exists():
        return direct
    if direct.is_dir():
        raise SystemExit(f"no app.log under {direct}")
    return LABEL_ROOT / f"sweep-{argument}"


def analyse(directory: Path) -> dict[str, str]:
    result = subprocess.run(
        [sys.executable, str(ANALYZE), "--log", str(directory / "app.log"),
         "--captures", str(directory / "capture")],
        capture_output=True, text=True, check=False,
    )
    if result.returncode != 0 and "VERDICT=" not in result.stdout:
        print(f"warn: analyze.py failed for {directory} (rc={result.returncode}): "
              f"{result.stderr.strip()[:300]}", file=sys.stderr)
    output = result.stdout
    found: dict[str, str] = {}
    for line in output.splitlines():
        for name, pattern in METRICS:
            if name in found:
                continue
            if match := pattern.search(line):
                found[name] = " ".join(match.groups()) if len(match.groups()) > 1 else match.group(1)
    for name, _ in METRICS:
        found.setdefault(name, "-")
    return found


def load_perf(directory: Path) -> dict[str, str]:
    found: dict[str, str] = dict.fromkeys(PERF_KEYS, "-")
    summary = directory / "perf-summary.json"
    if not summary.is_file():
        return found
    try:
        data = json.loads(summary.read_text())
    except (OSError, ValueError):
        return found
    if not isinstance(data, dict):
        return found
    for key in PERF_KEYS:
        value = data.get(key)
        if isinstance(value, (int, float)):
            found[key] = f"{value:g}"
        elif value is not None:
            found[key] = str(value)
    return found


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("sweeps", nargs="+", help="labels or sweep directories")
    args = ap.parse_args()
    if len(args.sweeps) < 2:
        ap.error("two sweeps at least")
    columns = []
    for argument in args.sweeps:
        directory = resolve(argument)
        if not (directory / "app.log").exists():
            print(f"no app.log under {directory}", file=sys.stderr)
            return 2
        columns.append((argument, analyse(directory) | load_perf(directory)))
    rows = [name for name, _ in METRICS] + list(PERF_KEYS)
    width = max(len(name) for name in rows) + 2
    cell = 28
    header = "".ljust(width) + "".join(label.ljust(cell) for label, _ in columns)
    print(header)
    for name in rows:
        print(name.ljust(width) + "".join(values[name][:cell - 1].ljust(cell) for _, values in columns))
    return 0


if __name__ == "__main__":
    sys.exit(main())
