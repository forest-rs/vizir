// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transform IR types.

use alloc::vec::Vec;

use vizir_core::{ColumnId, TableId, TablePatch};

/// Stack baseline offset mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackOffset {
    /// Stack positive and negative values around 0 (Vega default).
    Zero,
    /// Center stacks using Vega's `"center"` offset.
    ///
    /// This computes stack offsets using the sum of absolute values and shifts each group so the
    /// group is vertically centered relative to the maximum group sum.
    Center,
    /// Streamgraph-style "wiggle" baseline.
    ///
    /// This corresponds to Vega's `"wiggle"` offset (D3's `stackOffsetWiggle`) and attempts to
    /// minimize weighted changes in slope across the stacked series.
    ///
    /// Notes:
    /// - This is primarily intended for positive-valued stacked areas.
    /// - The v0 executor requires a per-group sort key (`sort_by`) so series order is consistent,
    ///   and an ordered grouping key (typically the x dimension).
    Wiggle,
    /// Normalize stacks to the range `[0, 1]` using Vega's `"normalize"` offset.
    ///
    /// This computes stack offsets using the sum of absolute values and scales each group so its
    /// total height is `1.0`.
    Normalize,
}

/// Aggregation operation for [`Transform::Aggregate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateOp {
    /// Count rows.
    Count,
    /// Sum values (skips non-finite).
    Sum,
    /// Minimum value (skips non-finite).
    Min,
    /// Maximum value (skips non-finite).
    Max,
    /// Mean value (skips non-finite).
    Mean,
}

/// A single aggregated output field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AggregateField {
    /// Operation to apply.
    pub op: AggregateOp,
    /// Input column.
    pub input: ColumnId,
    /// Output column id.
    pub output: ColumnId,
}

/// Sorting order for [`Transform::Sort`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

/// Comparison operators for numeric predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==` (exact float equality)
    Eq,
    /// `!=` (exact float inequality)
    Ne,
}

/// Arithmetic operator for [`Transform::Calculate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalculateOp {
    /// `left + right`
    Add,
    /// `left - right`
    Sub,
    /// `left * right`
    Mul,
    /// `left / right`
    Div,
}

/// One operand in a narrow calculate expression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalculateOperand {
    /// Read a value from the given input column.
    Column(ColumnId),
    /// Use a numeric literal.
    Constant(f64),
}

/// A narrow calculate expression producing one derived numeric column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalculateExpr {
    /// Left operand.
    pub left: CalculateOperand,
    /// Arithmetic operator.
    pub op: CalculateOp,
    /// Right operand.
    pub right: CalculateOperand,
}

/// Window operation for [`Transform::Window`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowOp {
    /// 1-based row number within the sorted window partition.
    RowNumber,
    /// 1-based rank within the sorted window partition, with gaps for peer groups.
    Rank,
}

/// One derived output field for [`Transform::Window`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowField {
    /// Window operation to evaluate.
    pub op: WindowOp,
    /// Output column id.
    pub output: ColumnId,
}

/// One output field for [`Transform::Lookup`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupField {
    /// Column to read from the lookup table.
    pub input: ColumnId,
    /// Output column id in the enriched result.
    pub output: ColumnId,
}

/// One explicit pivot slot for [`Transform::Pivot`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PivotValue {
    /// Numeric pivot-key value to match.
    pub value: f64,
    /// Output column id for the aggregated values in this slot.
    pub output: ColumnId,
}

/// A row predicate used by [`Transform::Filter`].
///
/// This is intentionally tiny at v0: it supports a single numeric comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Predicate {
    /// Column to read.
    pub col: ColumnId,
    /// Comparison operator.
    pub op: CompareOp,
    /// Right-hand constant.
    pub value: f64,
}

impl Predicate {
    /// Evaluate the predicate for a given numeric value.
    pub fn eval(&self, v: f64) -> bool {
        match self.op {
            CompareOp::Lt => v < self.value,
            CompareOp::Le => v <= self.value,
            CompareOp::Gt => v > self.value,
            CompareOp::Ge => v >= self.value,
            CompareOp::Eq => v == self.value,
            CompareOp::Ne => v != self.value,
        }
    }
}

