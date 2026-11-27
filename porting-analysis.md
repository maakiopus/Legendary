# Porting Analysis

**Target:** `Belobug/VIctory-Belobug-AMD`, `a-publier@83730a9`  
**Original triage baseline:** `4eb6cfd`  
**Hardware:** AMD RX 6700 XT 12 GB, Ryzen 5 5600G, 38 GiB RAM, Linux/RADV  
**Date:** 2026-11-27  
**Scope:** Belobug-first. External PRs are candidates only when the fix is missing here.

## Recommended order

1. Port **#497** first.
2. Run the repository's full validation gates.
3. Benchmark **#437** on the RX 6700 XT.
4. Test **#166** opt-in before considering a Wine default.
5. Port **#247**, **#427**, or **#338** only for matching symptoms.

## Current-tree recheck (`83730a9`)

| Candidate | Current status | Action |
|---|---|---|
| Belobug #124 | Present in `c9f2d10` | Do not port |
| Belobug #497 | Missing; patch applies cleanly | Port first |
| Belobug #437 | Missing; patch applies cleanly | Benchmark next |
| Belobug #247 | Missing; `watch.rs` hunk applies cleanly | Test isolated |
| avatarmanz #166 | Missing; patch conflicts in pipeline/GameInput code | Manual adaptation |
| avatarmanz #427 | Missing; patch applies cleanly | Small QoL port |
| avatarmanz #338 | Partial helper path exists; root-signature path missing; patch conflicts | Manual adaptation + filter fix |
| Belobug #444 | Env switches exist; old 12 GB defaults remain; patch applies cleanly | A/B before default change |
| Belobug #536 | Missing; patch applies cleanly | Correctness port |
| Belobug #248 | Superseded by `tools/linux-run.sh`, `LINUX.md`, and Linux docs | Do not port as-is |
| Belobug #575 / #576 | Missing; both patches apply cleanly | Low-risk QoL |
| DexNom #60 / #96 / #122 | Already present | Do not port |

> Earlier local test results were produced against `4eb6cfd`; the apply-status column above was rechecked against current `83730a9`.

## Port first

### 1. Belobug #497 — XMA audio decoder lock split + MMCSS

- **Why:** Best match for audio cutouts during slow world frames.
- **Change:** `advance_read` no longer waits on decoding; the decoder uses MMCSS "Pro Audio" with a priority fallback.
- **Compatibility:** Missing at current `83730a9`; patch applies cleanly.
- **Evidence:** Upstream Marvisak #2788 is CI-green and approved. Local audio suite passed 9/9.
- **Limit:** Gameplay audio was not tested.
- **Port effort:** Small, one file.

### 2. Belobug #437 — indexed card-step dependency planner

- **Why:** Best scheduling candidate. Replaces quadratic dependency scans for large card-step batches.
- **Compatibility:** Missing at current `83730a9`; patch applies cleanly.
- **Evidence:** Randomized equivalence tests; 12.1x synthetic planner speedup; RX 6950 XT Windows suite passed 397 tests. Windows-GNU check passed.
- **Limit:** Gameplay FPS was not measured.
- **Port effort:** Small, one file.

## Test next

### Belobug #247 — Wine placeholder fallback

- Use only the `watch.rs` hunk. It is missing at current `83730a9` and applies cleanly; the full PR still conflicts because of its older base.
- RX 9070 XT Linux: hole queries fell from ~9,800 to 14 per frame; guard wait fell from ~36 ms to ~19 ms.
- Frame time was unchanged in that scene.
- **Risk:** Ordinary `PAGE_NOACCESS` reservations can be treated as placeholders under Wine; the PR documents and tests the recovery path.
- **Port effort:** Small if isolated.

### avatarmanz #166 — bounded wait for once-only compute dispatches

- Missing at current `83730a9`. The full patch conflicts in `pipeline/make.rs` and `gameinput.rs`, so adapt only the scheduler and keep the duplicate library/docs work out.
- RX 9070/7900 XT Linux evidence shows restored character skin and fewer cold-start stalls.
- Isolate the scheduler and keep `VICTORY_COMPUTE_WAIT=1` opt-in until an RX 6700 XT A/B passes.
- **Risk:** Changes queue priority and wait behavior; default-on under Wine is not yet justified on the target card.
- **Port effort:** Medium.

### avatarmanz #427 — frame flip statistics

- Fixes opening-video buffer reuse by reporting when a frame actually flipped.
- Missing at current `83730a9`; patch applies cleanly. Local frame suite passed 3/3 at the earlier baseline.
- Windows runtime reduced out-of-order opening-video frames from 9 to 0.
- **Limit:** Not AMD-specific and not tested under Linux.
- **Port effort:** Small.

