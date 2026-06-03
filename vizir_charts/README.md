<div align="center">

# vizir_charts

**Vega-ish chart building blocks for VizIR.**

[![Latest published version.](https://img.shields.io/crates/v/vizir_charts.svg)](https://crates.io/crates/vizir_charts)
[![Documentation build status.](https://img.shields.io/docsrs/vizir_charts.svg)](https://docs.rs/vizir_charts)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=vizir_charts --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Vega-ish chart building blocks for `vizir_core`.

This crate is a small, reusable layer above `vizir_core`:
- **Scales** map data values into screen coordinates.
- **Guides** (axes, legends) are built by generating `vizir_core::Mark`s.

It is designed so higher-level frontends (a Rust DSL, or a future Vega/Vega-Lite
lowering layer) can compile down to:
- input tables/signals, and
- a set of stable-identity marks (with encodings) suitable for incremental diffing.

Text shaping and layout are out of scope; text marks store unshaped strings.

<!-- cargo-rdme end -->

## Feature Flags

- `libm` (default): keeps the crate usable in `no_std + alloc` builds.
- `std`: forwards standard-library support to dependencies.
- `json`: enables the narrow serde-backed JSON parser for the experimental authored-spec seam.

The JSON parser intentionally covers only the checked support slice. See
[`../plans/support-matrix.md`](../plans/support-matrix.md).

## Minimum Supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE] or <http://www.apache.org/licenses/LICENSE-2.0>), or
- MIT license ([LICENSE-MIT] or <http://opensource.org/licenses/MIT>),

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Contribution

Contributions are welcome by pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[LICENSE-APACHE]: https://github.com/forest-rs/vizir/blob/main/LICENSE-APACHE
[LICENSE-MIT]: https://github.com/forest-rs/vizir/blob/main/LICENSE-MIT
