<div align="center">

# vizir_backend_svg

**SVG adapter for evaluated VizIR mark diffs.**

[![Latest published version.](https://img.shields.io/crates/v/vizir_backend_svg.svg)](https://crates.io/crates/vizir_backend_svg)
[![Documentation build status.](https://img.shields.io/docsrs/vizir_backend_svg.svg)](https://docs.rs/vizir_backend_svg)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=vizir_backend_svg --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

SVG rendering adapter for evaluated `vizir_core` marks.

`vizir_backend_svg` owns conversion from [`vizir_core::MarkDiff`] streams into an SVG
document. It does not own chart construction, mark evaluation, or browser integration.

<!-- cargo-rdme end -->

## Usage

Apply `MarkDiff` values to `SvgScene`, optionally set a view box, then serialize the retained
scene with stable `(z_index, MarkId)` ordering.

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
