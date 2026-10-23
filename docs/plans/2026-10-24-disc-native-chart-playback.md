# Native chart playback for IIDX 15 and 16

Written: 2026-10-24

## Goal

Remove the intermediate `Ply -> BMS text -> parse_bms` path for IIDX 15 and
IIDX 16 disc charts. Disc charts should be parsed once into the player's
existing chart model, while preserving the original sound numbers without the
BMS two-character `00..ZZ` limit.

Folder and package charts continue to use the existing BMS loader.

## Current state

- `cs-disc` reads and decrypts IIDX 15 and 16 chart files as `Ply`.
- Both disc modules now expose `Disc::chart_ply()`.
- The active gameplay loader now calls `chart_ply()` and converts directly to
  `BmsChart`; scanner/legacy byte-reading tools still use `chart_bms()`.
- `ply_bms` currently contains compatibility logic that compacts sounds when
  a chart exceeds the BMS sound range and silences any remainder.

## Design

Keep `BmsChart` as the runtime contract for judgement, timing, rendering, and
audio. Native means that no BMS text is generated or parsed; it does not
require a second gameplay chart type.

The conversion layer will be a pure function in the app/core side. It will
accept `Ply` plus disc metadata and produce a `BmsChart` directly:

```text
Ply events
  -> bar/timing model (same frame-drift correction as ply_bms)
  -> BmsChart notes, BGM events, timing events, and metadata
```

Sound IDs will remain `u16` values in `WavId`, so disc charts are not limited
to 1295 sounds. The disc sound loader will use those IDs directly rather than
looking up BMS-generated filenames.

## Implementation steps

1. Extract the timing primitives from `ply_bms`:
   - bar discovery and time-signature handling;
   - least-squares correction for rounded frame times;
   - tempo-change segments;
   - snapped `(measure, fraction)` positions.
2. Add a direct `Ply -> BmsChart` conversion function.
   - Map P1/P2 lanes to `Lane`.
   - Emit tap and long-note endpoints.
   - Preserve sample and auto events as `WavId(u16)`.
   - Emit BPM, stop/timing, measure-length, and play-mode information.
   - Keep the existing zero-time placeholder-note behavior.
3. Extend the disc source abstraction with a native chart result, while
   retaining the BMS-byte path for folder/package sources.
4. Switch both `OpenDisc::Iidx15` and `OpenDisc::Iidx16` to `chart_ply()` and
   the direct converter.
5. Update disc sound loading to iterate the native chart's sound table and
   decode the corresponding original key-sound number.
6. Remove the sound compaction and `ZZ` fallback from the disc path. Keep BMS
   range validation only for actual BMS text serialization.
7. Keep `chart_bms()` isolated for scanner/diagnostics. It is not part of the
   gameplay load path and can be removed only after scanner metadata gets a
   native-chart equivalent.

## Validation

- Unit tests for timing, lane mapping, long notes, auto sounds, and sound IDs
  above 1295.
- Existing `cs-disc` unit and real-disc tests.
- IIDX 15: KODO, VOX UP, and several charts with large sound tables.
- IIDX 16: SP/DP charts, beginner/black charts, and at least one chart with
  BPM changes inside a bar.
- Verify identical note/timing results against the current generated-BMS path
  for charts within the BMS sound range.
- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo build --release`

## Rollback and compatibility

The migration will be split into commits so the current `chart_bms()` path can
remain as a diagnostic fallback until both disc versions pass validation. No
changes are required for installed BMS or `.bmsp` content.

## Completion criteria

- IIDX 15 and 16 disc playback no longer creates BMS text.
- No disc chart sound is dropped because of `ZZ`.
- Disc chart timing and note counts match the validated current path.
- The generated-BMS compatibility code is clearly isolated to scanner and
  diagnostics only.
