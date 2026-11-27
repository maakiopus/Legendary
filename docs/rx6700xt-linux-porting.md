# Porting Knowledge

**Target:** `Belobug/VIctory-Belobug-AMD`  
**Hardware:** AMD RX 6700 XT 12 GB, Ryzen 5 5600G, 38 GiB RAM, Linux/RADV  
**Baseline:** `a-publier@83730a9` (the earlier PR shortlist was pinned to `4eb6cfd`)  
**Purpose:** Preserve only knowledge from `~/clone/victors` that applies to this fork or is useful for its Linux/AMD porting.

## Method

- Scanned all 80 Markdown files under `~/clone/victors`.
- Deduplicated identical contents to 53 unique files.
- Reviewed every unique file.
- Kept current, repo-specific facts; excluded stale plans for other trees, duplicates, unrelated projects, secrets, and unsupported claims.

## Knowledge already preserved in this repo

| Topic | Canonical file here |
|---|---|
| PR shortlist and port order | [`porting-analysis.md`](../porting-analysis.md) |
| Linux build/run instructions | [`LINUX.md`](../LINUX.md) |
| RX 6700 XT Linux root causes and measurements | [`docs/linux-research.md`](linux-research.md) |
| Chronological Linux runs and regressions | [`docs/linux-worklog.md`](linux-worklog.md) |
| Current Linux/Proton handoff | [`docs/linux-handoff.md`](linux-handoff.md) |
| Ghost-texture investigation | [`docs/ghost-fix.md`](ghost-fix.md) |
| Architecture | [`docs/architecture.md`](architecture.md) |
| Asset policy | [`docs/asset-policy.md`](asset-policy.md) |
| Ray tracing and ambient fallback | [`docs/fork-ray-tracing.md`](fork-ray-tracing.md) |
| Replay recording and analysis | [`docs/replay.md`](replay.md) |
| Fork switches and known problems | [`docs/README-fxceless.md`](README-fxceless.md) |
| Performance findings | [`docs/findings.md`](findings.md) |
| Linux sweep harness | [`tools/linux-sweep/README.md`](../tools/linux-sweep/README.md) |
| Kernel/GPU performance tracing | [`tools/perf/README.md`](../tools/perf/README.md) |
| Wineserver view-tree patch | [`linux/wineserver/README.md`](../linux/wineserver/README.md) |
| Huge pages for renderer port 1 | [`linux/thp/README.md`](../linux/thp/README.md) |

## Current HEAD already contains

The current branch includes the later Legendary Linux port. Do not re-port these without identifying a specific gap:

- Empty Wine pipeline library and stalled-worker bounds — `c9f2d10`.
- `VICTORY_KEYS` reaching the first connected pad — `1267a2c`.
- One back edge per loop header for dxil-spirv — `b3e757f`.
- Proton-readable log file — `5154b8e`.
- Wine kernel-read refresh path instead of remap-lock waiting — `4707087`.
- Texture recheck, relook, UAV recheck, and view-ask fixes — `bde4516`, `291ea3e`, `41ac6e1`, `c83ada6`, `bf5a590`, `aca40db`.
- Renderer port 1 private pages under Wine — `1a9c1ec`, `a054884`.
- Linux run/sweep/perf/packaging kit and research docs — `c72d1b7`, `83730a9`.

## External lessons worth preserving

### Validation and performance gates

- **Target budgets:** RSS ≤16 GiB, VRAM ≤11 GiB, with GTT checked. No default-on change without a 900 s, n=2 on/off A/B, `world_marker`, all gates green, and paired base medians within ±10%. Source: `~/clone/victors/COOL_FOX_COVERAGE.md` §target budgets/acceptance.
- **Warm the exact binary:** pipeline translation happens when the title creates a pipeline; `warm` only prepopulates the same cache. Shader/translator changes require a fresh warm-up before visual or FPS comparisons. Source: `~/clone/victors/VIctory/docs/architecture.md` §Shaders; `~/clone/victors/dexnom-victory/README.upstream.md`.
- **Compilation is not execution:** Linux tests plus `xwin check/clippy` do not prove Windows-only runtime paths. Keep runnable Linux tests and explicit Windows-runtime evidence separate. Source: `~/clone/victors/PR62_FIX_STATUS.md` §validation.
- **Validate DXC-built helper blobs:** embedding a helper is insufficient unless a real consumer calls it and the produced DXIL/root signature is validated. Source: `~/clone/victors/PR62_FIX_STATUS.md` §embedded helpers.

