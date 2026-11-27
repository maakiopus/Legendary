# Ghosting on the character's body and shirt

> **Status, 27 Nov 2026:** the ghost is still present. Two GPU fixes were
> tried and did not remove it; all GPU changes are reverted to HEAD. The
> attempt is kept as `docs/ghost-notes/2026-11-27-alias-attempt.patch`.
> Treat this file as a working record, not a fix that is in the tree.

## Symptom

Frames captured from the Series X build (`sweep-ghost4`, frames
2290/2298/2306/2314) show rectangular blocks on the character's back and
shirt. The `alias-fix1` diagnostic capture shows the same tiles:
`/tmp/opencode/alias-fix1/capture/frame-2038.png`. The blocks move
with the character and contain what looks like scene content (sky, trees,
porch), not an older shirt. They are texture contents, not a compose or
present problem.

The tiles are the size of the decoration bake's 256x256 pages inside a
2048x2048 atlas.

## What the port does

- The title's decoration bake draws into a 256x256 corner of a 2048x2048
  target. `crates/victory-gpu/src/draw.rs` recognizes four bake pixel
  shaders (`BAKE_PIXEL_SHADERS`) and gives the texture pass a larger budget
  before them.
- The result reaches the atlas by `CopyTextureRegion`
  (`crates/victory-gpu/src/list.rs`), which marks the destination
  `texture::host_written` and `descriptor::host::gpu_written`.
- The atlas is sampled through a *placed* texture view: a separate host
  resource made by `resource::texture_at` over the atlas's title address.
  That is a second host copy, and the only bridge from the written atlas to
  it is `resource::alias_of` -> `texture::alias`.
- A slot read through a table is removed from the texture queue by
  `texture_slot_tabled`, so the scheduled texture look never rechecks it.
  Its freshness depends entirely on the draw path's alias/upload choice.

## What was tried

### 1. Relax `fills` in `alias_of` (did not fix)

`alias_of` rejected placed views before looking for a source when
`flags & 7 != 0` (`fills`). Bake/atlas views carry those target/UAV flags.
The change allowed a host-written address to bypass `fills`, plus a
`VICTORY_ALIAS_WHY=1` diagnostic and a per-source write epoch.

Run `alias-fix1` (`run-20261127-134344.txt`, 4.3 MB, VT):

- 59 alias copies, all early `written_as=42 read_as=67` (R32_UINT ->
  R9G9B9E5 light probes). No BC3/BC7 atlas pair.
- 397 `a placed texture is not read from a host-written texture
  reason="no target or host-written texture at its address"`.
- 3 `reason="not a placed texture of one shape"`:
  1024x1024 f65 `@0xf188f0000`, 2048x2048 f65 `@0xf184f0000`,
  3840x2160 f28 `@0xf13cc0000`.
- 0 `HERO LEAD`; 0 mismatched-shape warnings.
- Every 5 s line: `0 shown from a texture the host wrote`.
- No 2048x2048 BC3 (f77/f78) miss. The atlas is probably *not* the placed
  view being rejected; it is sampled through the title resource itself.
- Miss shapes include 512x468 f77, 64x64 f77, 512x512 m8 f71, 256x256 f98,
  960x128 f98, 128x72 m7 f41, and 17x17x17 f10.

So `fills` was not the cause. The failures are address/source lookups:
`texture::host_written_at(address)` has no entry covering the placed view's
address.

### 2. Shape bridge, robust address lookup, P1 guard (did not boot)

Added:

- `HOST_WRITTEN_DESC`, keyed by the resource description with filler words
  1 and 13 zeroed; `host_written_like(desc)` returns a source only when
  exactly one live host-written resource has that shape.
- `alias_of` falls back to the shape bridge; `host_written_here` checks it
  too.
- `host_written_at` scans all starts at or below the address and returns
  the first covering interval, instead of only the nearest start.
- `host_written` records the address whenever it becomes known, not only
  on the first call.
- `readable` skips `upload_from` when the address is host-written (P1).
- Diagnostics: `placed texture made`, `host write recorded`,
  `no host-written texture covers a placed view's address`.

The build passed, but the validation run (`/tmp/opencode/alias-desc`)
produced no app log: Wine reported
`WARNING: Executable not found: Z:\home\maaki\.cache\main-target\...\victory.exe`
and exited 2. The exe was being rebuilt at that moment, so this was a
launch race, not proof that the changes broke boot. A later run
(`alias-desc2`) was killed. The GPU changes were reverted anyway, because
they had not fixed the ghost and were unvalidated; the game now runs the
known-good HEAD binary again.

## Best explanation so far

Two different address spaces meet in `HOST_WRITTEN_AT`:

