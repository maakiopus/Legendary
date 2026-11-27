#!/usr/bin/env bash
# Run this fork's Windows build through one Proton under umu-run.
#
# Unlike a `.bat` through cmd.exe this starts the layer itself: no console
# window that stays empty while the log goes somewhere else. The log is
# written by the layer to VICTORY_LOG_FILE (a file under VICTORY_LOG_DIR)
# and tailed into this terminal. MangoHud shows the clock, the frame rate,
# and the processor's and the card's use, temperature and power in the
# corner: gamescope `--mangoapp` here, or the MangoHud Vulkan layer with
# VICTORY_GAMESCOPE=0.
#
#   tools/linux-run.sh /media/SSD/GTA6.Xbox.v1.0.2.1-CYBERLEEK/game_scarlett_master.exe
#   tools/linux-run.sh --dry-run <game>     print command and environment
#   tools/linux-run.sh --bat <game>         write a .bat for cmd.exe users
#   tools/linux-run.sh --print-bat <game>   the same, and print the file
#
# Environment:
#   VICTORY_EXE       the diag or release victory.exe (default: target/.../diag)
#   VICTORY_CONSOLE   --console value (default: series-x)
#   VICTORY_LOG_DIR   where run logs go (default: ~/victory-logs)
#   VICTORY_LOG       the layer's filter (default: info)
#   VICTORY_HUD       0 off, 1 compact (default), 2 full
#   VICTORY_GAMESCOPE 0 runs umu-run directly, MangoHud's Vulkan layer draws
#   VICTORY_BAT       where --bat writes (default: beside the log)
#   VICTORY_THP       0 leaves the huge-page helper off (default: on)
#   PROTONPATH        a Proton install (default: Proton - Experimental)
#   VICTORY_KEYS      scripted pad presses (diagnostics builds), as the layer
#                     documents them
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
EXE=${VICTORY_EXE:-$ROOT/target/x86_64-pc-windows-msvc/diag/victory.exe}
CONSOLE=${VICTORY_CONSOLE:-series-x}
PROTON=${PROTONPATH:-$HOME/.local/share/Steam/steamapps/common/Proton - Experimental}
LOG_DIR=${VICTORY_LOG_DIR:-$HOME/victory-logs}
LINK=$HOME/.local/share/Steam/steamapps/common/victory-run
LIBPATHS=${STEAM_COMPAT_LIBRARY_PATHS:-$HOME/.local/share/Steam/steamapps}

usage() {
    cat <<EOF
usage: $0 [--dry-run] [--bat|--print-bat] <game executable> [victory run arguments]

  --dry-run     print the command and environment, start and write nothing
  --bat         write a .bat that runs the layer under cmd.exe (a useful window)
  --print-bat   the same, and print the file
EOF
}

MODE=run
DRY=0
PRINT=0
GAME=
while [ $# -gt 0 ]; do
    case $1 in
    --dry-run) DRY=1; shift ;;
    --bat) MODE=bat; shift ;;
    --print-bat) MODE=bat; PRINT=1; shift ;;
    -h | --help) usage; exit 0 ;;
    --)
        shift
        GAME=${1:-}
        if [ $# -gt 0 ]; then shift; fi
        break
        ;;
    -*)
        echo "unknown option: $1" >&2
        usage >&2
        exit 2
        ;;
    *)
        GAME=$1
        shift
        break
        ;;
    esac
done
GAME=${GAME:-${VICTORY_GAME:-}}
if [ -z "$GAME" ]; then
    usage >&2
    exit 2
fi
RUN_ARGS=("$@")

