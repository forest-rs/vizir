// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Axis mark generation.
//!
//! Vega models axes as a single axis spec with an `orient` of `top`, `bottom`,
//! `left`, or `right`. This module mirrors that shape: a single [`AxisSpec`]
//! that can be measured (for layout) and arranged (to generate marks).

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

#[cfg(not(feature = "std"))]
use crate::float::FloatExt;

use kurbo::{BezPath, Rect};
use peniko::Brush;
use peniko::color::palette::css;
use vizir_core::{Mark, MarkId, TextAnchor, TextBaseline};

use crate::format::format_tick_with_step;
use crate::rule_mark::RuleMarkSpec;
use crate::scale::{
    ScaleBand, ScaleContinuous, ScaleLinear, ScaleLog, ScalePoint, ScaleSpec, ScaleTime,
};
use crate::z_order;
use crate::{TextMeasurer, TextStyle};

/// A paint + width pair for stroked paths (domain lines, ticks, gridlines).
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    /// Stroke paint.
    pub brush: Brush,
    /// Stroke width in scene coordinates.
    pub stroke_width: f64,
}

impl StrokeStyle {
    /// Convenience for a solid stroke.
    pub fn solid(brush: impl Into<Brush>, stroke_width: f64) -> Self {
        Self {
            brush: brush.into(),
            stroke_width,
        }
    }
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self::solid(css::BLACK, 1.0)
    }
}

/// Axis styling defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisStyle {
    /// Style for the axis domain line and tick marks.
    pub rule: StrokeStyle,
    /// Fill paint for tick labels.
    pub label_fill: Brush,
    /// Font size for tick labels.
    pub label_font_size: f64,
    /// Fill paint for the axis title.
    pub title_fill: Brush,
    /// Font size for the axis title.
    pub title_font_size: f64,
}

impl Default for AxisStyle {
    fn default() -> Self {
        let rule = StrokeStyle::default();
        Self {
            rule: rule.clone(),
            label_fill: rule.brush.clone(),
            label_font_size: 10.0,
            title_fill: rule.brush,
            title_font_size: 11.0,
        }
    }
}

/// Gridline styling.
#[derive(Clone, Debug, PartialEq)]
pub struct GridStyle {
    /// Stroke style for gridlines.
    pub stroke: StrokeStyle,
}

impl Default for GridStyle {
    fn default() -> Self {
        Self {
            stroke: StrokeStyle {
                brush: Brush::Solid(css::BLACK.with_alpha(40.0 / 255.0)),
                stroke_width: 1.0,
            },
        }
    }
}

/// Axis orientation, matching Vega’s axis `orient` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AxisOrient {
    /// A horizontal axis placed above the plot area.
    Top,
    /// A horizontal axis placed below the plot area.
    Bottom,
    /// A vertical axis placed to the left of the plot area.
    Left,
    /// A vertical axis placed to the right of the plot area.
    Right,
}

/// A Vega-ish axis specification (single type + `orient`).
#[derive(Clone)]
pub struct AxisSpec {
    /// Stable-id base; each generated mark uses a deterministic offset from this base.
    pub id_base: u64,
    /// The axis scale specification.
    pub scale: ScaleSpec,
    /// Axis placement relative to the plot.
    pub orient: AxisOrient,
    /// Approximate number of ticks.
    pub tick_count: usize,
    /// Tick line length (in pixels). Direction depends on [`AxisSpec::orient`].
    pub tick_size: f64,
    /// Whether to draw tick marks.
    pub ticks: bool,
    /// Whether to draw tick labels.
    pub labels: bool,
    /// Whether to draw the axis domain line.
    pub show_domain: bool,
    /// Padding between the tick end and the tick label.
    ///
    /// This corresponds to Vega’s `tickPadding`.
    pub tick_padding: f64,
    /// Extra padding applied between the axis/ticks and tick labels.
    ///
    /// This corresponds to Vega’s `labelPadding`.
    pub label_padding: f64,
    /// Axis styling.
    pub style: AxisStyle,
    /// Optional gridline styling.
    ///
    /// If `Some`, gridline marks are generated spanning the plot area.
    pub grid: Option<GridStyle>,
    /// Optional axis title text.
    pub title: Option<String>,
    /// Distance from tick labels to the title.
    pub title_offset: f64,
    /// Optional tick label formatter.
    ///
    /// If provided, this is used for both measuring and rendering tick labels.
    /// The second argument is the tick step (best-effort), which can be used for consistent
    /// decimal formatting.
    pub tick_formatter: Option<Arc<dyn Fn(f64, f64) -> String>>,
    /// Tick label rotation angle in degrees.
    ///
    /// This corresponds to Vega’s `labelAngle`.
    pub label_angle: f64,
}

impl core::fmt::Debug for AxisSpec {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AxisSpec")
            .field("id_base", &self.id_base)
            .field("scale", &self.scale)
            .field("orient", &self.orient)
            .field("tick_count", &self.tick_count)
            .field("tick_size", &self.tick_size)
            .field("ticks", &self.ticks)
            .field("labels", &self.labels)
            .field("show_domain", &self.show_domain)
            .field("tick_padding", &self.tick_padding)
            .field("label_padding", &self.label_padding)
            .field("style", &self.style)
            .field("grid", &self.grid)
            .field("title", &self.title)
            .field("title_offset", &self.title_offset)
            .field("tick_formatter", &self.tick_formatter.is_some())
            .field("label_angle", &self.label_angle)
            .finish()
    }
}

impl AxisSpec {
    /// Creates a new axis specification with sensible defaults.
    ///
    /// The provided scale specification controls the domain and "nice" policy for ticks.
    ///
    /// The returned axis has:
    /// - `tick_count = 10`
    /// - `tick_size = 5`
    /// - `tick_padding = 12` for top/bottom, `6` for left/right
    /// - `label_padding = 0`
    /// - `style = AxisStyle::default()`
    /// - no title and no grid.
    pub fn new(id_base: u64, scale: impl Into<ScaleSpec>, orient: AxisOrient) -> Self {
        let tick_padding = match orient {
            AxisOrient::Top | AxisOrient::Bottom => 12.0,
            AxisOrient::Left | AxisOrient::Right => 6.0,
        };
        Self {
            id_base,
            scale: scale.into(),
            orient,
            tick_count: 10,
            tick_size: 5.0,
            ticks: true,
            labels: true,
            show_domain: true,
            tick_padding,
            label_padding: 0.0,
            style: AxisStyle::default(),
            grid: None,
            title: None,
            title_offset: 10.0,
            tick_formatter: None,
            label_angle: 0.0,
        }
    }

    /// Convenience constructor for a `bottom` axis.
    pub fn bottom(id_base: u64, scale: impl Into<ScaleSpec>) -> Self {
        Self::new(id_base, scale, AxisOrient::Bottom)
    }

