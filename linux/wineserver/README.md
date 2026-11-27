# wineserver view tree

`wineserver-view-tree.patch` keeps a process's mapped views in a red-black tree
keyed by `(process, base)` beside the per-process list Wine walks on every map
and unmap. The stock server scans that list; the tree turns the scan into a
lookup. If two views share a base the code falls back to the list, so the
patch never changes behaviour it cannot represent.

Source: `kroeeee-eeee/VIctory-Linux-Proton-Wine-7900XT-XTX` commit
`47fd862c0da9d5b60490ba47c64dd378822119b0` (the view-tree part of
`a14cfcbf`). Measured there on a 7900 XTX: 8.4 -> 17.0-18.5 fps, 119-122 ->
54-59 ms/frame, write watch 17-26 -> 4.6-4.7 ms, launch-to-world 112-120 ->
43-60 s.

The patch touches `server/mapping.c`, so the server must be built from the
Wine source of the Proton in use: the client/server protocol differs between
Proton builds even when `wine --version` matches. Proton Experimental is
protocol 931, GE-Proton11-14 is 938. A server built from plain Wine 11.0 is
rejected.

## Packaging

The package is `linux/pkgbuild/proton-ge-git/`: the AUR source package plus
this patch. It builds the full Proton-GE tree with podman and applies the
patch in `prepare()` after GE's own patch series, so GE's patch list and
checksums stay in one place. The patch is a listed source with its checksum;
the symlink points at this directory so there is one copy.

The full `makepkg` has not been run end to end yet. The wineserver half was
built and tested from the same recipe: 5,385,968 bytes, sha256
`b27f7c98...f08247`, with `view_tree` symbols present in `nm`. A `-bin`
variant that only rebuilds the server was tried and dropped: it duplicated
GE's 44 KB prep script and pinned the package to GE-Proton11-14.

## Building the server by hand

For a GE-Proton11-14 tree:

1. Unpack `ValveSoftware/wine` `a47c73f52fd7b2efafc1c560710f3a5099ae51e0`.
2. Apply the Wine block of GE's `patches/protonprep-valve-staging.sh`
   (wine-staging 11.0 plus GE's patches; the full recipe is in
   `~/.cache/wineserver-build/REPORT.md` on the build host).
3. `patch -Np1 --batch -d wine -i wineserver-view-tree.patch`.
4. `autoreconf -f` and `./tools/make_requests`.
5. Configure with `--enable-win64 --disable-tests` and the `--without-*`
   list used by the package, then `make server/wineserver`.
6. Copy the binary over `files/bin/wineserver` in a **copy** of the Proton
   install and point `PROTONPATH` at that copy.

Do not replace the server in a shared Steam install; keep a separate
`GE-Proton11-14-vt` directory so an A/B run is one environment variable.

## Huge pages

`linux/thp/victory-thp-collapse` is a separate runtime helper. It collapses
port 1's 32-bit heap into 2 MiB pages and does not depend on this patch; the
launcher runs it every 15 s. The view-tree server and the huge-page helper can
be used together.
