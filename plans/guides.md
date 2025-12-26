# Guides: Axes, Grid, Legends

## Current state

- Single `AxisSpec` with `orient` (top/bottom/left/right), `GridStyle`, and optional title.
- `AxisSpec` supports `labelAngle` via `with_label_angle(...)` (rotation) and uses z-order conventions.
- `LegendSwatchesSpec` (vertical swatches + labels).
- `ChartLayout` provides measure/arrange-style rectangles for plot+axes+legend.
- `vizir_core` supports `TextAnchor` + `TextBaseline` + `angle` for text.

## Goal

Match Vega’s guide semantics where feasible (axis/legend are “guide specs” that compile to marks),
and keep compatibility with future Understory measure/arrange layout.

## Near-term fixes / additions

- Axis label formatting:
  - consistent integer/decimal formatting (already has tests)
  - add an API hook for a custom formatter
- Axis: optional `grid` per axis already exists; consider independent grid z-layering.

## Staged milestones

### M0: Axis parity basics

- Add `RuleMarkSpec` usage throughout (reduce duplicated path boilerplate).
- Add axis options commonly used in Vega:
  - `ticks: bool`, `labels: bool`, `domain: bool`
  - `tickCount` (already), `tickSize` (already), `tickPadding` (already)
  - `labelAngle` (supported via text `angle`)

### M1: Better layout

- Incorporate label measurement into axis placement:
  - handle rotated label sizes (basic support exists; refine overlap avoidance)
  - avoid overlap/clipping at extremes (first/last ticks)
- Legend sizing:
  - improve bounds accounting for baseline/anchor (and optionally angle)
  - allow multi-column legends

### M2: Guide composition

- “Guide layer” convention for stable ordering:
  - grid behind, then series, then axes, then legend.
  - use `vizir_core::Mark::z_index` (and renderer sorting by `(z_index, MarkId)`) instead of
    relying on `MarkId` ordering.

## Related plans

- `plans/scales.md` (ticks depend on scales).
- `plans/rendering.md` (text measurement and rendering adapters).
