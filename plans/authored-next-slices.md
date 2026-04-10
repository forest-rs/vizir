# Authored Spec Next Slices

Fence:
- `vizir_charts::spec` owns authored chart composition and lowering boundaries; it explicitly does
  not own generic runtime table/string storage or full Vega-Lite resolution semantics.

Current state:
- Unit specs lower through a real authored seam.
- Shared-plot layering exists with child-local transforms and unit-shaped child entries, while the
  base child still owns the shared x/domain and color/legend shell.
- Layer children can now carry literal fill/stroke/opacity styles through the same lowered seam.
- `rule` now lowers as a full-span threshold mark, including layered rule overlays with child-local
  aggregate transforms.
- Unit specs now support grouped categorical color bars, plus explicit stacked bars through
  stack-derived `y`/`y2` spans, in the same lowering path.
- Structural `order` and `detail` channels lower for line/area marks.
- Styling channels now include point-local `opacity`, `stroke`, and `strokeWidth`.
- The parser-facing adapter and feature-gated JSON path target the same seam through checked-in
  fixtures.

Goals:
- Keep tightening layered diagnostics now that unit-shaped child entries are real.
- Decide the next useful structural channel or composition slice without widening domain semantics.
- Keep fixture-driven JSON coverage aligned with the support matrix.

Non-goals:
- Full nested unit specs inside `layer`.
- Full scale/domain conflict resolution across unrelated child specs.
- Generic string-valued table columns in `vizir_core`.

Planned slices:
1. Next structural channel/composition decision
   - Decide whether the next gain is richer styling or broader transform support.
   - Do not blur the current shared-domain fence accidentally.
2. Layered diagnostics + conflict tests
   - Keep shared data and shared plot guides.
   - Make fences like `color + detail`, aggregated `order`, and base-child domain ownership
     explicit in tests and docs.
3. Fixture-driven expansion
   - Add new checked-in JSON fixtures only for slices that are fully tested and documented.
   - Reuse fixtures from tests and the demo where practical.

Main risks:
- Base-child inheritance can make layering look more general than the domain policy really is.
- Structural channels can imply broader Vega-Lite parity than the current seam actually supports.
- JSON fixtures can drift unless the support matrix stays current.

Acceptance bar:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
