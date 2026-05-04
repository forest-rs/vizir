# `vizir_backend_imaging`

`imaging` command-stream adapter for evaluated `vizir_core` marks.

This crate keeps the VizIR core renderer-agnostic while letting downstream renderers consume an
`imaging::record::Scene` or any `imaging::PaintSink`. Text marks remain unshaped in `vizir_core`,
so callers provide a text painter when they want text emitted as imaging glyph runs.

Enable the `parley` feature to use `ParleyTextPainter`, a small default text painter that shapes
unstyled text with system fonts and emits imaging glyph runs.