### AMD and Linux correctness

- **Wave64 is not a safe forced-wave32 fallback:** split/paired forms omit or incompletely model LDS, whole-wave sections, permutes, and multi-wave groups. RX 6700 XT should use native wave64 where available, but divergent `v_readlane`/`v_writelane` still needs correctness coverage. Source: `~/clone/victors/aOVictory/docs/wave64-on-wave32.md`.
- **Large partial dispatches need the full mechanism:** splitting a >65,535-group dispatch without a complete requested-thread guard can execute extra threads or produce blocky artifacts. Source: `~/clone/victors/PR62_FIX_STATUS.md` §partial dispatch.
- **Profile before “fixing” dark interiors:** dropped RT/BVH builder dispatches and CPU copy/copy-back cost can dominate the frame; GPU time may remain low. Do not attribute the symptom to gamma, sRGB, or tone mapping without that evidence. Source: `~/clone/victors/GAMMA_TRIAGE.md`.
- **Do not rebuild `victory.exe` during an active Proton run:** Cargo's executable replacement caused “Executable not found” in the sibling Linux port. Source: `~/clone/victors/Legendary_Legacy/docs/ghost-fix.md` §reproduction hazards.

### Memory and scheduling candidates

- **Dirty-page marks are not a direct port:** the other tree compared ~1.4 GB/frame for ~3.6 MB written, but the mechanism depends on guard/fault semantics that differ here. Measure watched versus unwatched `Shadow::writing` ranges first. Source: `~/clone/victors/victory-royale-code-plan.md`.
- **Host-copy lifetime is a concrete RAM target:** the surveyed tree kept numbered copies until unregister/release. Bounded LRU retirement may help the 38 GiB host, but needs rebuild/thrash testing. Source: `~/clone/victors/victory-royale-code-plan.md`.
- **Guest read/write classification can dominate RT cost:** one RX 7900 XTX Windows measurement reduced compared guest memory from 12,793 to 2,655 MB/frame and mean frame time from 3,469 to 969 ms. Treat as a pattern, not a target-card result. Source: `~/clone/victors/dexnom-victory/BENCHMARK.md`.
- **Narrow staging locks:** one sample spent 846 ms on the global mapping lock; range-scoped locking reduced a focused texture-staging operation to 77 µs. Source: `~/clone/victors/dexnom-victory/BENCHMARK.md`.

### Concrete code gaps worth investigating

- **Helper-shader build path: resolved.** `tools/build.sh` now detects that this tree has no DXC bundling step, reports the `victory-dxil-bundled` marker honestly, and points at the native DLL route; `tools/d3dcompiler.sh` stages a modern `d3dcompiler_47` and refuses the broken 2013 Windows 8 build by SHA-256. The build script no longer claims bundled DXIL it cannot produce.
- **`first_mip()` ignores 2D-array views:** the current implementation only recognizes the plain 2D view shape. A single-layer array view starting at a nonzero mip can be treated as mip 0, causing stale/overlapping texture slots. Source: `~/clone/victors/.agent-knowledge.md` §`first_mip`; confirmed in current `crates/victory-gpu/src/resource.rs:1260`.
- **Texture detile commit is not failure-atomic:** current `crates/victory-gpu/src/texture.rs:1498` still returns `()`. The sibling `aOVictory` work preflights every subresource and commits digest/asked state only after complete detile; external validation reported 39/39 format and 15/15 GPU texture tests. Source: `~/clone/victors/aOVictory/README.md` §atomic texture-detile commit.
- **Write-combined copy-back bypass is absent:** `VICTORY_WC_NO_BACK=1` is an external candidate for watched, card-backed WC ranges ≥64 KiB. It reduced copy-back from 23.5 to 1.7 ms in the source tree, but CPU-read correctness—especially water/ocean data—must be proven here. Source: `~/clone/victors/aOVictory/README.md` §write-combined copy-back.
- **Release automation gaps:** current HEAD lacks `VICTORY_KEYS_FILE` and `VICTORY_COMPUTE_WAIT_MS`. The external build's release-input support is useful; its FPS result was not a clear win. Source: `~/clone/victors/aOVictory/README.md` §release input/compute budget.
- **Keep BVH/RT builder opt-in on this 12 GB card:** sibling Proton runs compiled builder pipelines but crashed in builder copy-back; quarantine avoided corruption but withheld counters, and narrow copy-back remained unplayable. Source: `~/clone/victors/.agent-knowledge.md` §BVH pipelines.

