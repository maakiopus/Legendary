# The helper shaders of ours (card compare, up check, argument clamps) only
# DXC compiles: Wine's d3dcompiler_47 (vkd3d-shader) defines no Load4, Store
# or Interlocked* method on byte-address buffers, so their HLSL fails there
# with E5005, and without the clamp the world's indirect records drop (a
# black world) or reach the GPU with the counts the clamp would have cut (a
# device removed). Two ways out:
#
# - a build from a tree with the DXC bundling step (commit b52f4e3) carries
#   the helpers as DXIL (`VICTORY_BUILD_DXC`, or `dxc` on the path; the exe
#   says so: it has the bytes `victory-dxil-bundled` in it). This tree
#   excludes that step, so its builds take the second way, and
# - a native Microsoft d3dcompiler_47 beside the exe answers the run-time
#   compile: Wine prefers a native dll under the override, and a modern
#   Microsoft compiler has no trouble with the intrinsics. The 2013
#   Windows 8 build (winetricks' standalone-SDK cab) does not support
#   shader model 5.1: it rejects `cs_5_1` with X3506, the helpers fall
#   back, and the atlas host copies go stale (the ghost tiles). Never
#   stage that build; put a modern dll in ~/.cache/victory-tools or beside
#   the exe.

# SHA-256 of the 2013 Windows 8 cab's d3dcompiler_47.dll (the broken one).
BAD_D3DCOMPILER_SHA256=e994847e01a6f1e4cbdc5a864616ac262f67ee4f14db194984661a8d927ab7f4

# Whether `file` is the 2013 build that cannot compile cs_5_1.
d3dcompiler_bad() {
    local file=$1 sum
    [ -f "$file" ] || return 1
    sum=$(sha256sum "$file" 2>/dev/null | cut -d' ' -f1)
    [ "$sum" = "$BAD_D3DCOMPILER_SHA256" ]
}

# Whether the exe carries the helpers as DXIL (a build with DXC).
dxil_bundled() {
    local exe=$1
    [ -f "$exe" ] && grep -aq "victory-dxil-bundled" "$exe" 2>/dev/null
}

# Stages the native dll beside `exe` (`VICTORY_NATIVE_D3DCOMPILER=0`: off).
ensure_d3dcompiler_47() {
    local exe=$1 dir cache dll cached
    if [ "${VICTORY_NATIVE_D3DCOMPILER:-1}" != 1 ]; then
        return 1
    fi
    dir=$(dirname "$exe")
    dll=$dir/d3dcompiler_47.dll
    if [ -f "$dll" ] && ! d3dcompiler_bad "$dll"; then
        echo "d3dcompiler_47: the native one is beside the exe"
        return 0
    fi
    if [ -f "$dll" ]; then
        echo "d3dcompiler_47: the one beside the exe is the 2013 Windows 8 build" >&2
        echo "  it rejects cs_5_1 (X3506): the helpers stay uncompiled (ghost tiles)" >&2
    fi
    cache=$HOME/.cache/victory-tools
    mkdir -p "$cache"
    cached=$cache/d3dcompiler_47.dll
    if d3dcompiler_bad "$cached"; then
        echo "d3dcompiler_47: the cached dll is the 2013 Windows 8 build (no cs_5_1)" >&2
        echo "  replace $cached with a modern d3dcompiler_47.dll" >&2
        return 1
    fi
    if [ ! -s "$cached" ]; then
        echo "d3dcompiler_47: no modern dll in $cache" >&2
        echo "  copy a modern d3dcompiler_47.dll there (Windows 10+ or SDK redist);" >&2
        echo "  the 2013 Windows 8 cab predates shader model 5.1 (X3506)" >&2
        return 1
    fi
    cp "$cached" "$dll" &&
        echo "d3dcompiler_47: the modern one staged beside the exe"
}

# Whether a build without the bundled helpers has a native compiler staged
# beside its exe (the run scripts say so, and set the override).
d3dcompiler_native() {
    local file
    file=$(dirname "$1")/d3dcompiler_47.dll
    [ -s "$file" ] && ! d3dcompiler_bad "$file"
}
