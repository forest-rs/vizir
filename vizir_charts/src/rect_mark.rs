// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Rectangle mark generation.

use kurbo::Rect;
use peniko::Brush;
use vizir_core::{Mark, MarkId};

/// A rectangle mark spec.
#[derive(Clone, Debug)]
pub struct RectMarkSpec {
    /// Stable mark id.
    pub id: MarkId,
    /// Rectangle geometry in scene coordinates.
    pub rect: Rect,
    /// Fill paint.
    pub fill: Brush,
    /// Rendering order hint (`vizir_core::Mark::z_index`).
    pub z_index: i32,
}

impl RectMarkSpec {
    /// Creates a new rectangle mark spec.
    pub fn new(id: MarkId, rect: Rect) -> Self {
        Self {
            id,
            rect,
            fill: Brush::default(),
            z_index: crate::z_order::SERIES_FILL,
        }
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

    /// Generates the mark.
    pub fn mark(&self) -> Mark {
        Mark::builder(self.id)
            .rect()
            .z_index(self.z_index)
            .x_const(self.rect.x0)
            .y_const(self.rect.y0)
            .w_const(self.rect.width())
            .h_const(self.rect.height())
            .fill_brush_const(self.fill.clone())
            .build()
    }
}
