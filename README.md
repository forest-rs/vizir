# Vizir

Incremental, “Vega-ish” visualization runtime + chart building blocks.

Vizir is a small family of crates designed to be combined in different stacks over time. The focus
is on clean separation of concerns, pluggable performance trade‑offs, and long‑term architectural
stability.

## Crates

- `vizir_core`
  - `#![no_std]` + `alloc` incremental evaluation core.
  - Versioned inputs: `Table` (row keys + optional column accessor) and typed `Signal<T>`.
  - Stable identity via `MarkId`, and diffs `Enter/Update/Exit` keyed by `MarkId`.
  - Mark primitives: `Rect`, `Path`, `Text` (unshaped), plus `z_index` for ordering.

- `vizir_charts`
  - `#![no_std]` + `alloc` chart building blocks that generate `vizir_core::Mark`s.
  - Scales: `ScaleLinear`, `ScaleBand` (v1).
  - Guides: `AxisSpec` (Vega-style `orient`), `LegendSwatchesSpec` (now supports columns).
  - Mark specs (Swift Charts / Vega-inspired): `BarMarkSpec`, `LineMarkSpec`, `PointMarkSpec`,
    `AreaMarkSpec`, `RuleMarkSpec`, `SectorMarkSpec`, plus `RectMarkSpec`/`TextMarkSpec`.

- `vizir_text`
  - `#![no_std]` + `alloc` text measurement traits used for chart layout.
  - Shared types: `TextMeasurer`, `TextStyle`, `TextMetrics`.
  - Includes a small `HeuristicTextMeasurer` for demos/early layout.

- `vizir_text_web`
  - `wasm32` text measurement adapter using `web-sys` Canvas 2D `measureText` (Vega-style).
  - Non-`wasm32` builds fall back to heuristic measurement so the crate can still compile in a workspace.

- `vizir_text_parley`
  - Native text measurement adapter using Parley (shaping-aware metrics).

- `vizir_charts_demo`
  - A tiny demo binary that emits SVG dumps: `bar.svg`, `scatter.svg`, `line.svg`, `area.svg`,
    `sector.svg`.
  - This crate can depend on `std` and is where we experiment with renderer adapters.

- `vizir_vello_demo`
  - A native demo binary that renders `vizir_core` marks via Vello (`winit` + `wgpu`).

- `vizir_examples`
  - Scratch/example binaries (kept separate from core crates).

## Design principles

- Long-term architecture over short-term compatibility.
- Keep core crates `no_std`-first and minimize dependencies.
- Keep rendering out of the core: consumers apply `MarkDiff` to their own display/imaging layers.
- Prefer stable identity + diffs over rebuilding whole scenes.

## Plans

Living roadmap/design notes live in `plans/`:
- `plans/README.md`

## Getting started

- Run the demo SVG emitter:
  - `cargo run -p vizir_charts_demo`
- Validate the workspace:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace --all-features`

## MSRV & License

- Minimum supported Rust: 1.88.
- Dual-licensed under Apache‑2.0 and MIT.