    /// Convenience constructor for a `top` axis.
    pub fn top(id_base: u64, scale: impl Into<ScaleSpec>) -> Self {
        Self::new(id_base, scale, AxisOrient::Top)
    }

    /// Convenience constructor for a `left` axis.
    pub fn left(id_base: u64, scale: impl Into<ScaleSpec>) -> Self {
        Self::new(id_base, scale, AxisOrient::Left)
    }

    /// Convenience constructor for a `right` axis.
    pub fn right(id_base: u64, scale: impl Into<ScaleSpec>) -> Self {
        Self::new(id_base, scale, AxisOrient::Right)
    }

    /// Set the approximate tick count.
    pub fn with_tick_count(mut self, tick_count: usize) -> Self {
        self.tick_count = tick_count;
        self
    }

    /// Set tick size in scene coordinates.
    pub fn with_tick_size(mut self, tick_size: f64) -> Self {
        self.tick_size = tick_size;
        self
    }

    /// Enable or disable tick marks.
    pub fn with_ticks(mut self, ticks: bool) -> Self {
        self.ticks = ticks;
        self
    }

    /// Enable or disable tick labels.
    pub fn with_labels(mut self, labels: bool) -> Self {
        self.labels = labels;
        self
    }

    /// Enable or disable the axis domain line.
    pub fn with_domain(mut self, domain: bool) -> Self {
        self.show_domain = domain;
        self
    }

    /// Set tick padding in scene coordinates.
    pub fn with_tick_padding(mut self, tick_padding: f64) -> Self {
        self.tick_padding = tick_padding;
        self
    }

    /// Set label padding in scene coordinates.
    pub fn with_label_padding(mut self, label_padding: f64) -> Self {
        self.label_padding = label_padding;
        self
    }

    /// Set a custom tick label formatter.
    pub fn with_tick_formatter(mut self, f: impl Fn(f64, f64) -> String + 'static) -> Self {
        self.tick_formatter = Some(Arc::new(f));
        self
    }

    /// Set tick label rotation angle in degrees.
    pub fn with_label_angle(mut self, angle_degrees: f64) -> Self {
        self.label_angle = angle_degrees;
        self
    }

    /// Set the axis style.
    pub fn with_style(mut self, style: AxisStyle) -> Self {
        self.style = style;
        self
    }

    /// Enable gridlines using the provided style.
    pub fn with_grid(mut self, grid: GridStyle) -> Self {
        self.grid = Some(grid);
        self
    }

    /// Disable gridlines.
    pub fn without_grid(mut self) -> Self {
        self.grid = None;
        self
    }

    /// Set the axis title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Clear the axis title.
    pub fn without_title(mut self) -> Self {
        self.title = None;
        self
    }

    /// Set the title offset in scene coordinates.
    pub fn with_title_offset(mut self, title_offset: f64) -> Self {
        self.title_offset = title_offset;
        self
    }

    /// Enable or disable nice-domain behavior for this axis.
    pub fn with_nice_domain(mut self, nice_domain: bool) -> Self {
        if let ScaleSpec::Linear(s) = &mut self.scale {
            s.nice = nice_domain;
        }
        self
    }

    /// Returns a continuous scale suitable for mapping axis values into plot coordinates.
    ///
    /// Panics if this axis does not use a continuous scale.
    pub fn scale_continuous(&self, plot: Rect) -> ScaleContinuous {
        let range = match self.orient {
            AxisOrient::Top | AxisOrient::Bottom => (plot.x0, plot.x1),
            AxisOrient::Left | AxisOrient::Right => (plot.y1, plot.y0),
        };

        match self.scale {
            ScaleSpec::Linear(s) => {
                ScaleContinuous::Linear(s.instantiate_resolved(range, self.tick_count))
            }
            ScaleSpec::Log(s) => ScaleContinuous::Log(s.instantiate(range)),
            ScaleSpec::Time(s) => ScaleContinuous::Time(s.instantiate(range)),
            ScaleSpec::Point(_) | ScaleSpec::Band(_) => {
                panic!("scale_continuous called on a discrete axis scale")
            }
        }
    }

    /// Returns a point scale suitable for mapping indices into plot coordinates.
    ///
    /// Panics if this axis does not use a point scale.
    pub fn scale_point(&self, plot: Rect) -> ScalePoint {
        let range = match self.orient {
            AxisOrient::Top | AxisOrient::Bottom => (plot.x0, plot.x1),
            AxisOrient::Left | AxisOrient::Right => (plot.y1, plot.y0),
        };
        match self.scale {
            ScaleSpec::Point(s) => s.instantiate(range),
            _ => panic!("scale_point called on a non-point axis scale"),
        }
    }

    /// Returns a band scale suitable for mapping indices into plot coordinates.
    ///
    /// Panics if this axis does not use a band scale.
    pub fn scale_band(&self, plot: Rect) -> ScaleBand {
        let range = match self.orient {
            AxisOrient::Top | AxisOrient::Bottom => (plot.x0, plot.x1),
            AxisOrient::Left | AxisOrient::Right => (plot.y1, plot.y0),
        };
        match self.scale {
            ScaleSpec::Band(s) => s.instantiate(range),
            _ => panic!("scale_band called on a non-band axis scale"),
        }
    }

    fn tick_values(&self) -> (Vec<f64>, f64) {
        match self.scale {
            ScaleSpec::Linear(s) => {
                let domain = s.resolved_domain(self.tick_count);
                let tmp = ScaleLinear::new(domain, (0.0, 1.0));
                let ticks = tmp.ticks(self.tick_count);
                let step = tick_step(&ticks);
                (ticks, step)
            }
            ScaleSpec::Log(s) => {
                let tmp = ScaleLog::new(s.domain, (0.0, 1.0)).with_base(s.base);
                let ticks = tmp.ticks(self.tick_count);
                (ticks, 0.0)
            }
            ScaleSpec::Time(s) => {
                let tmp = ScaleTime::new(s.domain, (0.0, 1.0));
                let ticks = tmp.ticks(self.tick_count);
                let step = tick_step(&ticks);
                (ticks, step)
            }
            ScaleSpec::Point(s) => {
                let ticks: Vec<f64> = (0..s.count).map(|i| i as f64).collect();
                (ticks, 1.0)
            }
            ScaleSpec::Band(s) => {
                let ticks: Vec<f64> = (0..s.count).map(|i| i as f64).collect();
                (ticks, 1.0)
            }
        }
    }

    fn continuous_domain(&self) -> Option<(f64, f64)> {
        match self.scale {
            ScaleSpec::Linear(s) => Some(s.resolved_domain(self.tick_count)),
            ScaleSpec::Log(s) => Some(s.domain),
            ScaleSpec::Time(s) => Some(s.domain),
            ScaleSpec::Point(_) | ScaleSpec::Band(_) => None,
        }
    }

