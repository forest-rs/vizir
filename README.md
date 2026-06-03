# VizIR

Semantic visualization IR and incremental runtime work for forest-rs.

VizIR is early, pre-release work. The public API is expected to change while the
core table model, authored chart lowering, renderer adapters, and demo surfaces
settle. The goal is a small set of durable crates for building inspectable,
animated, data-driven views without tying visualization semantics to one UI
toolkit or renderer.

## Workspace

| Crate | Purpose |
| --- | --- |
| [`vizir_core`](vizir_core/) | `no_std` incremental runtime: tables, signals, stable mark identity, semantic mark metadata, dependency tracking, and mark diffs. |
| [`vizir_transforms`](vizir_transforms/) | `no_std` table transform IR and full-recompute executor for filter/sort/aggregate/stack and related dataflow operators. |
| [`vizir_charts`](vizir_charts/) | `no_std` chart building blocks, authored-spec lowering, scales, guides, and mark specs that generate `vizir_core` marks. |
| [`vizir_backend_svg`](vizir_backend_svg/) | Retained SVG adapter for evaluated `vizir_core` mark diffs. |
| [`vizir_backend_imaging`](vizir_backend_imaging/) | `imaging` command-stream adapter for evaluated `vizir_core` mark diffs, with optional Parley-backed text painting. |
| [`vizir_demo_scenarios`](vizir_demo_scenarios/) | Shared renderer-neutral scenarios used by SVG and native demos. |
| [`vizir_charts_demo`](vizir_charts_demo/) | HTML/SVG report generator for the shared scenarios. |
| [`vizir_imaging_demo`](vizir_imaging_demo/) | Native `winit`/`wgpu` demo using `imaging_vello_hybrid`. |
| [`vizir_examples`](vizir_examples/) | Scratch examples for core runtime behavior. |

## Running Things

Generate the HTML/SVG report:

```sh
cargo run -p vizir_charts_demo
```

Run the native imaging demo:

```sh
cargo run -p vizir_imaging_demo
```

Run the basic runtime example:

```sh
cargo run -p vizir_examples
```

## Status

The foundational crates are intended to stay `no_std` where practical. Renderer
adapters, demos, and host-facing crates may depend on platform/runtime facilities
that do not belong in the core runtime.

Before publishing or pushing release-facing work, run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

For the no-std feature matrix used by CI:

```sh
cargo hack check -p vizir_core -p vizir_charts -p vizir_transforms -p vizir_backend_svg -p vizir_backend_imaging --locked --optional-deps --each-feature --ignore-unknown-features --features libm --exclude-features std,default,parley --target x86_64-unknown-none
```

## Design Direction

- Keep `vizir_core` small, stable, and renderer-neutral.
- Keep table data, transforms, authored specs, and rendering adapters as separate
  replaceable layers.
- Prefer stable identity and diffs over rebuilding whole scenes.
- Treat semantic metadata as part of the runtime contract for selection,
  inspection, accessibility, tooltips, and agent-facing diagnostics.
- Stay Vega-ish where it helps, but avoid implying full Vega/Vega-Lite parity
  before the supported slice is proven.

Living design notes are in [`plans/`](plans/), especially
[`plans/support-matrix.md`](plans/support-matrix.md).

## Minimum Supported Rust Version

This workspace currently targets **Rust 1.88** and later.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>), or
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>),

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
