# vizir_charts_demo

HTML/SVG report generator for the shared VizIR scenarios.

This binary renders every `vizir_demo_scenarios` scenario through
`vizir_backend_svg` and writes a single `vizir_charts_demo.html` file in the
current directory.

## Running

```sh
cargo run -p vizir_charts_demo
```

Open `vizir_charts_demo.html` in a browser to inspect the generated SVG report.

## Status

This crate is not published. It is intentionally separate from the core crates so
demo rendering code does not become a dev-dependency of foundational crates.

## Minimum Supported Rust Version

This crate follows the workspace MSRV: **Rust 1.88** and later.
