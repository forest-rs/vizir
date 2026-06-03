<div align="center">

# vizir_core

**Minimal incremental visualization runtime core.**

[![Latest published version.](https://img.shields.io/crates/v/vizir_core.svg)](https://crates.io/crates/vizir_core)
[![Documentation build status.](https://img.shields.io/docsrs/vizir_core.svg)](https://docs.rs/vizir_core)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=vizir_core --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

`vizir_core`: minimal incremental viz runtime (tables, signals, marks, diffs).

This crate provides:
- versioned inputs ([`Table`]/[`Signal`]), including column-level table versions
- stable mark identity ([`MarkId`])
- UI-neutral mark semantics ([`MarkMetadata`], [`MarkRole`], [`DatumRef`])
- explicit dependency tracking ([`InputRef`]) with helper constructors for common inputs
- incremental evaluation + diff output ([`MarkDiff`])
- per-kind mark payloads ([`MarkPayload`])

It intentionally does NOT provide a full visualization grammar.

Conceptually, a chart frontend can:
- store data in a [`Table`] (row keys + optional column access via [`TableData`])
- store interaction state in [`Signal`]s (zoom, selection, etc.)
- generate one [`Mark`] per row (bars/points/labels) with stable [`MarkId`]s
- call [`Scene::tick_table_rows`] and apply the resulting [`MarkDiff`] stream to a renderer.

<!-- cargo-rdme end -->

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