    /// Measure the thickness this axis needs along its normal direction.
    ///
    /// This is intended for a measure/arrange layout pass.
    pub fn measure(&self, measurer: &dyn TextMeasurer) -> f64 {
        let tick_extent = if self.ticks {
            self.tick_size.abs()
        } else {
            0.0
        };
        let label_gap = self.tick_padding.max(0.0) + self.label_padding.max(0.0);
        match self.orient {
            AxisOrient::Top | AxisOrient::Bottom => {
                let (ticks, step) = self.tick_values();

                let mut max_label_extent = 0.0_f64;
                if self.labels {
                    let theta = self.label_angle.to_radians();
                    let sin = theta.sin().abs();
                    let cos = theta.cos().abs();
                    for v in ticks {
                        let label = self.format_tick(v, step);
                        let metrics =
                            measurer.measure(&label, TextStyle::new(self.style.label_font_size));
                        let w = metrics.advance_width;
                        let h = metrics.line_height();
                        let rotated_h = sin * w + cos * h;
                        max_label_extent = max_label_extent.max(rotated_h);
                    }
                }

                let label_thickness = if self.labels {
                    label_gap + max_label_extent
                } else {
                    0.0
                };
                let mut out = tick_extent + label_thickness;
                if let Some(title) = &self.title {
                    let metrics =
                        measurer.measure(title, TextStyle::new(self.style.title_font_size));
                    out += self.title_offset.max(0.0) + metrics.line_height();
                }
                out
            }
            AxisOrient::Left | AxisOrient::Right => {
                let (ticks, step) = self.tick_values();

                let mut max_label_extent = 0.0_f64;
                if self.labels {
                    let theta = self.label_angle.to_radians();
                    let sin = theta.sin().abs();
                    let cos = theta.cos().abs();
                    for v in ticks {
                        let label = self.format_tick(v, step);
                        let metrics =
                            measurer.measure(&label, TextStyle::new(self.style.label_font_size));
                        let w = metrics.advance_width;
                        let h = metrics.line_height();
                        let rotated_w = cos * w + sin * h;
                        max_label_extent = max_label_extent.max(rotated_w);
                    }
                }

                let label_thickness = if self.labels {
                    label_gap + max_label_extent
                } else {
                    0.0
                };
                let mut out = tick_extent + label_thickness;
                if self.title.is_some() {
                    // With a rotated title, height maps to width.
                    out += self.title_offset.max(0.0) + self.style.title_font_size;
                }
                out
            }
        }
    }

    /// Generate axis marks for the given plot rectangle and arranged axis rectangle.
    ///
    /// `axis_rect` should be the reserved region for this axis, adjacent to `plot`.
    pub fn marks(&self, plot: Rect, axis_rect: Rect) -> Vec<Mark> {
        match self.orient {
            AxisOrient::Top => self.marks_top(plot, axis_rect),
            AxisOrient::Bottom => self.marks_bottom(plot, axis_rect),
            AxisOrient::Left => self.marks_left(plot, axis_rect),
            AxisOrient::Right => self.marks_right(plot, axis_rect),
        }
    }

    fn format_tick(&self, v: f64, step: f64) -> String {
        match &self.tick_formatter {
            Some(f) => (f)(v, step),
            None => match self.scale {
                ScaleSpec::Time(_) => crate::time::format_time_seconds(v, step),
                _ => format_tick_with_step(v, step),
            },
        }
    }

    fn marks_bottom(&self, plot: Rect, axis_rect: Rect) -> Vec<Mark> {
        let y = plot.y1;
        let tick_size = self.tick_size.abs();
        let tick_extent = if self.ticks { tick_size } else { 0.0 };
        let label_gap = (self.tick_padding + self.label_padding).max(0.0);
        let (ticks, step) = self.tick_values();

        let continuous_scale = matches!(
            self.scale,
            ScaleSpec::Linear(_) | ScaleSpec::Log(_) | ScaleSpec::Time(_)
        )
        .then(|| self.scale_continuous(plot));
        let point_scale = matches!(self.scale, ScaleSpec::Point(_)).then(|| self.scale_point(plot));
        let band_scale = matches!(self.scale, ScaleSpec::Band(_)).then(|| self.scale_band(plot));
        let band_width = band_scale.map(|b| b.band_width()).unwrap_or(0.0);

        let tick_x = |v: f64| match self.scale {
            ScaleSpec::Linear(_) | ScaleSpec::Log(_) | ScaleSpec::Time(_) => {
                continuous_scale.expect("missing continuous scale").map(v)
            }
            ScaleSpec::Point(_) => point_scale
                .expect("missing point scale")
                .x(discrete_index(v)),
            ScaleSpec::Band(_) => {
                band_scale.expect("missing band scale").x(discrete_index(v)) + 0.5 * band_width
            }
        };

        let mut out = Vec::new();

        if let Some(grid) = &self.grid {
            let mut ticks_in_plot: Vec<f64> = ticks
                .iter()
                .copied()
                .filter(|v| {
                    let x = tick_x(*v);
                    x >= plot.x0 - 1.0e-9 && x <= plot.x1 + 1.0e-9
                })
                .collect();
            // Ensure the plot boundaries (domain endpoints) get a grid line even if the tick
            // generator doesn't include them.
            if let Some((d0, d1)) = self.continuous_domain() {
                push_if_missing(&mut ticks_in_plot, d0);
                push_if_missing(&mut ticks_in_plot, d1);
            }
            out.extend(grid_vertical(
                self.id_base,
                &ticks_in_plot,
                tick_x,
                plot,
                &grid.stroke.brush,
                grid.stroke.stroke_width,
                z_order::GRID_LINES,
            ));
        }

        // Domain line.
        if self.show_domain {
            let mut domain = BezPath::new();
            domain.move_to((plot.x0, y));
            domain.line_to((plot.x1, y));
            out.push(domain_mark(
                self.id_base,
                domain,
                &self.style.rule.brush,
                self.style.rule.stroke_width,
                z_order::AXIS_RULES,
            ));
        }

        let ticks_len = ticks.len();
        for (i, v) in ticks.iter().copied().enumerate() {
            let x = tick_x(v);
            if x < plot.x0 - 1.0e-9 || x > plot.x1 + 1.0e-9 {
                continue;
            }
            let label = self.format_tick(v, step);

            if self.ticks {
                let mut tick = BezPath::new();
                tick.move_to((x, y));
                tick.line_to((x, y + tick_size));
                out.push(tick_mark(
                    self.id_base,
                    i,
                    tick,
                    &self.style.rule.brush,
                    self.style.rule.stroke_width,
                    z_order::AXIS_RULES,
                ));
            }

            if self.labels {
                let (anchor, x) = if i == 0 {
                    (TextAnchor::Start, x.clamp(plot.x0, plot.x1))
                } else if i + 1 == ticks_len {
                    (TextAnchor::End, x.clamp(plot.x0, plot.x1))
                } else {
                    (TextAnchor::Middle, x)
                };

                // If we rotate around the label's `(x, y)` origin, changing the text anchor
                // changes the rotation origin relative to the label's center. That can manifest
                // as a vertical shift for the first/last tick labels (as the x-offset rotates
                // into y).
                //
                // We compensate by estimating the label width and adjusting `y` so the visual
                // midline stays aligned. This is a heuristic stand-in for real text metrics.
                //
                // TODO: Once we have a real text-metrics provider (e.g. Parley, a JS bridge, etc.),
                // use measured bounds here and implement Vega-like overlap/clipping policies for
                // `labelAngle`.
                let y_label = {
                    let mut y_label = y + tick_extent + label_gap;
                    if self.label_angle != 0.0 {
                        let theta = self.label_angle.to_radians();
                        let sin = theta.sin();
                        if sin != 0.0 {
                            let w = estimate_text_width(&label, self.style.label_font_size);
                            let dy = 0.5 * w * sin;
                            match anchor {
                                TextAnchor::Start => y_label -= dy,
                                TextAnchor::End => y_label += dy,
                                TextAnchor::Middle => {}
                            }
                        }
                    }
                    y_label
                };
                out.push(
                    Mark::builder(MarkId::from_raw(self.id_base + 1000 + i as u64))
                        .text()
                        .z_index(z_order::AXIS_LABELS)
                        .x_const(x)
                        .y_const(y_label)
                        .text_const(label)
                        .text_anchor(anchor)
                        .text_baseline(TextBaseline::Hanging)
                        .angle_const(self.label_angle)
                        .font_size_const(self.style.label_font_size)
                        .fill_brush_const(self.style.label_fill.clone())
                        .build(),
                );
            }
        }

        if let Some(title) = &self.title {
            let x = (plot.x0 + plot.x1) * 0.5;
            // Place the title in the "title strip" at the outer edge of `axis_rect`.
            // See `marks_left` for rationale.
            let y = axis_rect.y1 - self.style.title_font_size;
            out.push(
                Mark::builder(MarkId::from_raw(self.id_base + 9000))
                    .text()
                    .z_index(z_order::AXIS_TITLES)
                    .x_const(x)
                    .y_const(y)
                    .text_const(title.clone())
                    .font_size_const(self.style.title_font_size)
                    .fill_brush_const(self.style.title_fill.clone())
                    .text_anchor_middle()
                    .text_baseline(TextBaseline::Hanging)
                    .build(),
            );
        }

        out
    }

