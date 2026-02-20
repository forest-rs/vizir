# vizir_text_web

`vizir_text_web` provides a `wasm32`-oriented `vizir_text::TextMeasurer` implementation.

On `wasm32` targets, it measures text using an offscreen HTML canvas via
`web-sys`/`wasm-bindgen` (`CanvasRenderingContext2d::measureText`), similar to Vega’s
approach.

On non-`wasm32` targets, `WebTextMeasurer` falls back to `vizir_text::HeuristicTextMeasurer`
so the crate can still compile as part of a wider workspace.