# The Wine drive umu-run exposes: / -> Z:.
wine() {
    local path=$1
    if [[ $path = /* ]]; then
        printf 'Z:%s' "${path//\//\\}"
    else
        printf '%s' "$path"
    fi
}

# dirname for both separators: `dirname Z:\a\b.exe` is only ".".
game_dir() {
    local dir
    dir=${1%/*}
    if [ "$dir" = "$1" ]; then
        dir=${1%\\*}
    fi
    if [ "$dir" = "$1" ]; then
        dir=.
    fi
    printf '%s' "$dir"
}

STAMP=$(date +%Y%m%d-%H%M%S)
LOG=$LOG_DIR/run-$STAMP.txt
BAT=${VICTORY_BAT:-$LOG_DIR/run-$STAMP.bat}
ZEXE=$(wine "$EXE")
ZGAME=$(wine "$GAME")
ZLOG=$(wine "$LOG")
GAME_NAME=${GAME##*[\\/]}

# HUD levels. Every key is a MangoHud option
# (docs: /usr/share/doc/mangohud/MangoHud.conf.example). MANGOHUD_CONFIG
# replaces a host value and, because it does not set `read_cfg`, stops
# MangoHud reading the host's MangoHud.conf as well: these keys are the
# whole overlay, whatever the host had.
HUD_BASE="read_cfg=0,time=1,fps,frametime,cpu_stats=1,gpu_stats=1,cpu_temp=1,gpu_temp=1,cpu_power=1,gpu_power=1,core_load=1,position=top-left,font_scale=0.7,background_alpha=0.35,no_display=0"
case ${VICTORY_HUD:-1} in
0) HUD=0; HUD_LABEL=off; HUD_CFG= ;;
1) HUD=1; HUD_LABEL=compact; HUD_CFG="$HUD_BASE,hud_compact" ;;
2) HUD=2; HUD_LABEL=full; HUD_CFG="$HUD_BASE,hud_compact=0,gpu_name,vram,ram,cpu_mhz,gpu_core_clock,gpu_mem_clock,frame_timing,frame_count" ;;
*)
    echo "VICTORY_HUD must be 0 (off), 1 (compact) or 2 (full): ${VICTORY_HUD}" >&2
    exit 2
    ;;
esac

GAMESCOPE=${VICTORY_GAMESCOPE:-1}
case $GAMESCOPE in
0 | 1) ;;
*)
    echo "VICTORY_GAMESCOPE must be 0 or 1: ${VICTORY_GAMESCOPE}" >&2
    exit 2
    ;;
esac
if [ "$GAMESCOPE" = 1 ] && ! command -v gamescope >/dev/null 2>&1; then
    echo "linux-run: gamescope not found; running under umu-run directly (MangoHud layer)" >&2
    GAMESCOPE=0
fi
if [ "$HUD" != 0 ] && [ "$GAMESCOPE" = 0 ] && ! command -v mangohud >/dev/null 2>&1; then
    echo "linux-run: mangohud not installed; no overlay (install mangohud or VICTORY_HUD=0)" >&2
fi

# gamescope's mangoapp draws the overlay out of process and reads
# MANGOHUD_CONFIG; MANGOHUD=1 as well would load the Vulkan layer into the
# game and draw a second one. Without gamescope the layer is the only path.
if [ "$HUD" = 0 ]; then
    unset MANGOHUD MANGOHUD_CONFIG
elif [ "$GAMESCOPE" = 1 ]; then
    unset MANGOHUD
    export MANGOHUD_CONFIG=$HUD_CFG
else
    export MANGOHUD=1
    export MANGOHUD_CONFIG=$HUD_CFG
fi

export STEAM_COMPAT_INSTALL_PATH=$LINK
export STEAM_COMPAT_LIBRARY_PATHS=$LIBPATHS
export PROTONPATH=$PROTON
export VICTORY_LOG=${VICTORY_LOG:-info}
export VICTORY_LOG_FILE=$ZLOG
if [ -z "${VICTORY_GAME_DIR:-}" ]; then
    VICTORY_GAME_DIR=$(wine "$(game_dir "$GAME")")
fi
export VICTORY_GAME_DIR

GS=(gamescope -W 1920 -H 1080 -r 75 -f)
if [ "$HUD" != 0 ]; then
    GS+=(--mangoapp)
fi
GS+=(--force-grab-cursor --)
UMU=(umu-run "$ZEXE" run "$ZGAME" --execute --console "$CONSOLE" "${RUN_ARGS[@]}")
if [ "$GAMESCOPE" = 1 ]; then
    CMD=("${GS[@]}" "${UMU[@]}")
else
    CMD=("${UMU[@]}")
fi

quote() {
    if [ -z "$1" ]; then
        printf "''"
    elif [[ $1 =~ ^[A-Za-z0-9_./:=+,-]*$ ]]; then
        printf '%s' "$1"
    elif [[ $1 == *"'"* ]]; then
        printf '%q' "$1"
    else
        printf "'%s'" "$1"
    fi
}
print_cmd() {
    local arg
    for arg in "$@"; do
        if [[ $arg == [A-Za-z_]*=* ]]; then
            printf '%s=%s ' "${arg%%=*}" "$(quote "${arg#*=}")"
        else
            printf '%s ' "$(quote "$arg")"
        fi
    done
    printf '\n'
}
pct() { printf '%s' "${1//%/%%}"; }

# Batch file for users who insist on cmd.exe. It runs the layer directly and
# redirects nothing, so the layer's stderr (logging.rs always keeps a stderr
# layer, VICTORY_LOG_FILE only adds the file) has the console to itself. The
# title and the banner carry the log path even if Wine still keeps the layer's
# stderr out of that window.
BAT_ENV=()
while IFS= read -r name; do
    case $name in
    VICTORY_BAT | VICTORY_CONSOLE | VICTORY_EXE | VICTORY_GAME | VICTORY_GAME_DIR | \
        VICTORY_GAMESCOPE | VICTORY_HUD | VICTORY_LOG | VICTORY_LOG_DIR | \
        VICTORY_LOG_FILE | VICTORY_THP) continue ;;
    esac
    BAT_ENV+=("$name")
done < <(compgen -e | grep '^VICTORY_' | sort)

BAT_RUN="\"$(pct "$ZEXE")\" run \"$(pct "$ZGAME")\" --execute --console \"$(pct "$CONSOLE")\""
for arg in "${RUN_ARGS[@]}"; do
    BAT_RUN+=" \"$(pct "$arg")\""
done

render_bat() {
    local name
    printf '@echo off\n'
    printf 'setlocal\n'
    printf 'title VIctory - %s - log: %s\n' "$GAME_NAME" "$ZLOG"
    printf 'echo VIctory: %s\n' "$GAME_NAME"
    printf 'echo Log: %s\n' "$ZLOG"
    printf 'echo Linux: tail -f "%s"\n' "$LOG"
    printf 'echo If this window stays empty, Wine keeps the layer stderr out of it; the log above has every line.\n'
    printf 'set "VICTORY_LOG_FILE=%s"\n' "$ZLOG"
    printf 'set "VICTORY_LOG=%s"\n' "$(pct "$VICTORY_LOG")"
    printf 'set "VICTORY_GAME_DIR=%s"\n' "$(pct "$VICTORY_GAME_DIR")"
    for name in "${BAT_ENV[@]}"; do
        printf 'set "%s=%s"\n' "$name" "$(pct "${!name}")"
    done
    printf 'echo.\n'
    printf '%s\n' "$BAT_RUN"
    printf 'set "VICTORY_RC=%%ERRORLEVEL%%"\n'
    printf 'echo.\n'
    printf 'echo VIctory exited: %%VICTORY_RC%% (log: %s)\n' "$ZLOG"
    printf 'exit /b %%VICTORY_RC%%\n'
}

write_bat() {
    mkdir -p "$(dirname "$1")"
    render_bat | sed 's/$/\r/' >"$1"
}

# The cmd.exe command for --bat: same HUD and Proton on the command line, so
# the printed line is all a user needs. The batch path goes bare: Wine quotes
# it for cmd.exe when it contains spaces.
BATWIN=$(wine "$BAT")
INNER=(umu-run 'C:\windows\system32\cmd.exe' /c "$BATWIN")
BAT_PREFIX=()
if [ "$HUD" = 0 ]; then
    BAT_PREFIX+=(MANGOHUD=0)
elif [ "$GAMESCOPE" = 0 ]; then
    BAT_PREFIX+=(MANGOHUD=1 "MANGOHUD_CONFIG=$HUD_CFG")
else
    BAT_PREFIX+=(MANGOHUD=0 "MANGOHUD_CONFIG=$HUD_CFG")
fi
BAT_PREFIX+=("PROTONPATH=$PROTON" "STEAM_COMPAT_INSTALL_PATH=$LINK" "STEAM_COMPAT_LIBRARY_PATHS=$LIBPATHS")
if [ "$GAMESCOPE" = 1 ]; then
    OUTER=("${GS[@]}" "${INNER[@]}")
else
    OUTER=("${INNER[@]}")
fi

# Both paths need the install-path symlink umu-run maps the prefix through.
if [ "$DRY" = 0 ]; then
    mkdir -p "$LOG_DIR" "$(dirname "$LINK")"
    ln -sfn "$(dirname "$EXE")" "$LINK"
fi

if [ "$MODE" = bat ]; then
    if [ "$DRY" = 1 ]; then
        echo "# dry run: nothing started, nothing written"
        echo "# bat: $BAT"
        echo "# logs: $LOG"
        echo
        render_bat
        echo
        echo "# run this:"
        print_cmd "${BAT_PREFIX[@]}" "${OUTER[@]}"
        exit 0
    fi
    write_bat "$BAT"
    echo "bat: $BAT"
    echo "logs: $LOG"
    if [ "$PRINT" = 1 ]; then
        echo
        sed 's/\r$//' "$BAT"
        echo
    fi
    echo "run this:"
    print_cmd "${BAT_PREFIX[@]}" "${OUTER[@]}"
    if [ "${VICTORY_THP:-1}" = 1 ] && [ -x "$ROOT/linux/thp/victory-thp-collapse" ]; then
        echo "note: the bat path has no 15 s THP loop; the direct path does (linux/thp/README.md)" >&2
    fi
    exit 0
fi

if [ "$DRY" = 1 ]; then
    echo "# dry run: nothing started, nothing written"
    echo "# logs: $LOG"
    echo "# hud: $HUD_LABEL (VICTORY_HUD=$HUD)"
    echo "# the install-path symlink and the log directory are made on a real run"
    printf 'export VICTORY_LOG_FILE=%s\n' "$(quote "$VICTORY_LOG_FILE")"
    printf 'export VICTORY_LOG=%s\n' "$(quote "$VICTORY_LOG")"
    printf 'export VICTORY_GAME_DIR=%s\n' "$(quote "$VICTORY_GAME_DIR")"
    printf 'export PROTONPATH=%s\n' "$(quote "$PROTON")"
    printf 'export STEAM_COMPAT_INSTALL_PATH=%s\n' "$(quote "$LINK")"
    printf 'export STEAM_COMPAT_LIBRARY_PATHS=%s\n' "$(quote "$LIBPATHS")"
    if [ "$HUD" = 0 ]; then
        printf 'unset MANGOHUD MANGOHUD_CONFIG\n'
    elif [ "$GAMESCOPE" = 1 ]; then
        printf 'unset MANGOHUD\n'
        printf 'export MANGOHUD_CONFIG=%s\n' "$(quote "$HUD_CFG")"
    else
        printf 'export MANGOHUD=1\n'
        printf 'export MANGOHUD_CONFIG=%s\n' "$(quote "$HUD_CFG")"
    fi
    print_cmd "${CMD[@]}"
    exit 0
fi

echo "logs: $LOG"
echo "follow: tail -f $(quote "$LOG")"
echo "exe: $EXE"
echo "game: $GAME console=$CONSOLE proton=$PROTON"
echo "hud: $HUD_LABEL"
{
    echo "exe=$EXE"
    echo "game=$GAME console=$CONSOLE proton=$PROTON"
    echo "log=$LOG"
    echo "hud=$HUD_LABEL"
} >"$LOG.run"

# Renderer port 1's heap is a userptr under vkd3d-proton, and amdgpu walks
# every 4 KiB page of it on each submission. Once port 1's private pages are
# in the log, collapse them into 2 MiB pages every 15 s with the helper
# (`linux/thp/`); nothing else about the run changes. `VICTORY_THP=0`: not.
thp() {
    local helper=$ROOT/linux/thp/victory-thp-collapse
    [ "${VICTORY_THP:-1}" = 1 ] || return 0
    [ -x "$helper" ] || return 0
    while sleep 15; do
        [ -n "$(pgrep -f 'victory[^ ]*\.exe run' || true)" ] || return 0
        local pid ranges
        pid=$(for p in $(pgrep -f 'victory[^ ]*\.exe run' || true); do
                  echo "$(ls /proc/"$p"/task 2>/dev/null | wc -l) $p"
              done | sort -rn | awk 'NR==1{print $2}')
        [ -n "$pid" ] || continue
        ranges=$(grep -aoE "32-bit heap are private memory.*at=0x[0-9a-f]+ len=[0-9]+" "$LOG" 2>/dev/null \
            | sed -E 's/.*at=0x([0-9a-f]+) len=([0-9]+)/\1 \2/' | sort -u \
            | while read -r a l; do printf "%x %x " $((16#$a)) $((16#$a + l)); done)
        [ -n "$ranges" ] || continue
        "$helper" "$pid" $ranges >/dev/null 2>&1
    done
}

# Show the layer's log in this terminal as it is written: under Proton the
# cmd.exe console is a separate Wine window and does not carry the layer's
# stdout, which is why a .bat launch left an empty console.
tail -n +1 -F "$LOG" 2>/dev/null &
tail_pid=$!

"${CMD[@]}" &
game=$!
thp &
thp_pid=$!
trap 'kill "$thp_pid" "$tail_pid" 2>/dev/null' EXIT INT TERM
wait "$game"
status=$?
kill "$thp_pid" "$tail_pid" 2>/dev/null
exit "$status"
