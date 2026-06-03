# vizir_demo_scenarios

Shared renderer-neutral scenario builders for VizIR demos.

This crate owns small reusable charts and visualization scenes. Each scenario
returns evaluated `vizir_core::MarkDiff` values plus a view rectangle; renderer
crates and demo binaries can consume the same source through SVG, `imaging`, or
future adapters.

## Status

This crate is not published. It exists to keep demo content out of core crate
dev-dependencies and to make renderer comparisons use the same input scenes.

## Usage

```rust
for scenario in vizir_demo_scenarios::static_scenarios() {
    let frame = scenario.build();
    // Apply frame.diffs to a renderer-specific retained scene.
}
```

## Minimum Supported Rust Version

This crate follows the workspace MSRV: **Rust 1.88** and later.
