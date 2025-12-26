# Plans

This directory contains “living” plans for major feature areas in VizIR/Vizir.

These files are intentionally lightweight and evolve over time. They exist to:
- make long-term intent explicit,
- keep work aligned with “Vega-ish” goals,
- avoid re-deciding the same design questions repeatedly.

## Conventions

- Keep each plan focused on a single feature area.
- Prefer small sections over long prose.
- Include: current state, goals, non-goals, open questions, staged milestones.
- When a plan implies other work (e.g. transforms imply table diffing), link to the other plan.
- Prefer workspace-relative links to code and docs.

## Source of truth references

We want to stay “Vega-ish” where it makes sense. Local reference copies live at:
- `../../custodian/vega`
- `../../custodian/vega-lite`
- `../../custodian/vega-lite-api`

## Index

- Core + evolution: `plans/engine-evolution.md`
- Marks + mark specs: `plans/mark-specs.md`
- Guides (axes/legends/grid): `plans/guides.md`
- Scales: `plans/scales.md`
- Transforms / dataflow: `plans/transforms.md`
- Rendering/adapters: `plans/rendering.md`
- IO/backends (CSV/Arrow/DataFusion): `plans/io.md`
- Profiling overlay: `plans/profiling.md`
- Performance/SIMD: `plans/perf.md`
- Vega/Vega-Lite compatibility: `plans/vega-compat.md`
