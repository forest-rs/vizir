// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Web/WASM text measurement adapter.
//!
//! This crate provides a [`vizir_text::TextMeasurer`] implementation for
//! `wasm32-*` targets using HTML Canvas `measureText`, similar to Vega’s
//! approach.
//!
//! Notes:
//! - This uses `web-sys`/`wasm-bindgen` only on `wasm32` targets.
//! - Non-`wasm32` builds fall back to a heuristic measurer.

#![no_std]

extern crate alloc;

#[cfg(target_arch = "wasm32")]
use alloc::{format, string::String};
use vizir_text::{HeuristicTextMeasurer, TextMeasurer, TextMetrics, TextStyle};

/// A `wasm32` measurer backed by HTML Canvas 2D text metrics.
///
/// On non-`wasm32` targets, this type is still available but always falls back
/// to [`HeuristicTextMeasurer`].
#[derive(Clone, Debug)]
pub struct WebTextMeasurer {
    #[cfg(target_arch = "wasm32")]
    ctx: web_sys::CanvasRenderingContext2d,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for WebTextMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

impl WebTextMeasurer {
    #[cfg(target_arch = "wasm32")]
    fn css_font(style: &TextStyle) -> String {
        let family = style.font_family.as_css_family();
        let weight = style.font_weight.0;
        let font_style = match style.font_style {
            vizir_text::FontStyle::Normal => "normal",
            vizir_text::FontStyle::Italic => "italic",
            vizir_text::FontStyle::Oblique => "oblique",
        };
        format!("{font_style} {weight} {}px {family}", style.font_size)
    }

    /// Creates a web measurer using an offscreen canvas.
    ///
    /// This requires a browser-like environment with `window` and `document`.
    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Result<Self, wasm_bindgen::JsValue> {
        use wasm_bindgen::JsCast as _;

        let window = web_sys::window()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("vizir_text_web: missing window"))?;
        let document = window
            .document()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("vizir_text_web: missing document"))?;
        let canvas = document
            .create_element("canvas")?
            .dyn_into::<web_sys::HtmlCanvasElement>()?;
        let ctx = canvas
            .get_context("2d")?
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("vizir_text_web: missing 2d context"))?
            .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
        Ok(Self { ctx })
    }

    /// Creates a web measurer that uses an existing canvas 2D context.
    ///
    /// This is useful for embedders that want to reuse an existing canvas (or
    /// an offscreen canvas) instead of having `vizir_text_web` create DOM nodes.
    #[cfg(target_arch = "wasm32")]
    #[must_use]
    pub fn from_canvas_context(ctx: web_sys::CanvasRenderingContext2d) -> Self {
        Self { ctx }
    }

    /// Creates a non-web measurer that always falls back to heuristics.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl TextMeasurer for WebTextMeasurer {
    fn measure(&self, text: &str, style: TextStyle) -> TextMetrics {
        #[cfg(target_arch = "wasm32")]
        {
            self.ctx.set_font(&Self::css_font(&style));
            let metrics = match self.ctx.measure_text(text) {
                Ok(m) => m,
                Err(_) => return HeuristicTextMeasurer.measure(text, style),
            };

            // `width` is widely supported; the bounding box fields are supported in modern
            // browsers but may be 0 or absent in older engines. Treat zeros as unknown.
            let width = metrics.width();
            let ascent = metrics.actual_bounding_box_ascent();
            let descent = metrics.actual_bounding_box_descent();

            let ascent = if ascent > 0.0 {
                ascent
            } else {
                0.8 * style.font_size
            };
            let descent = if descent > 0.0 {
                descent
            } else {
                0.2 * style.font_size
            };

            TextMetrics {
                advance_width: width,
                ascent,
                descent,
                leading: 0.0,
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        HeuristicTextMeasurer.measure(text, style)
    }
}