    fn marks_top(&self, plot: Rect, axis_rect: Rect) -> Vec<Mark> {
        let y = plot.y0;
        let tick_size = self.tick_size.abs();
        let tick_extent = if self.ticks { tick_size } else { 0.0 };
        let label_gap = (self.tick_padding + self.label_padding).max(0.0);
        let (ticks, step) = self.tick_values();

        let continuous_scale = matches!(
            self.scale,
            ScaleSpec::Linear(_) | ScaleSpec::Log(_) | ScaleSpec::Time(_)
        )
        .then(|| self.scale_continuous(plot));
        let point_scale = matches!(self.scale, ScaleSpec::Point(_)).then(|| self.scale_point(plot));
        let band_scale = matches!(self.scale, ScaleSpec::Band(_)).then(|| self.scale_band(plot));
        let band_width = band_scale.map(|b| b.band_width()).unwrap_or(0.0);

        let tick_x = |v: f64| match self.scale {
            ScaleSpec::Linear(_) | ScaleSpec::Log(_) | ScaleSpec::Time(_) => {
                continuous_scale.expect("missing continuous scale").map(v)
            }
            ScaleSpec::Point(_) => point_scale
                .expect("missing point scale")
                .x(discrete_index(v)),
            ScaleSpec::Band(_) => {
                band_scale.expect("missing band scale").x(discrete_index(v)) + 0.5 * band_width
            }
        };

        let mut out = Vec::new();

        if let Some(grid) = &self.grid {
            let mut ticks_in_plot: Vec<f64> = ticks
                .iter()
                .copied()
                .filter(|v| {
                    let x = tick_x(*v);
                    x >= plot.x0 - 1.0e-9 && x <= plot.x1 + 1.0e-9
                })
                .collect();
            if let Some((d0, d1)) = self.continuous_domain() {
                push_if_missing(&mut ticks_in_plot, d0);
                push_if_missing(&mut ticks_in_plot, d1);
            }
            out.extend(grid_vertical(
                self.id_base,
                &ticks_in_plot,
                tick_x,
                plot,
                &grid.stroke.brush,
                grid.stroke.stroke_width,
                z_order::GRID_LINES,
            ));
        }

        // Domain line.
        if self.show_domain {
            let mut domain = BezPath::new();
            domain.move_to((plot.x0, y));
            domain.line_to((plot.x1, y));
            out.push(domain_mark(
                self.id_base,
                domain,
                &self.style.rule.brush,
                self.style.rule.stroke_width,
                z_order::AXIS_RULES,
            ));
        }

        let ticks_len = ticks.len();
        for (i, v) in ticks.iter().copied().enumerate() {
            let x = tick_x(v);
            if x < plot.x0 - 1.0e-9 || x > plot.x1 + 1.0e-9 {
                continue;
            }
            let label = self.format_tick(v, step);

            if self.ticks {
                let mut tick = BezPath::new();
                tick.move_to((x, y));
                tick.line_to((x, y - tick_size));
                out.push(tick_mark(
                    self.id_base,
                    i,
                    tick,
                    &self.style.rule.brush,
                    self.style.rule.stroke_width,
                    z_order::AXIS_RULES,
                ));
            }

            if self.labels {
                let (anchor, x) = if i == 0 {
                    (TextAnchor::Start, x.clamp(plot.x0, plot.x1))
                } else if i + 1 == ticks_len {
                    (TextAnchor::End, x.clamp(plot.x0, plot.x1))
                } else {
                    (TextAnchor::Middle, x)
                };

                // See `marks_bottom` for rotated label anchor compensation rationale.
                let y_label = {
                    let mut y_label = y - tick_extent - label_gap;
                    if self.label_angle != 0.0 {
                        let theta = self.label_angle.to_radians();
                        let sin = theta.sin();
                        if sin != 0.0 {
                            let w = estimate_text_width(&label, self.style.label_font_size);
                            let dy = 0.5 * w * sin;
                            match anchor {
                                TextAnchor::Start => y_label -= dy,
                                TextAnchor::End => y_label += dy,
                                TextAnchor::Middle => {}
                            }
                        }
                    }
                    y_label
                };
                out.push(
                    Mark::builder(MarkId::from_raw(self.id_base + 1000 + i as u64))
                        .text()
                        .z_index(z_order::AXIS_LABELS)
                        .x_const(x)
                        .y_const(y_label)
                        .text_const(label)
                        .text_anchor(anchor)
                        .text_baseline(TextBaseline::Ideographic)
                        .angle_const(self.label_angle)
                        .font_size_const(self.style.label_font_size)
                        .fill_brush_const(self.style.label_fill.clone())
                        .build(),
                );
            }
        }

        if let Some(title) = &self.title {
            let x = (plot.x0 + plot.x1) * 0.5;
            // Place the title in the "title strip" at the outer edge of `axis_rect`.
            // See `marks_left` for rationale.
            let y = axis_rect.y0 + self.style.title_font_size;
            out.push(
                Mark::builder(MarkId::from_raw(self.id_base + 9000))
                    .text()
                    .z_index(z_order::AXIS_TITLES)
                    .x_const(x)
                    .y_const(y)
                    .text_const(title.clone())
                    .font_size_const(self.style.title_font_size)
                    .fill_brush_const(self.style.title_fill.clone())
                    .text_anchor_middle()
                    .text_baseline(TextBaseline::Ideographic)
                    .build(),
            );
        }

        out
    }

