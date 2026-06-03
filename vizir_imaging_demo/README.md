# vizir_imaging_demo

Native renderer demo for shared VizIR scenarios.

This binary renders `vizir_demo_scenarios` through `vizir_backend_imaging`,
`imaging_vello_hybrid`, `wgpu`, and `winit`.

## Running

Native demos require a working desktop windowing and GPU stack.

```sh
cargo run -p vizir_imaging_demo
```

Use left/right arrow keys to switch scenarios and Escape to close the window.

## Status

This crate is not published. It is the native smoke test for renderer integration
and scenario navigation.

## Minimum Supported Rust Version

This crate follows the workspace MSRV: **Rust 1.88** and later.
