# vizir_core

Minimal incremental visualization runtime core.

Provides versioned `Table`/`Signal` inputs, table-column versions for narrow invalidation, semantic `TablePatch` descriptions, stable `MarkId` identity, UI-neutral mark metadata, table schema metadata, typed table-data accessors with text helpers and an optional bulk `f64` view, explicit dependency tracking, and `Enter/Update/Exit` diffs via `Scene::update`.

Computed encodings still have a low-level explicit `InputRef` escape hatch, but common table and signal constructors attach those dependencies mechanically. The chart/spec layers should prefer those helpers so closures and dependency lists do not drift apart.

This crate is `no_std` by default (uses `alloc` + `hashbrown`).

Geometry uses `kurbo`, and paint uses `peniko`.

For a chart-shaped demo (one rect mark per row with heights from a numeric column), see the `vizir_charts_demo` workspace crate.
