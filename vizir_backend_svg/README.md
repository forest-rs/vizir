# `vizir_backend_svg`

SVG rendering adapter for evaluated `vizir_core` marks.

This crate keeps SVG output outside the renderer-agnostic core. Consumers apply `MarkDiff` values
to `SvgScene`, then serialize the retained marks with stable `(z_index, MarkId)` ordering.
