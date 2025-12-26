# vizir_core

Minimal incremental visualization runtime core.

Provides versioned `Table`/`Signal` inputs, stable `MarkId` identity, explicit dependency tracking, and `Enter/Update/Exit` diffs via `Scene::update`.

This crate is `no_std` by default (uses `alloc` + `hashbrown`).

Geometry uses `kurbo`, and paint uses `peniko`.

For a chart-shaped demo (one rect mark per row with heights from a numeric column), see the `vizir_charts_demo` workspace crate.
