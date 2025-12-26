// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Suggested z-order conventions for chart-generated marks.
//!
//! `vizir_core` marks carry an explicit `z_index` for render ordering. The chart layer sets
//! z-indexes consistently so callers don't have to hand-tune paint order in every demo or chart.
//!
//! These values are intentionally coarse. Renderers should sort by `(z_index, MarkId)` for a
//! deterministic tie-break.

/// Plot background/frame fills.
pub const PLOT_BACKGROUND: i32 = -100;
/// Gridlines drawn behind series.
pub const GRID_LINES: i32 = -50;

/// Filled series marks (bars, areas).
pub const SERIES_FILL: i32 = 0;
/// Stroked series marks (lines, rules).
pub const SERIES_STROKE: i32 = 10;
/// Point series marks drawn above lines.
pub const SERIES_POINTS: i32 = 20;

/// Axis domain line and tick marks.
pub const AXIS_RULES: i32 = 30;
/// Axis tick labels.
pub const AXIS_LABELS: i32 = 40;
/// Axis title labels.
pub const AXIS_TITLES: i32 = 50;

/// Legend swatches.
pub const LEGEND_SWATCHES: i32 = 60;
/// Legend labels.
pub const LEGEND_LABELS: i32 = 70;
/// Chart-level titles and annotations.
pub const TITLES: i32 = 80;
