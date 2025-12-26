// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Vega-ish chart building blocks for `vizir_core`.
//!
//! This crate is a small, reusable layer above `vizir_core`:
//! - **Scales** map data values into screen coordinates.
//! - **Guides** (axes, legends) are built by generating `vizir_core::Mark`s.
//!
//! It is designed so higher-level frontends (a Rust DSL, or a future Vega/Vega-Lite
//! lowering layer) can compile down to:
//! - input tables/signals, and
//! - a set of stable-identity marks (with encodings) suitable for incremental diffing.
//!
//! Text shaping and layout are out of scope; text marks store unshaped strings.

#![no_std]

extern crate alloc;

mod area_mark;
mod axis;
mod bar_mark;
mod chart_spec;
#[cfg(not(feature = "std"))]
mod float;
mod format;
mod layout;
mod legend;
mod line_mark;
mod measure;
mod point_mark;
mod rect_mark;
mod rule_mark;
mod scale;
mod sector_mark;
mod stacked_area_chart;
mod stacked_area_mark;
mod stacked_bar_chart;
mod stacked_bar_mark;
#[cfg(test)]
mod stacked_tests;
mod symbol;
mod text_mark;
mod time;
mod title;
mod z_order;

pub use area_mark::AreaMarkSpec;
pub use axis::{AxisOrient, AxisSpec, AxisStyle, GridStyle, StrokeStyle};
pub use bar_mark::BarMarkSpec;
pub use chart_spec::ChartSpec;
pub use layout::{ChartLayout, ChartLayoutSpec, LegendOrient, LegendPlacement, Size};
pub use legend::{LegendItem, LegendSwatches, LegendSwatchesSpec};
pub use line_mark::LineMarkSpec;
pub use measure::{HeuristicTextMeasurer, TextMeasurer};
pub use point_mark::PointMarkSpec;
pub use rect_mark::RectMarkSpec;
pub use rule_mark::RuleMarkSpec;
pub use scale::{
    ScaleBand, ScaleBandSpec, ScaleContinuous, ScaleLinear, ScaleLinearSpec, ScaleLog,
    ScaleLogSpec, ScalePoint, ScalePointSpec, ScaleSpec, ScaleTime, ScaleTimeSpec,
    infer_domain_f64,
};
pub use sector_mark::SectorMarkSpec;
pub use stacked_area_chart::StackedAreaChartSpec;
pub use stacked_area_mark::StackedAreaMarkSpec;
pub use stacked_bar_chart::StackedBarChartSpec;
pub use stacked_bar_mark::StackedBarMarkSpec;
pub use symbol::Symbol;
pub use text_mark::TextMarkSpec;
pub use time::{format_time_seconds, nice_time_ticks_seconds};
pub use title::TitleSpec;
pub use z_order::*;
