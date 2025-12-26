# IO + External Data Backends (CSV / Arrow / DataFusion)

## Goal

Allow VizIR pipelines to ingest and transform real datasets without forcing heavyweight
dependencies into `vizir_core`/`vizir_charts`.

Core rule: keep IO/backends as separate adapter crates.

## Current state

- `vizir_core::Table` is versioned and can optionally store a `Box<dyn TableData>`.
- `TableData` is minimal (`f64` reads only), sufficient for basic chart prototypes.

## Principles

- `vizir_core` remains renderer-agnostic and backend-agnostic.
- IO crates produce a `Table`/`TableData` implementation (or an Arrow-backed implementation).
- Transform engines (DataFusion, etc.) are optional “executors” that produce derived tables.

## Proposed crates (top-level workspace members)

- `vizir_io`
  - CSV parsing and small utilities (feature-gated `std` likely required).
  - Output: a simple column store implementing `TableData`, or Arrow arrays.

- `vizir_arrow`
  - Adapter layer for Apache Arrow columnar arrays.
  - Provides `TableData` impl(s) over Arrow arrays and/or a dedicated `ArrowTable` wrapper.
  - Keep `default-features = false` where possible; expect `alloc` + maybe `std` depending on Arrow crate choices.

- `vizir_datafusion`
  - Optional executor for transform graphs / SQL-ish specs (DataFusion).
  - Converts query results into either Arrow arrays or a lightweight column store.

## Staged milestones

### M0: CSV ingestion (prototype)

- Parse CSV into:
  - `row_keys` (stable per row, e.g. sequential or a hashed composite key), and
  - numeric columns (at least `f64`).
- Provide a helper to build a `vizir_core::Table` with a `TableData` implementation.

### M1: Arrow adapter

- Implement `TableData` over Arrow arrays for numeric columns.
- Define a stable mapping from column names → `ColId` (likely an interner in chart-layer code).

### M2: DataFusion executor (optional)

- Provide an API that takes:
  - an input Arrow dataset (or in-memory record batch),
  - a transform/query spec,
  - and produces output tables plus stable row keys.

## Open questions

- Which Arrow crate (arrow-rs vs arrow2) is best given `no_std`/feature constraints?
- How do we manage schemas/column names across layers (tokens, interner, or string keys)?
- Do we extend `TableData` beyond `f64` (strings/categories/timestamps), or add a separate typed column API?

## Related plans

- `plans/transforms.md` (transform IR and table diffs)
- `plans/engine-evolution.md` (table representation evolution)
