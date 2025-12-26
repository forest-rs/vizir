// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Legend mark generation.
//!
//! In Vega, legends are guides that compile into mark groups. This module
//! provides a tiny "swatches + labels" legend as a starting point.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use kurbo::Rect;
use peniko::color::palette::css;
use peniko::{Brush, Color};
use vizir_core::{Mark, MarkId, TextAnchor, TextBaseline};

use crate::layout::Size;
use crate::measure::TextMeasurer;
use crate::z_order;

fn union_rect(a: Rect, b: Rect) -> Rect {
    Rect::new(
        a.x0.min(b.x0),
        a.y0.min(b.y0),
        a.x1.max(b.x1),
        a.y1.max(b.y1),
    )
}

fn text_bounds(
    x: f64,
    y: f64,
    size: (f64, f64),
    anchor: TextAnchor,
    baseline: TextBaseline,
) -> Rect {
    let (w, h) = size;
    let (x0, x1) = match anchor {
        TextAnchor::Start => (x, x + w),
        TextAnchor::Middle => (x - w * 0.5, x + w * 0.5),
        TextAnchor::End => (x - w, x),
    };
    let (y0, y1) = match baseline {
        TextBaseline::Middle => (y - h * 0.5, y + h * 0.5),
        TextBaseline::Alphabetic => (y - h, y),
        TextBaseline::Hanging => (y, y + h),
        TextBaseline::Ideographic => (y - h, y),
    };
    Rect::new(x0, y0, x1, y1)
}

/// A simple legend row item.
#[derive(Clone, Debug)]
pub struct LegendItem {
    /// The label string shown next to the swatch.
    pub label: String,
    /// The swatch fill paint.
    pub fill: Brush,
}

impl LegendItem {
    /// Convenience constructor for a solid-color swatch.
    pub fn solid(label: impl Into<String>, color: Color) -> Self {
        Self {
            label: label.into(),
            fill: Brush::Solid(color),
        }
    }
}

/// A minimal legend: a vertical list of color swatches with text labels.
#[derive(Clone, Debug)]
pub struct LegendSwatches {
    /// Stable-id base; each generated mark uses a deterministic offset from this base.
    pub id_base: u64,
    /// Legend origin (top-left).
    pub x: f64,
    /// Legend origin (top-left).
    pub y: f64,
    /// Swatch square size.
    pub swatch_size: f64,
    /// Vertical gap between rows.
    pub row_gap: f64,
    /// Horizontal gap between swatch and label.
    pub label_dx: f64,
    /// Number of columns.
    ///
    /// Items are laid out top-to-bottom, then left-to-right into columns.
    pub columns: usize,
    /// Horizontal gap between columns.
    pub column_gap: f64,
    /// Label font size.
    pub font_size: f64,
    /// Label color.
    pub text_fill: Brush,
    /// Items in display order.
    pub items: Vec<LegendItem>,
}

impl LegendSwatches {
    /// Generate legend marks (swatch rect + label text per item).
    pub fn marks(&self) -> Vec<Mark> {
        let mut out = Vec::new();
        let columns = self.columns.max(1);
        let rows_per_col = self.items.len().div_ceil(columns);
        let row_height = self.swatch_size.max(self.font_size);

        for (i, item) in self.items.iter().enumerate() {
            let col = i / rows_per_col;
            let row = i % rows_per_col;
            let x = self.x + col as f64 * (self.column_width() + self.column_gap);
            let y = self.y + row as f64 * (row_height + self.row_gap);
            let swatch_y = y + (row_height - self.swatch_size) * 0.5;
            let label_y = y + row_height * 0.5;

            // Swatch.
            out.push(
                Mark::builder(MarkId::from_raw(self.id_base + i as u64))
                    .rect()
                    .z_index(z_order::LEGEND_SWATCHES)
                    .x_const(x)
                    .y_const(swatch_y)
                    .w_const(self.swatch_size)
                    .h_const(self.swatch_size)
                    .fill_brush_const(item.fill.clone())
                    .build(),
            );

            // Label.
            out.push(
                Mark::builder(MarkId::from_raw(self.id_base + 1000 + i as u64))
                    .text()
                    .z_index(z_order::LEGEND_LABELS)
                    .x_const(x + self.swatch_size + self.label_dx)
                    .y_const(label_y)
                    .text_const(item.label.clone())
                    .font_size_const(self.font_size)
                    .fill_brush_const(self.text_fill.clone())
                    .text_anchor(TextAnchor::Start)
                    .text_baseline(TextBaseline::Middle)
                    .build(),
            );
        }
        out
    }

    fn column_width(&self) -> f64 {
        self.swatch_size + self.label_dx
    }

    /// Estimates legend bounds using the provided text measurer.
    ///
    /// This is intended for simple guide layout (computing margins / view boxes).
    pub fn bounds(&self, measurer: &impl TextMeasurer) -> Rect {
        let mut bounds: Option<Rect> = None;

        for mark in self.marks() {
            let b = match &mark.encodings {
                vizir_core::MarkEncodings::Rect(enc) => {
                    let vizir_core::Encoding::Const(x) = enc.x else {
                        continue;
                    };
                    let vizir_core::Encoding::Const(y) = enc.y else {
                        continue;
                    };
                    let vizir_core::Encoding::Const(w) = enc.w else {
                        continue;
                    };
                    let vizir_core::Encoding::Const(h) = enc.h else {
                        continue;
                    };
                    Rect::new(x, y, x + w, y + h)
                }
                vizir_core::MarkEncodings::Text(enc) => {
                    let vizir_core::Encoding::Const(x) = enc.x else {
                        continue;
                    };
                    let vizir_core::Encoding::Const(y) = enc.y else {
                        continue;
                    };
                    let vizir_core::Encoding::Const(text) = &enc.text else {
                        continue;
                    };
                    let vizir_core::Encoding::Const(font_size) = enc.font_size else {
                        continue;
                    };
                    let vizir_core::Encoding::Const(anchor) = enc.anchor else {
                        continue;
                    };
                    let vizir_core::Encoding::Const(baseline) = enc.baseline else {
                        continue;
                    };
                    let (w, h) = measurer.measure(text, font_size);
                    text_bounds(x, y, (w, h), anchor, baseline)
                }
                vizir_core::MarkEncodings::Path(_enc) => {
                    // This legend doesn't currently emit paths.
                    continue;
                }
            };
            bounds = Some(match bounds {
                None => b,
                Some(r) => union_rect(r, b),
            });
        }

        bounds.unwrap_or_else(|| Rect::new(self.x, self.y, self.x, self.y))
    }
}

