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
- One-field categorical `facet` now lowers a unit-shaped child chart into a fixed grid.
- Structural `order` and `detail` channels lower for line/area marks.
- Styling channels now include point-local `opacity`, `stroke`, and `strokeWidth`.
- Narrow arithmetic `calculate` now lowers through the runtime, authored, parsed, and JSON seams.
- Narrow `joinaggregate` and `window` transforms now lower through the same end-to-end seam.
- Narrow numeric-only `fold` now lowers from wide rows into repeated series slots.
- Narrow one-key `lookup` now lowers from a base scene table into one explicit secondary table.
- Narrow numeric-only `pivot` now lowers from long rows into explicit wide output slots.
- Text marks can read string table lanes for nominal/ordinal labels.
- Shared-layer diagnostics now reject child series that would require a wider y-domain than the
  base child owns.
- The parser-facing adapter and feature-gated JSON path target the same seam through checked-in
  fixtures.

Goals:
- Tighten the facet and layer fences now that both composition seams are real.
- Decide the next useful transform or styling slice without widening domain semantics accidentally.
- Keep fixture-driven JSON coverage aligned with the support matrix.

Non-goals:
- Full nested unit specs inside `layer`.
- Full scale/domain conflict resolution across unrelated child specs or facet cells.
- String-aware transform operators or storage backends.

Planned slices:
1. Facet + layer diagnostics
   - Make facet domain/legend fences and layer shared-shell fences explicit in tests and docs.
   - Do not let narrow composition slices read as full Vega-Lite resolve semantics.
2. Broader transform support
   - Continue with the next useful dataflow slices after `calculate` / `joinaggregate` /
     `window` / `fold` / `lookup` / `pivot`: likely richer window semantics or calmer
     diagnostics around the multi-table and numeric-only fences.
3. Fixture-driven expansion
   - Keep shared data and shared plot guides.
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