### avatarmanz #338 — built-in shaders and root signatures under Wine

- High value if Linux shows black intro videos, menus, loading screens, or world rendering.
- Current `83730a9` has a runtime/native helper-compiler path but no `rootstring` parser. The PR conflicts in `ambient.rs`; adapt it manually and add `FILTER_MIN_MAG_MIP_LINEAR` to `rootstring::filter`.
- RX 6800 XT Linux confirms that one-line parser fix.
- Combined patch cross-compiles.
- **Port effort:** Medium.

### Belobug #444 — 12 GB card-buffer defaults

- Directly relevant to the RX 6700 XT, but current evidence is NVIDIA/Windows.
- `VICTORY_CARD_BUFFERS_MIN_MB` and `VICTORY_CARD_BUFFERS_MB` already exist; only the 12 GB default tier is missing. The patch applies cleanly to current `83730a9`.
- Do not change defaults yet.
- First A/B these settings on the target card:
  - `VICTORY_CARD_BUFFERS_MIN_MB=1`
  - `VICTORY_CARD_BUFFERS_MIN_MB=1 VICTORY_CARD_BUFFERS_MB=1024`
- **Port effort:** Trivial code change, but requires measured validation before adoption.

### Belobug #536 — upload-ahead copy-queue race

- Clean correctness fix for a texture upload race.
- Missing at current `83730a9`; patch applies cleanly. Unit test passed 5/5; RX 7800 XT in-game comparison showed no regression.
- Not an RDNA2 device-reset fix.
- **Port effort:** Small, one file.

### Belobug #575 + #576 — CLI and environment QoL

- #575 rejects `NaN`, infinity, negative, and out-of-range replay pacing before opening a capture.
- #576 ignores empty `LOCALAPPDATA`, `XDG_CACHE_HOME`, and `HOME` values.
- Both are missing at current `83730a9` and apply cleanly. Local config suite passed 9/9; Windows-GNU app check passed at the earlier baseline.
- **Port effort:** Trivial.

## Already present — do not port

- **Belobug #248 / Linux launcher:** superseded by current `tools/linux-run.sh`, `LINUX.md`, `docs/linux-research.md`, and the sweep tooling. Do not add the older `scripts/play.sh` as-is.
- **Belobug #124 / Wine empty pipeline library:** current HEAD includes the null-blob fix and stalled-worker bounds in `c9f2d10`.
- **DexNom #60:** already implemented by Belobug commit `ebd990d` (`GPU: streamed textures without their largest mips as video memory fills up`). The tree already has `VICTORY_TEXTURE_MAX`, `over_cap`, and matching descriptor/list/resource/texture changes.
- **DexNom #96 and #122:** the continuation-packet XMA fix and transform rotation cache are already in `a-publier`.
- **DexNom #123:** extraction optimization only; not a correctness fix.

## Avoid or hold

- **Belobug #428:** fixes a real relay race, but RX 6600 XT and RX 6800 XT still reset. It measured about 0.7 ms/frame extra fetch wait. Not the RDNA2 stability fix.
- **Marvisak #3424:** diagnostic audio capture only, not playback; `CHANGES_REQUESTED` for cleanup paths and Clippy.
- **avatarmanz #448/#450:** all CI jobs are red.
- **avatarmanz #454:** generated fixture still fails contraction behavior on vkd3d.
- **avatarmanz #420/#440:** too large or experimental for a first port batch.
- **Marvisak #3431/#3441:** large scheduling rewrites that overlap this fork's relay, card-step, and async-submit design.

## Immediate no-code checks

- `VICTORY_RELAY=0` has a same-commit RX 6700 XT report: gameplay without resets, but only 4–7 FPS.
- Run paired `VICTORY_CARD_BUFFERS_*` A/Bs before adopting #444 defaults.

## Verification performed

- Current-tree apply recheck at `83730a9`: clean for #497, #437, #247 (`watch.rs` only), #427, #444, #536, #575, and #576; conflicts for #166 and #338.
- Earlier isolated-worktree checks at `4eb6cfd`:
  - Linux audio suite: 9/9 passed.
  - Linux frame suite: 3/3 passed.
  - Linux config suite: 9/9 passed.
  - `bash -n` passed for #248's launcher.
  - Windows-GNU `cargo check` passed for the combined candidate patches.
- No Wine or game run.
- No shared-worktree code edits, commits, pushes, merges, or PR comments.
