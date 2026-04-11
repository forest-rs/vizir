# Spec IR + Lowering Slice

## Goal

Define the first internal spec/lowering slice that moves Vizir from “Vega-ish primitives” toward
“a real Vega-Lite-compatible compilation boundary”.

This slice is intentionally small:
- one unit chart
- one dataset already present in a `vizir_core::Scene`
- a small transform subset
- `bar` / `line` / `point` / `area`
- `x`, `x2`, `y`, `y2`, `color`, `text`
- generated `ChartSpec` + series mark specs + optional transform program

The goal is not full JSON parsing yet. The goal is to prove that a canonical Vega-Lite-like chart can
be expressed in one authored IR and lowered without bespoke demo glue.

For the current supported slice and fences, see `plans/support-matrix.md`.

## Fence

This layer owns authored chart intent and lowering decisions; it explicitly does not own runtime
evaluation, renderer adapters, or full Vega/Vega-Lite parsing.

## Why this slice

Today the repo has the right ingredients in isolation:
- `vizir_core` owns incremental marks/diffs
- `vizir_transforms` owns a small transform IR/executor
- `vizir_charts` owns scales, guides, layout, and chart-layer mark specs

What is missing is the seam that connects them as one compilation path.

Without that seam, every new example is still hand-wired:
- choose transforms manually
- choose scales manually
- choose axes/legends manually
- choose mark specs manually

That means we cannot honestly say “this is a Vega-Lite subset” yet, even for the parts we
already have.

## Non-goals

- Full Vega or Vega-Lite JSON parsing
- Full channel parity
- Multi-view composition (`layer`, `facet`, `repeat`, `concat`)
- Browser event streams / `params`
- New table substrate work
- A public stable API on day one

## Options

### Option A: Put the spec IR directly in `vizir_charts`

Pros:
- fastest path
- easy access to chart-layer scale/guide/mark builders
- no new crate boundary yet

Cons:
- risks mixing authored intent with chart rendering helpers too early

### Option B: New `vizir_spec` crate now

Pros:
- cleaner architecture long-term
- clearly separates authored spec from lowering/rendering

Cons:
- adds a new crate before the shape is proven
- likely churn while the first slice is still moving

### Option C: Put a private/internal module in `vizir_charts`, with a planned future extraction

Pros:
- preserves momentum
- keeps public API small
- gives us a clean extraction seam later

Cons:
- requires discipline to avoid leaking ad-hoc internals into the public surface

## Chosen design

Start with Option C.

Add an internal, not-yet-stable spec/lowering module inside `vizir_charts`. Prove the shape on a
small unit-spec subset. Extract into its own crate only after two or three chart families lower
cleanly through the same path.

This keeps the public API calm while still forcing the architectural seam to exist.

## First slice: supported authored IR

### View shape

Current supported view shapes:
- one unit chart
- one narrow shared-plot layer with shared data, child-local transforms, unit-shaped child entries, and literal child styles
- one narrow one-field facet over a unit-shaped child chart
- optional title
- optional x/y axes
- optional color legend

No repeat or concat yet. Facet is still a narrow fixed-grid slice.

### Data shape

Only an existing input table:

```rust
pub enum DataRef {
    Table(TableId),
}
```

This keeps the first slice focused on lowering, not on parsing/loading.

### Transform shape

Support only the transform variants we already execute well:
- `Filter`
- `Sort`
- `Calculate`
- `JoinAggregate`
- `Aggregate`
- `Bin`
- `Fold`
- `Window`
- `Stack`

The authored transform IR can be one of:

1. Thin wrapper over `vizir_transforms::Transform`
2. Slightly more user-facing spec that lowers into `vizir_transforms::Transform`

For the first slice, prefer a slightly more authored form so field intent stays visible.

### Mark shape

Support:
- `Bar`
- `Line`
- `Point`
- `Area`
- `Rule`
- `Text`

### Channel shape

Support only:
- `x`
- `x2`
- `y`
- `y2`
- `color`
- `size`
- `shape`
- `opacity`
- `stroke`
- `strokeWidth`
- `order`
- `detail`
- `text`

Layering rule:
- one designated base child owns the shared chart shell and current x/y domains
- other children may be fully specified with their own mark, transforms, and encoding block
- later children may inherit base-child positional/grouping defaults
- children may also carry literal fill/stroke/opacity styles through the layer seam
- child entries may not fork the shared x/domain or color/legend shell today

And only the most useful authored meanings:
- `x`: position or grouped category
- `x2`: secondary position for ranged areas
- `y`: position or aggregate value
- `y2`: secondary position for ranged areas and explicit bar spans
- `rule`: a full-span threshold from exactly one authored `x` or `y` channel
- `color`: categorical series split for legend/fill
  for bar marks this currently means grouped bars by default, or stacked bars when an explicit
  stack transform drives `y`/`y2`
- `text`: direct label channel for simple annotations later

## Proposed minimal authored types

These names are illustrative; the exact syntax can stay private until proven.

```rust
pub struct UnitSpec {
    pub data: DataRef,
    pub transforms: Vec<TransformSpec>,
    pub mark: MarkDef,
    pub encoding: EncodingSet,
    pub width: f64,
    pub height: f64,
    pub title: Option<String>,
}

pub enum MarkDef {
    Bar,
    Line,
    Point,
    Area,
}

pub struct EncodingSet {
    pub x: Option<ChannelDef>,
    pub x2: Option<ChannelDef>,
    pub y: Option<ChannelDef>,
    pub y2: Option<ChannelDef>,
    pub color: Option<ChannelDef>,
    pub text: Option<ChannelDef>,
}

pub struct ChannelDef {
    pub field: ColumnId,
    pub kind: FieldKind,
    pub aggregate: Option<AggregateOp>,
    pub title: Option<String>,
}

pub enum FieldKind {
    Quantitative,
    Ordinal,
    Nominal,
    Temporal,
}
```