    fn marks_left(&self, plot: Rect, axis_rect: Rect) -> Vec<Mark> {
        let x = plot.x0;
        let tick_size = self.tick_size.abs();
        let tick_extent = if self.ticks { tick_size } else { 0.0 };
        let label_gap = (self.tick_padding + self.label_padding).max(0.0);
        let (ticks, step) = self.tick_values();

        let continuous_scale = matches!(
            self.scale,
            ScaleSpec::Linear(_) | ScaleSpec::Log(_) | ScaleSpec::Time(_)
        )
        .then(|| self.scale_continuous(plot));
        let point_scale = matches!(self.scale, ScaleSpec::Point(_)).then(|| self.scale_point(plot));
        let band_scale = matches!(self.scale, ScaleSpec::Band(_)).then(|| self.scale_band(plot));
        let band_width = band_scale.map(|b| b.band_width()).unwrap_or(0.0);

        let tick_y = |v: f64| match self.scale {
            ScaleSpec::Linear(_) | ScaleSpec::Log(_) | ScaleSpec::Time(_) => {
                continuous_scale.expect("missing continuous scale").map(v)
            }
            ScaleSpec::Point(_) => point_scale
                .expect("missing point scale")
                .x(discrete_index(v)),
            ScaleSpec::Band(_) => {
                band_scale.expect("missing band scale").x(discrete_index(v)) + 0.5 * band_width
            }
        };

        let mut out = Vec::new();

        if let Some(grid) = &self.grid {
            // Clamp grid lines to the plot bounds. Ticks may be "niced" beyond the domain, but
            // we don't want grid lines to render outside the plot (e.g. into a title strip).
            let mut ticks_in_plot: Vec<f64> = ticks
                .iter()
                .copied()
                .filter(|v| {
                    let y = tick_y(*v);
                    y >= plot.y0 - 1.0e-9 && y <= plot.y1 + 1.0e-9
                })
                .collect();
            if let Some((d0, d1)) = self.continuous_domain() {
                push_if_missing(&mut ticks_in_plot, d0);
                push_if_missing(&mut ticks_in_plot, d1);
            }
            out.extend(grid_horizontal(
                self.id_base,
                &ticks_in_plot,
                tick_y,
                plot,
                &grid.stroke.brush,
                grid.stroke.stroke_width,
                z_order::GRID_LINES,
            ));
        }

        // Domain line.
        if self.show_domain {
            let mut domain = BezPath::new();
            domain.move_to((x, plot.y0));
            domain.line_to((x, plot.y1));
            out.push(domain_mark(
                self.id_base,
                domain,
                &self.style.rule.brush,
                self.style.rule.stroke_width,
                z_order::AXIS_RULES,
            ));
        }

        for (i, v) in ticks.into_iter().enumerate() {
            let y = tick_y(v);
            if y < plot.y0 - 1.0e-9 || y > plot.y1 + 1.0e-9 {
                continue;
            }
            let label = self.format_tick(v, step);

            if self.ticks {
                let mut tick = BezPath::new();
                tick.move_to((x, y));
                tick.line_to((x - tick_size, y));
                out.push(tick_mark(
                    self.id_base,
                    i,
                    tick,
                    &self.style.rule.brush,
                    self.style.rule.stroke_width,
                    z_order::AXIS_RULES,
                ));
            }

            if self.labels {
                out.push(
                    Mark::builder(MarkId::from_raw(self.id_base + 1000 + i as u64))
                        .text()
                        .z_index(z_order::AXIS_LABELS)
                        .x_const(x - tick_extent - label_gap)
                        .y_const(y)
                        .text_const(label)
                        .text_anchor_end()
                        .text_baseline(TextBaseline::Middle)
                        .angle_const(self.label_angle)
                        .font_size_const(self.style.label_font_size)
                        .fill_brush_const(self.style.label_fill.clone())
                        .build(),
                );
            }
        }

        if let Some(title) = &self.title {
            // Place the rotated title in the "title strip" at the outer edge of `axis_rect`.
            //
            // `axis_rect` is laid out using `AxisSpec::measure`, which includes (in order):
            // tick extent + label extent + `title_offset` + title thickness.
            // Placing the title at the axis_rect edge therefore respects `title_offset` and
            // avoids overlapping tick labels.
            let x = axis_rect.x0 + 0.5 * self.style.title_font_size;
            let y = (plot.y0 + plot.y1) * 0.5;
            out.push(
                Mark::builder(MarkId::from_raw(self.id_base + 9000))
                    .text()
                    .z_index(z_order::AXIS_TITLES)
                    .x_const(x)
                    .y_const(y)
                    .text_const(title.clone())
                    .font_size_const(self.style.title_font_size)
                    .fill_brush_const(self.style.title_fill.clone())
                    .text_anchor_middle()
                    .angle_const(-90.0)
                    .build(),
            );
        }

        out
    }

