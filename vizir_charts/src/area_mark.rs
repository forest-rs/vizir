// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Area mark generation.

extern crate alloc;

use alloc::vec::Vec;

use kurbo::BezPath;
use peniko::{Brush, Color};
use vizir_core::{ColId, InputRef, Mark, MarkId, TableId};

use crate::axis::StrokeStyle;
use crate::scale::ScaleContinuous;

/// An area mark derived from a table.
///
/// This generates:
/// - one filled [`vizir_core::MarkKind::Path`] mark for the area, and
/// - optionally one stroked [`vizir_core::MarkKind::Path`] mark for the outline.
#[derive(Clone, Debug)]
pub struct AreaMarkSpec {
    /// Stable-id base for marks emitted by this mark.
    pub id_base: u64,
    /// Source table id.
    pub table: TableId,
    /// Column for x values.
    pub x: ColId,
    /// Column for y values.
    pub y: ColId,
    /// X scale mapping data x into scene x.
    pub x_scale: ScaleContinuous,
    /// Y scale mapping data y into scene y.
    pub y_scale: ScaleContinuous,
    /// Baseline in data units (typically `0.0`).
    pub baseline: f64,
    /// Fill paint for the area.
    pub fill: Brush,
    /// Optional stroke for the outline.
    pub stroke: Option<StrokeStyle>,
    /// Rendering order hint (`vizir_core::Mark::z_index`) for the filled area.
    pub z_index: i32,
}

impl AreaMarkSpec {
    /// Creates an area mark with a baseline at `0` and default fill (`Brush::default()`).
    pub fn new(
        id_base: u64,
        table: TableId,
        x: ColId,
        y: ColId,
        x_scale: ScaleContinuous,
        y_scale: ScaleContinuous,
    ) -> Self {
        Self {
            id_base,
            table,
            x,
            y,
            x_scale,
            y_scale,
            baseline: 0.0,
            fill: Brush::default(),
            stroke: None,
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

    /// Sets the outline stroke.
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
        let x_scale = self.x_scale;
        let y_scale = self.y_scale;
        let baseline = self.baseline;

        let fill = self.fill.clone();
        let area_id = MarkId::from_raw(self.id_base);
        let z_index = self.z_index;
        let area = Mark::builder(area_id)
            .path()
            .z_index(z_index)
            .path_compute([InputRef::Table { table: table_id }], move |ctx, _| {
                let n = ctx.table_row_count(table_id).unwrap_or(0);
                let mut p = BezPath::new();
                if n == 0 {
                    return p;
                }

                let y0 = y_scale.map(baseline);
                let mut last_x = x_scale.map(0.0);

                for row in 0..n {
                    let x = ctx.table_f64(table_id, row, x_col).unwrap_or(0.0);
                    let y = ctx.table_f64(table_id, row, y_col).unwrap_or(baseline);
                    let pt = (x_scale.map(x), y_scale.map(y));
                    last_x = pt.0;
                    if row == 0 {
                        p.move_to((pt.0, y0));
                        p.line_to(pt);
                    } else {
                        p.line_to(pt);
                    }
                }

                p.line_to((last_x, y0));
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
                        let y = ctx.table_f64(table_id, row, y_col).unwrap_or(baseline);
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
