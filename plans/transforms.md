# Table Transforms (Vega-ish Dataflow Operators)

## Goal

Add a transform layer that can be used by a future Vega/Vega-Lite lowering pipeline, and is also
useful for ergonomic Rust chart APIs.

## Current state

- `vizir_core::Table`: row keys + table/column versions; `TableData` supports typed lanes.
- Core has semantic `TablePatch` values; transform patch propagation is currently bounded to
  `Project`, with replacement fallback for other operators.
- `vizir_transforms` provides a first transform IR + full-recompute executor for numeric columns:
  - `Filter`, `Project`, `Sort`, `Calculate`, `JoinAggregate`, `Aggregate`, `Bin`, `Fold`,
    `Lookup`, `Pivot`, `Window`, and `Stack` are implemented.
  - Transform row-key provenance is documented in the IR and covered by focused executor tests.
  - `TableFrame` extraction uses the optional bulk `f64` table view when available and falls back
    to per-cell reads.
  - `Project` can propagate bounded row/column patches.

## Staged milestones

### M0: Represent transforms (API + IR)

- Define a small “transform IR” that can be executed incrementally:
  - `Filter`, `Project`, `Sort`, `Bin`, `Aggregate`, `Stack`, `Window` (initial subset).
- Define data inputs/outputs: `TableId` in, `TableId` out.
- Decide ownership: likely a new `vizir_transforms` crate (no_std-first) or inside `vizir_core`
  behind a module.
  - Current: `vizir_transforms` crate exists and implements `Filter`/`Project`/`Sort` for `f64`.

### M1: Execution model

- Start with full recompute per transform, but structured so we can add incremental patches.
- Add stable row IDs through transforms (lineage/provenance):
  - carry original row keys + transform-specific keys.
  - Current: row-preserving transforms keep upstream row keys, sort moves keys with rows, and
    aggregate/fold/pivot derive deterministic output row keys from transform semantics.

### M2: Incremental table patches

- Define `TablePatch` / row-level diffs:
  - insert/update/delete by row key, plus “changed columns” metadata.
- Enable transform nodes to propagate patches downstream.

### M3: Vega-ish operators

- `Bin` + `Aggregate` (groupby) need stable grouping keys and deterministic output ordering.
- `Stack` follow-ups: additional offsets (Vega also has `wiggle` for streamgraphs).

## Open questions

- Do we standardize on Arrow/Arrow2 later, or keep a small bespoke column interface?
- Do we need a “dataflow scheduler” (dirty-set + topo) or keep transforms as explicit “build step”
  in `vizir_charts` first?

## Related plans

- See `plans/table-data.md` for typed table access, row provenance, schema, and patch design.
- See `plans/engine-evolution.md` for table storage and diff representation.
- See `plans/scales.md` for scale domains that often depend on transforms (aggregate/bin).
