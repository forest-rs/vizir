# Vega / Vega-Lite Compatibility Plan

This plan tracks “Vega-ish” compatibility work in VizIR/Vizir.

The goal is not 1:1 parity quickly; it is to keep decisions aligned so that a future
Vega/Vega-Lite lowering path remains plausible without redesigning the core.

Local reference copies:
- `../../custodian/vega`
- `../../custodian/vega-lite`
- `../../custodian/vega-lite-api`

Schema inventory / checklist:
- `plans/vega-gap.md`

## Scope + non-goals

**In scope**
- Execution-model compatibility: dataflow, scales, guides, interactivity.
- IR shape that can plausibly be a lowering target for a Vega-ish frontend.
- Deterministic incremental updates (stable identity + diffs) as a first-class requirement.

**Non-goals (for now)**
- Full Vega JSON parsing and full Vega operator parity.
- A monolithic scenegraph identical to Vega’s.
- Browser-level text measurement/shaping as part of core crates.

## Compatibility “drivers” (prioritized)

### 1) Transforms + dataflow operators

Vega’s compiled runtime is a dataflow graph: datasets → transforms → scales → marks.
Right now we have only “tables/signals/marks” with no transform layer.

**Milestones**
- **T0: Transform IR (API + representation)**
  - Introduce a small transform IR with explicit inputs/outputs:
    - `Filter`, `Project`, `Sort`
    - `Bin`, `Aggregate` (groupby) (even if implementation is delayed)
    - `Stack`, `Window` are allowed as IR variants but can be “unimplemented” initially.
  - Decide crate ownership:
    - prefer a separate `vizir_transforms` crate (no_std-first) to keep `vizir_core` small.
  - Define a minimal “table view” interface for transforms that does **not** force an immediate
    Table-v2 decision (see `plans/engine-evolution.md`).

- **T1: Execution model (full recompute to start)**
  - Implement an executor that runs transforms in deterministic order.
  - Full recompute is acceptable at first, but must be structured so patches can be added.
  - Preserve stable row identity:
    - define what it means for a transform output row to map back to input rows.
    - carry provenance keys explicitly.

- **T2: Table patches (incremental propagation)**
  - Define a `TablePatch` model compatible with:
    - insert/update/delete by row key
    - changed columns metadata
  - Add patch propagation through a subset of transforms (start with `Filter`, `Project`, `Sort`).
  - Introduce a scheduler if necessary (dirty set + topo).

**Vega-ish success criteria**
- A Vega-Lite “filter + aggregate + bar” chart can be lowered without bespoke glue.

### 2) Scales + tick formatting parity

Vega relies heavily on D3-inspired tick generation and formatting.
We have basic scales; formatting is currently ad-hoc and intentionally small.

**Milestones**
- **S0: Formatting API**
  - Add a formatting interface that can express Vega-ish numeric formatting, minimally:
    - fixed decimals, significant digits, trimming, thousands separators (later)
  - Keep it `no_std`-friendly and avoid pulling in heavy formatting crates prematurely.
  - Wire formatting through:
    - `AxisSpec` (tick labels)
    - legend labels (for binned/quantized domains later)

- **S1: Tick generation / nice domain behavior**
  - Align `tickCount` behavior with Vega expectations:
    - domain rounding (“nice”) and tick placement.
  - Improve `ScaleLog` tick behavior (minor ticks later).
  - Expand `ScaleTime` toward calendar-aware intervals (days/months/years) when we have a date/time story.

- **S2: Domain inference hooks**
  - Domain inference should work as a transform-like operation:
    - min/max over columns
    - aggregate-derived domains
  - Keep the boundary clean so the chart layer can request “compute domain” without coupling
    to a specific table substrate.

**Vega-ish success criteria**
- Axes from common Vega-Lite examples produce ticks/labels that look “plausibly Vega”.

### 3) Guides + layout + overlap policies

Vega’s guide system is mature: axes and legends are “guide specs” that compile to mark groups,
with multiple overlap avoidance strategies.

**Milestones**
- **G0: Label overlap policy**
  - Add a deterministic policy for tick labels:
    - first/last only
    - every-N
    - hide overlaps based on measured bounds
  - Make policies testable and independent of rendering backends.

- **G1: Measure/arrange alignment**
  - Continue converging on a measure/arrange model that can later align with Understory Display.
  - Improve rotated label measurement, title placement, and legend measurement.

- **G2: More legend types**
  - Symbol legend (for points/lines) and categorical legends beyond swatches.
  - Gradient legend for continuous color scales (later).

**Vega-ish success criteria**
- A chart with long tick labels does not overlap/clamp incorrectly; legends don’t clip.

### 4) Selections + event streams → signals

Vega’s interactivity is built on event streams and signals; Vega-Lite compiles selections into signals.
We have `Signal`, but no event binding model and no demos that prove incremental updates end-to-end.

**Milestones**
- **I0: Demo-level interactivity**
  - Add an HTML demo that:
    - embeds diffs for several signal states
    - applies diffs in the browser (minimal JS) to show hover/selection.
  - Ensure only affected marks update (diffs are small).

- **I1: Selection primitives**
  - Define minimal selection concepts:
    - `hovered_mark_id`, `selected_row_key(s)`, `brush_rect`, `zoom_domain`
  - Keep these as adapters on top of `Signal` rather than baking into `vizir_core`.

**Vega-ish success criteria**
- Hover/selection interactions look like Vega-Lite examples and update incrementally.

### 5) Compilation boundary / spec IR (future Vega/Vega-Lite lowering target)

We want to stay programmatic for now, but still keep a plausible compilation boundary.

**Milestones**
- **C0: Internal “spec IR”**
  - Define a small IR that mirrors Vega concepts enough to lower later:
    - datasets, transforms, scales, axes, legends, mark groups
  - IR must lower into:
    - tables/signals/transforms
    - stable mark sets (with `MarkId` and encodings)

- **C1: Vega-Lite subset in Rust**
  - Implement a Rust builder DSL that can express a subset of Vega-Lite:
    - point/line/bar + axes/legends + simple transforms
  - Do not parse JSON yet; focus on lowering pipeline correctness.

**Vega-ish success criteria**
- We can take a few canonical Vega-Lite examples and express them with the IR + lowering path.

## Tracking

- Treat this plan as the “north star” for compatibility and keep the other plans (transforms/scales/guides)
  aligned with it.
- Add links to concrete issues/PRs as work proceeds.