    fn marks_right(&self, plot: Rect, axis_rect: Rect) -> Vec<Mark> {
        let x = plot.x1;
        let tick_size = self.tick_size.abs();
        let tick_extent = if self.ticks { tick_size } else { 0.0 };
        let label_gap = (self.tick_padding + self.label_padding).max(0.0);
        let (ticks, step) = self.tick_values();

        let continuous_scale = matches!(
            self.scale,
            ScaleSpec::Linear(_) | ScaleSpec::Log(_) | ScaleSpec::Time(_)
        )
        .then(|| self.scale_continuous(plot));
        let point_scale = matches!(self.scale, ScaleSpec::Point(_)).then(|| self.scale_point(plot));
        let band_scale = matches!(self.scale, ScaleSpec::Band(_)).then(|| self.scale_band(plot));
        let band_width = band_scale.map(|b| b.band_width()).unwrap_or(0.0);

        let tick_y = |v: f64| match self.scale {
            ScaleSpec::Linear(_) | ScaleSpec::Log(_) | ScaleSpec::Time(_) => {
                continuous_scale.expect("missing continuous scale").map(v)
            }
            ScaleSpec::Point(_) => point_scale
                .expect("missing point scale")
                .x(discrete_index(v)),
            ScaleSpec::Band(_) => {
                band_scale.expect("missing band scale").x(discrete_index(v)) + 0.5 * band_width
            }
        };

        let mut out = Vec::new();

        if let Some(grid) = &self.grid {
            let mut ticks_in_plot: Vec<f64> = ticks
                .iter()
                .copied()
                .filter(|v| {
                    let y = tick_y(*v);
                    y >= plot.y0 - 1.0e-9 && y <= plot.y1 + 1.0e-9
                })
                .collect();
            if let Some((d0, d1)) = self.continuous_domain() {
                push_if_missing(&mut ticks_in_plot, d0);
                push_if_missing(&mut ticks_in_plot, d1);
            }
            out.extend(grid_horizontal(
                self.id_base,
                &ticks_in_plot,
                tick_y,
                plot,
                &grid.stroke.brush,
                grid.stroke.stroke_width,
                z_order::GRID_LINES,
            ));
        }

        // Domain line.
        if self.show_domain {
            let mut domain = BezPath::new();
            domain.move_to((x, plot.y0));
            domain.line_to((x, plot.y1));
            out.push(domain_mark(
                self.id_base,
                domain,
                &self.style.rule.brush,
                self.style.rule.stroke_width,
                z_order::AXIS_RULES,
            ));
        }

        for (i, v) in ticks.into_iter().enumerate() {
            let y = tick_y(v);
            if y < plot.y0 - 1.0e-9 || y > plot.y1 + 1.0e-9 {
                continue;
            }
            let label = self.format_tick(v, step);

            if self.ticks {
                let mut tick = BezPath::new();
                tick.move_to((x, y));
                tick.line_to((x + tick_size, y));
                out.push(tick_mark(
                    self.id_base,
                    i,
                    tick,
                    &self.style.rule.brush,
                    self.style.rule.stroke_width,
                    z_order::AXIS_RULES,
                ));
            }

            if self.labels {
                out.push(
                    Mark::builder(MarkId::from_raw(self.id_base + 1000 + i as u64))
                        .text()
                        .z_index(z_order::AXIS_LABELS)
                        .x_const(x + tick_extent + label_gap)
                        .y_const(y)
                        .text_const(label)
                        .text_anchor(TextAnchor::Start)
                        .text_baseline(TextBaseline::Middle)
                        .angle_const(self.label_angle)
                        .font_size_const(self.style.label_font_size)
                        .fill_brush_const(self.style.label_fill.clone())
                        .build(),
                );
            }
        }

        if let Some(title) = &self.title {
            // See `marks_left` for rationale.
            let x = axis_rect.x1 - 0.5 * self.style.title_font_size;
            let y = (plot.y0 + plot.y1) * 0.5;
            out.push(
                Mark::builder(MarkId::from_raw(self.id_base + 9000))
                    .text()
                    .z_index(z_order::AXIS_TITLES)
                    .x_const(x)
                    .y_const(y)
                    .text_const(title.clone())
                    .font_size_const(self.style.title_font_size)
                    .fill_brush_const(self.style.title_fill.clone())
                    .text_anchor_middle()
                    .angle_const(90.0)
                    .build(),
            );
        }

        out
    }
}

fn domain_mark(
    id_base: u64,
    path: BezPath,
    stroke: &Brush,
    stroke_width: f64,
    z_index: i32,
) -> Mark {
    let mut it = path.into_iter();
    let (x0, y0) = match it.next() {
        Some(kurbo::PathEl::MoveTo(p)) => (p.x, p.y),
        _ => (0.0, 0.0),
    };
    let (x1, y1) = match it.next() {
        Some(kurbo::PathEl::LineTo(p)) => (p.x, p.y),
        _ => (x0, y0),
    };
    RuleMarkSpec::new(MarkId::from_raw(id_base), x0, y0, x1, y1)
        .with_stroke(stroke.clone(), stroke_width)
        .with_z_index(z_index)
        .mark()
}

fn grid_vertical(
    id_base: u64,
    ticks: &[f64],
    map: impl Fn(f64) -> f64,
    plot: Rect,
    stroke: &Brush,
    stroke_width: f64,
    z_index: i32,
) -> Vec<Mark> {
    let base = id_base.wrapping_sub(5_000);
    let mut out = Vec::new();
    for (i, v) in ticks.iter().copied().enumerate() {
        let x = map(v);
        out.push(
            RuleMarkSpec::vertical(MarkId::from_raw(base + i as u64), x, plot.y0, plot.y1)
                .with_stroke(stroke.clone(), stroke_width)
                .with_z_index(z_index)
                .mark(),
        );
    }
    out
}

fn grid_horizontal(
    id_base: u64,
    ticks: &[f64],
    map: impl Fn(f64) -> f64,
    plot: Rect,
    stroke: &Brush,
    stroke_width: f64,
    z_index: i32,
) -> Vec<Mark> {
    let base = id_base.wrapping_sub(5_000);
    let mut out = Vec::new();
    for (i, v) in ticks.iter().copied().enumerate() {
        let y = map(v);
        out.push(
            RuleMarkSpec::horizontal(MarkId::from_raw(base + i as u64), y, plot.x0, plot.x1)
                .with_stroke(stroke.clone(), stroke_width)
                .with_z_index(z_index)
                .mark(),
        );
    }
    out
}

fn tick_mark(
    id_base: u64,
    index: usize,
    path: BezPath,
    stroke: &Brush,
    stroke_width: f64,
    z_index: i32,
) -> Mark {
    let mut it = path.into_iter();
    let (x0, y0) = match it.next() {
        Some(kurbo::PathEl::MoveTo(p)) => (p.x, p.y),
        _ => (0.0, 0.0),
    };
    let (x1, y1) = match it.next() {
        Some(kurbo::PathEl::LineTo(p)) => (p.x, p.y),
        _ => (x0, y0),
    };
    RuleMarkSpec::new(MarkId::from_raw(id_base + 1 + index as u64), x0, y0, x1, y1)
        .with_stroke(stroke.clone(), stroke_width)
        .with_z_index(z_index)
        .mark()
}

