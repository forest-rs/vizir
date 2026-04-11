# Transform Breadth Slice

Goals:
- Add a narrow `joinaggregate` transform through `vizir_transforms`, the authored spec seam,
  the parser-facing adapter, and the JSON parser.
- Add a narrow `window` transform through the same path.
- Prove both slices with fixture-backed demos and tests.

Non-goals:
- Full Vega/Vega-Lite expression support.
- General window-frame semantics.
- String-valued fields or generic non-numeric transform outputs.

Planned slices:
1. `joinaggregate`
   - Reuse aggregate ops already supported in the runtime.
   - Partition by `group_by`, compute aggregate values per group, and write them back per row.
   - Require explicit carry-through `columns` plus derived output aliases.
2. `window`
   - Start with `row_number` and `rank`.
   - Support optional `group_by` and required `sort_by`.
   - Emit one derived numeric column per requested window field.

Risks:
- Derived output alias allocation can drift between authored and parsed seams if tests are weak.
- `window` semantics can over-promise if we imply full SQL/Vega window behavior.
- These transforms can make demos look more general than the supported parser slice really is.

Acceptance bar:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo run -p vizir_charts_demo`
