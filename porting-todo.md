# Porting TODO

Source: [`porting-analysis.md`](porting-analysis.md). The analysis was checked
at `83730a9`; this TODO is at `b5b01e8` (same tree plus the sweep/perf work).

PR sources: `Belobug/VIctory-Belobug-AMD` for #247/#437/#444/#497/#536/#575/#576;
`avatarmanz/VIctory-7900XTX` for #166/#338/#427. Apply status verified at
`7110740`: #536/#575/#576/#444 and avatarmanz #427 are clean; #247 is clean
for `watch.rs` only; avatarmanz #166/#338 conflict.

Every port: code review before commit; 900 s sweep x 2 per arm; world marker;
0 faults, crashes, shader drops, indirect drops; water present; valid colours;
RSS <= 16 GiB; VRAM <= 11 GiB with GTT checked. No push.

## Batch 1 - port and measure

- [ ] **#497 audio: XMA decoder lock split + MMCSS** (small, one file; audio
      cutouts during slow frames; no FPS claim)
  - [ ] code review
  - [ ] build + 900 s x 2; audio check (`takes`/`partial` counters, no cutouts)
- [ ] **#437 indexed card-step dependency planner** (small, one file; 12.1x
      synthetic planner speedup; gameplay FPS unmeasured)
  - [ ] code review
  - [ ] baseline vs patched, 900 s x 2; compare `perf-summary.json` median/p95

## Batch 2 - small correctness and QoL

- [ ] **#536 upload-ahead copy-queue race** (verified clean at 7110740; unit
      test 5/5, no regression)
- [ ] **#575 replay pacing validation** (verified clean at 7110740; reject
      NaN/inf/negative/out-of-range)
- [ ] **#576 empty environment defaults** (verified clean at 7110740; ignore
      empty HOME/XDG/LOCALAPPDATA)
- [ ] **avatarmanz/VIctory-7900XTX #427 frame flip statistics** (verified
      reachable and clean at 7110740; do not confuse with Belobug #427
      "Texture store: serialize resident cache admission")

## Batch 3 - opt-in experiments only

- [ ] **#247 Wine hole fallback** (`watch.rs` hunk applies clean; the other
      four files conflict on upstream drift - port the hunk only; RX 9070 XT:
      hole queries 9,800 -> 14/frame; frame time unchanged; A/B first)
- [ ] **avatarmanz/VIctory-7900XTX #166 compute wait** (manual adaptation;
      `pipeline/make.rs` and `gameinput.rs` conflict, `lib.rs`/`wine.rs`
      hunks unaffected; keep `VICTORY_COMPUTE_WAIT=1` opt-in until an
      RX 6700 XT A/B passes)
- [ ] **avatarmanz/VIctory-7900XTX #338 rootstring** (manual adaptation;
      13-file surface, conflicts from `ambient.rs:241`; add
      `FILTER_MIN_MAG_MIP_LINEAR`; only for matching symptoms)
- [ ] **#444 12 GB card-buffer defaults** (verified clean at 7110740; A/B
      `VICTORY_CARD_BUFFERS_MIN_MB=1` and
      `VICTORY_CARD_BUFFERS_MIN_MB=1 VICTORY_CARD_BUFFERS_MB=1024` before any
      default change)

## Already present - do not port

#124, #248, DexNom #60/#96/#122/#123. See the analysis.

## Hold or avoid

#428, Marvisak #3424, avatarmanz #448/#450/#454/#420/#440, Marvisak
#3431/#3441. See the analysis.

## Open sweep question

The analyzer still fails a run on in-engine cutscene black frames with
1,200-1,866 draws and bright captures (the 240 s smoke). Resolve the fade
gate with a full 660 s run before treating a FAIL as a rendering regression.