fn tick_step(ticks: &[f64]) -> f64 {
    let step = ticks
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(f64::INFINITY, f64::min);
    if step.is_finite() { step } else { 0.0 }
}

fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    // Rough heuristic (matches the demo SVG viewBox heuristic): ~0.6em per glyph.
    //
    // TODO: Replace with real shaped text metrics when available.
    0.6 * font_size * text.chars().count() as f64
}

fn discrete_index(v: f64) -> usize {
    if !v.is_finite() || v < 0.0 {
        return 0;
    }
    let v = v.round().min(10_000.0);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "value is clamped to a small non-negative range"
    )]
    {
        v as usize
    }
}

fn push_if_missing(ticks: &mut Vec<f64>, v: f64) {
    if !v.is_finite() {
        return;
    }
    let eps = 1.0e-9;
    if ticks.iter().any(|t| (*t - v).abs() <= eps) {
        return;
    }
    ticks.push(v);
}

#[cfg(test)]
mod tests {
    extern crate std;

    use kurbo::Rect;
    use kurbo::Shape;
    use vizir_core::{Encoding, MarkEncodings, MarkKind, TextEncodings};

    use super::*;
    use crate::HeuristicTextMeasurer;
    use crate::scale::{ScaleLinearSpec, ScaleLogSpec, ScaleTimeSpec};

    #[test]
    fn axis_measure_respects_ticks_and_labels_toggles() {
        let measurer = HeuristicTextMeasurer;
        let axis = AxisSpec::left(1, ScaleLinearSpec::new((0.0, 10.0))).with_tick_count(3);

        let with_all = axis.measure(&measurer);
        let no_labels = axis.clone().with_labels(false).measure(&measurer);
        let no_ticks = axis.clone().with_ticks(false).measure(&measurer);
        let none = axis
            .clone()
            .with_ticks(false)
            .with_labels(false)
            .with_domain(false)
            .measure(&measurer);

        assert!(with_all > 0.0);
        assert!(no_labels < with_all);
        assert!(no_ticks < with_all);
        assert_eq!(none, 0.0);
    }

    #[test]
    fn axis_measure_accounts_for_label_angle() {
        let measurer = HeuristicTextMeasurer;
        let axis = AxisSpec::bottom(1, ScaleLinearSpec::new((0.0, 10.0)))
            .with_tick_count(6)
            .with_label_angle(0.0);
        let a0 = axis.measure(&measurer);
        let a45 = axis.with_label_angle(45.0).measure(&measurer);
        assert!(a45 >= a0);
    }

    #[test]
    fn axis_uses_custom_tick_formatter_for_labels() {
        let plot = Rect::new(0.0, 0.0, 100.0, 50.0);
        let axis_rect = Rect::new(0.0, 50.0, 100.0, 60.0);

        let axis = AxisSpec::bottom(1, ScaleLinearSpec::new((0.0, 10.0)))
            .with_tick_count(3)
            .with_tick_formatter(|_v, _step| String::from("X"));

        let marks = axis.marks(plot, axis_rect);
        let mut saw_label = false;
        for m in marks {
            if m.kind != MarkKind::Text {
                continue;
            }
            let MarkEncodings::Text(e) = &m.encodings else {
                continue;
            };
            let TextEncodings { text, .. } = e.as_ref();
            if let Encoding::Const(s) = text {
                assert_eq!(s, "X");
                saw_label = true;
            }
        }
        assert!(saw_label);
    }

    #[test]
    fn axis_left_title_uses_axis_rect_edge_to_avoid_label_overlap() {
        let measurer = HeuristicTextMeasurer;
        let plot = Rect::new(100.0, 0.0, 200.0, 100.0);

        let axis = AxisSpec::left(1, ScaleLinearSpec::new((0.0, 10.0)))
            .with_tick_count(3)
            .with_title("Y")
            .with_title_offset(10.0);

        let w = axis.measure(&measurer);
        let axis_rect = Rect::new(plot.x0 - w, plot.y0, plot.x0, plot.y1);
        let marks = axis.marks(plot, axis_rect);

        let title_id = MarkId::from_raw(1 + 9000);
        let mut title_x = None;
        for m in marks {
            if m.id == title_id
                && let MarkEncodings::Text(enc) = m.encodings
                && let Encoding::Const(x) = enc.x
            {
                title_x = Some(x);
            }
        }

        let title_x = title_x.expect("missing title x");
        let expected = axis_rect.x0 + 0.5 * axis.style.title_font_size;
        assert!((title_x - expected).abs() < 1e-9);
    }

    #[test]
    fn axis_right_title_uses_axis_rect_edge_to_avoid_label_overlap() {
        let measurer = HeuristicTextMeasurer;
        let plot = Rect::new(0.0, 0.0, 100.0, 100.0);

        let axis = AxisSpec::right(1, ScaleLinearSpec::new((0.0, 10.0)))
            .with_tick_count(3)
            .with_title("Y")
            .with_title_offset(10.0);

        let w = axis.measure(&measurer);
        let axis_rect = Rect::new(plot.x1, plot.y0, plot.x1 + w, plot.y1);
        let marks = axis.marks(plot, axis_rect);

        let title_id = MarkId::from_raw(1 + 9000);
        let mut title_x = None;
        for m in marks {
            if m.id == title_id
                && let MarkEncodings::Text(enc) = m.encodings
                && let Encoding::Const(x) = enc.x
            {
                title_x = Some(x);
            }
        }

        let title_x = title_x.expect("missing title x");
        let expected = axis_rect.x1 - 0.5 * axis.style.title_font_size;
        assert!((title_x - expected).abs() < 1e-9);
    }

    #[test]
    fn axis_bottom_title_uses_axis_rect_edge_to_avoid_label_overlap() {
        let measurer = HeuristicTextMeasurer;
        let plot = Rect::new(0.0, 0.0, 100.0, 100.0);

        let axis = AxisSpec::bottom(1, ScaleLinearSpec::new((0.0, 10.0)))
            .with_tick_count(3)
            .with_title("X")
            .with_title_offset(10.0);

        let h = axis.measure(&measurer);
        let axis_rect = Rect::new(plot.x0, plot.y1, plot.x1, plot.y1 + h);
        let marks = axis.marks(plot, axis_rect);

        let title_id = MarkId::from_raw(1 + 9000);
        let mut title_y = None;
        for m in marks {
            if m.id == title_id
                && let MarkEncodings::Text(enc) = m.encodings
                && let Encoding::Const(y) = enc.y
            {
                title_y = Some(y);
            }
        }

        let title_y = title_y.expect("missing title y");
        let expected = axis_rect.y1 - axis.style.title_font_size;
        assert!((title_y - expected).abs() < 1e-9);
    }

