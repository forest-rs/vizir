# Table Data Design

## Goal

Make VizIR's table layer strong enough for real authored visualization without turning
`vizir_core` into a dataframe, Arrow wrapper, or query engine.

The desired path is:

```text
external data -> table adapter -> transforms -> authored/spec lowering -> semantic marks/diffs
```

`vizir_core` should expose small, durable table primitives. Rich data loading, schema naming,
transform execution, and backend-specific storage should live outside core.

## Non-Goals

- Do not add Arrow, DataFusion, CSV, or filesystem IO to `vizir_core`.
- Do not make a dynamic `Value` enum the primary cell model.
- Do not make transforms depend on a renderer or chart crate.
- Do not optimize for SIMD or patches before the basic data model is coherent.
- Do not promise Vega/Vega-Lite parity before row identity and typed schema are explicit.

## Fence

`vizir_core` owns stable table identity, row identity, versions, typed column access, and future
patch contracts; it explicitly does not own data loading, query planning, transform algorithms,
renderer adapters, or authored visualization semantics.

## Current State

- `Table` stores:
  - `TableId`
  - table-level `Version`
  - stable `row_keys: Vec<u64>`
  - `TableSchema`
  - optional per-column versions for `InputRef::TableCol`
  - optional `Box<dyn TableData>`
- `TableSchema` carries known column ids, physical types, and optional names.
- `TableData` exposes typed lanes:
  - `get_f64`
  - `get_i64`
  - `get_u64`
  - `get_bool`
  - `get_str`
  - optional `column_type`
- Existing charts and transforms still consume mostly `f64` lanes.
- `InputRef::TableCol` uses explicit column versions when present and falls back to table-level
  versions otherwise.
- `vizir_transforms::TableFrame` is an owned numeric frame used by the current full-recompute
  transform executor.

The recent typed-lane API is a scaffold, not the full design.

## Invariants

- **Stable identity is separate from row order.** Row keys identify conceptual rows; row indices are
  positions in the current table view.
- **Column identity is not column naming.** `ColumnId` is the runtime token. Name resolution belongs
  in adapters/spec layers.
- **Physical type is not semantic type.** `ColumnType::F64` may represent a quantitative measure, a
  category key, or timestamp seconds. Authored semantics stay above core.
- **Core access is typed.** Callers request a typed lane. Missing or mismatched data returns `None`.
- **Fast paths are optional.** A table can expose per-cell access only; slice/Arrow-backed fast
  paths can be added without invalidating the small trait object path.
- **Patches are semantic, not storage-specific.** Future table patches should talk about row keys
  and changed columns, not Arrow buffers or Vec indices.

## Glossary

- **Row key:** Stable row identity used to preserve mark continuity across frames.
- **Row index:** Current positional index into a table view.
- **Column id:** Stable runtime token for a column.
- **Column name:** Human/authored identifier resolved outside core.
- **Physical type:** Storage/access lane such as `f64`, `bool`, or text.
- **Semantic type:** Authored visualization meaning such as quantitative, nominal, ordinal, or
  temporal.

## Design Options

### Option A: Keep Only Per-Cell Trait Access

Core keeps the current `dyn TableData` shape with typed getters and optional `column_type`.

Pros:
- Smallest core surface.
- Easy for demos, generated tables, and transform outputs.
- Works in `no_std + alloc`.

Cons:
- Per-cell virtual dispatch is a poor fast path for domain inference and transforms.
- No standard schema metadata.
- No direct path to column-level versions or patches.

### Option B: Add Optional Typed Column Views

Keep `TableData` as the base trait, but add optional methods returning typed column views:

```rust
pub enum F64ColumnRef<'a> {
    Slice(&'a [f64]),
    Accessor(&'a dyn F64Column),
}
```

The exact type names are placeholders. The important bit is that bulk code can ask for a column
view and fall back to per-cell access.

Pros:
- Preserves the small trait object path.
- Gives transforms/scales a route to fast contiguous data.
- Keeps Arrow adapters possible without forcing Arrow into core.

Cons:
- More API surface and lifetime care.
- Needs clear semantics for nulls/missing values.
- Must avoid turning core into a collection of ad hoc mini-array traits.

### Option C: Introduce a Dedicated Table Store Crate Now

Create something like `vizir_table` for schema, typed arrays, row identity, patches, and fast paths.
Core would reference a smaller table handle or trait from that crate.

