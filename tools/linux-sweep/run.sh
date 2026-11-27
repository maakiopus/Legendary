#!/usr/bin/env bash
# Fixed graphics/performance sweep for this fork on Linux.
#
# Every sweep uses identical Proton, keys, capture points and duration, so
# results from different commits and different agents are comparable.
# No system Wine is used at any point: only umu-run + Proton Experimental.
#
# Ported from victory-royale tools/linux-sweep/run.sh. This fork's title
# command is `victory run <exe> --execute --console <profile>`; the binary
# must be the `diag` profile for VICTORY_KEYS and VICTORY_CAPTURE to exist.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
LABEL=${SWEEP_LABEL:-sweep}
WORK=/tmp/opencode/sweep-$LABEL
EXE=${SWEEP_EXE:-$ROOT/target/x86_64-pc-windows-msvc/diag/victory.exe}
# The helpers (card compare, up check, argument clamps) only DXC compiles:
# without them the world's indirect records drop (`tools/d3dcompiler.sh`).
source "$ROOT/tools/d3dcompiler.sh"
source "$ROOT/tools/dxc.sh"
GAME=${SWEEP_GAME:-Z:\\media\\SSD\\GTA6.Xbox.v1.0.2.1-CYBERLEEK\\game_scarlett_master.exe}
CONSOLE=${SWEEP_CONSOLE:-series-x}
PROTON=${PROTONPATH:-$HOME/.local/share/Steam/steamapps/common/Proton - Experimental}
DURATION=${SWEEP_DURATION:-660}
# A shared cache outside the tree: the shader cache survives runs, and no
# game-derived output is written below the repository root (docs/asset-policy).
CACHE=${SWEEP_CACHE:-$HOME/.cache/victory-sweep}
KEYS=${SWEEP_KEYS:-10:Enter,20:Enter,30:Enter,40:Enter,50:Enter,60:Enter,70:Enter,80:Enter,90:Enter,100:Enter,110:Enter,120:Enter,130:Enter,140:Enter,150:Enter,160:Enter,170:Enter,180:Enter,190:Enter,200:Enter}
# `SWEEP_DEBUG=1`: the log at debug level, and the port's own
# `VICTORY_D3D12_DEBUG=1`, which names the command a device removal was in.
LOG_FILTER=${SWEEP_LOG:-info}

# The preflight: nothing runs past a missing piece, so a sweep is never a
# half-remembered one.
if [ ! -f "$EXE" ]; then
    echo "sweep: no exe at $EXE (tools/build.sh fast builds it)" >&2
    exit 2
fi
if ! dxil_bundled "$EXE" && ! d3dcompiler_native "$EXE"; then
    echo "sweep: the exe has no helper shaders and no native d3dcompiler beside it;" >&2
    echo "       under Wine the clamp fails and the world draws black. Fix either:" >&2
    echo "         tools/build.sh fast   (builds with dxc, the helpers bundled)" >&2
    echo "         or stage d3dcompiler_47 beside the exe (tools/d3dcompiler.sh)" >&2
    if [ "${SWEEP_NO_GUARDS:-0}" != 1 ]; then
        echo "sweep: refusing to run (SWEEP_NO_GUARDS=1 runs it anyway, for a black world)" >&2
        exit 2
    fi
