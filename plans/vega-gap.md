# Vega(-Lite) Gap Inventory (Schema-Driven)

This document is a **schema-driven inventory** of what Vega-Lite and Vega can express, plus a
first-pass mapping of what VizIR supports today.

Sources (local copies):
- Vega-Lite JSON Schema: `../../custodian/vega-lite/build/vega-lite-schema.json`
- Vega JSON Schema: `../../custodian/vega/docs/vega-schema.json`
- Vega-Lite API docs/examples (optional, human-friendly): `../../custodian/vega-lite-api`

Regenerating the schema lists (ad-hoc):
- Use small scripts that walk `definitions` and follow local `#/definitions/...` `$ref`s.
- Avoid trying to fully “resolve” the entire schema graph; for inventory, simple ref-following is enough.

## 1) Vega-Lite: Top-Level Spec Surface

Resolved from `#/definitions/TopLevelSpec`, the top-level keys are:

- `$schema`
- `align`
- `autosize`
- `background`
- `bounds`
- `center`
- `columns`
- `concat`
- `config`
- `data`
- `datasets`
- `description`
- `encoding`
- `facet`
- `hconcat`
- `height`
- `layer`
- `mark`
- `name`
- `padding`
- `params`
- `projection`
- `repeat`
- `resolve`
- `spacing`
- `spec`
- `title`
- `transform`
- `usermeta`
- `vconcat`
- `view`
- `width`

### Composition forms (things Vega-Lite “can be”)

- Unit spec: `mark` + `encoding` (+ optional `data`, `transform`, `projection`)
- Layered spec: `layer: [...]`
- Facet spec: `facet: ...` + `spec: ...`
- Repeat spec: `repeat: ...` + `spec: ...`
- Concat specs: `concat` / `hconcat` / `vconcat`

## 2) Vega-Lite: Marks

From `#/definitions/Mark`:

- `arc`
- `area`
- `bar`
- `image`
- `line`
- `point`
- `rect`
- `rule`
- `text`
- `tick`
- `trail`
- `circle`
- `square`
- `geoshape`

Also relevant:
- Composite marks are distinct concepts: `BoxPlot`, `ErrorBar`, `ErrorBand` (see `CompositeMark`).

## 3) Vega-Lite: Encodings / Channels

From `#/definitions/FacetedEncoding` (41 channels):

- `angle`
- `color`
- `column`
- `description`
- `detail`
- `facet`
- `fill`
- `fillOpacity`
- `href`
- `key`
- `latitude`
- `latitude2`
- `longitude`
- `longitude2`
- `opacity`
- `order`
- `radius`
- `radius2`
- `row`
- `shape`
- `size`
- `stroke`
- `strokeDash`
- `strokeOpacity`
- `strokeWidth`
- `text`
- `theta`
- `theta2`
- `time`
- `tooltip`
- `url`
- `x`
- `x2`
- `xError`
- `xError2`
- `xOffset`
- `y`
- `y2`
- `yError`
- `yError2`
- `yOffset`

## 4) Vega-Lite: Transforms (Operators)

From `#/definitions/Transform` (19 variants) and their primary configuration keys:

- `AggregateTransform`: `aggregate`, `groupby`
- `BinTransform`: `bin`, `field`, `as`
- `CalculateTransform`: `calculate`, `as`
- `DensityTransform`: `density`, `as`, `bandwidth`, `extent`, `groupby`, `steps`, ...
- `ExtentTransform`: `extent`, `param`
- `FilterTransform`: `filter`
- `FlattenTransform`: `flatten`, `as`
- `FoldTransform`: `fold`, `as`
- `ImputeTransform`: `impute`, `key`, `groupby`, `method`, `value`, `frame`, ...
- `JoinAggregateTransform`: `joinaggregate`, `groupby`
- `LoessTransform`: `loess`, `on`, `as`, `bandwidth`, `groupby`
- `LookupTransform`: `lookup`, `from`, `as`, `default`
- `QuantileTransform`: `quantile`, `probs`, `step`, `as`, `groupby`
- `RegressionTransform`: `regression`, `on`, `as`, `method`, `order`, `extent`, `params`, `groupby`
- `TimeUnitTransform`: `timeUnit`, `field`, `as`
- `SampleTransform`: `sample`
- `StackTransform`: `stack`, `groupby`, `sort`, `offset`, `as`
- `WindowTransform`: `window`, `frame`, `groupby`, `sort`, `ignorePeers`
- `PivotTransform`: `pivot`, `value`, `groupby`, `op`, `limit`

## 5) Vega-Lite: Axes + Legends (Config Surface)

This is a major gap area for us today; Vega-Lite can configure a *lot* here.

### Axis

From `#/definitions/Axis` (78 keys). High-signal examples:

- `orient`, `offset`, `position`
- `tickCount`, `tickMinStep`, `values`, `format`, `formatType`
- `grid`, `gridColor`, `gridDash`, `gridWidth`, ...
- `ticks`, `tickSize`, `tickPadding`, `tickBand`, ...
- `labels`, `labelAngle`, `labelOverlap`, `labelPadding`, `labelFlush`, ...
- `title`, `titleAngle`, `titlePadding`, `titleAlign`, ...

