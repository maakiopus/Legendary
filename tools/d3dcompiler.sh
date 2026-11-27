# The helper shaders of ours (card compare, up check, argument clamps) only
# DXC compiles: Wine's d3dcompiler_47 (vkd3d-shader) defines no Load4, Store
# or Interlocked* method on byte-address buffers, so their HLSL fails there
# with E5005, and without the clamp the world's indirect records drop (a
# black world) or reach the GPU with the counts the clamp would have cut (a
# device removed). Two ways out:
#
# - a build with DXC carries the helpers as DXIL (`VICTORY_BUILD_DXC`, or
#   `dxc` on the path; the exe says so: it has the bytes
#   `victory-dxil-bundled` in it), and
# - otherwise a native Microsoft d3dcompiler_47 beside the exe answers the
#   run-time compile: Wine prefers a native dll under the override, and
#   Microsoft's compiler has no trouble with the intrinsics. The file is
#   fetched once from Microsoft's standalone SDK installers, as winetricks
#   fetches it, and kept in ~/.cache/victory-tools.

# The cab that holds the 64-bit d3dcompiler_47.dll (winetricks' own source).
CAB=https://download.microsoft.com/download/B/0/C/B0C80BA3-8AD6-4958-810B-6882485230B5/standalonesdk/Installers/61d57a7a82309cd161a854a6f4619e52.cab
FIL=fil3585cb2ea5db13cc0838f8d06b5c9679

# Whether the exe carries the helpers as DXIL (a build with DXC).
dxil_bundled() {
    local exe=$1
    [ -f "$exe" ] && grep -aq "victory-dxil-bundled" "$exe" 2>/dev/null
}

# Stages the native dll beside `exe` (`VICTORY_NATIVE_D3DCOMPILER=0`: off).
ensure_d3dcompiler_47() {
    local exe=$1 dir cache dll cab
    if [ "${VICTORY_NATIVE_D3DCOMPILER:-1}" != 1 ]; then
        return 1
    fi
    dir=$(dirname "$exe")
    if [ -f "$dir/d3dcompiler_47.dll" ]; then
        echo "d3dcompiler_47: the native one is beside the exe"
        return 0
    fi
    cache=$HOME/.cache/victory-tools
    mkdir -p "$cache"
    dll=$cache/d3dcompiler_47.dll
    if [ ! -s "$dll" ]; then
        cab=$cache/61d57a7a82309cd161a854a6f4619e52.cab
        if [ ! -s "$cab" ]; then
            curl -fsSL --max-time 300 -o "$cab" "$CAB" || {
                echo "d3dcompiler_47: the download failed; the helpers stay uncompiled" >&2
                return 1
            }
        fi
        if command -v cabextract >/dev/null 2>&1; then
            cabextract -q -F "$FIL" "$cab" -d "$cache" >/dev/null 2>&1 &&
                mv "$cache/$FIL" "$dll"
        elif command -v 7z >/dev/null 2>&1; then
            7z e -y -so "$cab" "$FIL" > "$dll" 2>/dev/null
        else
            echo "d3dcompiler_47: no cabextract or 7z to unpack it; the helpers stay uncompiled" >&2
            return 1
        fi
    fi
    if [ ! -s "$dll" ]; then
        echo "d3dcompiler_47: the unpack failed; the helpers stay uncompiled" >&2
        return 1
    fi
    cp "$dll" "$dir/d3dcompiler_47.dll" &&
        echo "d3dcompiler_47: the native one staged beside the exe"
}

# Whether a build without the bundled helpers has a native compiler staged
# beside its exe (the run scripts say so, and set the override).
d3dcompiler_native() {
    [ -f "$(dirname "$1")/d3dcompiler_47.dll" ]
}