fi
GAME_UNIX=${GAME//\\//}; GAME_UNIX=${GAME_UNIX/Z://}
if [ ! -f "$GAME_UNIX" ]; then
    echo "sweep: no game at $GAME_UNIX (SWEEP_GAME names it)" >&2
    exit 2
fi
if [ ! -d "$PROTON" ]; then
    echo "sweep: no Proton at $PROTON (PROTONPATH names it; the patched one is" >&2
    echo "       ~/.local/share/Steam/steamapps/common/GE-Proton11-14-vt)" >&2
    exit 2
fi
command -v umu-run >/dev/null 2>&1 || { echo "sweep: no umu-run on the path" >&2; exit 2; }
if [ "${SWEEP_DEBUG:-0}" = 1 ]; then
    LOG_FILTER=debug
fi
DXC=$(find_dxc || true)
echo "dxc: ${DXC:-none} (the build bundles the helpers with it; tools/build.sh fast does)"
[ "${SWEEP_DEBUG:-0}" = 1 ] && echo "sweep: debug mode (VICTORY_LOG=debug, VICTORY_D3D12_DEBUG=1)"

mkdir -p "$WORK" "$WORK/capture" "$CACHE"
# Write the batch file from the Unix path, then translate the one path it
# needs into the Z: drive umu-run exposes (/ -> Z:\).
BAT="$WORK/game.bat"
LOG="$WORK/app.log"

# Without the helpers in the exe, a native Microsoft d3dcompiler_47 beside
# it answers the run-time compile, under the Wine override.
if ! dxil_bundled "$EXE" && ensure_d3dcompiler_47 "$EXE"; then
    EXTRA_OVERRIDE='set WINEDLLOVERRIDES=d3dcompiler_47=n,b'
fi

ZEXE="Z:${EXE//\//\\}"
ZCAP="Z:${WORK//\//\\}\\capture"
ZGAME="$GAME"
ZCACHE="Z:${CACHE//\//\\}"

cat > "$BAT" <<EOF
@echo off
set VICTORY_LOG=$LOG_FILTER
set VICTORY_KEYS=$KEYS
set VICTORY_CACHE_DIR=$ZCACHE
set VICTORY_CAPTURE=$ZCAP
set VICTORY_CAPTURE_WORLD=0,3
EOF
[ "${SWEEP_DEBUG:-0}" = 1 ] && printf 'set VICTORY_D3D12_DEBUG=1\n' >> "$BAT"
[ -n "${EXTRA_OVERRIDE:-}" ] && printf '%s\n' "$EXTRA_OVERRIDE" >> "$BAT"
printf '%s\n' "\"$ZEXE\" run \"$ZGAME\" --execute --console $CONSOLE > Z:${WORK//\//\\\\}app.log 2>&1" >> "$BAT"

# Optional extra VICTORY_* settings for a diagnostic sweep, e.g.
# SWEEP_EXTRA='set VICTORY_PORT_CONSTANTS=1'. Inserted before the run line.
if [ -n "${SWEEP_EXTRA:-}" ]; then
    { head -3 "$BAT"; printf '%s\n' "$SWEEP_EXTRA"; tail -n +4 "$BAT"; } > "$BAT.tmp" && mv "$BAT.tmp" "$BAT"
fi

# A leftover run of this fork holds the prefix's wineserver and the Steam
# compat path; every new launch then fails or distorts timing.
if [ "${SWEEP_KILL:-1}" = 1 ]; then
    pkill -f 'victory[^ ]*\.exe run ' 2>/dev/null
    sleep 1
fi

export PROTONPATH="$PROTON"
export STEAM_COMPAT_INSTALL_PATH=${STEAM_COMPAT_INSTALL_PATH:-$HOME/.local/share/Steam/steamapps/common/victory-run}
export STEAM_COMPAT_LIBRARY_PATHS=${STEAM_COMPAT_LIBRARY_PATHS:-$HOME/.local/share/Steam/steamapps}

# The default compat path must be a symlink to the binary's directory.
mkdir -p "$(dirname "$STEAM_COMPAT_INSTALL_PATH")"
if [ ! -L "$STEAM_COMPAT_INSTALL_PATH" ] || [ "$(readlink -f "$STEAM_COMPAT_INSTALL_PATH")" != "$(dirname "$EXE")" ]; then
    rm -rf "$STEAM_COMPAT_INSTALL_PATH"
    ln -s "$(dirname "$EXE")" "$STEAM_COMPAT_INSTALL_PATH"
fi

echo "sweep=$LABEL exe=$EXE game=$GAME console=$CONSOLE duration=${DURATION}s"
echo "proton=$PROTONPATH cache=$CACHE"
timeout "$DURATION" umu-run 'C:\windows\system32\cmd.exe' /c "Z:${WORK//\//\\}\\game.bat" > "$WORK/runner.log" 2>&1
umu=$?
echo "umu_exit=$umu"

# The verdict, and the one line that says what the run was: nothing is
# diagnosed twice.
if python3 "$ROOT/tools/linux-sweep/analyze.py" --log "$LOG" --captures "$WORK/capture" | tee "$WORK/verdict.txt"; then
    :
fi
grep -q "no argument clamp" "$LOG" 2>/dev/null && echo "hint: the helpers are not in the exe — tools/build.sh fast builds them in"
