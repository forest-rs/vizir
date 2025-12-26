// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text measurement hooks for guide layout.
//!
//! In Vega, guide layout is driven by renderer text metrics. In `VizIR` we keep
//! shaping/layout downstream, so guides accept a measurer callback for rough
//! bounds estimation.

/// A minimal text measurement interface used by guide generators.
///
/// This is used by axes/legends to estimate their extents (margins) before the
/// marks are generated. Callers can plug in a real text measurement backend
/// (e.g. based on shaping), or use [`HeuristicTextMeasurer`].
pub trait TextMeasurer {
    /// Returns `(width, height)` in the same coordinate system as the marks.
    fn measure(&self, text: &str, font_size: f64) -> (f64, f64);
}

/// A tiny heuristic text measurer suitable for demos and early layout.
///
/// It assumes an average glyph width of ~0.6em and height of 1em.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeuristicTextMeasurer;

impl TextMeasurer for HeuristicTextMeasurer {
    fn measure(&self, text: &str, font_size: f64) -> (f64, f64) {
        let width = 0.6 * font_size * text.chars().count() as f64;
        (width, font_size)
    }
}