- A copy or UAV write marks the resource the port copied *into*. For a
  committed resource that is a host allocation address
  (`create_committed` -> `Memory::for_resource`), not a title address.
- A placed view is a separate `Backing::Address` resource at the title's
  virtual address (`create_at_address` / `texture_at`). Its host copy is a
  twin, and `title_address()` is true only for that twin.

So the atlas's written resource and the atlas's placed twin can have
different `data()` values, and the address map cannot connect them. The
shape bridge is an attempt to connect them by description, but the
atlas's sampled view may not be a placed view at all.

A second gap: **render-target writes are never marked host-written.**
`gpu_written` is called from the UAV path and from copies; a bake that
draws into a title texture through an RTV has no corresponding call. Its
fresh texels live only in the render target's host copy. Any later placed
view of that atlas reads title memory that the bake never wrote.

The scene-like tile content is consistent with a twin initialized from
title memory that was already reused for scene targets, or with an
un-baked atlas region that was never initialized.

## Next steps

1. Do not rebuild `victory.exe` while a game run is active. That is what
   made `alias-desc` see a missing executable; cargo replaces the file
   while Wine is starting it.
2. Instrument the bake path only: when `early_textures` sees a
   `BAKE_PIXEL_SHADERS` draw, mark that draw's render targets with
   `texture::host_written` (new env `VICTORY_BAKE_HOST=1`). Do not mark all
   render targets; that would make every title RT alias forever.
3. Record the baked box (the 256x256 corner), not just the texture. A
   whole-atlas alias copy is 2048x2048 every bake; a box copy is 256x256.
   `CopyTextureRegion` already knows the box; carry it into the alias
   record.
4. Add 5 s counters: alias bytes by format, `no source` reasons, bake
   targets marked, placed twins created. Then a short warm-cache run at
   the porch can prove whether the atlas is a placed twin or the title
   resource.
5. Keep `VICTORY_UAV_RECHECK_TABLES` (2096c3f) separate. It is a safe
   diagnostic, but it re-enters the same `readable` path and cannot fix a
   failed alias.
6. Only after the atlas path is proven, consider `VICTORY_COPY_OVER_TITLE`
   or zero-initializing un-baked atlas pages.

## Caveats if the reverted patch is resurrected

`alias_of` returning a source whose shape does not match the placed view is
not the same as "no source". The kept P1 guard
(`docs/ghost-notes/2026-11-27-alias-attempt.patch`) skips `upload_from` for
any host-written address, including an offset view inside a larger written
texture. `alias()` then fails the shape check (the three
`not a placed texture of one shape` cases in `alias-fix1`), and the view is
left black rather than stale. Keep a fallback fill for shape failures, or
retry after N frames.

Two smaller holes in the same patch:

- The epoch is read while recording; a `host_written` on another record
  thread between the read and execution can leave one stale frame.
- `through.epoch` is per placed view, not per (placed, source); two sources
  with equal epochs can suppress a needed re-copy after a source flip.

Also, `alias_of`'s new non-`fills` acceptance means those placed views no
longer take the level-holding path (`resource.rs` `held_only`): memory only.

## Reproduction

Build with diagnostics. **Do not set `VICTORY_BUILD_DXC`** while the
dark-model regression is open; the runtime `d3dcompiler_47` path is the
known-good one (see `docs/linux-worklog.md`, build note):

```sh
CARGO_TARGET_DIR=$HOME/.cache/main-target cargo xwin build --profile diag \
    -p victory --features diagnostics --target x86_64-pc-windows-msvc
```

Run warm, with the scripted keys used for the porch captures:

```sh
VICTORY_EXE=$HOME/.cache/main-target/x86_64-pc-windows-msvc/diag/victory.exe \
VICTORY_LOG_DIR=/tmp/opencode/ghost-next VICTORY_PAGE_WRITES=1 \
VICTORY_ALIAS_WHY=1 VICTORY_CACHE_DIR=$HOME/.cache/victory-sweep \
PROTONPATH=$HOME/.local/share/Steam/steamapps/common/GE-Proton11-14-vt \
tools/linux-run.sh /media/SSD/GTA6.Xbox.v1.0.2.1-CYBERLEEK/game_scarlett_master.exe
```

Do not pass `--execute`; `tools/linux-run.sh` adds it. Capture the porch
frames with `VICTORY_CAPTURE_FRAMES` or `VICTORY_CAPTURE_WORLD`.

Expected markers for a real fix:

- `alias copy ... written_as=77 read_as=77` or `78/78`, not only the light
  probes.
- `shown from a texture the host wrote` above zero.
- No `HERO LEAD` for the atlas.
- The porch shirt has no rectangular scene tiles in the capture.
