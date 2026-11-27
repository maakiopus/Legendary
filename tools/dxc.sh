# The DXC that compiles the helper shaders of ours at build time in a tree
# with the bundling step (commit b52f4e3: `VICTORY_BUILD_DXC` in build.rs;
# `dxil.rs` carries them as DXIL in the exe when there was one). This tree
# excludes that commit, so find_dxc/fetch_dxc only matter if it is restored.
# Wine's d3dcompiler_47 (vkd3d-shader) refuses the byte-address intrinsics of
# the card compare, the up check and the argument clamps (E5005); without a
# bundled DXIL a native Microsoft dll beside the exe compiles them instead
# (`tools/d3dcompiler.sh` says the rest).

# Microsoft's release this fork's dxc is from: 1.9.2609. The Linux asset is
# what a build here runs; the Windows zip is the reference the release names.
DXC_LINUX=https://github.com/microsoft/DirectXShaderCompiler/releases/download/v1.9.2609/linux_dxc_2026_09_28.x86_x64.tar.gz
DXC_WINDOWS=https://github.com/microsoft/DirectXShaderCompiler/releases/download/v1.9.2609/dxc_2026_09_29.zip

# The exe's own note that the build carries the helpers
# (`crates/victory-gpu/src/dxil.rs` BUNDLED_MARKER).
dxil_bundled() {
    local exe=$1
    [ -f "$exe" ] && grep -aq "victory-dxil-bundled" "$exe" 2>/dev/null
}

# A DXC to build with: the environment's naming first, then the path, then
# the copies this fork has used (`~/.cache/dxc/bin/dxc`,
# `tools-cache/dxc/bin/dxc`).
find_dxc() {
    local c
    if [ -n "${VICTORY_BUILD_DXC:-}" ] && [ -x "$VICTORY_BUILD_DXC" ]; then
        printf '%s' "$VICTORY_BUILD_DXC"
        return 0
    fi
    for c in "$(command -v dxc 2>/dev/null)" \
        "$HOME/.cache/dxc/bin/dxc" \
        "$HOME/clone/victors/tools-cache/dxc/bin/dxc"; do
        if [ -n "$c" ] && [ -x "$c" ]; then
            printf '%s' "$c"
            return 0
        fi
    done
    return 1
}

# Downloads dxc 1.9.2609 into ~/.cache/dxc (the Linux release; the Windows
# zip is named in DXC_WINDOWS for a Windows host, which has no need of it).
fetch_dxc() {
    local cache=$HOME/.cache/dxc
    local tar=$cache/linux_dxc_2026_09_28.x86_x64.tar.gz
    if [ -x "$cache/bin/dxc" ]; then
        echo "dxc: already at $cache/bin/dxc"
        return 0
    fi
    mkdir -p "$cache"
    curl -fsSL --max-time 600 -o "$tar" "$DXC_LINUX" || {
        echo "dxc: the download failed ($DXC_LINUX)" >&2
        return 1
    }
    tar -xzf "$tar" -C "$cache" || {
        echo "dxc: the unpack failed" >&2
        return 1
    }
    [ -x "$cache/bin/dxc" ] && echo "dxc: fetched to $cache/bin/dxc"
}
