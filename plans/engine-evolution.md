# Engine Evolution (vizir_core)

## Goal

Keep the incremental evaluation core small, `no_std`-friendly, and fast, while enabling a richer
charting layer and future Vega/Vega-Lite lowering.

## Current state

- Tables/signals with versions.
- Marks with explicit deps and incremental per-encoding updates.
- Diffs: `Enter/Update/Exit` with optional bounds (text bounds unknown).
- Marks have an explicit `z_index` for rendering order; diffs carry z-index changes so renderers
  can reorder without relying on `MarkId` sort order.

## Staged milestones

### M0: Tables v1→v2

- Decide the column interface shape (and how it interacts with `no_std`):
  - **Borrowed slices**: columns as `&[T]` obtained from a table handle.
    - Pros: fastest, simplest call sites, easy SIMD.
    - Cons: lifetime/threading constraints; hard to model partial updates/patched views.
  - **Trait object accessor** (`dyn TableData`-ish) with typed getters and row count.
    - Pros: flexible backing stores (custom, Arrow, DataFusion, generated).
    - Cons: per-element virtual dispatch; harder to batch/SIMD; less “Rust-y” for fast paths.
  - **Arrow(-ish)** (or Arrow2) as the eventual “real” table substrate.
    - Pros: interoperable; rich types; already has compute ecosystem.
    - Cons: dependency weight; `no_std` compatibility is nuanced; API churn risk.
- Decide the row-id story (stable identity through transforms):
  - v1 uses `row_keys: Vec<u64>` as stable identity for “one mark per row” charts.
  - v2 needs an explicit model for:
    - **row identity** (stable key) vs **row index** (position) vs **row order** (sort output),
    - how transforms produce new row sets while preserving provenance.
  - Likely direction:
    - carry an **origin key** (the upstream stable key) plus an optional **derived key**
      (e.g. group key for aggregates, bin key for binning, window frame key).
- Decide versioning granularity:
  - table-level version only (v1) vs column-level versions vs patch-based versions.
  - column-level versions can reduce re-eval when only one column changes.
- Decide whether/when to introduce table patches (diffs):
  - Keep v2 compatible with a future `TablePatch` model (insert/update/delete by row key).
  - Don’t commit to a specific patch encoding until transform foundations land
    (`plans/transforms.md` M2).

### M1: Damage / bounds

- Improve bounds for `Text` (likely stays downstream; core may carry “unknown”).
- Optional damage rect hints for `Update` diffs (union of old/new bounds when available).
- Consider including “ordering-only” updates (z-index changes) in the damage/patch model so display
  layers can update ordering without reconstructing geometry.

### M2: Scheduling / batching

- If transforms land: introduce a dirty-set scheduler for table/transform/mark evaluation.
- Consider frame budgets and priority (UI-critical first).

### M3: Ergonomics

- Keep core primitives stable; prefer adding ergonomic helpers in `vizir_charts`.
- Avoid monolithic “do everything” crates; keep adapters separate.
