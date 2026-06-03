<div align="center">

# vizir_transforms

**Vega-ish table transform IR and executor for VizIR.**

[![Latest published version.](https://img.shields.io/crates/v/vizir_transforms.svg)](https://crates.io/crates/vizir_transforms)
[![Documentation build status.](https://img.shields.io/docsrs/vizir_transforms.svg)](https://docs.rs/vizir_transforms)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=vizir_transforms --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Vega-ish table transforms.

This crate provides:
- a small transform IR that models `TableId -> TableId` operators, and
- a full-recompute executor suitable as a first “transform foundations” landing.

The executor is intentionally simple:
- it preserves upstream `row_keys` as stable identity for per-row marks, and
- it only supports numeric (`f64`) columns for now.

<!-- cargo-rdme end -->

## Supported Slice

The current transform IR includes filter, project, sort, bin, aggregate, calculate,
joinaggregate, fold, lookup, pivot, window, and stack operators. Many operators are
intentionally narrow and numeric-first; see
[`../plans/support-matrix.md`](../plans/support-matrix.md) for parser-facing limits.

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
