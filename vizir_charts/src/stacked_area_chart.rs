// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Convenience builder for stacked area charts.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use alloc::string::String;

use peniko::Brush;
use peniko::color::palette::css;
use vizir_core::{ColId, TableId};
use vizir_transforms::{CompareOp, Predicate, Program, SortOrder, StackOffset, Transform};

use crate::LegendItem;

/// A minimal stacked-area chart builder.
///
/// This is a small convenience wrapper around:
/// - a `vizir_transforms::Transform::Stack` that produces `y0`/`y1`, and
/// - per-series extraction helpers suitable for rendering with `StackedAreaMarkSpec`.
///
/// v0 limitations:
/// - splitting into per-series tables is performed via `Filter + Sort(x)` today,
///   until we add a more ergonomic "facet/partition" transform.
#[derive(Clone, Debug)]
pub struct StackedAreaChartSpec {
    /// Input table.
    pub input: TableId,
    /// Output table (stack result).
    pub stacked: TableId,
    /// X column. This is used as the stack group key and the horizontal coordinate.
    pub x: ColId,
    /// Series column.
    pub series: ColId,
    /// Value column to stack.
    pub value: ColId,
    /// Output y0 column.
    pub y0: ColId,
    /// Output y1 column.
    pub y1: ColId,
    /// Baseline offset mode.
    ///
    /// Default: `StackOffset::Zero`.
    pub stack_offset: StackOffset,
}

impl StackedAreaChartSpec {
    /// Creates a stacked-area chart spec.
    pub fn new(
        input: TableId,
        stacked: TableId,
        x: ColId,
        series: ColId,
        value: ColId,
        y0: ColId,
        y1: ColId,
    ) -> Self {
        Self {
            input,
            stacked,
            x,
            series,
            value,
            y0,
            y1,
            stack_offset: StackOffset::Zero,
        }
    }

    /// Sets the stack baseline offset mode (Vega `stack.offset`).
    pub fn with_stack_offset(mut self, offset: StackOffset) -> Self {
        self.stack_offset = offset;
        self
    }

    /// Returns a transform program that produces the stacked output table.
    ///
    /// This corresponds roughly to Vega's `stack` transform:
    /// - `groupby = [x]`
    /// - `sort = { field: series, order: asc }`
    pub fn program(&self) -> Program {
        let mut p = Program::new();
        p.push(Transform::Stack {
            input: self.input,
            output: self.stacked,
            group_by: vec![self.x],
            offset: self.stack_offset,
            sort_by: Some(self.series),
            sort_order: SortOrder::Asc,
            field: self.value,
            output_start: self.y0,
            output_end: self.y1,
            columns: vec![self.x, self.series, self.value],
        });
        p
    }

    /// Returns a program that extracts a single series from the stacked table.
    ///
    /// The output table will include columns `[x, y0, y1]` and be sorted by `x` ascending.
    pub fn series_program(&self, out: TableId, series_value: f64) -> Program {
        let mut p = Program::new();
        p.push(Transform::Filter {
            input: self.stacked,
            output: out,
            predicate: Predicate {
                col: self.series,
                op: CompareOp::Eq,
                value: series_value,
            },
            columns: vec![self.x, self.y0, self.y1],
        });
        p.push(Transform::Sort {
            input: out,
            output: out,
            by: self.x,
            order: SortOrder::Asc,
            columns: vec![self.x, self.y0, self.y1],
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
}
