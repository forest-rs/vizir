# vizir_text_parley

`vizir_text_parley` provides a `vizir_text::TextMeasurer` implementation backed by
[Parley](https://crates.io/crates/parley).

This is intended for native (non-web) use where you want shaping-aware text metrics.
Font loading/resolution is still an evolving story for VizIR; this crate focuses on
measurement and keeps higher-level policy out of `vizir_text`.