Pros:
- Clean long-term boundary if table work grows.
- Avoids overloading `vizir_core`.
- Could become the shared substrate for transforms, IO, and spec lowering.

Cons:
- Extra crate before the design is proven.
- More churn across current call sites.
- Risks designing an abstract table system before Arrow/adapter needs are concrete.

## Chosen Direction

Use Option B as the next design target, with Option C kept as an extraction path.

That means:

- Keep `Table` and `TableData` in `vizir_core` for now.
- Treat the current typed getters as the stable minimum.
- Add schema and fast-path access incrementally, only after each semantic rule is written down.
- Keep transform-owned `TableFrame` numeric until the runtime needs typed transform operators.
- Do not introduce Arrow/DataFusion dependencies until adapter crates are ready.

This is conservative: it avoids a premature table crate, but it also avoids pretending per-cell
trait access is enough for the ecosystem.

## Proposed Core Surface

Names are illustrative until implemented.

```rust
pub struct Table {
    pub id: TableId,
    pub version: Version,
    pub row_keys: Vec<u64>,
    pub schema: TableSchema,
    pub column_versions: HashMap<ColumnId, Version>,
    pub data: Option<Box<dyn TableData>>,
}

pub trait TableData: fmt::Debug {
    fn row_count(&self) -> usize;
    fn column_type(&self, col: ColumnId) -> Option<ColumnType>;

    fn get_f64(&self, row: usize, col: ColumnId) -> Option<f64>;
    fn get_i64(&self, row: usize, col: ColumnId) -> Option<i64>;
    fn get_u64(&self, row: usize, col: ColumnId) -> Option<u64>;
    fn get_bool(&self, row: usize, col: ColumnId) -> Option<bool>;
    fn get_str(&self, row: usize, col: ColumnId) -> Option<&str>;

    fn f64_column(&self, col: ColumnId) -> Option<F64ColumnRef<'_>>;
    fn text_column(&self, col: ColumnId) -> Option<TextColumnRef<'_>>;
}
```

Open before implementation:

- whether column views should be enum refs, trait refs, or small adapter structs,
- how missing values are represented in bulk views,
- whether text views return `&str` by row, offset buffers, or another abstraction,
- whether column types should include timestamp/category dictionary lanes now or later.

## Row Identity Design

Current `row_keys: Vec<u64>` is enough for one mark per input row. It is not enough for all
transform outputs.

Needed concepts:

- **Origin key:** the stable key from the upstream input row, when a row still corresponds to one
  upstream row.
- **Derived key:** a stable key computed from transform semantics, such as an aggregate group, bin,
  pivot slot, or folded field.
- **Row order:** current ordering after sort/window/facet partitioning.

Likely model:

```text
output row key = hash(transform namespace, origin key(s), derived key parts)
```

Current transform rules:

- Filter preserves upstream row keys.
- Project preserves upstream row keys.
- Sort moves row keys with rows and changes row order only.
- Calculate preserves row keys and adds derived columns.
- Aggregate creates derived keys from group values.
- JoinAggregate preserves row keys and writes aggregate values back per input row.
- Bin preserves row keys and adds derived bin columns.
- Fold creates derived keys from `(origin key, folded field id)`.
- Lookup preserves input row keys and appends looked-up values.
- Pivot creates derived keys from group values.
- Stack/window preserve row keys when they add columns per existing row.

These rules are now in place for the current full-recompute executor and should remain the
baseline before adding incremental table patches.

## Versioning And Patches

Current table-level versions are the coarse invalidation path. Optional column versions let
`InputRef::TableCol` avoid unrelated column updates when callers provide per-column metadata.

Target shape:

- table-level version remains the quick invalidation path,
- optional column-level versions allow `InputRef::TableCol` to avoid unrelated updates,
- optional `TablePatch` carries row/column changes for downstream transforms and renderers.

Sketch:

```rust
pub enum TablePatch {
    RowsInserted { keys: Vec<u64> },
    RowsRemoved { keys: Vec<u64> },
    RowsUpdated { keys: Vec<u64>, columns: Vec<ColumnId> },
    ColumnsUpdated { columns: Vec<ColumnId> },
    Replaced,
}
```

Open questions:

- Do row insertions include order positions, or is order a separate table view update?
- Which adapter crates should maintain column versions directly versus using coarse table bumps?
- Does `Scene` store pending table patches, or does a future scheduler own them?

## Schema And Names

