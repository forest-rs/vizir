# Authored Spec Next Slices

Fence:
- `vizir_charts::spec` owns authored chart composition and lowering boundaries; it explicitly does
  not own generic runtime table/string storage or full Vega-Lite resolution semantics.

Current state:
- Unit specs lower through a real authored seam.
- Shared-plot layering exists with child encoding overrides.
- The parser-facing adapter and feature-gated JSON path target the same seam.

Goals:
- Add child-local transforms for narrow shared-plot layering.
- Add one more useful authored mark slice without forcing new runtime storage semantics.
- Move JSON coverage from inline examples toward checked-in fixtures.

Non-goals:
- Full nested unit specs inside `layer`.
- Full scale/domain conflict resolution across unrelated child specs.
- Generic string-valued table columns in `vizir_core`.

Planned slices:
1. Child-local layer transforms
   - Keep shared data and shared plot guides.
   - Let each child append its own transform chain after shared transforms.
   - Keep the base child responsible for the chart shell and current domain policy.
2. Authored text mark slice
   - Add `text` as an authored mark kind.
   - Lower numeric text fields via formatting from existing numeric table access.
   - Keep string-backed text out of scope until runtime storage grows.
3. Fixture-driven JSON coverage
   - Add small checked-in JSON fixtures for supported unit/layer shapes.
   - Reuse fixtures from tests and the demo where practical.

Main risks:
- Child-local transforms can make layering look more general than the domain policy really is.
- Text marks can imply broader string support than the runtime currently provides.
- JSON fixtures can drift unless the support matrix stays current.

Acceptance bar:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
