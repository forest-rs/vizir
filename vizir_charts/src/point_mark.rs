// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Point mark generation.

extern crate alloc;

use alloc::vec::Vec;

use peniko::Brush;
use vizir_core::{ColId, InputRef, Mark, MarkId, TableId};

use crate::scale::ScaleContinuous;
use crate::symbol::Symbol;

/// A point mark derived from a table.
///
/// This generates one [`vizir_core::MarkKind::Rect`] mark per row key, using a square as the
/// point glyph.
#[derive(Clone, Debug)]
pub struct PointMarkSpec {
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
    /// Square size in scene coordinates.
    pub size: f64,
    /// The point glyph shape.
    pub symbol: Symbol,
    /// Fill paint for the point glyphs.
    pub fill: Brush,
    /// Rendering order hint (`vizir_core::Mark::z_index`).
    pub z_index: i32,
}

impl PointMarkSpec {
    /// Creates a point mark spec with a size of 6 and a default fill (`Brush::default()`).
    pub fn new(
        table: TableId,
        x: ColId,
        y: ColId,
        x_scale: ScaleContinuous,
        y_scale: ScaleContinuous,
    ) -> Self {
        Self {
            table,
            x,
            y,
            x_scale,
            y_scale,
            size: 6.0,
            symbol: Symbol::Square,
            fill: Brush::default(),
            z_index: crate::z_order::SERIES_POINTS,
        }
    }

    /// Sets the glyph size.
    pub fn with_size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }

    /// Sets the fill paint.
    pub fn with_fill(mut self, fill: impl Into<Brush>) -> Self {
        self.fill = fill.into();
        self
    }

    /// Sets the symbol shape.
    pub fn with_symbol(mut self, symbol: Symbol) -> Self {
        self.symbol = symbol;
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
        let x_col = self.x;
        let y_col = self.y;
        let x_scale = self.x_scale;
        let y_scale = self.y_scale;
        let size = self.size;
        let symbol = self.symbol;
        let fill = self.fill.clone();
        let z_index = self.z_index;

        row_keys
            .iter()
            .copied()
            .enumerate()
            .map(|(row, row_key)| {
                let id = MarkId::for_row(table_id, row_key);
                match symbol {
                    Symbol::Square => Mark::builder(id)
                        .rect()
                        .z_index(z_index)
                        .x_compute(
                            [InputRef::TableCol {
                                table: table_id,
                                col: x_col,
                            }],
                            move |ctx, _| {
                                x_scale.map(ctx.table_f64(table_id, row, x_col).unwrap_or(0.0))
                                    - size / 2.0
                            },
                        )
                        .y_compute(
                            [InputRef::TableCol {
                                table: table_id,
                                col: y_col,
                            }],
                            move |ctx, _| {
                                y_scale.map(ctx.table_f64(table_id, row, y_col).unwrap_or(0.0))
                                    - size / 2.0
                            },
                        )
                        .w_const(size)
                        .h_const(size)
                        .fill_brush_const(fill.clone())
                        .build(),
                    Symbol::Circle => Mark::builder(id)
                        .path()
                        .z_index(z_index)
                        .path_compute(
                            [
                                InputRef::TableCol {
                                    table: table_id,
                                    col: x_col,
                                },
                                InputRef::TableCol {
                                    table: table_id,
                                    col: y_col,
                                },
                            ],
                            move |ctx, _| {
                                let x =
                                    x_scale.map(ctx.table_f64(table_id, row, x_col).unwrap_or(0.0));
                                let y =
                                    y_scale.map(ctx.table_f64(table_id, row, y_col).unwrap_or(0.0));
                                symbol.path(x, y, size)
                            },
                        )
                        .fill_brush_const(fill.clone())
                        .stroke_width_const(0.0)
                        .build(),
                }
            })
            .collect()
    }
}