Core should not own names as the primary lookup mechanism, but adapters need a stable place for
schema metadata.

Near-term rule:

- `ColumnId` stays in core.
- Field names are resolved in `vizir_charts::spec_adapter` or future IO/spec crates.
- A future `TableSchema` may be carried beside data, but core APIs should still consume
  `ColumnId`.

Schema metadata likely needs:

- column id,
- optional name,
- physical type,
- optional semantic hint,
- optional nullability/missing-value policy.

The semantic hint must not replace authored `FieldKind`; it is only a fallback/default.

## Transform Implications

`vizir_transforms` can stay numeric-first until the first non-numeric transform is needed.

Before broadening transforms:

- remove or reduce explicit carry-through column lists where possible,
- define row-key generation per transform,
- decide how transform outputs report `ColumnType`,
- decide whether `TableFrame` becomes typed or remains a numeric fast path.

Recommended sequence:

1. Keep `TableFrame` numeric for current operators.
2. Add row-key provenance rules and tests.
3. Add typed output metadata for existing numeric outputs.
4. Only then add string/category-aware transforms.

## Adapter Implications

### CSV

CSV belongs in an adapter crate, not core. It should infer or accept a schema, then produce a small
owned table implementation.

### Arrow

Arrow belongs in an adapter crate. It should implement `TableData` and optional column views over
Arrow arrays where practical.

### DataFusion

DataFusion should consume/produce adapter tables or Arrow-backed tables. It should not become the
default transform executor for `vizir_core`.

## Staged Milestones

### M0: Document Current Scaffold

- Keep current typed getters.
- Update docs and support matrix to state runtime typed lanes exist.
- Keep authored spec and transforms numeric-first.

Status: done.

### M1: Schema Metadata

- Add a minimal schema representation, or document why schema stays adapter-owned for another
  slice.
- Ensure generated/transform tables can report `ColumnType` for all carried columns.
- Add tests for schema/type reporting through transform outputs.

Status: done.

Exit criteria:

- authored/spec lowering can ask whether a field has a usable physical lane,
- current numeric behavior remains unchanged.

### M2: Row Provenance

- Define stable row-key rules for each existing transform.
- Add tests for current transform row-key behavior, including preserving, reordering, and derived
  output keys.
- Make support docs state which transforms preserve origin keys and which derive new keys.

Status: done.

Exit criteria:

- mark identity through transform outputs is intentional, not incidental.

### M3: Column Versions

- Add column-level version tracking without removing table-level versioning.
- Make `InputRef::TableCol` consult column versions when available.
- Preserve table-level invalidation as the fallback.

Status: done.

Exit criteria:

- updating one column can avoid recomputing marks that depend on unrelated columns.

### M4: Optional Column Views

- Add one typed fast path first, likely `f64`.
- Use it in domain inference or transform extraction.
- Keep per-cell fallback.

Exit criteria:

- no caller needs to know the backing store to get bulk numeric reads.

### M5: Table Patches

- Define `TablePatch`.
- Decide whether patches live in `Scene`, transform executor state, or a future scheduler.
- Add patch propagation for one simple transform, probably filter or calculate.

Exit criteria:

- a row/column update can propagate as a bounded table change instead of full recompute.

### M6: Non-Numeric Authored Use

- Wire text table lanes into text marks.
- Decide category key representation for string-backed nominal/ordinal fields.
- Keep `styled_text` integration separate from core text storage.

Exit criteria:

- a simple authored text-label chart can read actual string data without numeric placeholders.

## Next Implementation Slice

The next code slice should be M4, not patches or Arrow:

1. Add one optional bulk column view, likely for `f64`.
2. Use it in a narrow hot path such as transform extraction or scale-domain inference.
3. Keep per-cell `TableData` access as the fallback.
4. Keep patch storage and propagation out of this slice.

This improves read performance without introducing table patches or a storage backend dependency.

## Risks

- Per-cell trait access may hide performance problems if benchmarks come too late.
- Schema metadata can become a second authored-spec system if semantic hints get too powerful.
- Row-key generation can break animation/selection if transform-derived keys are unstable.
- Adding Arrow too early could pull storage concerns into core.
- Adding patches before row identity is precise will make incremental transforms hard to trust.

## Related Plans

- `plans/engine-evolution.md`
- `plans/transforms.md`
- `plans/io.md`
- `plans/perf.md`
- `plans/spec-ir.md`
