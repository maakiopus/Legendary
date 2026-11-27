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


def resolve(argument: str) -> Path:
    direct = Path(argument)
    if (direct / "app.log").exists():
        return direct
    return LABEL_ROOT / f"sweep-{argument}"


def analyse(directory: Path) -> dict[str, str]:
    output = subprocess.run(
        [sys.executable, str(ANALYZE), "--log", str(directory / "app.log"),
         "--captures", str(directory / "capture")],
        capture_output=True, text=True, check=False,
    ).stdout
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
        columns.append((argument, analyse(directory)))
    width = max(len(name) for name, _ in METRICS) + 2
    header = "".ljust(width) + "".join(label.ljust(24) for label, _ in columns)
    print(header)
    for name, _ in METRICS:
        print(name.ljust(width) + "".join(values[name].ljust(24) for _, values in columns))
    return 0


if __name__ == "__main__":
    sys.exit(main())