### Rendering and Linux runtime cautions

- **Ghosting: fixed (27 Nov 2026).** Cause: the 2013 Windows 8 `d3dcompiler_47.dll` rejects `cs_5_1` (`X3506`), so the helper shaders fell back and the atlas host copies went stale. A modern `d3dcompiler_47` (Edge 148 tested) resolves it; `tools/d3dcompiler.sh` refuses the 2013 build by SHA-256. The placed-atlas `fills` theory was wrong and is history.
- **Do not enable all renderer ports together:** on RX 6700 XT, port 1 + THP + copy switches reached 6 fps but removed water; all-port runs were slower and could fail world entry. Port 1 can make descriptor memory unwatchable, blocking port 2 synchronization. Sources: `~/clone/victors/Legendary_Legacy/README.md`; `~/clone/victors/Legendary_Legacy/docs/linux-handoff.md`.
- **More refresh threads are not automatically faster:** `VICTORY_WIDE_THREADS=6` regressed to 0.6 fps under Wine locks. The settled 12.9 fps/77 ms result had GPU at 12.9 ms and copies at 41.4 ms/21 MB; streaming frames remained 300–380 ms. Source: `~/clone/victors/Legendary_Legacy/docs/linux-worklog.md`.
- **THP is mechanism-proven, not FPS-proven on this card:** port 1's ~185 MB heap becomes an amdgpu userptr; `MADV_COLLAPSE` reduced ~47,000 page-table entries to ~93. The RX 6700 XT run collapsed ranges successfully, but no world FPS result was measured. Source: `~/clone/victors/Legendary_Legacy/linux/thp/README.md`.
- **Vulkan placed-memory layer is reference-only:** `aOVictory/tools/vk-layer-placed` enables `VK_KHR_map_memory2`/`VK_EXT_map_memory_placed` for `VICTORY_PORT_PLACED=1`; this fork has no corresponding implementation. Source: `~/clone/victors/aOVictory/tools/vk-layer-placed/README.md`.
- **Keep 12 GB memory experiments opt-in:** `VICTORY_LEVELS_ON_DEMAND=1` saved several GiB in the source tree but remained experimental. BVH builder, memory dumps, full readbacks, and 4096 MB card-memory settings are unsafe defaults for this host. Sources: `~/clone/victors/cool_fox/docs/README-fxceless.md`; `~/clone/victors/cool_fox/docs/fork-ray-tracing.md`.

### Known AMD-negative evidence

Do not enable these without fresh target-card proof: `VICTORY_TEXTURE_COPY=1`, `VICTORY_WAVE_WIDE=1`, relay plus `KEEP_SNAPSHOTS`, `INPLACE=2`, unordered early pipelines, or `VICTORY_SHADER_OPT=127`. Sixteen-thread page mapping showed no gain and longer frames. Source: `~/clone/victors/COOL_FOX_COVERAGE.md` §AMD negative evidence.

### Linux run discipline

- **Match wineserver and Proton exactly:** Experimental uses protocol 931, GE-Proton11-14 uses 938, and plain Wine 11.0 protocol 930 is rejected. Build the patched server from the exact Proton Wine tree and install it into a separate Proton copy. Source: `~/clone/victors/Legendary_Legacy/linux/wineserver/README.md`.
- **Use the supported Proton route:** the sibling sweep rejects UMU-Proton 10/11 because they emitted invalid SPIR-V that faulted RADV. Do not downgrade the working Proton to make a comparison. Source: `~/clone/victors/Legendary_Legacy/tools/linux-sweep/README.md`.
- **PASS needs more than a screenshot:** require a world marker, zero faults/crashes/shader drops, no black world frames, no Wine shader/library failures, and valid luminance/color. PNG size alone is not proof. Source: `~/clone/victors/Legendary_Legacy/tools/linux-sweep/README.md`.
- **Diagnostics builds are required for scripted keys and captures.** The sweep defaults to Experimental; GE-vt requires an explicit `PROTONPATH`. Source: `~/clone/victors/Legendary_Legacy/tools/linux-sweep/README.md`.
- **Never rebuild `victory.exe` during an active Wine launch.** Cargo's replacement caused “Executable not found” in the sibling port. Source: `~/clone/victors/Legendary_Legacy/docs/ghost-fix.md`.

