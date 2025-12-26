# Rendering / Adapters

## Goal

Keep `vizir_core` renderer-agnostic while enabling:
- SVG dumps (for dev/test),
- Understory imaging/display list emission,
- incremental updates (diff-driven), and
- animation/hit-testing downstream.

## Current state

- `vizir_charts_demo` has a minimal SVG emitter (`vizir_charts_demo/src/svg.rs`).
- `vizir_core` diffs include `Enter/Update/Exit` keyed by `MarkId`.
- `Text` payload is unshaped; bounds for text are `None` in core.
- Marks have an explicit `z_index`; renderers/adapters should sort by `(z_index, MarkId)` for
  stable ordering.

## Staged milestones

### M0: Better SVG adapter

- Support `TextBaseline`/`TextAnchor`/`angle` (baseline is now emitted).
- Optionally support strokes/fills for non-solid brushes (as “none” or minimal subset).
- Ensure drawing order respects `z_index` (not just `MarkId` order).

### M1: Understory adapter

- Convert `MarkDiff` into Understory imaging ops (rect/path/text).
- Decide strategy for caching and incremental updates (per-mark op ids).

### M2: Hit testing + interaction

- Provide a “mark bounds index” downstream.
- Maintain continuity through stable `MarkId`.

### M3: Animation

- Consume `old/new` channel values to drive timeline interpolation.

## Related plans

- `plans/transforms.md` (big scenes need efficient data updates).
- `plans/engine-evolution.md` (damage rect hints).
