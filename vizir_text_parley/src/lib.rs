// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Parley-backed text measurement adapter.
//!
//! This crate implements [`vizir_text::TextMeasurer`] using Parley, enabling
//! shaping-aware text metrics for chart guide layout (axes, legends, titles).

#![no_std]

extern crate alloc;

use alloc::borrow::Cow;
use core::cell::RefCell;

use parley::style::{FontFamily as ParleyFontFamily, FontStack, GenericFamily, StyleProperty};
use parley::{Alignment, AlignmentOptions, FontContext, FontStyle as ParleyFontStyle, FontWeight};
use vizir_text::{FontFamily, FontStyle, TextMeasurer, TextMetrics, TextStyle};

/// A [`TextMeasurer`] backed by Parley.
///
/// Today this is primarily intended for single-line measurement in chart guides.
pub struct ParleyTextMeasurer {
    font_cx: RefCell<FontContext>,
    layout_cx: RefCell<parley::LayoutContext<()>>,
    display_scale: f32,
    quantize: bool,
}

impl core::fmt::Debug for ParleyTextMeasurer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ParleyTextMeasurer")
            .field("display_scale", &self.display_scale)
            .field("quantize", &self.quantize)
            .finish_non_exhaustive()
    }
}

impl ParleyTextMeasurer {
    /// Creates a new Parley-backed text measurer.
    ///
    /// Note: font loading and resolution policy is still evolving in `VizIR`; for
    /// now this measurer uses Parley’s default system font configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            font_cx: RefCell::new(FontContext::new()),
            layout_cx: RefCell::new(parley::LayoutContext::new()),
            display_scale: 1.0,
            quantize: true,
        }
    }

    /// Sets the display scale passed to Parley.
    ///
    /// This is typically a device pixel ratio. Measurements returned by this
    /// measurer are scaled back into logical coordinates (divide by scale).
    #[must_use]
    pub fn with_display_scale(mut self, display_scale: f32) -> Self {
        self.display_scale = display_scale.max(0.0);
        self
    }

    /// Sets whether Parley should quantize layout coordinates to pixel boundaries.
    #[must_use]
    pub fn with_quantize(mut self, quantize: bool) -> Self {
        self.quantize = quantize;
        self
    }

    fn parley_font_stack<'a>(family: &'a FontFamily) -> FontStack<'a> {
        let family = match family {
            FontFamily::Serif => ParleyFontFamily::Generic(GenericFamily::Serif),
            FontFamily::SansSerif => ParleyFontFamily::Generic(GenericFamily::SansSerif),
            FontFamily::Monospace => ParleyFontFamily::Generic(GenericFamily::Monospace),
            FontFamily::Named(name) => ParleyFontFamily::Named(Cow::Borrowed(name.as_ref())),
        };
        FontStack::from(family)
    }

    fn parley_font_style(style: FontStyle) -> ParleyFontStyle {
        match style {
            FontStyle::Normal => ParleyFontStyle::Normal,
            FontStyle::Italic => ParleyFontStyle::Italic,
            FontStyle::Oblique => ParleyFontStyle::Oblique(None),
        }
    }

    fn font_size_f32(font_size: f64) -> f32 {
        if !font_size.is_finite() {
            return 0.0;
        }
        let font_size = font_size.max(0.0);
        if font_size >= f64::from(f32::MAX) {
            f32::MAX
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "Value is clamped to f32::MAX above"
            )]
            {
                font_size as f32
            }
        }
    }
}

impl Default for ParleyTextMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextMeasurer for ParleyTextMeasurer {
    fn measure(&self, text: &str, style: TextStyle) -> TextMetrics {
        let text = text.split('\n').next().unwrap_or("");
        if text.is_empty() {
            return TextMetrics {
                advance_width: 0.0,
                ascent: 0.0,
                descent: 0.0,
                leading: 0.0,
            };
        }

        let scale = self.display_scale.max(1.0e-6);

        let mut font_cx = self.font_cx.borrow_mut();
        let mut layout_cx = self.layout_cx.borrow_mut();

        let mut builder = layout_cx.ranged_builder(&mut font_cx, text, scale, self.quantize);
        builder.push_default(StyleProperty::FontSize(Self::font_size_f32(
            style.font_size,
        )));
        builder.push_default(StyleProperty::FontStack(Self::parley_font_stack(
            &style.font_family,
        )));
        builder.push_default(StyleProperty::FontStyle(Self::parley_font_style(
            style.font_style,
        )));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(
            style.font_weight.0 as f32,
        )));

        let mut layout: parley::Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        layout.align(None, Alignment::Start, AlignmentOptions::default());

        let Some(line) = layout.lines().next() else {
            return TextMetrics {
                advance_width: 0.0,
                ascent: 0.0,
                descent: 0.0,
                leading: 0.0,
            };
        };

        let m = line.metrics();
        TextMetrics {
            advance_width: m.advance as f64 / scale as f64,
            ascent: m.ascent as f64 / scale as f64,
            descent: m.descent as f64 / scale as f64,
            leading: m.leading as f64 / scale as f64,
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn parley_measurer_is_nonzero_for_nonempty_text() {
        let m = ParleyTextMeasurer::new();
        let metrics = m.measure("Hello", TextStyle::new(12.0));
        assert!(metrics.advance_width > 0.0);
        assert!(metrics.ascent > 0.0);
        assert!(metrics.descent > 0.0);
    }
}