Important constraint: this IR owns authored meaning, not geometry.

It should say:
- “x is ordinal category”
- “y is aggregated sum”
- “color splits series by category”

It should not say:
- exact plot rectangle
- exact axis rect
- exact `MarkId` values
- exact `ScaleContinuous` instances

Those are lowering outputs.

## Lowering outputs

The first slice should lower into an explicit plan, not directly mutate a `Scene`.

```rust
pub struct LoweredUnit {
    pub input_table: TableId,
    pub program: Option<vizir_transforms::Program>,
    pub derived_tables: Vec<TableId>,
    pub chart: ChartSpec,
    pub series: SeriesPlan,
}

pub enum SeriesPlan {
    Bar(BarSeriesPlan),
    Line(LineSeriesPlan),
    Point(PointSeriesPlan),
}
```

This boundary matters. It gives us:
- something testable without rendering
- a stable seam for future parser work
- a place to insert diagnostics later

## Lowering rules for the first slice

### Rule 1: authored transforms lower before scale/guide construction

The lowering pipeline should be:

1. authored `UnitSpec`
2. transform plan
3. resolved source/output table ids
4. scale inference/spec choice
5. axis/legend/chart construction
6. series mark plan

This prevents chart code from having to guess whether it is looking at raw or transformed data.

### Rule 2: scale choice comes from field kind + mark kind

Initial heuristics:
- `Bar` + ordinal `x` -> `ScaleBand`
- `Line`/`Point` + quantitative `x` -> `ScaleLinear`
- quantitative `y` -> `ScaleLinear`
- temporal `x` -> `ScaleTime`

### Rule 3: color is categorical only in the first slice

If `color` is present:
- create a categorical legend
- pick fills/strokes from a deterministic palette

Do not attempt continuous color scales yet.

### Rule 4: one transformed table per unit chart, plus derived per-series views only when needed

For `Line` and `Point` with color grouping:
- lower to a transformed base table
- then derive per-series filtered/sorted views if necessary

This matches current stacked chart helpers and avoids inventing grouping/facet semantics too early.

### Rule 5: diagnostics are part of the design

The lowering entry point should return explicit errors for:
- missing required channel combinations
- unsupported field kind + mark kind pairings
- unsupported transform sequences
- unsupported legend/axis requests

“Unsupported” must be visible and testable, not hidden in panics.

## Concrete first examples

These are the first examples worth supporting through the new path.

### Example A: Vega-Lite-ish aggregate bar

Intent:
- input table with `category`, `value`
- aggregate sum(value) by category
- x = category
- y = sum(value)
- mark = bar

This proves:
- transform lowering
- ordinal x + quantitative y
- bar mark lowering
- axes and basic guide generation

### Example B: grouped point chart

Intent:
- input table with `x`, `y`, `series`
- x = quantitative
- y = quantitative
- color = series
- mark = point

This proves:
- legend generation
- categorical color split
- per-series planning

### Example C: line chart with sort

Intent:
- input table with `x`, `y`, `series`
- sort by x
- mark = line
- optional color = series

This proves:
- transform + line lowering
- path mark generation through the same authored IR

## File-level implementation sketch

If we implement this next, keep the write scope narrow:

- `vizir_charts/src/spec_ir.rs`
  - internal authored types
- `vizir_charts/src/lowering.rs`
  - `lower_unit(...) -> Result<LoweredUnit, LoweringError>`
- `vizir_charts/src/lib.rs`
  - internal module wiring only, keep exports private at first
- `vizir_charts/src/...tests...`
  - lowering tests for the three canonical examples
- `vizir_examples/src/main.rs` or a new example binary
  - one end-to-end proof path using the lowered plan

Do not add a new crate yet.

## Invariants

- `vizir_core` remains geometric and incremental; no Vega-specific authored concepts move there.
- `vizir_transforms` remains the execution layer for table-to-table operations.
- Authored spec IR never directly constructs scene geometry.
- Lowering is deterministic: same spec + same table ids -> same transform/guide/series plan.
- Unsupported authored shapes fail with structured errors, not silent fallback.

## Risks

### Risk: spec IR becomes a second chart API

Mitigation:
- keep it internal first
- optimize for lowering clarity, not end-user ergonomics

### Risk: lowering logic duplicates chart helper code

Mitigation:
- lower into existing `ChartSpec`, `AxisSpec`, `LegendSwatchesSpec`, and mark specs
- do not rebuild a second guide system

### Risk: transform output table management becomes ad-hoc

Mitigation:
- reserve deterministic derived table ids in one place
- make them part of `LoweredUnit`

## Extension points after the first slice

Once the first unit-slice works, the next additions should be:

1. `Area`
2. `x2` / `y2`
3. `layer`
4. symbol legends
5. a real `key` / provenance story for aggregate outputs
6. parser-facing adapters from JSON/schema shapes into the authored IR

## Checklist

- [ ] Add an internal authored unit-spec IR to `vizir_charts`
- [ ] Add lowering output plan types
- [ ] Implement `bar` lowering with aggregate/groupby
- [ ] Implement `point` lowering with categorical color legend
- [ ] Implement `line` lowering with sort
- [ ] Add lowering diagnostics
- [ ] Add end-to-end tests against three canonical examples
- [ ] Revisit crate extraction only after the slice is proven
