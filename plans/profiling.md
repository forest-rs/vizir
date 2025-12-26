# Profiling + Per-Frame Timings (Mini-profiler Overlay)

## Goal

Make it easy to answer:
- “What changed this frame?”
- “Where did time go?” (evaluation, transforms, rendering)

…and visualize it as an overlay chart using the same VizIR pipeline.

## Approach

### A. Collect metrics (no UI coupling)

Add an optional instrumentation layer that records:
- wall-clock time for `Scene::update` (total),
- per-mark evaluation time (optional, sample-based),
- counts: marks entered/updated/exited, encodings recomputed, table/signal versions touched.

Keep this behind a feature flag and/or injected callbacks to avoid paying overhead by default.

### B. Emit a “profiling table”

Represent time series metrics as a `Table`:
- row = frame index or timestamp
- columns = durations/counters
- stable row keys = frame counter

Then a chart (area/line) can visualize:
- frame time history,
- breakdown lines (eval vs render),
- spikes and moving averages.

## Staged milestones

### M0: Basic counters

- Add a small `FrameStats` struct returned from `Scene::update_with_stats()` (or similar).

### M1: Timing hooks

- Add timing support via a trait/object passed in from `std` world:
  - avoid hard depending on any timing crate in core.

### M2: Overlay chart crate

- New demo or “tools” crate that:
  - collects stats each frame,
  - updates a profiling table,
  - uses `vizir_charts` mark specs to render a mini profiler chart.

## Open questions

- Where should the data live: inside `Scene` or as an external collector?
- Do we expose per-mark timings or only aggregate buckets (axes/series/legends)?

## Related plans

- `plans/rendering.md` (render timings are downstream)
- `plans/transforms.md` (transform timings)
