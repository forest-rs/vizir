// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transform IR types.

extern crate alloc;

use alloc::vec::Vec;

use vizir_core::{ColId, TableId};

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
    pub input: ColId,
    /// Output column id.
    pub output: ColId,
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

/// A row predicate used by [`Transform::Filter`].
///
/// This is intentionally tiny at v0: it supports a single numeric comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Predicate {
    /// Column to read.
    pub col: ColId,
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
    Filter {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Predicate to apply per row.
        predicate: Predicate,
        /// Columns to carry through to the output table.
        columns: Vec<ColId>,
    },
    /// Select a subset of columns.
    Project {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Columns to include in the output table.
        columns: Vec<ColId>,
    },
    /// Reorder rows by a numeric key column.
    Sort {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Column used as the sort key.
        by: ColId,
        /// Sort order.
        order: SortOrder,
        /// Columns to carry through to the output table.
        columns: Vec<ColId>,
    },
    /// Group rows by one or more key columns and compute aggregates.
    ///
    /// Output columns are `group_by` (in order) followed by the `fields` outputs (in order).
    Aggregate {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Group-by key columns.
        group_by: Vec<ColId>,
        /// Aggregated fields.
        fields: Vec<AggregateField>,
    },
    /// Bin a numeric column into fixed-width buckets.
    ///
    /// This is a v0 placeholder: we want the IR shape before committing to all bin options.
    Bin {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Input column to bin.
        input_col: ColId,
        /// Output column containing the bin start value.
        output_start: ColId,
        /// Bin step size in data units.
        step: f64,
        /// Columns to carry through to the output table.
        columns: Vec<ColId>,
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
    Stack {
        /// Input table.
        input: TableId,
        /// Output table.
        output: TableId,
        /// Group-by key columns defining independent stacks.
        group_by: Vec<ColId>,
        /// Baseline offset mode.
        offset: StackOffset,
        /// Optional per-group sort key.
        ///
        /// This corresponds to Vega's `sort` parameter (in a v0 form). When set, rows are stacked
        /// in sorted order within each group, but the output table row order is preserved.
        sort_by: Option<ColId>,
        /// Sort order when `sort_by` is set.
        sort_order: SortOrder,
        /// Input column providing the value to accumulate.
        field: ColId,
        /// Output column containing the stack start offset (default `y0` in Vega).
        output_start: ColId,
        /// Output column containing the stack end offset (default `y1` in Vega).
        output_end: ColId,
        /// Columns to carry through to the output table.
        columns: Vec<ColId>,
    },
}
