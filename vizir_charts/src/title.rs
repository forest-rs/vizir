// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Chart titles.
//!
//! Vega/Vega-Lite treat titles as part of guide/layout rather than as ordinary data-bound marks.
//! In `VizIR`, titles participate in chart layout (reserve space in [`crate::ChartLayout`]) but are
//! rendered as one or more [`vizir_core::Mark`] values.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use kurbo::Rect;
use peniko::Brush;
use vizir_core::{Mark, MarkId, TextAnchor, TextBaseline};

use crate::z_order;
use crate::{TextMeasurer, TextStyle};

/// A chart-level title.
#[derive(Clone, Debug)]
pub struct TitleSpec {
    /// Stable mark id.
    pub id: MarkId,
    /// Title text (unshaped).
    pub text: String,
    /// Optional subtitle text (unshaped).
    pub subtitle: Option<String>,
    /// Font size in scene coordinates.
    pub font_size: f64,
    /// Subtitle font size in scene coordinates.
    pub subtitle_font_size: f64,
    /// Fill paint.
    pub fill: Brush,
    /// Subtitle fill paint.
    pub subtitle_fill: Brush,
    /// Extra vertical padding around the title text, applied above and below.
    pub padding: f64,
    /// Additional vertical gap between the title and subtitle.
    pub subtitle_gap: f64,
    /// Horizontal anchor within the title rectangle.
    pub anchor: TextAnchor,
    /// Vertical baseline within the title rectangle.
    pub baseline: TextBaseline,
    /// Rendering order hint (`vizir_core::Mark::z_index`).
    pub z_index: i32,
}

impl TitleSpec {
    /// Creates a title spec with default styling.
    pub fn new(id: MarkId, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
            subtitle: None,
            font_size: 12.0,
            subtitle_font_size: 11.0,
            fill: Brush::default(),
            subtitle_fill: Brush::default(),
            padding: 6.0,
            subtitle_gap: 2.0,
            anchor: TextAnchor::Middle,
            baseline: TextBaseline::Middle,
            z_index: z_order::TITLES,
        }
    }

    /// Returns the thickness (height) reserved by this title in chart layout.
    pub fn measure(&self, measurer: &dyn TextMeasurer) -> f64 {
        let pad = self.padding.max(0.0);
        let title_metrics = measurer.measure(&self.text, TextStyle::new(self.font_size));
        let mut total = 2.0 * pad + title_metrics.line_height();
        if let Some(sub) = &self.subtitle {
            let sub_metrics = measurer.measure(sub, TextStyle::new(self.subtitle_font_size));
            total += self.subtitle_gap.max(0.0) + sub_metrics.line_height();
        }
        total.max(0.0)
    }

    /// Sets the subtitle text.
    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Clears the subtitle text.
    pub fn without_subtitle(mut self) -> Self {
        self.subtitle = None;
        self
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

    /// Sets the subtitle fill paint.
    pub fn with_subtitle_fill(mut self, fill: impl Into<Brush>) -> Self {
        self.subtitle_fill = fill.into();
        self
    }

    /// Sets the subtitle font size.
    pub fn with_subtitle_font_size(mut self, font_size: f64) -> Self {
        self.subtitle_font_size = font_size;
        self
    }

    /// Sets the gap between title and subtitle.
    pub fn with_subtitle_gap(mut self, gap: f64) -> Self {
        self.subtitle_gap = gap;
        self
    }

    /// Sets the vertical padding.
    pub fn with_padding(mut self, padding: f64) -> Self {
        self.padding = padding;
        self
    }

    /// Emits the title marks placed within the provided title rectangle.
    pub fn marks(&self, measurer: &dyn TextMeasurer, title_rect: Rect) -> Vec<Mark> {
        let x = match self.anchor {
            TextAnchor::Start => title_rect.x0,
            TextAnchor::Middle => 0.5 * (title_rect.x0 + title_rect.x1),
            TextAnchor::End => title_rect.x1,
        };

        let pad = self.padding.max(0.0);
        let title_metrics = measurer.measure(&self.text, TextStyle::new(self.font_size));
        let th = title_metrics.line_height();

        let y_title = title_rect.y0 + pad + 0.5 * th;
        let mark = Mark::builder(self.id)
            .text()
            .z_index(self.z_index)
            .x_const(x)
            .y_const(y_title)
            .text_const(self.text.clone())
            .font_size_const(self.font_size)
            .fill_brush_const(self.fill.clone())
            .text_anchor(self.anchor)
            .text_baseline(self.baseline)
            .angle_const(0.0)
            .build();

        let mut out = Vec::new();
        out.push(mark);

        if let Some(subtitle) = &self.subtitle {
            let sub_metrics = measurer.measure(subtitle, TextStyle::new(self.subtitle_font_size));
            let sh = sub_metrics.line_height();
            let y_sub = y_title + 0.5 * th + self.subtitle_gap.max(0.0) + 0.5 * sh;
            out.push(
                Mark::builder(MarkId::from_raw(self.id.0.wrapping_add(1)))
                    .text()
                    .z_index(self.z_index)
                    .x_const(x)
                    .y_const(y_sub)
                    .text_const(subtitle.clone())
                    .font_size_const(self.subtitle_font_size)
                    .fill_brush_const(self.subtitle_fill.clone())
                    .text_anchor(self.anchor)
                    .text_baseline(self.baseline)
                    .angle_const(0.0)
                    .build(),
            );
        }

        out
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use kurbo::Rect;

    use super::*;
    use crate::HeuristicTextMeasurer;

    #[test]
    fn subtitle_increases_measured_height_and_emits_two_marks() {
        let measurer = HeuristicTextMeasurer;
        let title = TitleSpec::new(MarkId::from_raw(10), "Title")
            .with_subtitle("Subtitle")
            .with_font_size(12.0)
            .with_subtitle_font_size(10.0);

        let h = title.measure(&measurer);
        assert!(h > 12.0);

        let rect = Rect::new(0.0, 0.0, 200.0, h);
        let marks = title.marks(&measurer, rect);
        assert_eq!(marks.len(), 2);
    }
}
