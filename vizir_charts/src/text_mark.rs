// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text mark generation.

extern crate alloc;

use alloc::string::String;

use kurbo::Point;
use peniko::Brush;
use vizir_core::{Mark, MarkId, TextAnchor, TextBaseline};

/// A text mark spec.
#[derive(Clone, Debug)]
pub struct TextMarkSpec {
    /// Stable mark id.
    pub id: MarkId,
    /// Anchor position in scene coordinates.
    pub pos: Point,
    /// Text content (unshaped).
    pub text: String,
    /// Font size in scene coordinates.
    pub font_size: f64,
    /// Text rotation angle in degrees.
    pub angle: f64,
    /// Horizontal anchor.
    pub anchor: TextAnchor,
    /// Vertical baseline.
    pub baseline: TextBaseline,
    /// Fill paint.
    pub fill: Brush,
    /// Rendering order hint (`vizir_core::Mark::z_index`).
    pub z_index: i32,
}

impl TextMarkSpec {
    /// Creates a new text mark spec with default styling.
    pub fn new(id: MarkId, pos: Point, text: impl Into<String>) -> Self {
        Self {
            id,
            pos,
            text: text.into(),
            font_size: 12.0,
            angle: 0.0,
            anchor: TextAnchor::Start,
            baseline: TextBaseline::Middle,
            fill: Brush::default(),
            z_index: crate::z_order::TITLES,
        }
    }

    /// Sets the font size.
    pub fn with_font_size(mut self, font_size: f64) -> Self {
        self.font_size = font_size;
        self
    }

    /// Sets the fill paint.
    pub fn with_fill(mut self, fill: impl Into<Brush>) -> Self {
        self.fill = fill.into();
        self
    }

    /// Sets the text anchor.
    pub fn with_anchor(mut self, anchor: TextAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Sets the text baseline.
    pub fn with_baseline(mut self, baseline: TextBaseline) -> Self {
        self.baseline = baseline;
        self
    }

    /// Sets the text rotation angle (degrees).
    pub fn with_angle(mut self, angle: f64) -> Self {
        self.angle = angle;
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
            .text()
            .z_index(self.z_index)
            .x_const(self.pos.x)
            .y_const(self.pos.y)
            .text_const(self.text.clone())
            .font_size_const(self.font_size)
            .fill_brush_const(self.fill.clone())
            .text_anchor(self.anchor)
            .text_baseline(self.baseline)
            .angle_const(self.angle)
            .build()
    }
}
