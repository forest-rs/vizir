// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Rule mark generation.
//!
//! A "rule" is a straight line segment (often used for baselines, gridlines, and axis domain
//! lines). This is a Vega mark type and also a Swift Charts primitive.

use kurbo::BezPath;
use peniko::{Brush, Color};
use vizir_core::{Mark, MarkId};

use crate::z_order;

/// A rule mark spec (a stroked line segment).
#[derive(Clone, Debug)]
pub struct RuleMarkSpec {
    /// Stable mark id.
    pub id: MarkId,
    /// Start point x in scene coordinates.
    pub x0: f64,
    /// Start point y in scene coordinates.
    pub y0: f64,
    /// End point x in scene coordinates.
    pub x1: f64,
    /// End point y in scene coordinates.
    pub y1: f64,
    /// Stroke paint.
    pub stroke: Brush,
    /// Stroke width in scene coordinates.
    pub stroke_width: f64,
    /// Rendering order hint (`vizir_core::Mark::z_index`).
    pub z_index: i32,
}

impl RuleMarkSpec {
    /// Creates a new rule between two points.
    pub fn new(id: MarkId, x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self {
            id,
            x0,
            y0,
            x1,
            y1,
            stroke: Brush::default(),
            stroke_width: 1.0,
            z_index: z_order::SERIES_STROKE,
        }
    }

    /// Creates a horizontal rule.
    pub fn horizontal(id: MarkId, y: f64, x0: f64, x1: f64) -> Self {
        Self::new(id, x0, y, x1, y)
    }

    /// Creates a vertical rule.
    pub fn vertical(id: MarkId, x: f64, y0: f64, y1: f64) -> Self {
        Self::new(id, x, y0, x, y1)
    }

    /// Sets stroke paint and width.
    pub fn with_stroke(mut self, stroke: impl Into<Brush>, stroke_width: f64) -> Self {
        self.stroke = stroke.into();
        self.stroke_width = stroke_width;
        self
    }

    /// Sets the z-index used for render ordering.
    pub fn with_z_index(mut self, z_index: i32) -> Self {
        self.z_index = z_index;
        self
    }

    /// Generates the rule mark.
    pub fn mark(&self) -> Mark {
        let mut p = BezPath::new();
        p.move_to((self.x0, self.y0));
        p.line_to((self.x1, self.y1));
        Mark::builder(self.id)
            .path()
            .path_const(p)
            .z_index(self.z_index)
            .fill_const(Color::TRANSPARENT)
            .stroke_brush_const(self.stroke.clone())
            .stroke_width_const(self.stroke_width)
            .build()
    }
}
