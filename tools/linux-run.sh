#!/usr/bin/env bash
# Run this fork's Windows build through one Proton under umu-run.
#
# Unlike a `.bat` through cmd.exe this starts the layer itself: no console
# window that stays empty while the log goes somewhere else. The log is
# written by the layer to VICTORY_LOG_FILE (a file under VICTORY_LOG_DIR,
# or the one given), and MangoHud shows the clock, the frame rate, and the
# processor's and the card's use in the corner of the game's window.
#
#   tools/linux-run.sh /media/SSD/GTA6.Xbox.v1.0.2.1-CYBERLEEK/game_scarlett_master.exe
#
# Environment:
#   VICTORY_EXE      the diag or release victory.exe (default: target/.../diag)
#   VICTORY_CONSOLE  --console value (default: series-x)
#   VICTORY_LOG_DIR  where run logs go (default: ~/victory-logs)
#   VICTORY_LOG      the layer's filter (default: info)
#   PROTONPATH       a Proton install (default: Proton - Experimental)
#   VICTORY_HUD      extra MangoHud settings, comma separated, appended
#   VICTORY_KEYS     scripted pad presses (diagnostics builds), as the layer
#                    documents them
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
EXE=${VICTORY_EXE:-$ROOT/target/x86_64-pc-windows-msvc/diag/victory.exe}
GAME=${1:-${VICTORY_GAME:-}}
if [ -z "$GAME" ]; then
    echo "usage: $0 <game executable> [more victory run arguments]" >&2
    exit 2
fi
shift
CONSOLE=${VICTORY_CONSOLE:-series-x}
PROTON=${PROTONPATH:-$HOME/.local/share/Steam/steamapps/common/Proton - Experimental}
LOG_DIR=${VICTORY_LOG_DIR:-$HOME/victory-logs}
mkdir -p "$LOG_DIR"
LOG=$LOG_DIR/run-$(date +%Y%m%d-%H%M%S).txt

# The Wine drive umu-run exposes: / -> Z:.
wine() {
    local path=$1
    if [[ $path = /* ]]; then
        printf 'Z:%s' "${path//\//\\}"
    else
        printf '%s' "$path"
    fi
}

link="$HOME/.local/share/Steam/steamapps/common/victory-run"
mkdir -p "$(dirname "$link")"
ln -sfn "$(dirname "$EXE")" "$link"
export STEAM_COMPAT_INSTALL_PATH="$link"
export STEAM_COMPAT_LIBRARY_PATHS=${STEAM_COMPAT_LIBRARY_PATHS:-$HOME/.local/share/Steam/steamapps}
export PROTONPATH="$PROTON"
# The layer writes its own log where a Proton run can read it (a console
# redirect catches nothing under Wine).
export VICTORY_LOG_FILE="$(wine "$LOG")"
export VICTORY_LOG=${VICTORY_LOG:-info}
if [ -z "${VICTORY_GAME_DIR:-}" ]; then
    export VICTORY_GAME_DIR="$(wine "$(dirname "$GAME")")"
fi

# The overlay: a clock, frame rate, and the processor's and card's use and
# temperature. MangoHud draws it into the game's window; there is no window
# manager title bar in a gamescope fullscreen run to put it in.
export MANGOHUD_CONFIG="time=1,cpu_stats=1,gpu_stats=1,cpu_temp=1,gpu_temp=1,core_load=1,cpu_power=1,gpu_power=1,fps,frametime,position=top-left,font_scale=.7,background_alpha=0.35,no_display=0"
if [ -n "${VICTORY_HUD:-}" ]; then
    export MANGOHUD_CONFIG="$MANGOHUD_CONFIG,$VICTORY_HUD"
fi

echo "exe=$EXE" >"$LOG.run"
echo "game=$GAME console=$CONSOLE proton=$PROTON" >>"$LOG.run"
echo "log=$LOG" >>"$LOG.run"

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

gamescope -W 1920 -H 1080 -r 75 -f --mangoapp --force-grab-cursor -- \
    umu-run "$(wine "$EXE")" run "$(wine "$GAME")" --execute --console "$CONSOLE" "$@" &
game=$!
thp &
thp_pid=$!
trap 'kill "$thp_pid" 2>/dev/null' EXIT INT TERM
wait "$game"
status=$?
kill "$thp_pid" 2>/dev/null
exit "$status"
