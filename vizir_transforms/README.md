# vizir_transforms

Vega-ish table transforms for VizIR.

This crate provides a small transform IR plus a full-recompute executor for:
- `Filter`
- `Project`
- `Sort`
- `Bin`
- `Aggregate`
- `Stack` (offset = "zero")

It is `no_std`-first (uses `alloc`). It intentionally focuses on numeric (`f64`) columns for now.
