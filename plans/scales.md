# Scales (Vega-ish)

## Current state

- `ScaleLinear` (continuous) with “nice-ish” ticks.
- `ScaleBand` (discrete) with inner/outer padding.
- `ScalePoint`, `ScaleLog`, and `ScaleTime` exist as initial additional scale types.

## Goal

Build a scale toolbox close to Vega’s conceptual model while keeping APIs ergonomic in Rust.

## Staged milestones

### M0: API shape

- Separate “scale spec” (domain/range/options) from “scale instance” (fast mapping function).
- Keep ticks/formatting adjacent (Vega: scale + tickCount + format specifier).
  - Initial “spec” types exist (`ScaleLinearSpec`, `ScaleBandSpec`, etc); refine options over time.

### M1: More scale types

- `ScalePoint` (like band without width).
- `ScaleTime` (domain is timestamps; needs formatting).
- `ScaleLog` / `ScaleSymlog`.
- Color scales (continuous and ordinal) targeting `peniko::Brush`/`Color`.

#### Note: time/date dependencies (future)

Vega’s `time` scales operate on real datetimes with UTC/local semantics, calendar-aware intervals
(days/months/years), and flexible formatting. Our current `ScaleTime` models time as numeric seconds
with a small interval ladder (seconds/minutes/hours).

When we need true calendar/timezone behavior, prefer:
- Keep `vizir_charts` `no_std`-friendly; put heavy timezone / tzdb support behind an opt-in `std`
  feature (or in a separate crate).
- Consider the `time` crate (`default-features = false`) over `chrono` for core date/time handling.
- Defer “local time” (DST + tzdb) semantics until we have a clear product need and a dependency plan.

### M2: Domain inference

- Compute domains from tables (min/max) as a transform-like operation.
- Support “nice” domain behavior compatible with Vega’s.
  - Initial helper exists for numeric domains: `infer_domain_f64`.

## Open questions

- How much of D3/Vega “nice” should we match? (tick generation and domain rounding details.)
- Numeric formatting: adopt a small formatting module compatible with Vega format specifiers?
