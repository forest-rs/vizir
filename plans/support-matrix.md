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
| Shared-plot `layer` spec | partial | Shared data with child-local transforms, base-child positional/grouping defaults, and child encoding overrides; proven for overlays like line + point, area + line, and bar + text |
| Existing input table via `DataRef::Table` | supported | No URL/data loading in authored spec path |
| Deterministic lowering to `Program + ChartSpec + series plan` | supported | Same seam used by demos/tests |
| Multi-view composition beyond shared-plot `layer` (`facet`, `repeat`, `concat`) | missing | Not modeled yet |
| Params / selections / event streams | missing | No interaction model yet |

### Marks

| Mark | Status | Notes |
|---|---|---|
| `bar` | supported | Ordinal/nominal `x`, quantitative `y` |
| `line` | supported | Quantitative/temporal `x`, quantitative `y` |
| `point` | supported | Quantitative/temporal `x`, quantitative `y`, plus point-only `size` and `shape` channels |
| `area` | supported | Plain area, categorical color-split area, ranged area via `y2`, paired-edge area via `x2` + `y2` |
| `text` | partial | Numeric text labels over shared x/y scales; text field formatting is supported, but string-backed text data is not |
| `rect`, `rule`, `arc`, `tick`, `trail`, `image`, `geoshape` | missing | Some runtime/chart primitives exist, but not in authored spec lowering |

### Channels

| Channel | Status | Notes |
|---|---|---|
| `x` | supported | Ordinal, nominal, quantitative, temporal |
| `y` | supported | Quantitative only in lowering slice |
| `x2` | partial | Area-only today; requires `y2` |
| `y2` | partial | Area-only today |
| `color` | partial | Categorical split only; legend generation supported |
| `size` | partial | Point-only; quantitative values map into a fixed visual size range |
| `shape` | partial | Point-only; distinct values map into a fixed symbol palette |
| `opacity` | partial | Bar/point/text only; quantitative values map into a fixed alpha range |
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
| Rust-authored `LayerSpec` | partial | Narrow shared-plot layering with child-local transforms, base-child positional/grouping defaults, and child encoding overrides |
| Name-based `ParsedUnitSpec` adapter | supported | Resolves field names and derived aliases into `ColumnId`s |
| Name-based `ParsedLayerSpec` adapter | partial | Shared data plus child-local transforms, base-child defaults, and child encoding overrides |
| Narrow JSON parser behind `json` feature | partial | Supports the current unit slice plus shared-plot `layer` with child-local transforms, base-child defaults, and structural `order`/`detail` channels |
| Full Vega-Lite JSON coverage | missing | Current JSON parser is intentionally narrow |

## Important fences

- `color` splitting is currently rejected for `bar` and `text`.
- Shared `layer` currently keeps one shared plot shell. Child-specific transforms and encoding
  overrides are supported, and later children can inherit positional/grouping defaults from the
  base child, but the base child still owns the shared x/y domains. Per-child nested unit specs
  and independent domain resolution are not modeled yet.
- `aggregate` on `x`, `x2`, `color`, `y2`, `opacity`, `order`, `detail`, and `text` is rejected.
- `x2`/`y2` are currently only supported on `area`.
- `opacity` currently requires a quantitative channel and is only supported on `bar`, `point`, and `text`.
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
