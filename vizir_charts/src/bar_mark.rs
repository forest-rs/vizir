// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bar mark generation.

extern crate alloc;

use alloc::vec::Vec;

use peniko::Brush;
use vizir_core::{ColId, InputRef, Mark, MarkId, TableId};

use crate::scale::{ScaleBand, ScaleContinuous};

/// A vertical bar mark derived from a table.
///
/// This generates one [`vizir_core::MarkKind::Rect`] mark per row key, with bar geometry
/// derived from a numeric value and a baseline.
#[derive(Clone, Debug)]
pub struct BarMarkSpec {
    /// Source table id.
    pub table: TableId,
    /// Column for bar values.
    pub y: ColId,
    /// Band scale used for bar positions along x.
    pub band: ScaleBand,
    /// Linear scale used for bar positions along y.
    pub y_scale: ScaleContinuous,
    /// Baseline in data units (typically `0.0`).
    pub baseline: f64,
    /// Fill paint for bars.
    pub fill: Brush,
    /// Rendering order hint (`vizir_core::Mark::z_index`).
    pub z_index: i32,
}

impl BarMarkSpec {
    /// Creates a bar mark spec with `baseline = 0` and a default fill (`Brush::default()`).
    pub fn new(table: TableId, y: ColId, band: ScaleBand, y_scale: ScaleContinuous) -> Self {
        Self {
            table,
            y,
            band,
            y_scale,
            baseline: 0.0,
            fill: Brush::default(),
            z_index: crate::z_order::SERIES_FILL,
        }
    }

    /// Sets the baseline in data units.
    pub fn with_baseline(mut self, baseline: f64) -> Self {
        self.baseline = baseline;
        self
    }

    /// Sets the fill paint.
    pub fn with_fill(mut self, fill: impl Into<Brush>) -> Self {
        self.fill = fill.into();
        self
    }

    /// Sets the z-index used for render ordering.
    pub fn with_z_index(mut self, z_index: i32) -> Self {
        self.z_index = z_index;
        self
    }

    /// Generates marks for the provided row keys.
    ///
    /// Mark identity is derived from `(table_id, row_key)` so it stays stable across frames.
    pub fn marks(&self, row_keys: &[u64]) -> Vec<Mark> {
        let table_id = self.table;
        let y_col = self.y;
        let band = self.band;
        let bw = band.band_width();
        let y_scale = self.y_scale;
        let baseline = self.baseline;
        let y0 = y_scale.map(baseline);
        let fill = self.fill.clone();
        let z_index = self.z_index;

        row_keys
            .iter()
            .copied()
            .enumerate()
            .map(|(row, row_key)| {
                let id = MarkId::for_row(table_id, row_key);
                Mark::builder(id)
                    .rect()
                    .z_index(z_index)
                    .x_const(band.x(row))
                    .y_compute(
                        [InputRef::TableCol {
                            table: table_id,
                            col: y_col,
                        }],
                        move |ctx, _| {
                            let v = ctx.table_f64(table_id, row, y_col).unwrap_or(baseline);
                            y_scale.map(v).min(y0)
                        },
                    )
                    .w_const(bw)
                    .h_compute(
                        [InputRef::TableCol {
                            table: table_id,
                            col: y_col,
                        }],
                        move |ctx, _| {
                            let v = ctx.table_f64(table_id, row, y_col).unwrap_or(baseline);
                            (y_scale.map(v) - y0).abs()
                        },
                    )
                    .fill_brush_const(fill.clone())
                    .build()
            })
            .collect()
    }
}
