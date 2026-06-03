# vizir_examples

Scratch examples for `vizir_core` runtime behavior.

This binary is intentionally separate from the core crate so examples can depend
on `std` without adding dev-dependencies or feature pressure to foundational
crates.

## Running

```sh
cargo run -p vizir_examples
```

The current example prints mark diff counts and sample `Enter`/`Update`/`Exit`
payloads for a small scene.

## Status

This crate is not published. It is a workspace-local place for small examples and
manual runtime checks.

## Minimum Supported Rust Version

This crate follows the workspace MSRV: **Rust 1.88** and later.
