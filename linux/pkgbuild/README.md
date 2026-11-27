# PKGBUILD

One package: `proton-ge-git/` is the AUR source package plus
`../wineserver/wineserver-view-tree.patch`. `prepare()` runs GE's own
`patches/protonprep-valve-staging.sh` and then applies the view-tree patch to
`wine/`, so GE's patch list and checksums are not duplicated here.

Build:

```
cd linux/pkgbuild/proton-ge-git
makepkg -si
```

It needs podman, network and disk for the full Proton-GE build. The patch is
applied after GE's prep; if it stops applying to a newer GE tree, `prepare()`
fails and names the patch. Keep that step, do not fall back to a prebuilt
server.

The installed tree is `Proton-GE-git`. The view-tree server protocol must
match its Wine client; if GE bumps the protocol, rebuild from the same tree.

The git source is unpinned (`SKIP` in `b2sums`), so `makepkg` builds GE
master, not necessarily `GE.Proton11.14`. The view-tree patch failing to
apply is the tripwire; a protocol bump without a patch conflict would not be,
so rebuild the server from the same checkout the client came from.
