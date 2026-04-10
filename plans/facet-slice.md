# Facet Slice

Fence:
- `vizir_charts::spec` owns narrow one-field facet composition over already-lowered unit/layer
  specs; it explicitly does not own independent scale resolution, nested facet trees, or general
  Vega-Lite multi-view semantics in this slice.

Goals:
- Add a small authored facet shell for one categorical field.
- Reuse existing unit/layer lowering rather than inventing a second chart runtime path.
- Keep the JSON/adapter/demo/docs path aligned with the same narrow seam.

Non-goals:
- Independent per-facet scales or legends.
- Facet over arbitrary child specs with conflicting domains.
- `row`/`column` channel semantics, repeated views, or concat/repeat.

Steps:
1. Add a narrow facet spec and lowered facet plan in `vizir_charts::spec`.
2. Partition the input table by one nominal/ordinal field into derived tables.
3. Lower one child unit spec per facet partition with shared sizing and titles.
4. Add a small grid layout/render path for faceted scenes in the demo.
5. Mirror the slice in the parser-facing adapter and feature-gated JSON path.
6. Add fixture/tests/docs for the supported fence.

Risks:
- Domain semantics can look more general than they are if facets silently diverge.
- Legend duplication can get noisy if each cell owns its own legend.
- Layout glue can sprawl if the facet shell leaks chart-specific assumptions outward.