/// A table transform from an input table to an output table.
#[derive(Debug, Clone, PartialEq)]
pub enum Transform {
    /// Keep only rows that satisfy a predicate.
    ///
    /// Output row keys preserve the surviving input row keys.
    Filter {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Predicate to apply per row.
        predicate: Predicate,
        /// Columns to carry through to the output table.
        columns: Vec<ColumnId>,
    },
    /// Select a subset of columns.
    ///
    /// Output row keys preserve input row keys.
    Project {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Columns to include in the output table.
        columns: Vec<ColumnId>,
    },
    /// Reorder rows by a numeric key column.
    ///
    /// Output row keys move with their rows; row identity is preserved.
    Sort {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Column used as the sort key.
        by: ColumnId,
        /// Sort order.
        order: SortOrder,
        /// Columns to carry through to the output table.
        columns: Vec<ColumnId>,
    },
    /// Compute one derived numeric column from a narrow arithmetic expression.
    ///
    /// Output columns are `columns` (in order) followed by `output`.
    /// Output row keys preserve input row keys.
    Calculate {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Arithmetic expression to evaluate per row.
        expr: CalculateExpr,
        /// Output column id for the derived values.
        output_col: ColumnId,
        /// Columns to carry through to the output table.
        columns: Vec<ColumnId>,
    },
    /// Group rows by one or more keys, compute aggregates, and write the results back per row.
    ///
    /// Output columns are `columns` (in order) followed by the `fields` outputs (in order).
    /// Output row keys preserve input row keys.
    JoinAggregate {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Group-by key columns.
        group_by: Vec<ColumnId>,
        /// Aggregated fields to join back per input row.
        fields: Vec<AggregateField>,
        /// Columns to carry through to the output table.
        columns: Vec<ColumnId>,
    },
    /// Group rows by one or more key columns and compute aggregates.
    ///
    /// Output columns are `group_by` (in order) followed by the `fields` outputs (in order).
    /// Output row keys are derived from the group key values.
    Aggregate {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Group-by key columns.
        group_by: Vec<ColumnId>,
        /// Aggregated fields.
        fields: Vec<AggregateField>,
    },
    /// Bin a numeric column into fixed-width buckets.
    ///
    /// This is a v0 placeholder: we want the IR shape before committing to all bin options.
    /// Output row keys preserve input row keys; the bin start is a derived column.
    Bin {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Input column to bin.
        input_col: ColumnId,
        /// Output column containing the bin start value.
        output_start: ColumnId,
        /// Bin step size in data units.
        step: f64,
        /// Columns to carry through to the output table.
        columns: Vec<ColumnId>,
    },
    /// Fold several numeric columns into repeated rows with a numeric slot id and folded value.
    ///
    /// Output columns are `columns` (in order) followed by `output_key`, `output_value`.
    /// Output row keys are derived from the input row key and folded field slot.
    Fold {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Input columns to fold in the provided order.
        fields: Vec<ColumnId>,
        /// Output column containing the 0-based field slot index.
        output_key: ColumnId,
        /// Output column containing the folded numeric value.
        output_value: ColumnId,
        /// Columns to carry through to the output table.
        columns: Vec<ColumnId>,
    },
    /// Enrich rows by looking up values from another table on one numeric key.
    ///
    /// Output columns are `columns` (in order) followed by the `fields` outputs (in order).
    /// Output row keys preserve input row keys.
    Lookup {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Lookup source table.
        from_table: TableId,
        /// Key column in the input table.
        key: ColumnId,
        /// Key column in the lookup table.
        from_key: ColumnId,
        /// Lookup value columns to append.
        fields: Vec<LookupField>,
        /// Columns to carry through to the output table.
        columns: Vec<ColumnId>,
    },
    /// Group rows and widen a numeric pivot key into explicit output columns.
    ///
    /// Output columns are `group_by` (in order) followed by the `values` outputs (in order).
    /// Output row keys are derived from the group key values.
    Pivot {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Group-by key columns.
        group_by: Vec<ColumnId>,
        /// Numeric pivot key column.
        pivot: ColumnId,
        /// Numeric value column to aggregate into each pivot slot.
        value: ColumnId,
        /// Aggregation operation to apply within each `(group_by, pivot)` bucket.
        op: AggregateOp,
        /// Explicit pivot slots to materialize.
        values: Vec<PivotValue>,
    },
    /// Compute simple window values over sorted row partitions and write them per row.
    ///
    /// Output columns are `columns` (in order) followed by the `fields` outputs (in order).
    /// Output row keys preserve input row keys.
    Window {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Group-by key columns defining independent window partitions.
        group_by: Vec<ColumnId>,
        /// Column used to sort rows within each partition.
        sort_by: ColumnId,
        /// Sort order within each partition.
        sort_order: SortOrder,
        /// Window fields to compute per row.
        fields: Vec<WindowField>,
        /// Columns to carry through to the output table.
        columns: Vec<ColumnId>,
    },
    /// Compute a "zero" stack layout, writing start/end offsets per row.
    ///
    /// This corresponds to Vega's `stack` transform with `offset = "zero"`.
    ///
    /// Notes:
    /// - Within each stack group, rows are processed in input order. If you want a specific order
    ///   (Vega's `sort` parameter), run [`Transform::Sort`] upstream.
    /// - Negative values are stacked downward from 0, positive values upward from 0.
    ///
    /// Output columns are `columns` (in order) followed by `output_start`, `output_end`.
    /// Output row keys preserve input row keys; optional stack sorting affects layout, not row
    /// order.
    Stack {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Group-by key columns defining independent stacks.
        group_by: Vec<ColumnId>,
        /// Baseline offset mode.
        offset: StackOffset,
        /// Optional per-group sort key.
        ///
        /// This corresponds to Vega's `sort` parameter (in a v0 form). When set, rows are stacked
        /// in sorted order within each group, but the output table row order is preserved.
        sort_by: Option<ColumnId>,
        /// Sort order when `sort_by` is set.
        sort_order: SortOrder,
        /// Input column providing the value to accumulate.
        field: ColumnId,
        /// Output column containing the stack start offset (default `y0` in Vega).
        output_start: ColumnId,
        /// Output column containing the stack end offset (default `y1` in Vega).
        output_end: ColumnId,
        /// Columns to carry through to the output table.
        columns: Vec<ColumnId>,
    },
}

