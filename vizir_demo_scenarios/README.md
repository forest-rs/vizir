# `vizir_demo_scenarios`

Shared scenario builders for VizIR renderer demos.

Each scenario builds `vizir_core::Mark` values and a view rectangle without depending on a renderer.
Backends and demo binaries can render the same source through SVG, `imaging`, or future adapters.