    #[test]
    fn axis_top_title_uses_axis_rect_edge_to_avoid_label_overlap() {
        let measurer = HeuristicTextMeasurer;
        let plot = Rect::new(0.0, 0.0, 100.0, 100.0);

        let axis = AxisSpec::top(1, ScaleLinearSpec::new((0.0, 10.0)))
            .with_tick_count(3)
            .with_title("X")
            .with_title_offset(10.0);

        let h = axis.measure(&measurer);
        let axis_rect = Rect::new(plot.x0, plot.y0 - h, plot.x1, plot.y0);
        let marks = axis.marks(plot, axis_rect);

        let title_id = MarkId::from_raw(1 + 9000);
        let mut title_y = None;
        for m in marks {
            if m.id == title_id
                && let MarkEncodings::Text(enc) = m.encodings
                && let Encoding::Const(y) = enc.y
            {
                title_y = Some(y);
            }
        }

        let title_y = title_y.expect("missing title y");
        let expected = axis_rect.y0 + axis.style.title_font_size;
        assert!((title_y - expected).abs() < 1e-9);
    }

    #[test]
    fn rotated_bottom_labels_use_consistent_anchor() {
        let plot = Rect::new(0.0, 0.0, 100.0, 50.0);
        let axis_rect = Rect::new(0.0, 50.0, 100.0, 80.0);

        let axis = AxisSpec::bottom(1, ScaleLinearSpec::new((0.0, 10.0)))
            .with_tick_count(3)
            .with_label_angle(-45.0);

        let marks = axis.marks(plot, axis_rect);
        let mut saw = 0_usize;
        for m in marks {
            let MarkEncodings::Text(enc) = &m.encodings else {
                continue;
            };
            if m.id.0 <= 1000 || m.id.0 >= 2000 {
                continue;
            }
            let Encoding::Const(anchor) = enc.anchor else {
                panic!("expected const anchor");
            };
            assert!(matches!(
                anchor,
                TextAnchor::Start | TextAnchor::Middle | TextAnchor::End
            ));
            saw += 1;
        }
        assert!(saw > 0);
    }

    #[test]
    fn axis_without_ticks_emits_no_tick_path_marks() {
        let plot = Rect::new(0.0, 0.0, 100.0, 50.0);
        let axis_rect = Rect::new(0.0, 50.0, 100.0, 60.0);

        let axis = AxisSpec::bottom(1, ScaleLinearSpec::new((0.0, 10.0)))
            .with_tick_count(3)
            .with_ticks(false)
            .with_domain(false);

        let marks = axis.marks(plot, axis_rect);
        assert!(
            marks.iter().all(|m| m.kind != MarkKind::Path),
            "expected no path marks when ticks/domain are disabled"
        );
    }

    #[test]
    fn axis_left_grid_does_not_extend_outside_plot() {
        let plot = Rect::new(50.0, 30.0, 250.0, 130.0);
        let axis_rect = Rect::new(0.0, 30.0, 50.0, 130.0);

        let axis = AxisSpec::left(1, ScaleLinearSpec::new((-0.7, 3.29)))
            .with_tick_count(6)
            .with_grid(GridStyle {
                stroke: StrokeStyle::solid(css::BLACK, 1.0),
            });

        let marks = axis.marks(plot, axis_rect);
        for m in marks {
            if m.z_index != z_order::GRID_LINES {
                continue;
            }
            let MarkEncodings::Path(e) = &m.encodings else {
                continue;
            };
            let Encoding::Const(p) = &e.path else {
                continue;
            };
            let b = p.bounding_box();
            assert!(
                b.y0 >= plot.y0 - 1.0e-9,
                "grid above plot: {b:?} vs {plot:?}"
            );
            assert!(
                b.y1 <= plot.y1 + 1.0e-9,
                "grid below plot: {b:?} vs {plot:?}"
            );
        }
    }

    #[test]
    fn axis_grid_includes_domain_endpoints() {
        // Domain max is not a "nice" number; grid should still include a line at the plot edge.
        let plot = Rect::new(10.0, 20.0, 110.0, 120.0);
        let axis_rect = Rect::new(0.0, 20.0, 10.0, 120.0);
        let domain = (0.0, 3.29);

        let axis = AxisSpec::left(1, ScaleLinearSpec::new(domain)).with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK, 1.0),
        });

        let marks = axis.marks(plot, axis_rect);
        let mut saw_top_edge = false;
        for m in marks {
            if m.z_index != z_order::GRID_LINES {
                continue;
            }
            let MarkEncodings::Path(e) = &m.encodings else {
                continue;
            };
            let Encoding::Const(p) = &e.path else {
                continue;
            };
            let b = p.bounding_box();
            if (b.y0 - plot.y0).abs() < 1.0e-9 && (b.y1 - plot.y0).abs() < 1.0e-9 {
                saw_top_edge = true;
            }
        }
        assert!(
            saw_top_edge,
            "expected a grid line at plot.y0 for domain max"
        );
    }

    #[test]
    fn time_axis_defaults_to_time_formatter() {
        let plot = Rect::new(0.0, 0.0, 200.0, 100.0);
        let axis_rect = Rect::new(0.0, 100.0, 200.0, 140.0);

        let axis = AxisSpec::bottom(1, ScaleTimeSpec::new((0.0, 300.0))).with_tick_count(6);
        let marks = axis.marks(plot, axis_rect);
        let labels: Vec<String> = marks
            .into_iter()
            .filter_map(|m| match m.encodings {
                MarkEncodings::Text(enc) => match &enc.as_ref().text {
                    Encoding::Const(s) => Some(s.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert!(
            labels.iter().any(|s| s.contains(':')),
            "expected at least one time-formatted label, got {labels:?}"
        );
    }

    #[test]
    fn log_axis_includes_powers_of_base_in_ticks() {
        let plot = Rect::new(0.0, 0.0, 200.0, 100.0);
        let axis_rect = Rect::new(0.0, 0.0, 40.0, 100.0);

        let axis =
            AxisSpec::left(1, ScaleLogSpec::new((1.0, 1000.0)).with_base(10.0)).with_tick_count(10);

        let marks = axis.marks(plot, axis_rect);
        let labels: Vec<String> = marks
            .into_iter()
            .filter_map(|m| match m.encodings {
                MarkEncodings::Text(enc) => match &enc.as_ref().text {
                    Encoding::Const(s) => Some(s.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert!(labels.iter().any(|s| s == "1"), "missing '1' in {labels:?}");
        assert!(
            labels.iter().any(|s| s == "1000"),
            "missing '1000' in {labels:?}"
        );
    }
}