impl Transform {
    /// Propagate an input table patch to this transform's output table.
    ///
    /// The current bounded implementation supports [`Transform::Project`]. Other transforms
    /// conservatively report [`TablePatch::Replaced`] for non-empty input patches until their
    /// row/column semantics are implemented.
    #[must_use]
    pub fn propagate_patch(&self, patch: &TablePatch) -> TablePatch {
        if patch.is_empty() {
            return TablePatch::Empty;
        }

        match self {
            Self::Project { columns, .. } => project_patch(columns, patch),
            _ => TablePatch::Replaced,
        }
    }
}

fn project_patch(project_columns: &[ColumnId], patch: &TablePatch) -> TablePatch {
    match patch {
        TablePatch::Empty => TablePatch::Empty,
        TablePatch::RowsInserted { keys } => TablePatch::RowsInserted { keys: keys.clone() },
        TablePatch::RowsRemoved { keys } => TablePatch::RowsRemoved { keys: keys.clone() },
        TablePatch::RowsUpdated { keys, columns } => {
            let columns = projected_columns(project_columns, columns);
            if columns.is_empty() {
                TablePatch::Empty
            } else {
                TablePatch::RowsUpdated {
                    keys: keys.clone(),
                    columns,
                }
            }
        }
        TablePatch::ColumnsUpdated { columns } => {
            let columns = projected_columns(project_columns, columns);
            if columns.is_empty() {
                TablePatch::Empty
            } else {
                TablePatch::ColumnsUpdated { columns }
            }
        }
        TablePatch::Replaced => TablePatch::Replaced,
    }
}

fn projected_columns(project_columns: &[ColumnId], changed_columns: &[ColumnId]) -> Vec<ColumnId> {
    let mut out = Vec::new();
    for &col in changed_columns {
        if project_columns.contains(&col) && !out.contains(&col) {
            out.push(col);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    fn project() -> Transform {
        Transform::Project {
            input: TableId(1),
            output: TableId(2),
            columns: vec![ColumnId(0), ColumnId(2)],
        }
    }

    #[test]
    fn project_patch_filters_updated_columns() {
        let patch = TablePatch::RowsUpdated {
            keys: vec![10, 11],
            columns: vec![ColumnId(2), ColumnId(1), ColumnId(2)],
        };

        assert_eq!(
            project().propagate_patch(&patch),
            TablePatch::RowsUpdated {
                keys: vec![10, 11],
                columns: vec![ColumnId(2)]
            }
        );
    }

    #[test]
    fn project_patch_drops_unprojected_column_updates() {
        let patch = TablePatch::ColumnsUpdated {
            columns: vec![ColumnId(1)],
        };

        assert_eq!(project().propagate_patch(&patch), TablePatch::Empty);
    }

    #[test]
    fn project_patch_preserves_row_insertions() {
        let patch = TablePatch::RowsInserted { keys: vec![10, 11] };

        assert_eq!(project().propagate_patch(&patch), patch);
    }

    #[test]
    fn unsupported_transform_patch_falls_back_to_replaced() {
        let transform = Transform::Filter {
            input: TableId(1),
            output: TableId(2),
            predicate: Predicate {
                col: ColumnId(0),
                op: CompareOp::Gt,
                value: 0.0,
            },
            columns: vec![ColumnId(0)],
        };

        assert_eq!(
            transform.propagate_patch(&TablePatch::RowsInserted { keys: vec![10] }),
            TablePatch::Replaced
        );
        assert_eq!(
            transform.propagate_patch(&TablePatch::Empty),
            TablePatch::Empty
        );
    }
}