### Measurement and diagnostics

- Sample `/proc/<pid>/status` for `VmRSS`/`VmSwap` and DRM `mem_info_vram_used`/`mem_info_gtt_used` every five seconds. Source: `~/clone/victors/victory-royale-code-plan.md`.
- Under Proton, `GetWriteWatch` can become an ioctl/page walk; measure wineserver and kernel costs, not only Rust spans. Source: `~/clone/victors/victory-royale-code-plan.md`; `~/clone/victors/Legendary_Legacy/tools/perf/README.md`.
- Preserve Windows/Proton error reasons as Rust strings where the Wine stub loses them. Source: `~/clone/victors/victory-royale-code-plan.md` §Proton diagnostics.
- Use the same save, repeat each A/B arm twice, and require reproducibility before attributing rendering changes. Source: `~/clone/victors/GAMMA_TRIAGE.md` §A/B variance.

## Do not carry over

- **Stale baseline claims:** `4eb6cfd` already has renderer ports 1/2/5/6/7, replay, async submit, join parts, thin lists, land, card steps, and declare-once. Current `83730a9` also has the later Linux port. Re-check current code before calling anything missing.
- **Other-tree implementation plans:** `PORT-PLAN.md`, `FINAL_PLAN.md`, `ORCHESTRATOR_PROMPT.md`, `HANDOFF-SECOND-ORCHESTRATOR.md`, `KANBAN.archive.md`, and most `TRIAGE.md`/`PR62_FIX_STATUS.md` task status belong to different worktrees or baselines.
- **Raw performance numbers from other GPUs/trees:** RX 7900 XTX, RTX, and RX 6800 results are evidence for a mechanism, not a baseline for RX 6700 XT/Linux.
- **Duplicate documentation:** identical Legendary/cool_fox/VIctory-7900XTX copies add no new knowledge. Use the canonical files in this repo listed above.
- **Different asset policy:** DexNom's 512 KiB cap is not this repo's policy; Belobug uses 640 KiB and its own BCPack exception. Source: `~/clone/victors/dexnom-victory/docs/asset-policy.md` versus current `docs/asset-policy.md`.
- **NVIDIA-only view-heap workarounds:** the 1,000,000-view tier-2 failure was NVIDIA-specific; AMD accepted 1,100,001 slots in the cited test. Low priority unless reproduced on RADV. Source: `~/clone/victors/aOVictory/docs/view-heap-limit.md`.
- **Superseded bug reports:** `release_private` remap notification text in the early handoff was later fixed; current HEAD calls both `title_memory_unmapping()` and `port_private_gone()`.
- **Unsupported proposals and abandoned experiments:** memory architecture sketches, ReBAR defaults, lazy write-back, and forced-wave32 fallbacks need target measurements and correctness proof before use.
- **Secrets, proprietary game material, screenshots, shader dumps, executable bytes, session IDs, and agent/token bookkeeping.**

## Source coverage

- 80 Markdown files found before cleanup.
- 53 unique contents reviewed in full across four batches: 14 + 13 + 14 + 12.
- 27 files were byte-identical duplicates; 26 external copies were deleted while keeping the canonical Belobug copy.
- 11 stale plans/status files were deleted.
- 38 Markdown files remain: 19 canonical Belobug files and 19 external/reference files.
- Every remaining external/reference file has a banner naming its source, context, and canonical Belobug notes.
- Source material consolidated into current Belobug docs: Legendary Linux research, worklog, handoff, build/run, sweep, perf, wineserver, and THP notes.
- Remaining reference sources: `.agent-knowledge.md`, `COOL_FOX_COVERAGE.md`, `memory-perf-plan.md`, `KANBAN.md`, `TRIAGE.md`, `cool_fox` findings/status, `dexnom-victory` architecture/RT/benchmark notes, `VIctory` upstream docs, `VIctory-7900XTX` README, and `Legendary_Legacy/README.md`.
- The `aOVictory` notes were reviewed before that directory disappeared; their useful wave64, view-heap, placed-layer, and porting facts are distilled above.
