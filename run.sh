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
# runs, and VKD3D_CONFIG=no_upload_hvv so host-visible buffers go through
# ReBAR/SAM instead of the UPLOAD heap. VICTORY_INPLACE is deliberately not
# set: it regressed performance in testing. RUN_NO_SAM=1 leaves VKD3D_CONFIG
# alone; RUN_BUILD_WINE=0 skips the package build.
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

if [ "$DRY" = 0 ] && [ "${RUN_BUILD_WINE:-1}" = 1 ] &&
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
    echo "run: building the diag victory.exe (RUN_BUILD_GAME=0 skips)" >&2
    "$ROOT/tools/build.sh" fast ||
        echo "run: build failed; the existing exe is used" >&2
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
if [ "${RUN_NO_SAM:-0}" != 1 ]; then
    case ",${VKD3D_CONFIG:-}," in
    *,no_upload_hvv,*) ;;
    *) export VKD3D_CONFIG="${VKD3D_CONFIG:+$VKD3D_CONFIG,}no_upload_hvv" ;;
    esac
fi
exec "$ROOT/tools/linux-run.sh" "$@"
