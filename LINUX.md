# VIctory on Linux (Proton)

The Windows build runs under Wine/Proton. Renderer port 1
(`VICTORY_PORT_CONSTANTS=1`, off by default) has two Linux-side notes.

## Renderer port 1 under Wine

vkd3d-proton opens a heap from an address by giving the pages to the AMD driver
as a user pointer, and the driver takes anonymous memory only. On a view of
Wine's sections (shared, file-backed memory) the card read nothing and the
picture stayed black. Under Wine the 32-bit heap's fresh pages are therefore
private memory, and the heap is opened on the title's own pages.

`VICTORY_PORT_PRIVATE=0` keeps the views as before. Port 1 stays opt-in: neither
path runs with `VICTORY_PORT_CONSTANTS` unset.

## Huge pages for renderer port 1 (recommended)

Under vkd3d-proton the port-1 heap is a userptr, and amdgpu checks every page of
it on every submission. With 4 KiB pages that kept the `vkd3d_queue` thread at
100 % of a core; as 2 MiB pages the same memory is ~512 times fewer entries.
`linux/thp/victory-thp-collapse` collapses the heap into 2 MiB pages from
outside the process (`process_madvise(MADV_COLLAPSE)`, Linux 6.1+). On the
reference tree (RX 7900 XT, Ryzen 7 5800X3D, Proton Experimental 11):
**~9 -> ~18-19 fps in the same scene**.

It needs one capability, not root:

```sh
cc -O2 -o linux/thp/victory-thp-collapse linux/thp/victory-thp-collapse.c
sudo setcap cap_sys_nice+ep linux/thp/victory-thp-collapse
```

Run it by hand on the ranges the log names; the command is in
[`linux/thp/README.md`](linux/thp/README.md). This tree has no automatic start.
