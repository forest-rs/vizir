// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Line mark generation.

extern crate alloc;

use alloc::vec::Vec;

use kurbo::BezPath;
use peniko::Color;
use vizir_core::{ColId, InputRef, Mark, MarkId, TableId};

use crate::axis::StrokeStyle;
use crate::scale::ScaleContinuous;

/// A line mark derived from a table.
///
/// This generates a single [`vizir_core::MarkKind::Path`] mark.
#[derive(Clone, Debug)]
pub struct LineMarkSpec {
    /// Stable-id for the mark emitted by this spec.
    pub id: MarkId,
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
    /// Stroke style for the line.
    pub stroke: StrokeStyle,
    /// Rendering order hint (`vizir_core::Mark::z_index`).
    pub z_index: i32,
}

impl LineMarkSpec {
    /// Creates a line mark spec with a black stroke at width 1.
    pub fn new(
        id: MarkId,
        table: TableId,
        x: ColId,
        y: ColId,
        x_scale: ScaleContinuous,
        y_scale: ScaleContinuous,
    ) -> Self {
        Self {
            id,
            table,
            x,
            y,
            x_scale,
            y_scale,
            stroke: StrokeStyle::default(),
            z_index: crate::z_order::SERIES_STROKE,
        }
    }

    /// Sets the stroke style.
    pub fn with_stroke(mut self, stroke: StrokeStyle) -> Self {
        self.stroke = stroke;
        self
    }

    /// Sets the z-index used for render ordering.
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
        let stroke_brush = self.stroke.brush.clone();
        let stroke_width = self.stroke.stroke_width;
        let z_index = self.z_index;

        let line = Mark::builder(self.id)
            .path()
            .z_index(z_index)
            .path_compute([InputRef::Table { table: table_id }], move |ctx, _| {
                let n = ctx.table_row_count(table_id).unwrap_or(0);
                let mut p = BezPath::new();
                for row in 0..n {
                    let x = ctx.table_f64(table_id, row, x_col).unwrap_or(0.0);
                    let y = ctx.table_f64(table_id, row, y_col).unwrap_or(0.0);
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

        alloc::vec![line]
    }
}