### Legend

From `#/definitions/Legend` (66 keys). High-signal examples:

- `orient`, `offset`, `columns`, `columnPadding`, `rowPadding`
- `format`, `formatType`, `labelOverlap`
- symbol styling: `symbolType`, `symbolSize`, `symbolStrokeWidth`, ...
- gradient legends: `gradientLength`, `gradientThickness`, ...

## 6) Vega: Top-Level Spec Surface

The Vega schema copy we have is still useful for “macro shape”:

- `$schema`
- `autosize`
- `axes`
- `background`
- `config`
- `data`
- `description`
- `encode`
- `height`
- `layout`
- `legends`
- `marks`
- `padding`
- `projections`
- `scales`
- `signals`
- `style`
- `title`
- `usermeta`
- `width`

Notably: Vega explicitly includes `signals`, `scales`, `marks`, and guide lists at the top level.

## 7) Where VizIR Is Today (First Pass)

This section is intentionally “coarse”; it’s for prioritization, not compliance.

### Runtime / marks

- Mark geometry kinds: `Rect`, `Text`, `Path` (`vizir_core::MarkKind`).
- Stable identity + incremental diffs: enter/update/exit via `MarkDiff`.

### Scales (chart layer)

- Continuous: `linear`, `log`, `time` (current time is numeric seconds).
- Discrete: `point`, `band`.

### Axes / legends

- One `AxisSpec` with `orient` (Vega-ish) and a subset of label/tick/grid/title settings.
- One simple swatch legend.
- Measure/arrange is present, but text metrics are heuristic (for now).

### Transforms (separate crate)

- Implemented (M0): `Filter`, `Project`, `Sort`, `Aggregate`, `Bin`, `Stack` (with offsets including wiggle/normalize/center/zero).
- Missing: most of the rest of Vega-Lite’s transform surface (see list above).

## 8) Big Missing Features vs Vega-Lite (Checklist)

### Data + parsing
- [ ] URL-based data loading and format parsing (`data.url`, `data.format`).
- [ ] Named datasets (`datasets`) and dataset reuse/resolution.
- [ ] Inline `values` beyond ad-hoc demo wiring.

### Parameters / interaction
- [ ] `params` compilation target (selections, bindings, event streams → signals).
- [ ] Tooltips: structured tooltip content + formatting.

### Composition / multi-view
- [ ] `layer` as a first-class “chart composition” (beyond hand-wiring marks).
- [ ] `facet` / `repeat` / `concat` support and resolve rules.

### Marks
- [ ] `rule` / `tick` / `trail` parity as Vega-Lite mark types (not just “draw a path”).
- [ ] `image` marks.
- [ ] `geoshape` marks.
- [ ] Composite marks: `boxplot`, `errorbar`, `errorband` as compilable macros.

### Encodings / channels
- [ ] Full channel set (see `FacetedEncoding` list) including `x2/y2`, errors, offsets, theta/radius, geo lat/long, etc.
- [ ] Conditional encodings (`condition`), selection-based encoding changes.
- [ ] `key` encoding (stable join semantics) in a Vega-Lite-compatible way.

### Scales
- [ ] Full scale config parity: `clamp`, `domain`/`range` signals, `padding` behaviors, `nice` everywhere, `zero`, ...
- [ ] Scale resolution across layered/concat views.

### Guides (axes/legends/titles/layout)
- [ ] Axis label overlap policies beyond heuristics (`labelOverlap`, `labelBound`, `labelFlush`, ...).
- [ ] Full legend types (gradient, symbol legends).
- [ ] Autosize/padding/bounds parity (`autosize`, `bounds`, etc).
- [ ] Layout engine for faceting/concats and guide resolution.

### Transforms
- [ ] `calculate`
- [ ] `density`
- [ ] `extent` (as a param-producing transform)
- [ ] `flatten`
- [ ] `fold`
- [ ] `impute`
- [ ] `joinaggregate`
- [ ] `loess`
- [ ] `lookup`
- [ ] `quantile`
- [ ] `regression`
- [ ] `timeUnit` (and a real calendar model)
- [ ] `sample`
- [ ] `window`
- [ ] `pivot`

## 9) Suggested “Vega-Lite Subset Targets” (Practical)

These are concrete targets we can aim at and keep regression-tested via SVG dumps.

- **VL-Subset-0 (charts you can trust):**
  - Marks: `bar`, `line`, `point`, `area`, `text`, `rule`
  - Encodings: `x`, `y`, `color`, `text`, `size` (and minimal `tooltip`)
  - Transforms: `filter`, `aggregate`, `bin`, `sort`, `stack`
  - Scales: `linear`, `band`, `point`, `time` (small), `log` (basic)
  - Guides: axis + swatch legend + title/subtitle; measure/arrange correctness tests

- **VL-Subset-1 (more real-world):**
  - Transforms: `calculate`, `timeUnit`, `lookup`, `window` (min set)
  - Guides: overlap policy, gradient legends, better tick formatting controls

