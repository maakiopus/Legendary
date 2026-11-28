#!/usr/bin/env bash
# Soft launcher for playing under Proton.
#
#   ./run.sh <game.exe> [victory run arguments]
#
# Best effort, never fatal:
#   - builds the diag victory.exe with tools/build.sh fast (RUN_BUILD_GAME=0
#     skips);
#   - builds and installs the patched GE-Proton package with makepkg when it
#     is not installed and sudo is passwordless (RUN_BUILD_WINE=0 skips);
#   - builds linux/thp/victory-thp-collapse and gives it cap_sys_nice when it
#     can (VICTORY_THP=0 leaves huge pages off);
#   - sets the play defaults below unless the environment already has them,
#     then execs tools/linux-run.sh.
#
# Play defaults: warn-level logging, 0.667 render scale, joined card rehash
# runs, card steps on the card and joined copies, plus the memory-path set
# measured on a fixed save (157.7 -> 135.7 ms a frame at 660 s, p99 391 ->
# 208; a 480 s arm of the same set: 138.0): the relay's fetches on the copy
# queue, refreshes that skip unchanged bytes, the title's copies and stores
# written beside the guards, 1024 lists in the air, texture refreshes that do
# not wait for the remap lock, and card buffers from a 64 KiB floor to a
# 1536 MB cap. VICTORY_INPLACE is deliberately not set: it regressed in
# testing. RUN_BUILD_WINE=0 and RUN_BUILD_GAME=0 skip the package and exe
# builds.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")" && pwd)
DRY=0
for arg in "$@"; do
    case $arg in
    --dry-run | -h | --help) DRY=1 ;;
    esac
done

PKG=proton-ge-git
PKGDIR=$ROOT/linux/pkgbuild/wine-proton-ge
INSTALLED=/usr/share/steam/compatibilitytools.d/$PKG

# Replacing victory.exe under an active run makes umu report
# "Executable not found"; do not build anything while one is running.
ACTIVE=$(pgrep -f 'victory[^ ]*\.exe run ' 2>/dev/null || true)

if [ "$DRY" = 0 ] && [ -z "$ACTIVE" ] && [ "${RUN_BUILD_WINE:-1}" = 1 ] &&
    command -v makepkg >/dev/null 2>&1 && command -v pacman >/dev/null 2>&1 &&
    ! pacman -Q "$PKG" >/dev/null 2>&1 && [ ! -d "$INSTALLED" ]; then
    if sudo -n true 2>/dev/null; then
        echo "run: building and installing $PKG (slow; RUN_BUILD_WINE=0 skips)" >&2
        (cd "$PKGDIR" && makepkg -si --noconfirm) ||
            echo "run: makepkg failed; the installed Proton is used" >&2
    else
        echo "run: $PKG is not installed and sudo is not passwordless;" >&2
        echo "     build it by hand: cd '$PKGDIR' && makepkg -si" >&2
    fi
fi

if [ -z "${PROTONPATH:-}" ]; then
    GEVT=$HOME/.local/share/Steam/steamapps/common/GE-Proton11-14-vt
    if [ -d "$INSTALLED" ]; then
        export PROTONPATH=$INSTALLED
    elif [ -d "$GEVT" ]; then
        export PROTONPATH=$GEVT
    fi
fi

if [ "$DRY" = 0 ] && [ "${RUN_BUILD_GAME:-1}" = 1 ]; then
    if [ -n "$ACTIVE" ]; then
        echo "run: a victory run is active; not rebuilding the exe" >&2
    else
        echo "run: building the diag victory.exe (RUN_BUILD_GAME=0 skips)" >&2
        "$ROOT/tools/build.sh" fast ||
            echo "run: build failed; the existing exe is used" >&2
    fi
fi

HELPER=$ROOT/linux/thp/victory-thp-collapse
SOURCE=$ROOT/linux/thp/victory-thp-collapse.c
if [ "$DRY" = 0 ] && [ -f "$SOURCE" ] && [ "${VICTORY_THP:-1}" = 1 ]; then
    if [ ! -x "$HELPER" ] || [ "$SOURCE" -nt "$HELPER" ]; then
        cc -O2 -o "$HELPER" "$SOURCE" || echo "run: could not build $HELPER" >&2
    fi
    if [ -x "$HELPER" ] && command -v getcap >/dev/null 2>&1 &&
        ! getcap "$HELPER" 2>/dev/null | grep -q cap_sys_nice; then
        sudo -n setcap cap_sys_nice+ep "$HELPER" 2>/dev/null ||
            echo "run: setcap failed; huge pages stay off" >&2
    fi
fi

export VICTORY_LOG=${VICTORY_LOG:-warn}
export VICTORY_RENDER_SCALE=${VICTORY_RENDER_SCALE:-0.667}
export VICTORY_CARD_REHASH_RUNS=${VICTORY_CARD_REHASH_RUNS:-1}
export VICTORY_CARD_STEPS=${VICTORY_CARD_STEPS:-2}
export VICTORY_JOIN_COPIES=${VICTORY_JOIN_COPIES:-1}
export VICTORY_RELAY_COPY=${VICTORY_RELAY_COPY:-1}
export VICTORY_COPY_COMPARE=${VICTORY_COPY_COMPARE:-1}
export VICTORY_COPY_BESIDE=${VICTORY_COPY_BESIDE:-1}
export VICTORY_STORE_BESIDE=${VICTORY_STORE_BESIDE:-1}
export VICTORY_LATE_WAIT_LISTS=${VICTORY_LATE_WAIT_LISTS:-1024}
if [ -z "${VICTORY_CARD_BUFFERS_MIN_MB:-}" ]; then
    export VICTORY_CARD_BUFFERS_MIN_KB=${VICTORY_CARD_BUFFERS_MIN_KB:-64}
fi
export VICTORY_CARD_BUFFERS_MB=${VICTORY_CARD_BUFFERS_MB:-1536}
export VICTORY_TEXTURE_NO_WAIT=${VICTORY_TEXTURE_NO_WAIT:-1}
exec "$ROOT/tools/linux-run.sh" "$@"
