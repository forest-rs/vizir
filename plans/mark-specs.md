# Mark Specs (Chart-Layer “Primitives”)

## Current state

- `vizir_core` primitives: `Rect`, `Path`, `Text` with incremental diffing and stable `MarkId`.
- `vizir_charts` mark specs:
  - `AreaMarkSpec`, `LineMarkSpec`, `PointMarkSpec`, `BarMarkSpec`, `RuleMarkSpec`
  - `Symbol::{Square,Circle}` helper (circle is path-based).

## Goal

Expose a chart-layer mark vocabulary that is close to Vega/Swift Charts semantics while keeping
`vizir_core` small and geometric.

## Non-goals (for now)

- Full Vega scenegraph groups, scales/axes as implicit compilation artifacts.
- Text shaping/layout in `vizir_core` (stay downstream).

## Planned mark specs

- `RectMarkSpec`
  - Pure geometry wrapper for symmetry with other `*MarkSpec` types.
  - Useful as a building block for annotations and “rect overlays”.

- `SectorMarkSpec` (Swift Charts `SectorMark`, Vega `arc`)
  - Inputs: center, inner/outer radius, start/end angle, optional corner radius (later).
  - Output: `Path` mark (use `kurbo::CircleSegment`/`Arc` to generate `BezPath`).

- `TextMarkSpec`
  - Mostly a chart-layer convenience wrapper around `vizir_core::Text` channels:
    - anchor + baseline + angle + font size + fill.
  - Keep shaping downstream.

- `ImageMarkSpec` (optional; depends on Understory imaging integration)
  - Most likely a `Path`/`Rect` + `Brush::Image` once we decide the imaging story.

## Symbol roadmap

- Add more symbols (Vega-ish): `Triangle`, `Diamond`, `Cross`, `Wye`, etc.
- Add `SymbolMarkSpec` (syntactic sugar) vs keep symbol as part of `PointMarkSpec`.
- Consider a `size` semantic: area vs diameter (Vega uses size as area for symbols).

## Open questions

- Do we want `PointMarkSpec` to default to `Symbol::Circle` (more Vega-ish) or keep square
  (cheaper geometry)? Current default is square.
- Should `SectorMarkSpec` accept degrees or radians? (Vega uses radians internally but specs often
  look like degrees; Swift Charts APIs feel like “angle” values.)

## Dependencies

- `SectorMarkSpec` depends on `kurbo` arc/segment path generation.
- Future “corner radius” depends on extending `vizir_core` rect/path semantics.
