# Authored Spec Support Matrix

This file is the checked-in source of truth for what the experimental authored-spec path supports
today.

Scope:
- `vizir_charts::spec`
- `vizir_charts::spec_adapter`
- `vizir_charts` `json` feature

Status meanings:
- `supported`: implemented and covered by tests/demo paths
- `partial`: implemented with explicit fences or notable limitations
- `missing`: not implemented in the authored-spec path today

## Current shape

### Unit spec model

| Area | Status | Notes |
|---|---|---|
| Single unit chart | supported | One plot, optional title, x/y axes, optional legend |
| Shared-plot `layer` spec | partial | Shared data with child-local transforms, unit-shaped child entries, and literal child styles; later children can inherit base-child defaults, but they may not fork the shared x/domain or color/legend shell |
| Existing input table via `DataRef::Table` | supported | No URL/data loading in authored spec path |
| Deterministic lowering to `Program + ChartSpec + series plan` | supported | Same seam used by demos/tests |
| Multi-view composition beyond shared-plot `layer` (`facet`, `repeat`, `concat`) | missing | Not modeled yet |
| Params / selections / event streams | missing | No interaction model yet |

### Marks

| Mark | Status | Notes |
|---|---|---|
| `bar` | supported | Ordinal/nominal `x`, quantitative `y`, plus grouped categorical `color` |
| `line` | supported | Quantitative/temporal `x`, quantitative `y` |
| `point` | supported | Quantitative/temporal `x`, quantitative `y`, plus point-only `size` and `shape` channels |
| `area` | supported | Plain area, categorical color-split area, ranged area via `y2`, paired-edge area via `x2` + `y2` |
| `rule` | partial | Full-span threshold line from exactly one authored `x` or `y` channel; useful in layered overlays |
| `text` | partial | Numeric text labels over shared x/y scales; text field formatting is supported, but string-backed text data is not |
| `rect`, `arc`, `tick`, `trail`, `image`, `geoshape` | missing | Some runtime/chart primitives exist, but not in authored spec lowering |

### Channels

| Channel | Status | Notes |
|---|---|---|
| `x` | supported | Ordinal, nominal, quantitative, temporal; `rule` uses x for vertical full-span thresholds |
| `y` | supported | Quantitative only in lowering slice; `rule` uses y for horizontal full-span thresholds |
| `x2` | partial | Area-only today; requires `y2` |
| `y2` | partial | Area-only today |
| `color` | partial | Categorical split with legend generation; bar marks lower as grouped bars, not stacked bars |
| `size` | partial | Point-only; quantitative values map into a fixed visual size range |
| `shape` | partial | Point-only; distinct values map into a fixed symbol palette |
| `opacity` | partial | Bar/point/text only; quantitative values map into a fixed alpha range |
| `stroke` | partial | Point-only; categorical values map into a fixed stroke palette |
| `strokeWidth` | partial | Point-only; quantitative values map into a fixed stroke-width range |
| `text` | partial | Supported for `text` marks; text-as-annotation on other marks is not lowered yet |
| `order` | partial | Line/area only; sorts within each rendered series; not yet supported with aggregated `y` |
| `detail` | partial | Line/area only; categorical split without legend or color encoding |
| `tooltip`, `row`, `column`, etc. | missing | Not modeled in authored seam |

### Transforms

| Transform | Status | Notes |
|---|---|---|
| `filter` | partial | Requires explicit `columns` carry-through list in adapter/json layer |
| `sort` | partial | Requires explicit `columns` carry-through list in adapter/json layer |
| `aggregate` | supported | Derived output aliases supported via adapter/json path |
| `bin` | partial | Requires explicit `columns` carry-through list |
| `stack` | partial | Requires explicit `columns`; output aliases supported |
| Calculate/formula, joinaggregate, window, fold, pivot, flatten, lookup, density, regression, etc. | missing | Not in authored seam today |

### Parser-facing layers

| Layer | Status | Notes |
|---|---|---|
| Rust-authored `UnitSpec` | supported | Primary lowering target |
| Rust-authored `LayerSpec` | partial | Narrow shared-plot layering with child-local transforms, unit-shaped child entries, and literal child fill/stroke/opacity styles; explicit rejection for conflicting child x/color shells |
| Name-based `ParsedUnitSpec` adapter | supported | Resolves field names and derived aliases into `ColumnId`s |
| Name-based `ParsedLayerSpec` adapter | partial | Shared data plus child-local transforms, unit-shaped child entries, and literal child styles; the shared shell is still validated during lowering |
| Narrow JSON parser behind `json` feature | partial | Supports the current unit slice plus shared-plot `layer` with unit-shaped child entries, child-local transforms, literal child styles, and structural `order`/`detail` channels |
| Full Vega-Lite JSON coverage | missing | Current JSON parser is intentionally narrow |

## Important fences

- `color` splitting is currently rejected for `text`.
- Bar `color` lowering currently produces grouped bars only; stacking is not modeled in the authored
  seam yet.
- Shared `layer` currently keeps one shared plot shell. Child entries can be fully specified with
  their own mark, transforms, and encoding block, and later children can inherit
  positional/grouping defaults from the base child, but the base child still owns the shared x/y
  domains and color/legend shell. Independent per-child domain resolution is not modeled yet.
- `rule` currently means a full-span threshold mark, not an arbitrary segment. It requires exactly
  one authored `x` or `y` channel, and layered rules do not inherit positional channels from the
  shared shell.
- Literal child styles currently support constant `fill`, `stroke`, and `opacity` through the
  layer seam, but they may not be combined with conflicting data-driven channels like shared
  `color`, child `stroke`, or child `opacity`.
- `aggregate` on `x`, `x2`, `color`, `y2`, `opacity`, `stroke`, `strokeWidth`, `order`, `detail`, and `text` is rejected.
- `x2`/`y2` are currently only supported on `area`.
- `opacity` currently requires a quantitative channel and is only supported on `bar`, `point`, and `text`.
- `stroke` and `strokeWidth` currently require point marks; `stroke` must be categorical and
  `strokeWidth` must be quantitative.
- `order` and `detail` are currently line/area-only, `detail` must be categorical, and
  `color + detail` is rejected.
- `text` currently formats numeric columns only; string-backed table data is not in the runtime yet.
- The JSON/parser path is feature-gated and should stay narrower than the runtime until support is
  proven.

## Update rule

When support changes:
1. update this file,
2. update tests/demos for the new supported slice,
3. update `plans/spec-ir.md` if the architecture or fences changed.