/// An unpositioned legend specification (swatches + labels).
///
/// Use this with a measure/arrange layout pass:
/// - Measure: call [`LegendSwatchesSpec::measure`] to get a desired size.
/// - Arrange: call [`LegendSwatchesSpec::at`] once you know the origin.
#[derive(Clone, Debug)]
pub struct LegendSwatchesSpec {
    /// Stable-id base; each generated mark uses a deterministic offset from this base.
    pub id_base: u64,
    /// Swatch square size.
    pub swatch_size: f64,
    /// Vertical gap between rows.
    pub row_gap: f64,
    /// Horizontal gap between swatch and label.
    pub label_dx: f64,
    /// Number of columns.
    ///
    /// Items are laid out top-to-bottom, then left-to-right into columns.
    pub columns: usize,
    /// Horizontal gap between columns.
    pub column_gap: f64,
    /// Label font size.
    pub font_size: f64,
    /// Label color.
    pub text_fill: Brush,
    /// Items in display order.
    pub items: Vec<LegendItem>,
}

impl LegendSwatchesSpec {
    /// Creates a new legend specification with defaults.
    pub fn new(id_base: u64, items: Vec<LegendItem>) -> Self {
        Self {
            id_base,
            swatch_size: 10.0,
            row_gap: 6.0,
            label_dx: 6.0,
            columns: 1,
            column_gap: 12.0,
            font_size: 10.0,
            text_fill: css::BLACK.into(),
            items,
        }
    }

    /// Set the label text paint.
    pub fn with_text_fill(mut self, text_fill: impl Into<Brush>) -> Self {
        self.text_fill = text_fill.into();
        self
    }

    /// Set the label font size.
    pub fn with_font_size(mut self, font_size: f64) -> Self {
        self.font_size = font_size;
        self
    }

    /// Set the swatch size.
    pub fn with_swatch_size(mut self, swatch_size: f64) -> Self {
        self.swatch_size = swatch_size;
        self
    }

    /// Sets the number of columns.
    pub fn with_columns(mut self, columns: usize) -> Self {
        self.columns = columns.max(1);
        self
    }

    /// Sets the gap between columns.
    pub fn with_column_gap(mut self, column_gap: f64) -> Self {
        self.column_gap = column_gap.max(0.0);
        self
    }

    /// Measures the desired legend size (width/height).
    pub fn measure(&self, measurer: &impl TextMeasurer) -> Size {
        let legend = self.at(0.0, 0.0);
        let b = legend.bounds(measurer);
        Size {
            width: b.width(),
            height: b.height(),
        }
    }

    /// Creates a positioned legend at the given origin.
    pub fn at(&self, x: f64, y: f64) -> LegendSwatches {
        LegendSwatches {
            id_base: self.id_base,
            x,
            y,
            swatch_size: self.swatch_size,
            row_gap: self.row_gap,
            label_dx: self.label_dx,
            columns: self.columns,
            column_gap: self.column_gap,
            font_size: self.font_size,
            text_fill: self.text_fill.clone(),
            items: self.items.clone(),
        }
    }

    /// Generates marks for this legend for the given origin.
    pub fn marks(&self, x: f64, y: f64) -> Vec<Mark> {
        self.at(x, y).marks()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::vec;

    use super::*;
    use crate::measure::HeuristicTextMeasurer;

    #[test]
    fn measure_accounts_for_columns() {
        let measurer = HeuristicTextMeasurer;
        let items = vec![
            LegendItem::solid("A", css::BLACK),
            LegendItem::solid("BBBB", css::BLACK),
            LegendItem::solid("CC", css::BLACK),
            LegendItem::solid("DDDDDD", css::BLACK),
        ];

        let one_col = LegendSwatchesSpec::new(1, items.clone()).with_columns(1);
        let two_col = LegendSwatchesSpec::new(1, items).with_columns(2);

        let s1 = one_col.measure(&measurer);
        let s2 = two_col.measure(&measurer);

        assert!(s2.width > s1.width);
        assert!(s2.height < s1.height);
    }

    #[test]
    fn bounds_match_measure_at_origin() {
        let measurer = HeuristicTextMeasurer;
        let items = vec![
            LegendItem::solid("A", css::BLACK),
            LegendItem::solid("BBBB", css::BLACK),
            LegendItem::solid("CC", css::BLACK),
        ];
        let spec = LegendSwatchesSpec::new(1, items).with_columns(2);

        let desired = spec.measure(&measurer);
        let legend = spec.at(10.0, 20.0);
        let b = legend.bounds(&measurer);

        assert_eq!(b.x0, 10.0);
        assert_eq!(b.y0, 20.0);
        assert!((b.width() - desired.width).abs() < 1e-6);
        assert!((b.height() - desired.height).abs() < 1e-6);
    }
}
