// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Stacked area mark generation (using `y0`/`y1` columns).

extern crate alloc;

use alloc::vec::Vec;

use kurbo::BezPath;
use peniko::{Brush, Color};
use vizir_core::{ColId, InputRef, Mark, MarkId, TableId};

use crate::axis::StrokeStyle;
use crate::scale::ScaleContinuous;

/// A stacked area mark derived from a table.
///
/// This expects input data sorted by `x` for the series being rendered. It uses `y0`/`y1`
/// columns to define the bottom and top of the filled area (typically output from
/// `vizir_transforms::Transform::Stack`).
#[derive(Clone, Debug)]
pub struct StackedAreaMarkSpec {
    /// Stable-id base for marks emitted by this mark.
    pub id_base: u64,
    /// Source table id.
    pub table: TableId,
    /// Column for x values.
    pub x: ColId,
    /// Column for bottom values.
    pub y0: ColId,
    /// Column for top values.
    pub y1: ColId,
    /// X scale mapping data x into scene x.
    pub x_scale: ScaleContinuous,
    /// Y scale mapping data y into scene y.
    pub y_scale: ScaleContinuous,
    /// Fill paint for the area.
    pub fill: Brush,
    /// Optional stroke for the outline (drawn along `y1`).
    pub stroke: Option<StrokeStyle>,
    /// Rendering order hint (`vizir_core::Mark::z_index`) for the filled area.
    pub z_index: i32,
}

impl StackedAreaMarkSpec {
    /// Creates a stacked area mark with default fill (`Brush::default()`).
    pub fn new(
        id_base: u64,
        table: TableId,
        x: ColId,
        y0: ColId,
        y1: ColId,
        x_scale: ScaleContinuous,
        y_scale: ScaleContinuous,
    ) -> Self {
        Self {
            id_base,
            table,
            x,
            y0,
            y1,
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

    /// Sets the outline stroke (drawn along `y1`).
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
        let y0_col = self.y0;
        let y1_col = self.y1;
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
                let mut bot: Vec<(f64, f64)> = Vec::with_capacity(n);

                for row in 0..n {
                    let x = ctx.table_f64(table_id, row, x_col).unwrap_or(0.0);
                    let y0 = ctx.table_f64(table_id, row, y0_col).unwrap_or(0.0);
                    let y1 = ctx.table_f64(table_id, row, y1_col).unwrap_or(0.0);
                    top.push((x_scale.map(x), y_scale.map(y1)));
                    bot.push((x_scale.map(x), y_scale.map(y0)));
                }

                let mut p = BezPath::new();
                if top.is_empty() {
                    return p;
                }

                p.move_to(bot[0]);
                p.line_to(top[0]);
                for &pt in top.iter().skip(1) {
                    p.line_to(pt);
                }
                for &pt in bot.iter().rev() {
                    p.line_to(pt);
                }
                p.close_path();
                p
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
                    let mut p = BezPath::new();
                    for row in 0..n {
                        let x = ctx.table_f64(table_id, row, x_col).unwrap_or(0.0);
                        let y = ctx.table_f64(table_id, row, y1_col).unwrap_or(0.0);
                        let pt = (x_scale.map(x), y_scale.map(y));
                        if row == 0 {
                            p.move_to(pt);
                        } else {
                            p.line_to(pt);
                        }
                    }
                    p
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
