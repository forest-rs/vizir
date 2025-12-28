// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text measurement hooks for guide layout.
//!
//! `VizIR` charts need to measure text to perform **layout** (axes, legends, titles).
//! In Vega, guide layout is driven by renderer text metrics. In `VizIR` we keep
//! shaping and glyph layout downstream, so chart code depends on a tiny text
//! measurement interface.
//!
//! This crate is intentionally:
//! - small and dependency-light,
//! - `no_std`-friendly (it uses `alloc` for owned font family names), and
//! - renderer-agnostic (native shaping engines and web canvas measurement can
//!   both implement the same trait).

#![no_std]

extern crate alloc;

use alloc::sync::Arc;

/// A minimal text measurement interface used by guide generators.
///
/// This is used by axes/legends/titles to estimate their extents (margins)
/// before marks are generated.
///
/// Implementations can be:
/// - heuristic (fast, but inaccurate),
/// - backed by a shaping engine (e.g. Parley), or
/// - backed by web platform text measurement (e.g. HTML canvas).
pub trait TextMeasurer {
    /// Measure a single line of text.
    ///
    /// `text` is treated as a single line; callers should split on `\n` if they
    /// want multi-line layout.
    fn measure(&self, text: &str, style: TextStyle) -> TextMetrics;
}

/// Text styling inputs relevant to measurement.
///
/// This is intentionally minimal: it’s just enough to make chart layout
/// consistent. More detailed typography (attributed text, shaping options,
/// fallback, etc.) belongs in a higher-level text system.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Font size in the chart’s coordinate system (typically pixels).
    pub font_size: f64,
    /// The preferred font family.
    pub font_family: FontFamily,
    /// Font weight (e.g. `400` for normal, `700` for bold).
    pub font_weight: FontWeight,
    /// Font style (normal/italic/oblique).
    pub font_style: FontStyle,
}

impl TextStyle {
    /// Creates a default `TextStyle` with the given `font_size`.
    #[must_use]
    pub fn new(font_size: f64) -> Self {
        Self {
            font_size,
            font_family: FontFamily::SansSerif,
            font_weight: FontWeight::NORMAL,
            font_style: FontStyle::Normal,
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::new(12.0)
    }
}

/// Font family selection for measurement.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FontFamily {
    /// A generic serif family (CSS `serif`).
    Serif,
    /// A generic sans-serif family (CSS `sans-serif`).
    SansSerif,
    /// A generic monospace family (CSS `monospace`).
    Monospace,
    /// A named family (e.g. `"Inter"`, `"Helvetica Neue"`).
    Named(Arc<str>),
}

impl FontFamily {
    /// Returns the font family string for CSS-style font declarations.
    #[must_use]
    pub fn as_css_family(&self) -> &str {
        match self {
            Self::Serif => "serif",
            Self::SansSerif => "sans-serif",
            Self::Monospace => "monospace",
            Self::Named(name) => name,
        }
    }
}

/// CSS-style font weights.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontWeight(pub u16);

impl FontWeight {
    /// Normal weight (`400`).
    pub const NORMAL: Self = Self(400);
    /// Bold weight (`700`).
    pub const BOLD: Self = Self(700);
}

/// CSS-style font styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontStyle {
    /// Normal style.
    Normal,
    /// Italic style.
    Italic,
    /// Oblique style.
    Oblique,
}

/// Measured metrics for a single line of text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    /// The advance width (useful for horizontal layout).
    pub advance_width: f64,
    /// Distance from baseline to the top of typical glyphs.
    pub ascent: f64,
    /// Distance from baseline to the bottom of typical glyphs.
    pub descent: f64,
    /// Additional line spacing beyond ascent+descent.
    pub leading: f64,
}

impl TextMetrics {
    /// Returns `ascent + descent + leading`.
    #[must_use]
    pub fn line_height(&self) -> f64 {
        self.ascent + self.descent + self.leading
    }
}

/// A tiny heuristic text measurer suitable for demos and early layout.
///
/// It assumes an average glyph width of ~0.6em and a baseline at ~0.8em.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeuristicTextMeasurer;

impl TextMeasurer for HeuristicTextMeasurer {
    fn measure(&self, text: &str, style: TextStyle) -> TextMetrics {
        let advance_width = 0.6 * style.font_size * text.chars().count() as f64;
        let ascent = 0.8 * style.font_size;
        let descent = 0.2 * style.font_size;
        TextMetrics {
            advance_width,
            ascent,
            descent,
            leading: 0.0,
        }
    }
}
