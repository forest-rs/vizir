// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Ranged area mark generation using authored top and bottom edges.

extern crate alloc;

use alloc::vec::Vec;

use kurbo::BezPath;
use peniko::{Brush, Color};
use vizir_core::{ColumnId, InputRef, Mark, MarkId, TableId};

use crate::axis::StrokeStyle;
use crate::scale::ScaleContinuous;

/// A ranged area mark derived from a table.
///
/// This expects input rows already ordered along the intended path. Each row contributes one point
/// to the top edge (`x`, `y`) and one point to the bottom edge (`x2`, `y2`).
#[derive(Clone, Debug)]
pub struct RangeAreaMarkSpec {
    /// Stable-id base for marks emitted by this mark.
    pub id_base: u64,
    /// Source table id.
    pub table: TableId,
    /// Column for top-edge x values.
    pub x: ColumnId,
    /// Column for top-edge y values.
    pub y: ColumnId,
    /// Column for bottom-edge x values.
    pub x2: ColumnId,
    /// Column for bottom-edge y values.
    pub y2: ColumnId,
    /// X scale mapping data x values into scene x.
    pub x_scale: ScaleContinuous,
    /// Y scale mapping data y values into scene y.
    pub y_scale: ScaleContinuous,
    /// Fill paint for the area.
    pub fill: Brush,
    /// Optional stroke for the outline (drawn along the top edge).
    pub stroke: Option<StrokeStyle>,
    /// Rendering order hint (`vizir_core::Mark::z_index`) for the filled area.
    pub z_index: i32,
}

impl RangeAreaMarkSpec {
    /// Creates a ranged area mark with default fill (`Brush::default()`).
    #[allow(
        clippy::too_many_arguments,
        reason = "the authored geometry has four edge fields"
    )]
    pub fn new(
        id_base: u64,
        table: TableId,
        x: ColumnId,
        y: ColumnId,
        x2: ColumnId,
        y2: ColumnId,
        x_scale: ScaleContinuous,
        y_scale: ScaleContinuous,
    ) -> Self {
        Self {
            id_base,
            table,
            x,
            y,
            x2,
            y2,
            x_scale,
            y_scale,
            fill: Brush::default(),
            stroke: None,
            z_index: crate::z_order::SERIES_FILL,
        }
    }

    /// Sets the fill paint.
    pub fn with_fill(mut self, fill: impl Into<Brush>) -> Self {
        self.fill = fill.into();
        self
    }

    /// Sets the outline stroke (drawn along the top edge).
    pub fn with_stroke(mut self, stroke: StrokeStyle) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// Disables the outline stroke.
    pub fn without_stroke(mut self) -> Self {
        self.stroke = None;
        self
    }

    /// Sets the z-index used for render ordering.
    ///
    /// The optional outline stroke (if enabled) is drawn above the fill.
    pub fn with_z_index(mut self, z_index: i32) -> Self {
        self.z_index = z_index;
        self
    }

    /// Generates marks for this mark.
    pub fn marks(&self) -> Vec<Mark> {
        let table_id = self.table;
        let x_col = self.x;
        let y_col = self.y;
        let x2_col = self.x2;
        let y2_col = self.y2;
        let x_scale = self.x_scale;
        let y_scale = self.y_scale;
        let fill = self.fill.clone();

        let area_id = MarkId::from_raw(self.id_base);
        let z_index = self.z_index;
        let area = Mark::builder(area_id)
            .path()
            .z_index(z_index)
            .path_compute([InputRef::Table { table: table_id }], move |ctx, _| {
                let n = ctx.table_row_count(table_id).unwrap_or(0);
                let mut top: Vec<(f64, f64)> = Vec::with_capacity(n);
                let mut bottom: Vec<(f64, f64)> = Vec::with_capacity(n);

                for row in 0..n {
                    let x = ctx.table_f64(table_id, row, x_col).unwrap_or(0.0);
                    let y = ctx.table_f64(table_id, row, y_col).unwrap_or(0.0);
                    let x2 = ctx.table_f64(table_id, row, x2_col).unwrap_or(x);
                    let y2 = ctx.table_f64(table_id, row, y2_col).unwrap_or(y);
                    top.push((x_scale.map(x), y_scale.map(y)));
                    bottom.push((x_scale.map(x2), y_scale.map(y2)));
                }

                let mut path = BezPath::new();
                if top.is_empty() {
                    return path;
                }

                path.move_to(bottom[0]);
                path.line_to(top[0]);
                for &point in top.iter().skip(1) {
                    path.line_to(point);
                }
                for &point in bottom.iter().rev() {
                    path.line_to(point);
                }
                path.close_path();
                path
            })
            .fill_brush_const(fill)
            .stroke_width_const(0.0)
            .build();

        let mut out = alloc::vec![area];

        if let Some(stroke) = self.stroke.clone() {
            let line_id = MarkId::from_raw(self.id_base + 1);
            let stroke_brush = stroke.brush.clone();
            let stroke_width = stroke.stroke_width;
            let line = Mark::builder(line_id)
                .path()
                .z_index(z_index.saturating_add(crate::z_order::SERIES_STROKE))
                .path_compute([InputRef::Table { table: table_id }], move |ctx, _| {
                    let n = ctx.table_row_count(table_id).unwrap_or(0);
                    let mut path = BezPath::new();
                    for row in 0..n {
                        let x = ctx.table_f64(table_id, row, x_col).unwrap_or(0.0);
                        let y = ctx.table_f64(table_id, row, y_col).unwrap_or(0.0);
                        let point = (x_scale.map(x), y_scale.map(y));
                        if row == 0 {
                            path.move_to(point);
                        } else {
                            path.line_to(point);
                        }
                    }
                    path
                })
                .fill_const(Color::TRANSPARENT)
                .stroke_brush_const(stroke_brush)
                .stroke_width_const(stroke_width)
                .build();
            out.push(line);
        }

        out
    }
}
