// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Convenience builder for stacked bar charts.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use alloc::string::String;

use peniko::Brush;
use peniko::color::palette::css;
use vizir_core::{ColId, Mark, TableId};
use vizir_transforms::{Program, SortOrder, StackOffset, Transform};

use crate::LegendItem;
use crate::scale::{ScaleBand, ScaleContinuous};
use crate::stacked_bar_mark::StackedBarMarkSpec;

/// A minimal stacked-bar chart builder.
///
/// This is a small convenience wrapper around:
/// - a `vizir_transforms::Transform::Stack` that produces `y0`/`y1`, and
/// - a [`StackedBarMarkSpec`] that renders those stacked spans as rect marks.
///
/// It is intentionally v0:
/// - no aggregation: each input row corresponds to one stacked segment,
/// - stacking order is controlled by `stack_sort_by` (Vega `sort` in v0 form).
#[derive(Clone, Debug)]
pub struct StackedBarChartSpec {
    /// Input table.
    pub input: TableId,
    /// Output table (stack result).
    pub output: TableId,
    /// Category (stack group key).
    pub category: ColId,
    /// Series column (carried through, and can be used for fill palettes).
    pub series: ColId,
    /// Value column to stack.
    pub value: ColId,
    /// Output y0 column.
    pub y0: ColId,
    /// Output y1 column.
    pub y1: ColId,
    /// Optional per-group sort key for stacking.
    ///
    /// Default: `Some(series)` so stacked segment order is stable.
    pub stack_sort_by: Option<ColId>,
    /// Sort order for `stack_sort_by`.
    ///
    /// Default: `Asc`.
    pub stack_sort_order: SortOrder,
    /// Baseline offset mode.
    ///
    /// Default: `StackOffset::Zero`.
    pub stack_offset: StackOffset,
}

impl StackedBarChartSpec {
    /// Creates a stacked-bar chart spec.
    pub fn new(
        input: TableId,
        output: TableId,
        category: ColId,
        series: ColId,
        value: ColId,
        y0: ColId,
        y1: ColId,
    ) -> Self {
        Self {
            input,
            output,
            category,
            series,
            value,
            y0,
            y1,
            stack_sort_by: Some(series),
            stack_sort_order: SortOrder::Asc,
            stack_offset: StackOffset::Zero,
        }
    }

    /// Sets the stacking sort key (Vega `stack.sort`).
    pub fn with_stack_sort_by(mut self, sort_by: ColId, order: SortOrder) -> Self {
        self.stack_sort_by = Some(sort_by);
        self.stack_sort_order = order;
        self
    }

    /// Disables sorting within each stack (rows are processed in input order within each group).
    pub fn without_stack_sort(mut self) -> Self {
        self.stack_sort_by = None;
        self
    }

    /// Sets the stack baseline offset mode (Vega `stack.offset`).
    pub fn with_stack_offset(mut self, offset: StackOffset) -> Self {
        self.stack_offset = offset;
        self
    }

    /// Returns a transform program that produces the stacked output table.
    pub fn program(&self) -> Program {
        let mut p = Program::new();
        p.push(Transform::Stack {
            input: self.input,
            output: self.output,
            group_by: vec![self.category],
            offset: self.stack_offset,
            sort_by: self.stack_sort_by,
            sort_order: self.stack_sort_order,
            field: self.value,
            output_start: self.y0,
            output_end: self.y1,
            columns: vec![self.category, self.series, self.value],
        });
        p
    }

    /// Returns a default categorical fill palette suitable for stacked series.
    ///
    /// Colors are taken from named CSS colors and repeat if `count` exceeds the palette length.
    pub fn default_series_fills(count: usize) -> Vec<Brush> {
        const PALETTE: [peniko::Color; 8] = [
            css::CORNFLOWER_BLUE,
            css::ORANGE,
            css::MEDIUM_SEA_GREEN,
            css::CRIMSON,
            css::GOLDENROD,
            css::SLATE_BLUE,
            css::DARK_CYAN,
            css::HOT_PINK,
        ];

        (0..count)
            .map(|i| Brush::Solid(PALETTE[i % PALETTE.len()]))
            .collect()
    }

    /// Builds legend items from a label list and a fill palette.
    ///
    /// Items are produced in `labels` order and paired with fills by index. If the lists have
    /// different lengths, the shorter length wins.
    pub fn legend_items(labels: &[&str], fills: &[Brush]) -> Vec<LegendItem> {
        labels
            .iter()
            .copied()
            .zip(fills.iter().cloned())
            .map(|(label, fill)| LegendItem {
                label: String::from(label),
                fill,
            })
            .collect()
    }

    /// Returns the stacked bar mark spec that reads from the output table.
    pub fn marks(
        &self,
        row_keys: &[u64],
        band: ScaleBand,
        y_scale: ScaleContinuous,
        series_fills: Vec<Brush>,
    ) -> Vec<Mark> {
        StackedBarMarkSpec::new(self.output, self.category, self.y0, self.y1, band, y_scale)
            .with_series_fills(self.series, series_fills)
            .marks(row_keys)
    }
}
