// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Stacked bar mark generation (using `y0`/`y1` columns).

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;

use peniko::Brush;
use vizir_core::{ColId, InputRef, Mark, MarkId, TableId};

#[cfg(not(feature = "std"))]
use crate::float::FloatExt;

use crate::scale::{ScaleBand, ScaleContinuous};

/// A vertical stacked bar mark derived from a table.
///
/// This generates one [`vizir_core::MarkKind::Rect`] mark per row key, where the vertical span is
/// read from `y0`/`y1` columns (typically produced by `vizir_transforms::Transform::Stack`).
#[derive(Clone)]
pub struct StackedBarMarkSpec {
    /// Source table id.
    pub table: TableId,
    /// Column containing the category value (used to place bars along x).
    pub category: ColId,
    /// Optional series column (used for per-series fills).
    pub series: Option<ColId>,
    /// Column containing the stack start value.
    pub y0: ColId,
    /// Column containing the stack end value.
    pub y1: ColId,
    /// Band scale used for bar positions along x.
    pub band: ScaleBand,
    /// Linear scale used for bar positions along y.
    pub y_scale: ScaleContinuous,
    /// Mapping from category values to band indices.
    ///
    /// By default, this rounds the category value to the nearest integer and clamps it to the band
    /// range `[0, band.count())`.
    pub category_index: Arc<dyn Fn(f64) -> usize>,
    /// Optional per-series fill palette.
    ///
    /// If set, `series` must also be set. Series values are treated as `0..n` indices after
    /// rounding/clamping.
    pub series_fills: Option<Vec<Brush>>,
    /// Fill paint for bars when no `series_fills` palette is set.
    pub fill: Brush,
    /// Rendering order hint (`vizir_core::Mark::z_index`).
    pub z_index: i32,
}

impl core::fmt::Debug for StackedBarMarkSpec {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StackedBarMarkSpec")
            .field("table", &self.table)
            .field("category", &self.category)
            .field("series", &self.series)
            .field("y0", &self.y0)
            .field("y1", &self.y1)
            .field("band", &self.band)
            .field("y_scale", &self.y_scale)
            .field("category_index", &"<fn>")
            .field("series_fills", &self.series_fills.as_ref().map(|v| v.len()))
            .field("fill", &self.fill)
            .field("z_index", &self.z_index)
            .finish()
    }
}

impl StackedBarMarkSpec {
    /// Creates a stacked bar mark spec with a default fill (`Brush::default()`).
    pub fn new(
        table: TableId,
        category: ColId,
        y0: ColId,
        y1: ColId,
        band: ScaleBand,
        y_scale: ScaleContinuous,
    ) -> Self {
        let count = band.count();
        Self {
            table,
            category,
            series: None,
            y0,
            y1,
            band,
            y_scale,
            category_index: Arc::new(move |v| default_index(v, count)),
            series_fills: None,
            fill: Brush::default(),
            z_index: crate::z_order::SERIES_FILL,
        }
    }

    /// Sets the category-to-band index mapping.
    pub fn with_category_index(mut self, f: impl Fn(f64) -> usize + 'static) -> Self {
        self.category_index = Arc::new(f);
        self
    }

    /// Sets a constant fill paint.
    pub fn with_fill(mut self, fill: impl Into<Brush>) -> Self {
        self.fill = fill.into();
        self.series = None;
        self.series_fills = None;
        self
    }

    /// Uses a per-series fill palette.
    ///
    /// Series values are treated as `0..n` indices after rounding/clamping.
    pub fn with_series_fills(mut self, series: ColId, fills: Vec<Brush>) -> Self {
        self.series = Some(series);
        self.series_fills = Some(fills);
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
        let cat_col = self.category;
        let y0_col = self.y0;
        let y1_col = self.y1;
        let band = self.band;
        let bw = band.band_width();
        let y_scale = self.y_scale;
        let z_index = self.z_index;

        let category_index = self.category_index.clone();

        let series_col = self.series;
        let series_fills = self.series_fills.clone();
        let fill = self.fill.clone();

        row_keys
            .iter()
            .copied()
            .enumerate()
            .map(|(row, row_key)| {
                let id = MarkId::for_row(table_id, row_key);

                let x = {
                    let category_index = category_index.clone();
                    Mark::builder(id).rect().z_index(z_index).x_compute(
                        [InputRef::TableCol {
                            table: table_id,
                            col: cat_col,
                        }],
                        move |ctx, _| {
                            let cat = ctx.table_f64(table_id, row, cat_col).unwrap_or(0.0);
                            band.x(category_index(cat))
                        },
                    )
                };

                let y = x.y_compute(
                    [
                        InputRef::TableCol {
                            table: table_id,
                            col: y0_col,
                        },
                        InputRef::TableCol {
                            table: table_id,
                            col: y1_col,
                        },
                    ],
                    move |ctx, _| {
                        let a = ctx.table_f64(table_id, row, y0_col).unwrap_or(0.0);
                        let b = ctx.table_f64(table_id, row, y1_col).unwrap_or(0.0);
                        y_scale.map(a.max(b))
                    },
                );

                let h = y.h_compute(
                    [
                        InputRef::TableCol {
                            table: table_id,
                            col: y0_col,
                        },
                        InputRef::TableCol {
                            table: table_id,
                            col: y1_col,
                        },
                    ],
                    move |ctx, _| {
                        let a = ctx.table_f64(table_id, row, y0_col).unwrap_or(0.0);
                        let b = ctx.table_f64(table_id, row, y1_col).unwrap_or(0.0);
                        (y_scale.map(a) - y_scale.map(b)).abs()
                    },
                );

                let mut m = h.w_const(bw);

                if let (Some(series_col), Some(series_fills)) = (series_col, series_fills.clone()) {
                    m = m.fill_compute(
                        [InputRef::TableCol {
                            table: table_id,
                            col: series_col,
                        }],
                        move |ctx, _| {
                            let v = ctx.table_f64(table_id, row, series_col).unwrap_or(0.0);
                            let i = default_index(v, series_fills.len());
                            series_fills.get(i).cloned().unwrap_or_else(Brush::default)
                        },
                    );
                } else {
                    m = m.fill_brush_const(fill.clone());
                }

                m.build()
            })
            .collect()
    }
}

fn default_index(v: f64, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let v = v.round();
    if v.is_nan() {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, reason = "clamped before cast")]
    let i = v.clamp(0.0, (count - 1) as f64) as usize;
    i
}
