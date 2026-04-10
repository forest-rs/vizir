# vizir_charts

This crate provides Vega-ish chart building blocks (scales, axes, legends) that
generate `vizir_core` marks.

It is not a Vega/Vega-Lite implementation: there is no declarative grammar and
no automatic compilation step. Instead, higher-level layers can use these
building blocks to create stable-identity marks and feed them into a `vizir_core::Scene`.

Optional `json` feature:
- Enables a narrow serde-backed JSON parser that produces `ParsedUnitSpec` values for the
  experimental authored-spec seam.
- This is intentionally limited to the currently supported mark/channel/transform slice.

## Plans

Living roadmap/design notes are in `plans/` at the workspace root:
- `plans/README.md`
- `plans/support-matrix.md`
- `plans/mark-specs.md`
- `plans/transforms.md`
- `plans/scales.md`
- `plans/guides.md`
- `plans/rendering.md`
- `plans/engine-evolution.md`
